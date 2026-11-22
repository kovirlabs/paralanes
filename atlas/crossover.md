# Crossover sizes

The input size per op per machine above which the GPU beats the CPU reference.
Every entry links to the run files that produced it.

Two crossovers are recorded:

- **Latency.** The smallest measured size from which the GPU's median
  latency is below the CPU reference's at every larger measured size.
- **CPU time.** The same comparison for CPU time spent on the work. It shows
  where offloading frees the CPU even when it is not faster.

The sizes are 64 B × 4ⁿ up to 256 MiB. "None" means the GPU does not win from
any measured size upward. Method: [`README.md`](README.md).

## Ryzen 7 5700X + Radeon RX 7800 XT (PCIe 3.0 x16), 2026-11-22

- Build: bytelanes `1bd1232` (branch `m5-atlas`).
  - CPU reference without per-op copies.
  - GPU backend with pooled device buffers and program overlap.
- Windows 10.0.26300.10075, rustc 1.99.1, wgpu 30.0.2.
- Vulkan: AMD proprietary driver 26.11.2 (LLPC). DX12: driver
  32.0.32029.2010.
- One program in flight; CPU reference on one core.

| Op | Machine | wgpu backend | Crossover size (latency) | Crossover size (CPU time) | Run files |
| --- | --- | --- | --- | --- | --- |
| Checksum (CRC-32C) | ryzen | Vulkan | none ≤ 256 MiB | none ≤ 256 MiB | [checksum-2](runs/ryzen/2026-11-22-checksum-2.json) |
| Checksum (CRC-32C) | ryzen | DX12 | none ≤ 256 MiB | none ≤ 256 MiB | [checksum-2](runs/ryzen/2026-11-22-checksum-2.json) |
| Encrypt (AES-256-CTR) | ryzen | Vulkan | none ≤ 256 MiB | 16 MiB | [encrypt-2](runs/ryzen/2026-11-22-encrypt-2.json) |
| Encrypt (AES-256-CTR) | ryzen | DX12 | none ≤ 256 MiB | 16 MiB | [encrypt-2](runs/ryzen/2026-11-22-encrypt-2.json) |
| Decrypt (AES-256-CTR) | ryzen | Vulkan | none ≤ 256 MiB | 16 MiB | [decrypt-2](runs/ryzen/2026-11-22-decrypt-2.json) |
| Decrypt (AES-256-CTR) | ryzen | DX12 | none ≤ 256 MiB | 16 MiB | [decrypt-2](runs/ryzen/2026-11-22-decrypt-2.json) |
| Filter (u32, about 1% match) | ryzen | Vulkan | none ≤ 256 MiB | 256 MiB (marginal: 23.4 against 23.6 ms, below CPU-time resolution) | [filter-2](runs/ryzen/2026-11-22-filter-2.json) |
| Filter (u32, about 1% match) | ryzen | DX12 | none ≤ 256 MiB | 64 MiB | [filter-2](runs/ryzen/2026-11-22-filter-2.json) |

**On this machine the GPU never beats the CPU reference on latency for a
single op.** A Ryzen core has hardware CRC-32C and AES, and the GPU sits on
PCIe 3.0. The plan's risk table anticipated this result. Offloading saves
CPU time only for AES and filter on large inputs.

Median latency, then CPU time per call, from the run files:

| Op | Size | CPU reference | Vulkan | DX12 |
| --- | --- | --- | --- | --- |
| Checksum | 16 MiB | 0.486 ms; CPU 0.488 ms | 2.72 ms; CPU 2.00 ms | 2.84 ms; CPU 1.15 ms |
| Checksum | 256 MiB | 9.74 ms; CPU 9.71 ms | 31.4 ms; CPU 31.3 ms | 36.8 ms; CPU 13.4 ms |
| Encrypt | 16 MiB | 2.84 ms; CPU 2.84 ms | 5.00 ms; CPU 2.50 ms | 5.08 ms; CPU 1.92 ms |
| Encrypt | 256 MiB | 48.0 ms; CPU 48.4 ms | 68.0 ms; CPU 36.5 ms | 74.9 ms; CPU 25.7 ms |
| Filter | 64 MiB | 5.70 ms; CPU 5.68 ms | 7.59 ms; CPU 6.16 ms | 9.34 ms; CPU 3.07 ms |
| Filter | 256 MiB | 23.7 ms; CPU 23.6 ms | 29.6 ms; CPU 23.4 ms | 35.2 ms; CPU 12.4 ms |

Each GPU call has a fixed cost of about 0.26 to 0.4 ms (64 B: Vulkan
0.272 to 0.400 ms, DX12 0.257 to 0.361 ms).

CPU-time resolution: Windows counts CPU time in scheduler ticks (about
15.6 ms). Large sizes have few repetitions (about 10 to 40), so their
per-call CPU times carry roughly ±0.5 ms of error.

### Commit-log workload, one batch in flight (latency)

Batches of generated 256-byte payment records; the programs are described in
the run file. Run file: [commitlog-2](runs/ryzen/2026-11-22-commitlog-2.json).

| Workload | wgpu backend | Crossover (latency) | Crossover (CPU time) |
| --- | --- | --- | --- |
| Commit (fraud filter, plus encrypting the records; both results to the host) | Vulkan | none ≤ 1,048,576 transactions | 262,144 transactions |
| Commit | DX12 | none ≤ 1,048,576 | 65,536 |
| Audit (decrypt the amount column on the device, chained into the filter; only indices return) | Vulkan | 262,144 transactions | 262,144 |
| Audit | DX12 | 262,144 | 262,144 |

| Workload | Batch | CPU reference | Vulkan | DX12 |
| --- | --- | --- | --- | --- |
| Commit | 262,144 | 12.2 ms; CPU 12.2 ms | 18.7 ms; CPU 10.1 ms | 20.5 ms; CPU 8.29 ms |
| Commit | 1,048,576 | 48.3 ms; CPU 49.1 ms | 69.1 ms; CPU 38.5 ms | 82.8 ms; CPU 32.5 ms |
| Audit | 262,144 | 1.19 ms; CPU 1.19 ms | 0.931 ms; CPU 0.613 ms | 0.969 ms; CPU 0.531 ms |
| Audit | 1,048,576 | 4.63 ms; CPU 4.58 ms | 1.81 ms; CPU 1.19 ms | 2.00 ms; CPU 0.499 ms |

### Commit-log workload, 16 batches in flight (throughput)

Run file: [stream](runs/ryzen/2026-11-22-stream.json).

- **CPU** columns use the CPU reference on 1, 8 or 16 threads, one backend
  per thread, with batches spread round-robin.
- **GPU** columns show program overlap off ("serial") and on.
- Each cell shows transactions per second at the median repetition, then
  CPU time per transaction across all threads.

| Workload | Batch | CPU 1 thread | CPU 8 threads | CPU 16 threads | Vulkan serial | Vulkan overlap | DX12 serial | DX12 overlap |
| --- | --- | --- | --- | --- | --- | --- | --- | --- |
| Commit | 4,096 | 21.4 M/s; 46 ns | 82.9 M/s; 58 ns | 64.1 M/s; 68 ns | 3.28 M/s; 157 ns | 4.08 M/s; 162 ns | 3.10 M/s; 179 ns | 3.81 M/s; 178 ns |
| Commit | 65,536 | 21.5 M/s; 47 ns | 37.3 M/s; 209 ns | 36.2 M/s; 356 ns | 11.6 M/s; 47 ns | 12.2 M/s; 51 ns | 11.2 M/s; 35 ns | 11.9 M/s; 36 ns |
| Commit | 262,144 | 21.4 M/s; 47 ns | 37.9 M/s; 205 ns | 36.4 M/s; 393 ns | 13.6 M/s; 39 ns | 14.1 M/s; 39 ns | 12.5 M/s; 33 ns | 13.3 M/s; 31 ns |
| Audit | 4,096 | 234 M/s; 4 ns | 135 M/s; 16 ns | 72.9 M/s; 29 ns | 7.50 M/s; 87 ns | 8.64 M/s; 71 ns | 8.15 M/s; 88 ns | 10.2 M/s; 79 ns |
| Audit | 65,536 | 209 M/s; 5 ns | 1,507 M/s; 3 ns | 1,087 M/s; 4 ns | 84.6 M/s; 7 ns | 95.9 M/s; 6 ns | 90.4 M/s; 7 ns | 108 M/s; 7 ns |
| Audit | 262,144 | 211 M/s; 5 ns | 500 M/s; 15 ns | 509 M/s; 23 ns | 280 M/s; 2 ns | 344 M/s; 2 ns | 269 M/s; 2 ns | 347 M/s; 2 ns |

What the throughput run shows:

- **Overlap gain** (overlap on against serial):
  - Commit: +24% (Vulkan) and +23% (DX12) at 4,096; +4% and +6% at
    262,144.
  - Audit: +15% and +25% at 4,096; +23% and +29% at 262,144.
  - The gain is largest where per-program fixed costs, or PCIe transfers,
    can overlap with GPU work.
- **All cores beat the GPU on throughput** for both workloads at every
  batch size:
  - Commit at 262,144: CPU on 8 threads 37.9 M txn/s, against 14.1 M txn/s
    for the best GPU.
  - Audit at 262,144: CPU on 16 threads 509 M txn/s, against 347 M txn/s.
- **The CPU stops scaling past 8 threads.** On large commit batches, 8
  threads give 1.8× one thread, and CPU time per transaction rises from
  47 to 205 ns. The threads are waiting on memory bandwidth (AES reads and
  writes every byte, and the CRC reads it again). Small batches, which fit
  in the 32 MB L3, scale better: 3.9× at 4,096.
- **What the GPU offers here is CPU time, not speed.** On large batches it
  needs 31 to 39 ns of CPU per commit transaction, against 47 ns for one
  CPU thread and 205 ns or more once the CPU is bandwidth-bound. For audit
  it needs 2 ns, against 5 to 23 ns.

## Superseded runs (kept as the record)

These earlier 2026-11-22 runs are not valid comparisons. Their CPU
reference copied every input and output through freshly allocated buffers,
which made it about 3.5× slower at AES (probe: 1.88 against 6.55 GB/s for
the bare kernel), and slower still at checksum. They do show the GPU tuning
steps.

- **GPU tuned, before buffer pooling** (`cc13614`):
  [checksum](runs/ryzen/2026-11-22-checksum.json),
  [encrypt](runs/ryzen/2026-11-22-encrypt.json),
  [decrypt](runs/ryzen/2026-11-22-decrypt.json),
  [filter](runs/ryzen/2026-11-22-filter.json),
  [commitlog](runs/ryzen/2026-11-22-commitlog.json).
- **GPU backend as merged in M3** (`fb80a1a`):
  [checksum](runs/ryzen/2026-11-22-checksum-untuned.json),
  [encrypt](runs/ryzen/2026-11-22-encrypt-untuned.json),
  [decrypt](runs/ryzen/2026-11-22-decrypt-untuned.json),
  [filter](runs/ryzen/2026-11-22-filter-untuned.json),
  [commitlog](runs/ryzen/2026-11-22-commitlog-untuned.json).

## MacBook Pro (M5 Pro), unified memory

Not measured yet: agent C, after M4.
