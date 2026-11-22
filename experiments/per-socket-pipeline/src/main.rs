//! Experiment: diagsteno ingest as one complete pipeline per socket, built on
//! stagelanes (pinning, lock-free rings), compared with diagsteno's own
//! single, unpinned ingest. Not a milestone; diagsteno itself is unchanged
//! and used only as a library.
//!
//! ```text
//! per-socket-pipeline --layout baseline|per-socket|per-socket-unpinned
//!                     --store-dir DIR [--seconds S] [--workers W]
//! ```
//!
//! Each run sends `S` seconds of the 64-channel synthetic layout as fast as
//! the ingest takes it and prints one JSON line.
//!
//! - `baseline`: `diagsteno::ingest::Ingest` (one pipeline, plain threads,
//!   unpinned) fed by one sender with all 64 channels.
//! - `per-socket`: two shards. Shard `s` takes half of each sensor kind and
//!   runs on socket `s` only: an ingest core, a store core and `W` feature
//!   cores, connected by `stagelanes-ring` SPSC rings, plus its own sender
//!   core, TCP port and store directory. Each thread allocates what it
//!   uses, so memory stays on its socket (first-touch).
//! - `per-socket-unpinned`: the same two shards with no pinning, to separate
//!   the effect of sharding from the effect of pinning.

#![forbid(unsafe_code)]

use std::io::{BufReader, BufWriter, Write};
use std::net::TcpListener;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Barrier, OnceLock};
use std::time::{Duration, Instant};

use diagsteno::config::{ChannelSpec, Kind, default_layout};
use diagsteno::features::{Windower, reference};
use diagsteno::frame::{Frame, write_ack};
use diagsteno::ingest::{Ingest, IngestConfig};
use diagsteno::sender;
use diagsteno::synth::{Synth, ticks_for_seconds};
use diagsteno_store::{SampleBlock, Store};
use serde_json::json;
use stagelanes_arch::{OsCpu, Topology, pin_current_thread};
use stagelanes_ring::Ring;

type Error = Box<dyn std::error::Error + Send + Sync>;

const RING: usize = 4096;
const BATCH: Duration = Duration::from_millis(100);
const FEATURE_ROWS_PER_PUT: usize = 256;

fn cores_on(topo: &Topology, node: u32) -> Vec<OsCpu> {
    topo.cores
        .iter()
        .filter(|c| c.numa_node == Some(node))
        .map(|c| topo.cpus[c.cpus[0]].os)
        .collect()
}

fn pin(cpu: Option<OsCpu>) -> Result<(), Error> {
    if let Some(c) = cpu {
        pin_current_thread(c)?;
    }
    Ok(())
}

/// Spin-then-yield until `f` succeeds.
fn backoff(spins: &mut u32) {
    *spins += 1;
    if *spins < 64 {
        std::hint::spin_loop();
    } else {
        std::thread::yield_now();
    }
}

/// Shard `s` of 2: half of the accelerometers and half of the strain gauges.
fn shard_layout(s: usize) -> Vec<ChannelSpec> {
    default_layout()
        .into_iter()
        .filter(|c| {
            let within = match c.kind {
                Kind::Accel => c.id,
                Kind::Strain => c.id - 32,
            };
            usize::from(within / 16) == s
        })
        .collect()
}

#[derive(Default)]
struct ShardStats {
    frames: AtomicU64,
    samples: AtomicU64,
    windows: AtomicU64,
    features_done_ns: AtomicU64,
}

/// Run one shard's pipeline on `cores` (ingest, store, workers...), or
/// unpinned if `cores` is `None`. Returns after the sender closes and all
/// features are stored.
fn run_shard(
    listener: TcpListener,
    layout: Vec<ChannelSpec>,
    store_dir: PathBuf,
    cores: Option<Vec<OsCpu>>,
    workers: usize,
    start: Arc<OnceLock<Instant>>,
    stats: Arc<ShardStats>,
) -> Result<(), Error> {
    let core = |i: usize| cores.as_ref().map(|c| c[i]);
    let store = Store::open(&store_dir)?;
    let mut feature_store = store.table_writer();
    let mut store = store;

    // ingest -> store
    let (mut to_store, mut from_ingest) = Ring::<Frame, RING>::new().split_arc();
    let ingest_done = Arc::new(AtomicBool::new(false));
    // store -> worker w
    let mut worker_tx = Vec::new();
    let mut worker_rx = Vec::new();
    for _ in 0..workers {
        let (tx, rx) = Ring::<Frame, RING>::new().split_arc();
        worker_tx.push(tx);
        worker_rx.push(rx);
    }
    let store_done = Arc::new(AtomicBool::new(false));

    let (stream, _) = listener.accept()?;
    stream.set_nodelay(true)?;
    let ack_stream = stream.try_clone()?;

    let ingest = {
        let done = ingest_done.clone();
        let cpu = core(0);
        std::thread::spawn(move || -> Result<(), Error> {
            pin(cpu)?;
            let mut r = BufReader::with_capacity(1 << 16, stream);
            while let Some(mut f) = Frame::read_from(&mut r)? {
                let mut spins = 0;
                loop {
                    match to_store.push(f) {
                        Ok(()) => break,
                        Err(back) => {
                            f = back;
                            backoff(&mut spins);
                        }
                    }
                }
            }
            done.store(true, Ordering::Release);
            Ok(())
        })
    };

    let store_thread = {
        let ingest_done = ingest_done.clone();
        let store_done = store_done.clone();
        let stats = stats.clone();
        let cpu = core(1);
        std::thread::spawn(move || -> Result<(), Error> {
            pin(cpu)?;
            let mut acks = BufWriter::new(ack_stream);
            let mut batch: Vec<Frame> = Vec::new();
            let mut started = Instant::now();
            loop {
                let finished = ingest_done.load(Ordering::Acquire);
                let n = from_ingest.pop_with(1024, |f| batch.push(f));
                let due = started.elapsed() >= BATCH;
                let drained = finished && n == 0;
                if !batch.is_empty() && (due || drained || batch.len() >= 8192) {
                    let blocks: Vec<SampleBlock> = batch
                        .iter()
                        .map(|f| SampleBlock {
                            channel: f.channel,
                            seq: f.seq,
                            first_sample: f.first_sample,
                            sample_rate: f.sample_rate,
                            samples: f.samples.clone(),
                        })
                        .collect();
                    store.put_samples(&blocks)?; // durable before the ack (R8)
                    let mut last = std::collections::BTreeMap::new();
                    for f in &batch {
                        last.insert(f.channel, f.seq);
                    }
                    let entries: Vec<(u16, u64)> = last.into_iter().collect();
                    let _ = write_ack(&mut acks, &entries).and_then(|()| acks.flush());
                    stats
                        .frames
                        .fetch_add(batch.len() as u64, Ordering::Relaxed);
                    stats.samples.fetch_add(
                        batch.iter().map(|f| f.samples.len() as u64).sum(),
                        Ordering::Relaxed,
                    );
                    // Each channel always goes to the same worker, so its
                    // windows are cut in order.
                    for f in batch.drain(..) {
                        let w = usize::from(f.channel) % worker_tx.len();
                        let mut f = f;
                        let mut spins = 0;
                        loop {
                            match worker_tx[w].push(f) {
                                Ok(()) => break,
                                Err(back) => {
                                    f = back;
                                    backoff(&mut spins);
                                }
                            }
                        }
                    }
                    started = Instant::now();
                } else if drained && batch.is_empty() {
                    break;
                } else if n == 0 {
                    std::thread::yield_now();
                }
            }
            store_done.store(true, Ordering::Release);
            Ok(())
        })
    };

    // Workers: each owns a windower for its channels and writes features
    // through the shard's feature table writer, shared under a mutex (puts
    // are rare: one per 256 rows).
    let (row_tx, row_rx) = std::sync::mpsc::channel();
    let mut worker_threads = Vec::new();
    for (w, mut rx) in worker_rx.into_iter().enumerate() {
        let store_done = store_done.clone();
        let layout = layout.clone();
        let row_tx = row_tx.clone();
        let stats = stats.clone();
        let cpu = core(2 + w);
        worker_threads.push(std::thread::spawn(move || -> Result<(), Error> {
            pin(cpu)?;
            let mut windower = Windower::new(&layout);
            let mut rows = Vec::new();
            loop {
                let finished = store_done.load(Ordering::Acquire);
                let mut got = 0;
                let n = rx.pop_with(64, |f| {
                    got += 1;
                    for win in windower.push(&f) {
                        rows.push(reference(&win));
                    }
                });
                if rows.len() >= FEATURE_ROWS_PER_PUT || (finished && n == 0 && !rows.is_empty()) {
                    stats
                        .windows
                        .fetch_add(rows.len() as u64, Ordering::Relaxed);
                    if row_tx.send(std::mem::take(&mut rows)).is_err() {
                        break;
                    }
                }
                if finished && n == 0 && got == 0 {
                    break;
                }
                if n == 0 {
                    std::thread::yield_now();
                }
            }
            Ok(())
        }));
    }
    drop(row_tx);
    // The feature table writer runs on the store core's thread budget: here,
    // on the calling thread, pinned to the shard's store core.
    pin(core(1))?;
    for rows in row_rx {
        feature_store.put_features(&rows)?;
    }
    let t0 = start.get().copied().unwrap_or_else(Instant::now);
    stats
        .features_done_ns
        .store(t0.elapsed().as_nanos() as u64, Ordering::Relaxed);
    ingest.join().map_err(|_| "ingest panicked")??;
    store_thread.join().map_err(|_| "store panicked")??;
    for w in worker_threads {
        w.join().map_err(|_| "worker panicked")??;
    }
    Ok(())
}

fn frames_for(layout: &[ChannelSpec], seconds: u64) -> Vec<Frame> {
    Synth::new(layout, 2026, Some(ticks_for_seconds(seconds))).collect()
}

fn fresh_dir(base: &Path, name: &str) -> Result<PathBuf, Error> {
    let d = base.join(name);
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d)?;
    Ok(d)
}

fn baseline(store_dir: &Path, seconds: u64) -> Result<serde_json::Value, Error> {
    let layout = default_layout();
    let frames = frames_for(&layout, seconds);
    let dir = fresh_dir(store_dir, "baseline")?;
    let listener = TcpListener::bind("127.0.0.1:0")?;
    let addr = listener.local_addr()?;
    let t0 = Instant::now();
    let server = std::thread::spawn(move || -> Result<(diagsteno::ingest::Stats, u128), Error> {
        let mut ing = Ingest::open(IngestConfig::new(&dir, default_layout()))?;
        ing.serve(&listener, Some(1))?;
        let s = ing.close()?;
        Ok((s, t0.elapsed().as_nanos()))
    });
    let r = sender::send(addr, frames, layout.len(), false)?;
    let acked_s = r.elapsed.as_secs_f64();
    let (s, done_ns) = server.join().map_err(|_| "server panicked")??;
    Ok(json!({
        "samples": s.samples, "frames": s.frames, "missing_frames": s.missing_frames,
        "windows": s.feature_rows, "all_acked_s": acked_s,
        "all_features_stored_s": done_ns as f64 / 1e9,
        "error": r.error,
    }))
}

fn per_socket(
    topo: &Topology,
    store_dir: &Path,
    seconds: u64,
    workers: usize,
    pinned: bool,
) -> Result<serde_json::Value, Error> {
    // Senders generate their frames first; the clock starts when both are
    // ready, matching the baseline, whose frames exist before its clock.
    let ready = Arc::new(Barrier::new(3));
    let start: Arc<OnceLock<Instant>> = Arc::new(OnceLock::new());
    let mut shards = Vec::new();
    let mut senders = Vec::new();
    for s in 0..2usize {
        let layout = shard_layout(s);
        let cores = cores_on(topo, s as u32);
        // ingest, store, workers..., and the last core for the sender.
        if cores.len() < 3 + workers {
            return Err(format!("socket {s} needs {} cores", 3 + workers).into());
        }
        let shard_cores = pinned.then(|| cores.clone());
        let sender_core = pinned.then(|| cores[cores.len() - 1]);
        let dir = fresh_dir(store_dir, &format!("shard{s}"))?;
        let listener = TcpListener::bind("127.0.0.1:0")?;
        let addr = listener.local_addr()?;
        let stats = Arc::new(ShardStats::default());
        {
            let (layout, stats, start) = (layout.clone(), stats.clone(), start.clone());
            shards.push((
                stats.clone(),
                std::thread::spawn(move || {
                    run_shard(listener, layout, dir, shard_cores, workers, start, stats)
                }),
            ));
        }
        // The sender generates its frames on its own socket before the clock
        // that matters starts (all_acked is measured from the first send).
        let per_tick = layout.len();
        let ready = ready.clone();
        senders.push(std::thread::spawn(
            move || -> Result<sender::SendReport, Error> {
                pin(sender_core)?;
                let frames = frames_for(&layout, seconds);
                ready.wait(); // both senders have their data; start together
                Ok(sender::send(addr, frames, per_tick, false)?)
            },
        ));
    }
    ready.wait();
    let _ = start.set(Instant::now());
    let mut acked_s: f64 = 0.0;
    let mut errors = Vec::new();
    for h in senders {
        let r = h.join().map_err(|_| "sender panicked")??;
        acked_s = acked_s.max(r.elapsed.as_secs_f64());
        if let Some(e) = r.error {
            errors.push(e);
        }
    }
    let mut out = Vec::new();
    for (stats, h) in shards {
        h.join().map_err(|_| "shard panicked")??;
        out.push(stats);
    }
    let sum = |f: fn(&ShardStats) -> u64| out.iter().map(|s| f(s)).sum::<u64>();
    Ok(json!({
        "samples": sum(|s| s.samples.load(Ordering::Relaxed)),
        "frames": sum(|s| s.frames.load(Ordering::Relaxed)),
        "windows": sum(|s| s.windows.load(Ordering::Relaxed)),
        "all_acked_s": acked_s,
        "all_features_stored_s": out.iter().map(|s| s.features_done_ns.load(Ordering::Relaxed)).max().unwrap_or(0) as f64 / 1e9,
        "errors": errors,
    }))
}

fn main() -> Result<(), Error> {
    let args: Vec<String> = std::env::args().collect();
    let arg = |k: &str| {
        args.iter()
            .position(|a| a == k)
            .and_then(|i| args.get(i + 1))
            .cloned()
    };
    let layout = arg("--layout").ok_or("--layout is required")?;
    let store_dir = PathBuf::from(arg("--store-dir").ok_or("--store-dir is required")?);
    let seconds: u64 = arg("--seconds").map_or(Ok(20), |s| s.parse())?;
    let workers: usize = arg("--workers").map_or(Ok(4), |s| s.parse())?;
    let topo = Topology::detect()?;
    let result = match layout.as_str() {
        "baseline" => baseline(&store_dir, seconds)?,
        "per-socket" => per_socket(&topo, &store_dir, seconds, workers, true)?,
        "per-socket-unpinned" => per_socket(&topo, &store_dir, seconds, workers, false)?,
        _ => return Err("unknown --layout".into()),
    };
    let samples = result["samples"].as_f64().unwrap_or(0.0);
    let acked = result["all_acked_s"].as_f64().unwrap_or(f64::NAN);
    println!(
        "{}",
        json!({"layout": layout, "store_dir": store_dir, "seconds_of_data": seconds,
               "workers_per_shard": workers, "result": result,
               "acked_samples_per_s": (samples / acked).round()})
    );
    Ok(())
}
