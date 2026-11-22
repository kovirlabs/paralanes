//! Step 1 of the CPU throughput strategy's benchmark plan (§9): the
//! per-socket pipeline with full metrics, so later steps (isolation, cache
//! control, ...) change one layer at a time against the same measurement.
//!
//! Per socket, core index 0 is left for housekeeping (vCPUs 0 and 12 on the
//! Dell guest), so the core assignment is identical before and after
//! isolating the other cores. Socket `s` cores: 1 ingest, 2 store, 3.. W
//! feature workers, last core sender.
//!
//! Metrics:
//! - per-frame latency, ingest receipt -> durable ack written, and
//!   ingest receipt -> processed by its feature worker (p50/p99/p99.9/max);
//!   both include the 100 ms storage batch, by design
//! - jitter-sensitive: ring transit (ingest push -> store pop) per frame, and
//!   compute time per feature window (stretched by interrupts or preemption)
//! - ring occupancy sampled every 1 ms (ingest->store and store->workers)
//! - interrupts per vCPU over the run, from /proc/interrupts (all sources,
//!   and the local timer LOC alone), split into pipeline cores and the rest

use std::collections::BTreeMap;
use std::io::{BufReader, BufWriter, Write};
use std::net::TcpListener;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Barrier, Mutex};
use std::time::{Duration, Instant};

use diagsteno::config::{ChannelSpec, default_layout};
use diagsteno::features::{Windower, reference};
use diagsteno::frame::{Frame, write_ack};
use diagsteno::sender;
use diagsteno_store::{SampleBlock, Store};
use serde_json::json;
use stagelanes_arch::{OsCpu, Priority, Topology, set_current_thread_priority};
use stagelanes_ring::{Producer, Ring};

use crate::{BATCH, Error, RING, backoff, cores_on, frames_for, fresh_dir, pin, shard_layout};

/// Priority for the pipeline threads (ingest, store, feature workers), from
/// --priority; the sender and the monitor stay at Normal.
pub static STAGE_PRIORITY: std::sync::OnceLock<Priority> = std::sync::OnceLock::new();

/// Pins a pipeline thread and gives it the stage priority.
fn pin_stage(cpu: OsCpu) -> Result<(), Error> {
    pin(Some(cpu))?;
    let p = STAGE_PRIORITY.get().copied().unwrap_or(Priority::Normal);
    if p != Priority::Normal {
        set_current_thread_priority(p)?;
    }
    Ok(())
}

fn pct(sorted: &[u32], p: f64) -> Option<u32> {
    if sorted.is_empty() {
        return None;
    }
    let i = ((p / 100.0) * (sorted.len() - 1) as f64).round() as usize;
    sorted.get(i).copied()
}

fn summary(mut v: Vec<u32>) -> serde_json::Value {
    v.sort_unstable();
    json!({"n": v.len(), "p50": pct(&v, 50.0), "p99": pct(&v, 99.0),
           "p99_9": pct(&v, 99.9), "max": v.last()})
}

fn push_spin<T, R: std::ops::Deref<Target = Ring<T, RING>>>(
    tx: &mut Producer<T, RING, R>,
    mut item: T,
) {
    let mut spins = 0;
    while let Err(back) = tx.push(item) {
        item = back;
        backoff(&mut spins);
    }
}

/// Counts pushed and popped items so a monitor can sample occupancy.
#[derive(Default)]
struct Occ {
    pushed: AtomicU64,
    popped: AtomicU64,
}

impl Occ {
    fn now(&self) -> u32 {
        self.pushed
            .load(Ordering::Relaxed)
            .saturating_sub(self.popped.load(Ordering::Relaxed)) as u32
    }
}

struct Shard {
    transit_us: Vec<u32>,
    compute_us: Vec<u32>,
    ack_us: Vec<u32>,
    feature_us: Vec<u32>,
    row_us: Vec<u32>,
    check: Vec<(diagsteno::features::Window, diagsteno_store::FeatureRow)>,
    cpu_in_compute: Duration,
    frames: u64,
    samples: u64,
    windows: u64,
}

#[allow(clippy::too_many_arguments)]
fn run_shard(
    listener: TcpListener,
    layout: Vec<ChannelSpec>,
    store_dir: PathBuf,
    cores: Vec<OsCpu>,
    workers: usize,
    occ_ingest: Arc<Occ>,
    occ_workers: Arc<Vec<Occ>>,
) -> Result<Shard, Error> {
    let store = Store::open(&store_dir)?;
    let mut feature_store = store.table_writer();
    let mut store = store;
    let (mut to_store, mut from_ingest) = Ring::<(Instant, Frame), RING>::new().split_arc();
    let ingest_done = Arc::new(AtomicBool::new(false));
    let store_done = Arc::new(AtomicBool::new(false));
    let mut worker_tx = Vec::new();
    let mut worker_rx = Vec::new();
    for _ in 0..workers {
        let (tx, rx) = Ring::<(Instant, Frame), RING>::new().split_arc();
        worker_tx.push(tx);
        worker_rx.push(rx);
    }
    // GPU feature stage: open the device before the sender connects, so
    // device start-up is not inside the measured run.
    let mut gpu_engines: Vec<Option<GpuEngine>> = (0..workers)
        .map(|_| match FEATURE_ENGINE.get() {
            Some(FeatureEngine::Gpu { .. }) => open_gpu_engine().map(Some),
            _ => Ok(None),
        })
        .collect::<Result<_, Error>>()?;
    let (stream, _) = listener.accept()?;
    stream.set_nodelay(true)?;
    let ack_stream = stream.try_clone()?;

    let ingest = {
        let (done, occ, cpu) = (ingest_done.clone(), occ_ingest.clone(), cores[0]);
        std::thread::spawn(move || -> Result<(), Error> {
            pin_stage(cpu)?;
            let mut r = BufReader::with_capacity(1 << 16, stream);
            while let Some(f) = Frame::read_from(&mut r)? {
                push_spin(&mut to_store, (Instant::now(), f));
                occ.pushed.fetch_add(1, Ordering::Relaxed);
            }
            done.store(true, Ordering::Release);
            Ok(())
        })
    };

    let store_thread = {
        let (ingest_done, store_done) = (ingest_done.clone(), store_done.clone());
        let (occ_in, occ_w) = (occ_ingest.clone(), occ_workers.clone());
        let cpu = cores[1];
        std::thread::spawn(move || -> Result<(Vec<u32>, Vec<u32>, u64, u64), Error> {
            pin_stage(cpu)?;
            let mut acks = BufWriter::new(ack_stream);
            let mut batch: Vec<(Instant, Frame)> = Vec::new();
            let mut ack_us = Vec::new();
            let mut transit_us = Vec::new();
            let (mut frames, mut samples) = (0u64, 0u64);
            let mut started = Instant::now();
            loop {
                let finished = ingest_done.load(Ordering::Acquire);
                let n = from_ingest.pop_with(1024, |x| {
                    transit_us.push(x.0.elapsed().as_micros().min(u32::MAX as u128) as u32);
                    batch.push(x);
                });
                occ_in.popped.fetch_add(n as u64, Ordering::Relaxed);
                let drained = finished && n == 0;
                if !batch.is_empty()
                    && (started.elapsed() >= BATCH || drained || batch.len() >= 8192)
                {
                    let blocks: Vec<SampleBlock> = batch
                        .iter()
                        .map(|(_, f)| SampleBlock {
                            channel: f.channel,
                            seq: f.seq,
                            first_sample: f.first_sample,
                            sample_rate: f.sample_rate,
                            samples: f.samples.clone(),
                        })
                        .collect();
                    store.put_samples(&blocks)?;
                    let mut last = BTreeMap::new();
                    for (_, f) in &batch {
                        last.insert(f.channel, f.seq);
                    }
                    let entries: Vec<(u16, u64)> = last.into_iter().collect();
                    let _ = write_ack(&mut acks, &entries).and_then(|()| acks.flush());
                    for (t, f) in batch.drain(..) {
                        ack_us.push(t.elapsed().as_micros().min(u32::MAX as u128) as u32);
                        frames += 1;
                        samples += f.samples.len() as u64;
                        let w = usize::from(f.channel) % worker_tx.len();
                        push_spin(&mut worker_tx[w], (t, f));
                        occ_w[w].pushed.fetch_add(1, Ordering::Relaxed);
                    }
                    started = Instant::now();
                } else if drained && batch.is_empty() {
                    break;
                } else if n == 0 {
                    std::thread::yield_now();
                }
            }
            store_done.store(true, Ordering::Release);
            Ok((ack_us, transit_us, frames, samples))
        })
    };

    let (row_tx, row_rx) = std::sync::mpsc::channel::<Vec<diagsteno_store::FeatureRow>>();
    let mut worker_threads = Vec::new();
    for (w, mut rx) in worker_rx.into_iter().enumerate() {
        let (store_done, layout, row_tx, occ) = (
            store_done.clone(),
            layout.clone(),
            row_tx.clone(),
            occ_workers.clone(),
        );
        let cpu = cores[2 + w];
        let engine = FEATURE_ENGINE.get().copied().unwrap_or(FeatureEngine::Cpu);
        let gpu_engine = gpu_engines[w].take();
        worker_threads.push(std::thread::spawn(move || -> Result<WorkerOut, Error> {
            pin_stage(cpu)?;
            if let FeatureEngine::Gpu { batch } = engine {
                let engine = gpu_engine.ok_or("GPU engine not opened")?;
                return gpu_worker(engine, rx, &store_done, &layout, &row_tx, &occ[w], batch);
            }
            let mut out = WorkerOut::default();
            let mut windower = Windower::new(&layout);
            loop {
                let finished = store_done.load(Ordering::Acquire);
                let mut rows = Vec::new();
                let n = rx.pop_with(64, |(t, f)| {
                    for win in windower.push(&f) {
                        let c = Instant::now();
                        rows.push(reference(&win));
                        out.compute.push(us(c.elapsed()));
                        out.row_lat.push(us(t.elapsed()));
                    }
                    out.lat.push(us(t.elapsed()));
                });
                occ[w].popped.fetch_add(n as u64, Ordering::Relaxed);
                if !rows.is_empty() && row_tx.send(rows).is_err() {
                    break;
                }
                if n == 0 {
                    if finished {
                        break;
                    }
                    std::thread::yield_now();
                }
            }
            Ok(out)
        }));
    }
    drop(row_tx);
    // Feature rows: stored once per second by time (the store core's thread
    // budget; this thread is pinned there too).
    pin_stage(cores[1])?;
    let mut pending = Vec::new();
    let mut windows = 0u64;
    let mut deadline = Instant::now() + Duration::from_secs(1);
    loop {
        match row_rx.recv_timeout(deadline.saturating_duration_since(Instant::now())) {
            Ok(rows) => {
                pending.extend(rows);
                continue;
            }
            Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {}
            Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => break,
        }
        if !pending.is_empty() {
            feature_store.put_features(&pending)?;
            windows += pending.len() as u64;
            pending.clear();
        }
        deadline = Instant::now() + Duration::from_secs(1);
    }
    if !pending.is_empty() {
        feature_store.put_features(&pending)?;
        windows += pending.len() as u64;
    }
    ingest.join().map_err(|_| "ingest panicked")??;
    let (ack_us, transit_us, frames, samples) =
        store_thread.join().map_err(|_| "store panicked")??;
    let mut feature_us = Vec::new();
    let mut compute_us = Vec::new();
    let mut row_us = Vec::new();
    let mut check = Vec::new();
    let mut cpu_in_compute = Duration::ZERO;
    for w in worker_threads {
        let o = w.join().map_err(|_| "worker panicked")??;
        feature_us.extend(o.lat);
        compute_us.extend(o.compute);
        row_us.extend(o.row_lat);
        check.extend(o.check);
        cpu_in_compute += o.cpu_in_compute;
    }
    Ok(Shard {
        transit_us,
        compute_us,
        ack_us,
        feature_us,
        row_us,
        check,
        cpu_in_compute,
        frames,
        samples,
        windows,
    })
}

/// How feature windows are computed: the CPU reference on each worker, or
/// one worker batching windows through diagsteno's `Engine` on the GPU.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FeatureEngine {
    Cpu,
    Gpu { batch: usize },
}

/// Set from `--features cpu|gpu` and `--batch`.
pub static FEATURE_ENGINE: std::sync::OnceLock<FeatureEngine> = std::sync::OnceLock::new();

fn us(d: Duration) -> u32 {
    d.as_micros().min(u128::from(u32::MAX)) as u32
}

/// What one feature worker measured.
#[derive(Default)]
struct WorkerOut {
    /// Per frame: ingest receipt -> popped by this worker.
    lat: Vec<u32>,
    /// Per window: compute time (GPU: the batch's time / its windows).
    compute: Vec<u32>,
    /// Per window: ingest receipt of the frame that completed it -> its row
    /// computed.
    row_lat: Vec<u32>,
    /// GPU only: every window and the row the GPU computed, checked against
    /// the CPU reference after the run, outside the timing.
    check: Vec<(diagsteno::features::Window, diagsteno_store::FeatureRow)>,
    /// GPU only: CPU time this thread spent inside Engine::compute.
    cpu_in_compute: Duration,
}

type FrameRx =
    stagelanes_ring::Consumer<(Instant, Frame), RING, Arc<Ring<(Instant, Frame), RING>>>;

/// The GPU feature stage: windows the frames it receives, and computes them
/// in batches of `batch` with `Engine` on the first hardware adapter
/// (`WGPU_BACKEND` chooses the API). A partial batch is flushed once the
/// ring has been idle for 10 ms, and at the end.
type GpuEngine = diagsteno::features::Engine<bytelanes_wgpu::WgpuBackend>;

/// diagsteno's feature engine on the first hardware adapter
/// (`WGPU_BACKEND` chooses the API).
fn open_gpu_engine() -> Result<GpuEngine, Error> {
    let adapter = bytelanes_wgpu::hardware_adapters()
        .into_iter()
        .next()
        .ok_or("no hardware GPU adapter")?;
    let backend = bytelanes_wgpu::WgpuBackend::new(&adapter)?;
    Ok(diagsteno::features::Engine::new(backend))
}

fn gpu_worker(
    mut engine: GpuEngine,
    mut rx: FrameRx,
    store_done: &AtomicBool,
    layout: &[ChannelSpec],
    row_tx: &std::sync::mpsc::Sender<Vec<diagsteno_store::FeatureRow>>,
    occ: &Occ,
    batch: usize,
) -> Result<WorkerOut, Error> {
    let mut out = WorkerOut::default();
    let mut windower = Windower::new(layout);
    let mut pending: Vec<(Instant, diagsteno::features::Window)> = Vec::new();
    let mut last_pop = Instant::now();
    loop {
        let finished = store_done.load(Ordering::Acquire);
        let n = rx.pop_with(64, |(t, f)| {
            for win in windower.push(&f) {
                pending.push((t, win));
            }
            out.lat.push(us(t.elapsed()));
        });
        occ.popped.fetch_add(n as u64, Ordering::Relaxed);
        if n > 0 {
            last_pop = Instant::now();
        }
        let idle = n == 0 && last_pop.elapsed() >= Duration::from_millis(10);
        let drained = finished && n == 0;
        while pending.len() >= batch || (!pending.is_empty() && (idle || drained)) {
            let take = pending.len().min(batch);
            let (ts, wins): (Vec<Instant>, Vec<_>) = pending.drain(..take).unzip();
            let c = Instant::now();
            let tc = cpu_time::ThreadTime::now();
            let rows = engine
                .compute(&wins)
                .map_err(|e| format!("GPU features: {e:?}"))?;
            // Engine::compute submits and then spins on poll until the GPU
            // finishes; this thread's CPU time in here is that waiting plus
            // the host-side feature work (packing, band sums).
            out.cpu_in_compute += tc.elapsed();
            let per = us(c.elapsed()) / wins.len().max(1) as u32;
            for t in &ts {
                out.compute.push(per);
                out.row_lat.push(us(t.elapsed()));
            }
            out.check.extend(wins.into_iter().zip(rows.iter().cloned()));
            if row_tx.send(rows).is_err() {
                return Ok(out);
            }
        }
        if n == 0 {
            if drained && pending.is_empty() {
                break;
            }
            std::thread::yield_now();
        }
    }
    Ok(out)
}

/// Run the step-1 measurement: two pinned shards with one housekeeping core
/// per socket reserved, real-time or unpaced.
pub fn run(
    topo: &Topology,
    store_dir: &Path,
    seconds: u64,
    workers: usize,
    realtime: bool,
) -> Result<serde_json::Value, Error> {
    // One shard per socket; a single-socket machine runs one shard with the
    // whole layout.
    let single = cores_on(topo, 1).is_empty();
    let nshards = if single { 1 } else { 2 };
    let ready = Arc::new(Barrier::new(nshards + 1));
    let mut shards = Vec::new();
    let mut senders = Vec::new();
    let mut pipeline_cpus: Vec<u32> = Vec::new();
    let mut occ_ingest = Vec::new();
    let mut occ_workers = Vec::new();
    for s in 0..nshards {
        let layout = if single {
            default_layout()
        } else {
            shard_layout(s)
        };
        let all = cores_on(topo, s as u32);
        // Index 0: housekeeping (not used). 1 ingest, 2 store, 3.. workers.
        let cores: Vec<OsCpu> = all[1..].to_vec();
        if cores.len() < 3 + workers {
            return Err(format!("socket {s} needs {} cores", 4 + workers).into());
        }
        pipeline_cpus.extend(cores[..2 + workers].iter().map(|c| c.number));
        let sender_core = *all.last().ok_or("no cores")?;
        let dir = fresh_dir(store_dir, &format!("step-shard{s}"))?;
        let listener = TcpListener::bind("127.0.0.1:0")?;
        let addr = listener.local_addr()?;
        let oi = Arc::new(Occ::default());
        let ow: Arc<Vec<Occ>> = Arc::new((0..workers).map(|_| Occ::default()).collect());
        occ_ingest.push(oi.clone());
        occ_workers.push(ow.clone());
        {
            let layout = layout.clone();
            shards.push(std::thread::spawn(move || {
                run_shard(listener, layout, dir, cores, workers, oi, ow)
            }));
        }
        let (ready, per_tick) = (ready.clone(), layout.len());
        senders.push(std::thread::spawn(
            move || -> Result<sender::SendReport, Error> {
                pin(Some(sender_core))?;
                let frames = frames_for(&layout, seconds);
                ready.wait();
                Ok(sender::send(addr, frames, per_tick, realtime)?)
            },
        ));
    }
    // Ring occupancy monitor, sampled every 1 ms on a housekeeping core.
    let stop = Arc::new(AtomicBool::new(false));
    let samples: Arc<Mutex<(Vec<u32>, Vec<u32>)>> = Arc::default();
    let monitor = {
        let (stop, samples) = (stop.clone(), samples.clone());
        let (oi, ow) = (occ_ingest.clone(), occ_workers.clone());
        let hk = cores_on(topo, 0)[0];
        std::thread::spawn(move || -> Result<(), Error> {
            pin(Some(hk))?;
            while !stop.load(Ordering::Relaxed) {
                let ingest = oi.iter().map(|o| o.now()).max().unwrap_or(0);
                let worker = ow
                    .iter()
                    .flat_map(|v| v.iter().map(Occ::now))
                    .max()
                    .unwrap_or(0);
                {
                    let mut g = samples.lock().unwrap_or_else(|e| e.into_inner());
                    g.0.push(ingest);
                    g.1.push(worker);
                }
                std::thread::sleep(Duration::from_millis(1));
            }
            Ok(())
        })
    };
    let sampler = crate::irq::Sampler::start(store_dir)?;
    ready.wait();
    let t0 = Instant::now();
    let cpu0 = cpu_time::ProcessTime::now();
    let mut reports = Vec::new();
    for h in senders {
        reports.push(h.join().map_err(|_| "sender panicked")??);
    }
    let mut ack_us = Vec::new();
    let mut feature_us = Vec::new();
    let mut transit_us = Vec::new();
    let mut compute_us = Vec::new();
    let mut row_us = Vec::new();
    let mut check = Vec::new();
    let mut gpu_wait_cpu = Duration::ZERO;
    let (mut frames, mut samples_n, mut windows) = (0u64, 0u64, 0u64);
    for h in shards {
        let s = h.join().map_err(|_| "shard panicked")??;
        ack_us.extend(s.ack_us);
        transit_us.extend(s.transit_us);
        compute_us.extend(s.compute_us);
        feature_us.extend(s.feature_us);
        row_us.extend(s.row_us);
        check.extend(s.check);
        gpu_wait_cpu += s.cpu_in_compute;
        frames += s.frames;
        samples_n += s.samples;
        windows += s.windows;
    }
    let elapsed = t0.elapsed().as_secs_f64();
    let cpu_s = cpu0.elapsed().as_secs_f64();
    // GPU rows against the CPU reference, after the clock stopped.
    let checked = check.len();
    let mismatches = check
        .iter()
        .filter(|(win, row)| reference(win) != *row)
        .count();
    drop(check);
    let irq = sampler.stop(&pipeline_cpus, elapsed)?;
    stop.store(true, Ordering::Relaxed);
    monitor.join().map_err(|_| "monitor panicked")??;
    let (occ_i, occ_w) = {
        let g = samples.lock().unwrap_or_else(|e| e.into_inner());
        (g.0.clone(), g.1.clone())
    };
    let late: u64 = reports.iter().map(|r| r.late_ticks).sum();
    let mut out = json!({
        "realtime": realtime,
        "elapsed_s": elapsed,
        "frames": frames, "samples": samples_n, "windows": windows,
        "late_ticks": late,
        "errors": reports.iter().filter_map(|r| r.error.clone()).collect::<Vec<_>>(),
        "pipeline_cpus": pipeline_cpus,
        "latency_us": {"ingest_to_ack": summary(ack_us),
                       "ingest_to_feature_worker": summary(feature_us),
                       "ring_transit_ingest_to_store": summary(transit_us),
                       "window_compute": summary(compute_us),
                       "ingest_to_feature_row": summary(row_us)},
        "ring_occupancy": {"ingest_to_store_max_over_shards": summary(occ_i),
                           "store_to_worker_max_over_workers": summary(occ_w)},
        "feature_engine": format!("{:?}", FEATURE_ENGINE.get().copied().unwrap_or(FeatureEngine::Cpu)),
        "process_cpu_s": cpu_s,
        "gpu_feature_thread_cpu_in_compute_s": gpu_wait_cpu.as_secs_f64(),
        "gpu_rows_checked_against_reference": checked,
        "gpu_rows_differing_from_reference": mismatches,
    });
    // Interrupt fields: the Linux names as before, plus Windows' DPC counters.
    if let (Some(o), Some(i)) = (out.as_object_mut(), irq.as_object()) {
        o.extend(i.clone());
    }
    Ok(out)
}
