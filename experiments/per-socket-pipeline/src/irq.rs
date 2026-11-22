//! Per-CPU interrupt counts over a run, on Linux and Windows.
//!
//! - **Linux:** `/proc/interrupts` before and after; per CPU, the total over
//!   all sources and the local timer (`LOC`) alone.
//! - **Windows** (no `/proc/interrupts`): `typeperf` samples the Processor
//!   counters once a second during the run: `Interrupts/sec` (hardware
//!   interrupts, including the clock), `DPCs Queued/sec`, `% DPC Time` and
//!   `% Interrupt Time`. Windows has no separate local-timer count; the clock
//!   interrupt is inside `Interrupts/sec`. Rates are means over the samples
//!   taken during the run, so runs shorter than a few seconds are coarse.
//!
//! Chosen at run time by what the OS offers, not by `cfg`, so the Linux path
//! is exactly the one the Dell's step-1 run used.

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};

use serde_json::{Value, json};

use crate::Error;

/// Running measurement.
pub enum Sampler {
    Linux { total: Vec<u64>, loc: Vec<u64> },
    Windows { child: Child, csv: PathBuf },
}

/// Per-CPU totals from `/proc/interrupts` (all sources, and `LOC`).
fn proc_interrupts() -> Result<(Vec<u64>, Vec<u64>), Error> {
    let text = std::fs::read_to_string("/proc/interrupts")?;
    let mut lines = text.lines();
    let ncpu = lines
        .next()
        .ok_or("empty /proc/interrupts")?
        .split_whitespace()
        .count();
    let mut total = vec![0u64; ncpu];
    let mut loc = vec![0u64; ncpu];
    for line in lines {
        let mut parts = line.split_whitespace();
        let Some(label) = parts.next() else { continue };
        for (i, v) in parts.take(ncpu).enumerate() {
            let Ok(n) = v.parse::<u64>() else { break };
            total[i] += n;
            if label == "LOC:" {
                loc[i] += n;
            }
        }
    }
    Ok((total, loc))
}

const COUNTERS: [&str; 4] = [
    r"\Processor(*)\Interrupts/sec",
    r"\Processor(*)\DPCs Queued/sec",
    r"\Processor(*)\% DPC Time",
    r"\Processor(*)\% Interrupt Time",
];

impl Sampler {
    /// Starts measuring.
    pub fn start(scratch: &std::path::Path) -> Result<Self, Error> {
        if std::path::Path::new("/proc/interrupts").exists() {
            let (total, loc) = proc_interrupts()?;
            return Ok(Sampler::Linux { total, loc });
        }
        let csv = scratch.join("interrupts.csv");
        let child = Command::new("typeperf")
            .args(COUNTERS)
            .args(["-si", "1", "-f", "CSV", "-y", "-o"])
            .arg(&csv)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|e| format!("no /proc/interrupts and typeperf failed: {e}"))?;
        // typeperf writes its first sample after one interval; give it time
        // to start so the run is covered.
        std::thread::sleep(std::time::Duration::from_millis(1500));
        Ok(Sampler::Windows { child, csv })
    }

    /// Stops measuring. Returns the JSON fields for the run file, given the
    /// pipeline CPUs (OS numbers) and the elapsed run time.
    pub fn stop(self, pipeline: &[u32], elapsed: f64) -> Result<Value, Error> {
        match self {
            Sampler::Linux { total, loc } => {
                let (total1, loc1) = proc_interrupts()?;
                let per_cpu = |a: &[u64], b: &[u64]| -> BTreeMap<String, u64> {
                    a.iter()
                        .zip(b)
                        .enumerate()
                        .map(|(i, (x, y))| (format!("cpu{i:02}"), y.saturating_sub(*x)))
                        .collect()
                };
                let irq = per_cpu(&total, &total1);
                let lt = per_cpu(&loc, &loc1);
                let others: Vec<u32> = (0..total.len() as u32)
                    .filter(|c| !pipeline.contains(c))
                    .collect();
                let rate = |m: &BTreeMap<String, u64>, cpus: &[u32]| -> f64 {
                    let n: u64 = cpus
                        .iter()
                        .map(|c| m.get(&format!("cpu{c:02}")).copied().unwrap_or(0))
                        .sum();
                    (n as f64 / cpus.len().max(1) as f64 / elapsed * 10.0).round() / 10.0
                };
                Ok(json!({
                    "source": "/proc/interrupts",
                    "interrupts_per_cpu_per_s": {
                        "pipeline_cores_all": rate(&irq, pipeline),
                        "pipeline_cores_local_timer": rate(&lt, pipeline),
                        "other_cores_all": rate(&irq, &others),
                        "other_cores_local_timer": rate(&lt, &others)},
                    "interrupts_per_cpu_total": irq,
                    "local_timer_per_cpu_total": lt,
                }))
            }
            Sampler::Windows { mut child, csv } => {
                // One more interval so the last second of the run is sampled.
                std::thread::sleep(std::time::Duration::from_millis(1100));
                let _ = child.kill();
                let _ = child.wait();
                windows_summary(&std::fs::read_to_string(&csv)?, pipeline)
            }
        }
    }
}

/// Means per CPU and per counter from typeperf's CSV, and the pipeline /
/// other split.
fn windows_summary(text: &str, pipeline: &[u32]) -> Result<Value, Error> {
    let mut rows = text.lines().filter(|l| !l.trim().is_empty());
    let header = split_csv(rows.next().ok_or("empty typeperf output")?);
    // Column -> (counter, cpu), for numbered Processor instances only.
    let cols: Vec<Option<(usize, u32)>> = header
        .iter()
        .map(|h| {
            let open = h.find("Processor(")? + "Processor(".len();
            let close = open + h[open..].find(')')?;
            let cpu: u32 = h[open..close].parse().ok()?;
            let counter = COUNTERS
                .iter()
                .position(|c| h.ends_with(c.rsplit('\\').next().unwrap_or("")))?;
            Some((counter, cpu))
        })
        .collect();
    let mut sums: BTreeMap<(usize, u32), (f64, u32)> = BTreeMap::new();
    let mut samples = 0u32;
    for line in rows {
        let vals = split_csv(line);
        let mut any = false;
        for (v, col) in vals.iter().zip(&cols) {
            let (Some(key), Ok(x)) = (col, v.trim().parse::<f64>()) else { continue };
            let e = sums.entry(*key).or_insert((0.0, 0));
            e.0 += x;
            e.1 += 1;
            any = true;
        }
        samples += u32::from(any);
    }
    let mean = |counter: usize| -> BTreeMap<String, f64> {
        sums.iter()
            .filter(|((c, _), _)| *c == counter)
            .map(|((_, cpu), (s, n))| (format!("cpu{cpu:02}"), (s / f64::from(*n) * 10.0).round() / 10.0))
            .collect()
    };
    let group = |m: &BTreeMap<String, f64>, pipe: bool| -> f64 {
        let v: Vec<f64> = m
            .iter()
            .filter(|(k, _)| {
                let cpu: u32 = k[3..].parse().unwrap_or(u32::MAX);
                pipeline.contains(&cpu) == pipe
            })
            .map(|(_, x)| *x)
            .collect();
        (v.iter().sum::<f64>() / v.len().max(1) as f64 * 10.0).round() / 10.0
    };
    let (irq, dpc, dpc_pct, int_pct) = (mean(0), mean(1), mean(2), mean(3));
    Ok(json!({
        "source": "typeperf Processor counters, sampled every 1 s during the run",
        "samples": samples,
        "interrupts_per_cpu_per_s": {
            "pipeline_cores_all": group(&irq, true),
            "pipeline_cores_local_timer": null,
            "other_cores_all": group(&irq, false),
            "other_cores_local_timer": null},
        "dpcs_queued_per_cpu_per_s": {"pipeline_cores": group(&dpc, true), "other_cores": group(&dpc, false)},
        "dpc_time_pct": {"pipeline_cores": group(&dpc_pct, true), "other_cores": group(&dpc_pct, false)},
        "interrupt_time_pct": {"pipeline_cores": group(&int_pct, true), "other_cores": group(&int_pct, false)},
        "interrupts_per_s_by_cpu": irq,
        "dpcs_queued_per_s_by_cpu": dpc,
        "dpc_time_pct_by_cpu": dpc_pct,
        "interrupt_time_pct_by_cpu": int_pct,
    }))
}

/// Splits one line of typeperf's CSV (every field quoted, no embedded quotes).
fn split_csv(line: &str) -> Vec<String> {
    line.split("\",\"")
        .map(|f| f.trim_matches(|c| c == '"' || c == '\r').to_owned())
        .collect()
}
