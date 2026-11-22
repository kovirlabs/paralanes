# Crossover sizes

The input size per op per machine above which the GPU beats the CPU reference.
Every entry links to the run files that produced it.

Two crossovers are recorded:

- **Latency.** The smallest measured size from which the GPU's median
  latency is below the CPU reference's at every larger measured size.
- **CPU time.** The same comparison for CPU time spent on the work. It shows
  where offloading frees the CPU, which can happen at a smaller size than
  the latency crossover.

The sizes are 64 B × 4ⁿ up to 256 MiB. "None" means the GPU does not win from
any measured size upward. Method: [`README.md`](README.md).

## Ryzen 7 5700X + Radeon RX 7800 XT (PCIe 3.0 x16), 2026-11-22

- Tuned backend: bytelanes `cc13614` (branch `m5-atlas`).
- Windows 10.0.26300.10075, rustc 1.99.1, wgpu 30.0.2.
- Vulkan: AMD proprietary driver 26.11.2 (LLPC). DX12: driver
  32.0.32029.2010.

| Op | Machine | wgpu backend | Crossover size (latency) | Crossover size (CPU time) | Run files |
| --- | --- | --- | --- | --- | --- |
| Checksum (CRC-32C) | ryzen | Vulkan | 16 MiB | 4 MiB | [checksum](runs/ryzen/2026-11-22-checksum.json) |
| Checksum (CRC-32C) | ryzen | DX12 | 4 MiB (marginal: 0.853 ms against 0.861 ms) | 4 MiB | [checksum](runs/ryzen/2026-11-22-checksum.json) |
| Encrypt (AES-256-CTR) | ryzen | Vulkan | 4 MiB | 1 MiB | [encrypt](runs/ryzen/2026-11-22-encrypt.json) |
| Encrypt (AES-256-CTR) | ryzen | DX12 | 4 MiB | 1 MiB | [encrypt](runs/ryzen/2026-11-22-encrypt.json) |
| Decrypt (AES-256-CTR) | ryzen | Vulkan | 4 MiB | 1 MiB | [decrypt](runs/ryzen/2026-11-22-decrypt.json) |
| Decrypt (AES-256-CTR) | ryzen | DX12 | 4 MiB | 4 MiB | [decrypt](runs/ryzen/2026-11-22-decrypt.json) |
| Filter (u32, about 1% match) | ryzen | Vulkan | 256 MiB (also wins at 16 MiB; loses at 64 MiB) | 4 MiB | [filter](runs/ryzen/2026-11-22-filter.json) |
| Filter (u32, about 1% match) | ryzen | DX12 | 256 MiB (also wins at 16 MiB; loses at 64 MiB) | 4 MiB | [filter](runs/ryzen/2026-11-22-filter.json) |

Median latency and CPU time per call, from the run files:

| Op | Size | CPU reference | Vulkan | DX12 |
| --- | --- | --- | --- | --- |
| Checksum | 1 MiB | 0.229 ms, CPU 0.232 ms | 0.725 ms, CPU 0.397 ms | 0.739 ms, CPU 0.441 ms |
| Checksum | 16 MiB | 3.41 ms, CPU 3.43 ms | 2.71 ms, CPU 1.78 ms | 2.82 ms, CPU 1.40 ms |
| Checksum | 256 MiB | 56.7 ms, CPU 57.3 ms | 40.9 ms, CPU 24.4 ms | 48.5 ms, CPU 14.9 ms |
| Encrypt | 1 MiB | 0.567 ms, CPU 0.572 ms | 0.787 ms, CPU 0.521 ms | 0.881 ms, CPU 0.454 ms |
| Encrypt | 16 MiB | 8.81 ms, CPU 8.91 ms | 4.98 ms, CPU 2.41 ms | 5.31 ms, CPU 2.45 ms |
| Encrypt | 256 MiB | 144.3 ms, CPU 145.1 ms | 87.3 ms, CPU 41.7 ms | 92.6 ms, CPU 27.0 ms |
| Filter | 16 MiB | 4.09 ms, CPU 4.16 ms | 2.46 ms, CPU 1.78 ms | 3.02 ms, CPU 1.29 ms |
| Filter | 64 MiB | 16.8 ms, CPU 16.7 ms | 17.6 ms, CPU 7.40 ms | 17.9 ms, CPU 5.02 ms |
| Filter | 256 MiB | 68.3 ms, CPU 68.8 ms | 48.5 ms, CPU 29.0 ms | 53.6 ms, CPU 18.1 ms |

Each GPU call has a fixed cost of about 0.25 to 0.4 ms (64 B: Vulkan
0.256 to 0.389 ms, DX12 0.246 to 0.343 ms). At 256 KiB and below the CPU
reference finishes every op in less time than that fixed cost.

### Commit-log workload (transactions per batch)

Batches of generated 256-byte payment records. The method and the program
shapes are in the run file.

- **Commit** (write path) runs two programs per batch: a fraud-rule filter
  on the amount column, and encryption of the records for the log. Both
  results come back to the host.
- **Audit** (read path) is one chained program: decrypt the encrypted
  amount column into device memory, filter it there, and return only the
  flagged row indices.

| Workload | wgpu backend | Crossover (latency) | Crossover (CPU time) | Run file |
| --- | --- | --- | --- | --- |
| Commit | Vulkan | 16,384 transactions (4 MiB of records) | 16,384 | [commitlog](runs/ryzen/2026-11-22-commitlog.json) |
| Commit | DX12 | 16,384 transactions | 16,384 | [commitlog](runs/ryzen/2026-11-22-commitlog.json) |
| Audit | Vulkan | 262,144 transactions (2 MiB column) | 262,144 | [commitlog](runs/ryzen/2026-11-22-commitlog.json) |
| Audit | DX12 | 262,144 transactions | 262,144 | [commitlog](runs/ryzen/2026-11-22-commitlog.json) |

| Workload | Batch | CPU reference | Vulkan | DX12 |
| --- | --- | --- | --- | --- |
| Commit | 4,096 | 0.577 ms; 7.1 M txn/s; 142 ns CPU/txn | 1.23 ms; 3.3 M txn/s; 151 ns CPU/txn | 1.29 ms; 3.2 M txn/s; 182 ns CPU/txn |
| Commit | 65,536 | 8.86 ms; 7.4 M txn/s; 133 ns CPU/txn | 5.64 ms; 11.6 M txn/s; 47 ns CPU/txn | 5.99 ms; 10.9 M txn/s; 37 ns CPU/txn |
| Commit | 1,048,576 | 145.8 ms; 7.2 M txn/s; 138 ns CPU/txn | 88.3 ms; 11.9 M txn/s; 42 ns CPU/txn | 99.8 ms; 10.5 M txn/s; 31 ns CPU/txn |
| Audit | 65,536 | 0.250 ms; 3.9 ns CPU/txn | 0.758 ms; 5.1 ns CPU/txn | 0.729 ms; 7.3 ns CPU/txn |
| Audit | 1,048,576 | 4.74 ms; 4.5 ns CPU/txn | 1.82 ms; 1.0 ns CPU/txn | 1.99 ms; 0.8 ns CPU/txn |

**CPU reference** means the bytelanes CPU backend on one core, using the
hardware CRC-32C and AES instructions. A server spreading the work over
several cores would raise the CPU's throughput, but not its CPU time per
transaction.

### Before tuning

The same runs on the backend as merged in M3, before the M5 tuning:

- [checksum](runs/ryzen/2026-11-22-checksum-untuned.json),
  [encrypt](runs/ryzen/2026-11-22-encrypt-untuned.json),
  [decrypt](runs/ryzen/2026-11-22-decrypt-untuned.json),
  [filter](runs/ryzen/2026-11-22-filter-untuned.json),
  [commitlog](runs/ryzen/2026-11-22-commitlog-untuned.json).
- The untuned backend had no latency crossover for any single op or for
  the commit path.
- Its CPU-time crossovers were 4 MiB for every op and 16,384 transactions
  for the commit path. The audit path crossed at 262,144 transactions, as
  it does after tuning.

## MacBook Pro (M5 Pro), unified memory

Not measured yet: agent C, after M4.
