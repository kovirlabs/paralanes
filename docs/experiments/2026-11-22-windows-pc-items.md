# The Windows PC work list (strategy §11, items 1 to 9)

Agent: B   Machine: Ryzen 7 5700X, Radeon RX 7800 XT (PCIe 3.0 x16), WD_BLACK SN770 NVMe, Windows 11   Date: 2026-11-22

Status of every item agent A listed for this machine in
`docs/cpu-throughput-strategy.md` §11. Every figure comes from the run file
named next to it (`atlas/runs/ryzen/`); means over repeats with
[min..max].

| # | Item | Status |
| --- | --- | --- |
| 1 | GPU stage in the pipeline | Measured; GPU rows identical to the reference |
| 2 | §7 transfer rules | Done in bytelanes (M3, M5); measured there |
| 3 | More GPU batch work (decimation, RMS/peak, compression) | Designed, not built: needs a contract change (below) |
| 4 | Durable-write latency on the NVMe | Measured |
| 5 | IoRing batched store writes | See the item 5 section |
| 6 | DirectStorage feasibility | Researched (below) |
| 7 | Windows isolation (step 2) | 2a, 2b and 2c measured: none narrows the compute tail. See `2026-11-22-windows-isolation.md` |
| 8 | `stagelanes-pipe` (M6) | Not started: a milestone of its own |
| 9 | AMD L3 QoS on the 5700X | Checked: not usable |

## 1. The GPU as the feature stage

Run files: `2026-11-22-gpu-feature-stage.json` (unpaced: 60 s of the
64-channel layout as fast as possible, 3 runs per configuration) and
`2026-11-22-gpu-feature-stage-realtime.json`. Instrument: agent A's
step-metrics pipeline, with its feature workers replaced by one thread
that batches B windows per `diagsteno::features::Engine::compute` call on
the RX 7800 XT.

| Feature stage | Time for 60 s of data | Process CPU | Windows/s |
| --- | --- | --- | --- |
| CPU reference, 4 worker cores | 1.735 s [1.715..1.771] | 10.28 s | 7,986 |
| CPU reference, 1 worker core | 2.221 s [2.198..2.256] | 6.58 s | 6,239 |
| GPU, 1 thread, B = 1 | 7.531 s [7.500..7.584] | 26.96 s | 1,840 |
| GPU, B = 4 | 2.788 s | 9.62 s | 4,972 |
| GPU, B = 16 | 2.025 s | 6.48 s | 6,842 |
| GPU, B = 64 | 1.886 s | 5.84 s | 7,348 |
| GPU, B = 256 | 1.779 s | 5.37 s | 7,790 |
| GPU, B = 1,024 | 1.771 s [1.734..1.801] | 5.33 s | 7,824 |

- **Correctness:** 0 of 41,568 GPU feature rows (13,856 windows × 3 runs
  per configuration) differ from `features::reference`.
- **The in-pipeline crossover is a batch size, not an input size.**
  - One window per GPU call is 4× slower than the CPU and costs 2.6× the
    process CPU.
  - From B = 16, one GPU thread uses less process CPU than the CPU
    reference on 1 or 4 cores.
  - From B = 256, it matches 4 CPU cores' throughput with about half the
    process CPU, and the pipeline needs 1 feature core instead of 4.
- **The remaining GPU cost is waiting.** `Engine::compute` submits each
  program, then spins on `poll` until it completes. At B = 1,024 the GPU
  thread spends 0.44 s inside `compute`.
  - Scheduler rule 3 (SPEC) says a stage core should never wait like this.
  - **Suggestion for agent A:** give the engine a submit/collect form, so
    the feature thread keeps windowing while the GPU works.
- **Process CPU** counts every thread of the run: sender, ingest, store
  and feature stage. The 4 idle CPU workers spin and yield between frames,
  which is why 4 workers cost more CPU than 1.
- **Note on the run file:** it records stagelanes as having uncommitted
  changes. The benchmark binary was built at stagelanes `1b297f3` before
  those edits (the `DurableWriter` work below), which it does not use.

Real time (60 s at the real rate, 3 runs each; run file
`2026-11-22-gpu-feature-stage-realtime.json`):

| Feature stage | Process CPU over the 60 s | Feature row latency p50 | p99 | max | Sender late ticks (of 18,000) |
| --- | --- | --- | --- | --- | --- |
| CPU reference, 4 workers | 300.0 s | 55.4 ms | 106.1 ms | 108.2 ms | 25 |
| GPU, B = 16 | 120.9 s | 57.8 ms | 107.9 ms | 110.7 ms | 14 |
| GPU, B = 64 | 120.7 s | 67.2 ms | 118.3 ms | 120.2 ms | 2 |

- Rows differing from the reference: 0 of 41,568 for each GPU
  configuration.
- At the real rate, pipeline threads spin (with yields) while waiting for
  work, so process CPU mostly counts busy threads: about 5 cores' worth
  with 4 CPU workers, about 2 with the GPU stage.
- **The GPU stage frees 3 cores** for 2.4 ms more median latency at
  B = 16. At B = 64 it costs about 10 ms more, because batches take
  longer to fill at 231 windows/s.
- Latency includes the store's 100 ms batching, which is why the median
  sits near 55 ms either way.
- Pipeline cores took 680 to 696 interrupts/s in every configuration.

## 2. Transfer rules (§7)

Already in `bytelanes-wgpu`, and measured in M5:
- `submit` and `poll` never wait (since M3).
- Consecutive programs overlap. That gives +4% to +29% throughput for
  16-batch streams, in `atlas/runs/ryzen/2026-11-22-stream.json`.
- Staging buffers are pooled and reused, for uploads of 64 KiB and up and
  for readback.

wgpu cannot page-lock an ordinary host `Vec`. The reused, driver-allocated
host-visible staging buffers give the same effect: no fresh allocation
and no OS page zeroing per transfer.

## 3. More GPU batch work (decimation, RMS/peak, compression)

Not built. Each is a new op in the `bytelanes` op format (`Op` and its
parameters), with a CPU reference, golden vectors, and a WGSL kernel. That
makes it a contract change: by the repo rules it lands through a pull
request that passes on all three machines, as M7's FFT did.

A proposed order, by how much CPU and storage they save diagsteno:
1. **Block RMS, peak and mean over fixed windows,** as integer sums like
   the FFT features, so GPU and CPU agree exactly. This is diagsteno's
   time-domain feature work.
2. **Integer decimation** (FIR then downsample, with fixed-point taps).
   This cuts the samples stored for slow channels.
3. **Lossless compression of sample batches before storage.** Exact
   round-trip is easy to check, but a GPU compressor that is
   byte-for-byte the same as a CPU one is a large design job; delta plus a
   bit-packing scheme is the simplest exact candidate.

Each needs Evan's go-ahead on the op definition first.

## 4. Durable writes on the NVMe

Run file: `2026-11-22-diagsteno-ingest-nvme.json`. It ran the M7 gate
command with diagsteno `main` and the store on the SN770, plus the Dell's
raw probe.

| | Dell (M7 run file) | Ryzen, SN770 NVMe |
| --- | --- | --- |
| 1 MiB write + flush, p50 | 84.9 ms | 1.34 ms |
| 1 MiB write + flush, p90 | 104.8 ms | 2.83 ms |
| 1 MiB write + flush, max | 190.1 ms | 8.66 ms |
| M7 gate: acknowledgement delay p50 | 289 ms | 54 ms |
| M7 gate: acknowledgement delay p99 | 9.16 s | 112 ms |
| M7 gate: acknowledgement delay max | 9.60 s | 117 ms |
| Worst sample flush | 8,666 ms | 6 ms |
| Time spent storing samples | 45.0 s of 60 | 1.9 s of 60 |

- Every frame was stored, with 0 missing and 0 duplicate, and all 13,856
  feature rows were written.
- On the NVMe, acknowledgement delay is set by the 100 ms batching window,
  not by storage.
- **For the Dell:** a fast flush device removes its storage bottleneck.
  The SN770 is a consumer drive without power-loss protection, though, so
  it shows the speed, not the durability a production store needs.
  Agent A's recommendation still stands: a drive with power-loss
  protection, or a SLOG.

## 5. IoRing for the store

**A negative result: one IoRing submission per batch is no faster than one
system call per chunk plus a flush.**

- **What was built:** `stagelanes_arch::DurableWriter` (stagelanes
  `41a9fc7`). On Windows 11 it puts every write of a batch, plus a flush
  marked "drain preceding operations", into one IoRing submission, which
  is one system call. It works on an ordinary `std::fs::File` handle.
- **The benchmark:** `experiments/durable-writes`, run file
  `2026-11-22-durable-writes.json`. 100 batches per shape, each flushed
  before the next, each method run twice; the file contents were verified
  every time.

| Chunks of 64 KiB per batch | std (write per chunk + `sync_data`), latency p50 | IoRing, latency p50 |
| --- | --- | --- |
| 1 | 0.27 / 0.44 ms | 0.31 / 0.29 ms |
| 8 | 0.59 / 0.59 ms | 0.62 / 0.59 ms |
| 32 | 1.67 / 1.54 ms | 1.64 / 1.58 ms |
| 128 | 7.02 / 6.89 ms | 7.20 / 6.00 ms |

(The two values are the two runs.)

- **Latency:** the flush to the device dominates every batch, so saving up
  to 128 system calls does not show.
- **CPU time per batch:** not resolved. The values are multiples of
  156 µs, which is Windows' 15.6 ms CPU-time tick divided by 100 batches,
  and the two methods overlap. More batches would resolve it, but at 128
  chunks that writes about 8 GB per run to the drive.
- **Keep it:** the store gains nothing measurable today. `DurableWriter`
  stays as the place a later version can add what IoRing does help with:
  overlapping several batches in flight, and registered buffers.

## 6. DirectStorage (NVMe to GPU memory)

**Feasible on DX12, with `unsafe` interop, and not yet worth building for
this workload.**

- **DirectStorage writes into a D3D12 resource.** Its queue takes an
  `ID3D12Device`, and "the destination resource's device must match this
  device" (Microsoft Learn, `DSTORAGE_QUEUE_DESC`). Completion is an
  `ID3D12Fence` the GPU can wait on, and it can decompress on the GPU
  (GDeflate).
- **wgpu 30 can reach those objects, but only through `unsafe` hal
  interop** (checked in the wgpu 30.0.2 source):
  - `Device::as_hal::<Dx12>()` gives `raw_device()` and `raw_queue()`;
  - `Buffer::as_hal` gives a buffer's D3D12 resource;
  - `Device::create_buffer_from_hal` wraps a resource created elsewhere.
- **Synchronisation:** DirectStorage signals a fence, and the wgpu queue's
  `ID3D12CommandQueue::Wait` on that fence orders the GPU work after it,
  with no CPU wait.
- **Costs and limits:**
  - `bytelanes` forbids `unsafe`, so the interop would need a separate
    crate where it is allowed. That is a rule decision for Evan.
  - It is DX12 only: there is no Vulkan equivalent on AMD, and none on
    Linux or macOS.
  - Buffer state handling between DirectStorage and wgpu's tracking needs
    care: DirectStorage destinations must be in the COMMON state.
- **Where it would help:** reading stored data back for the GPU (replays,
  audits, rebuilding features), where it skips the CPU copy and the
  staging upload. diagsteno stores Parquet, which the CPU must decode, so
  it would need a raw sample format to benefit. The live path (network to
  GPU) gains nothing.

## 9. AMD L3 QoS on the Ryzen 7 5700X

Probe: `experiments/cpuid-qos` (CPUID, run on this machine).

| CPUID | Value |
| --- | --- |
| Leaf 7 EBX bit 12 (QoS monitoring) | 1 |
| Leaf 7 EBX bit 15 (QoS enforcement) | 1 |
| Highest basic leaf | 0xD |
| AMD leaf 0x80000020 EBX | 0 |

The CPU advertises QoS, but leaves 0xF and 0x10, which describe what
monitoring and allocation exist (L3 masks and classes of service), are
beyond the highest basic leaf. So they cannot be discovered, and Linux's
`resctrl` would not enable them. Windows offers no user-space cache
allocation interface at all.

**L3 partitioning is not usable on this machine.** Re-run the probe after
a BIOS update.
