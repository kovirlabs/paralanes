//! Item 5 of `docs/cpu-throughput-strategy.md` §11: durable batch writes
//! (the store's write + flush) as one system call per chunk plus a flush,
//! against one submission for the whole batch ("channel program" form).
//!
//! ```text
//! durable-writes <dir> [batches]
//! ```
//!
//! For each batch shape (K chunks of 64 KiB), writes `batches` batches back
//! to back into one file in `dir`, each followed by a flush to the device,
//! with:
//! - `std`: seek, one `write_all` per chunk, then `sync_data`;
//! - `durable-writer`: `stagelanes_arch::DurableWriter` (IoRing on Windows
//!   11: every write and the flush in one submission).
//!
//! Prints one JSON object: per method and shape, per-batch latency
//! percentiles and CPU time per batch (calling thread and whole process).
//! The file's contents are checked after each run.

use std::io::{Seek, SeekFrom, Write};
use std::time::Instant;

use serde_json::{Value, json};
use stagelanes_arch::DurableWriter;

const CHUNK: usize = 64 << 10;

fn pct(v: &mut [f64], p: f64) -> f64 {
    v.sort_by(f64::total_cmp);
    v[((p / 100.0) * (v.len() - 1) as f64).round() as usize]
}

fn run(dir: &std::path::Path, method: &str, k: usize, batches: usize) -> Result<Value, Box<dyn std::error::Error>> {
    let chunks: Vec<Vec<u8>> = (0..k)
        .map(|i| (0..CHUNK).map(|j| (j as u8) ^ (i as u8) ^ 0x5a).collect())
        .collect();
    let refs: Vec<&[u8]> = chunks.iter().map(Vec::as_slice).collect();
    let path = dir.join(format!("{method}-{k}.bin"));
    let file = std::fs::OpenOptions::new()
        .create(true)
        .truncate(true)
        .read(true)
        .write(true)
        .open(&path)?;
    let mut writer = DurableWriter::new()?;
    let used = if method == "std" { "std write+sync_data" } else { writer.method() };
    let batch_bytes = (k * CHUNK) as u64;
    let mut lat_ms = Vec::with_capacity(batches);
    let p0 = cpu_time::ProcessTime::now();
    let t0 = cpu_time::ThreadTime::now();
    for b in 0..batches {
        let off = b as u64 * batch_bytes;
        let s = Instant::now();
        if method == "std" {
            let mut f = &file;
            f.seek(SeekFrom::Start(off))?;
            for c in &refs {
                f.write_all(c)?;
            }
            file.sync_data()?;
        } else {
            writer.write(&file, off, &refs)?;
        }
        lat_ms.push(s.elapsed().as_secs_f64() * 1e3);
    }
    let thread_cpu = t0.elapsed().as_secs_f64();
    let process_cpu = p0.elapsed().as_secs_f64();
    drop(file);
    // Every batch must be on disk in order.
    let got = std::fs::read(&path)?;
    let ok = got.len() as u64 == batch_bytes * batches as u64
        && got
            .chunks(k * CHUNK)
            .all(|b| b.chunks(CHUNK).zip(&chunks).all(|(x, y)| x == y.as_slice()));
    std::fs::remove_file(&path)?;
    let n = batches as f64;
    Ok(json!({
        "method": method, "implementation": used, "chunks_per_batch": k,
        "batch_bytes": batch_bytes, "batches": batches, "contents_verified": ok,
        "latency_ms": {"p50": pct(&mut lat_ms.clone(), 50.0), "p90": pct(&mut lat_ms.clone(), 90.0),
                       "p99": pct(&mut lat_ms.clone(), 99.0), "max": pct(&mut lat_ms, 100.0)},
        "thread_cpu_us_per_batch": thread_cpu / n * 1e6,
        "process_cpu_us_per_batch": process_cpu / n * 1e6,
    }))
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args().skip(1);
    let dir = std::path::PathBuf::from(args.next().ok_or("usage: durable-writes <dir> [batches]")?);
    let batches: usize = args.next().map_or(Ok(100), |s| s.parse())?;
    std::fs::create_dir_all(&dir)?;
    let mut results = Vec::new();
    for k in [1usize, 8, 32, 128] {
        // Alternate the methods so drift affects both alike.
        for method in ["std", "durable-writer", "std", "durable-writer"] {
            results.push(run(&dir, method, k, batches)?);
        }
    }
    println!("{}", json!({"chunk_bytes": CHUNK, "results": results}));
    Ok(())
}
