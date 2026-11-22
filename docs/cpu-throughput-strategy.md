# CPU Transactional Throughput Strategy — Agent Brief

**Project:** paralanes (umbrella) · stagelanes (stage runtime) · bytelanes (GPU bytes library) · diagsteno (sensor data processing)
**Purpose:** Shared design guidance for the Claude Code agents building these components. Goal: maximize CPU transactional throughput and minimize interrupts/jitter on commodity x86/ARM, using a GPU and/or PCIe accelerator as helpers — approaching mainframe-style efficiency.

---

## 1. Core model: three roles

The strategy is hardware-neutral. Any silicon can fill each slot.

| Role | Mainframe analog | Job | Never does |
|---|---|---|---|
| **CPU worker cores** | Central processors | Run the transaction pipeline ("assembly line") on isolated, pinned cores | Handle interrupts, copy bulk data, wait on I/O |
| **I/O / channel role** (housekeeping cores + PCIe accelerator) | Channel subsystem / SAPs | Absorb all interrupts, poll I/O, fill and drain staging buffers, move data | Run transaction logic |
| **GPU** | (offload engine) | Bulk, high-throughput transforms between stages | Per-transaction work (launch + PCIe latency is µs-scale) |

**Rule of thumb for placement:**
- Large batch, latency-tolerant → **GPU**
- Data movement / I/O → **PCIe accelerator or channel cores**
- Per-transaction, latency-critical → **CPU worker cores**

Example accelerators for the channel role: DPU/SmartNIC, FPGA card, computational storage, on-chip DMA/copy engines (Intel I/OAT/CBDMA, Intel DSA, AMD PTDMA).

---

## 2. The memory-channel "read zone / write zone" idea — evaluation

**Original concept:** Dedicate RAM channels as read-only (filled from SSD by the GPU) and write-only (CPU cache output, drained to disk by the GPU), rotating channels so the device and CPU never share a channel. The CPU moves to the next channel once the current one is fully processed.

### What's valid
- DDR channels are half-duplex; read↔write turnaround (tWTR/tRTW) has a real cost. Segregation avoids some of it.
- Keeping device DMA traffic away from the CPU's working-set memory reduces contention.
- The **ping-pong / ring of staging buffers** pattern is correct and central: the device fills buffer N+1 while the CPU processes buffer N.

### Hardware realities to design around
1. **Physical addresses are interleaved across all channels** (hashed at cache-line granularity) for bandwidth. You cannot simply allocate "channel 1."
   - Option A: disable channel interleaving in BIOS → gives placement control but costs most single-stream bandwidth. Generally not worth it.
   - **Option B (preferred): Sub-NUMA Clustering (SNC)** on Xeon (or NPS settings on EPYC). Each sub-NUMA node maps to a subset of memory controllers. Allocate staging zones per node with `numactl` / libnuma / `mbind`. This is the practical "channel zone."
2. **SSD → RAM does not need the GPU.** NVMe already DMAs directly into host RAM with zero CPU copying. Routing through the GPU adds a PCIe hop. The real CPU cost is interrupts and submission/completion handling — solve that instead (Section 3).
3. **The CPU cannot redirect cache writeback.** Evicted lines return to their home address. Instead, write stage output with **non-temporal (streaming) stores** (`MOVNTDQ`/`_mm_stream_*` on x86; `STNP` on ARM) into the chosen output buffer. Output bypasses cache and doesn't evict the working set. This achieves the intent of "write out to the write channel."
4. **Memory controllers already batch writes** (write queue draining), so read/write channel separation is a marginal gain, not a dramatic one. Measure before investing.

### Recommended form of the idea
- Input staging rings and output staging rings live in dedicated NUMA/SNC zones.
- Channel cores + accelerator own filling inputs and draining outputs.
- Worker cores read inputs, write outputs with non-temporal stores, and advance to the next ready buffer.
- Buffers handed off via lock-free descriptors; ownership is explicit (device-owned / CPU-owned / free).

---

## 3. Interrupt elimination (highest-value, lowest-cost)

### Linux
- Boot params for worker cores: `isolcpus=<workers>` (or cpusets / `isolcpus=managed_irq,domain,<cpus>`), `nohz_full=<workers>`, `rcu_nocbs=<workers>`.
- Route all hardware IRQs to 1–2 housekeeping cores: `irqaffinity=<housekeeping>` and/or `/proc/irq/*/smp_affinity_list`. Disable `irqbalance` or constrain it.
- Use **polling, not interrupts**: io_uring with `IORING_SETUP_SQPOLL` (+ `IORING_SETUP_IOPOLL` for NVMe), or SPDK user-space NVMe; DPDK for networking.
- Disable deep C-states on worker cores (`intel_idle.max_cstate=`, `processor.max_cstate=`, or `/dev/cpu_dma_latency`), fix frequency (performance governor) to reduce wake-up jitter.
- Move kernel threads/workqueues off workers (`/sys/devices/virtual/workqueue/cpumask`).
- Verify with `/proc/interrupts`, `perf stat`, and `osnoise`/`timerlat` tracers.

### Windows
- No true core isolation. Use process/thread affinity masks and `SetThreadSelectedCpuSets`, raise priority, and set interrupt affinity per device (Interrupt Affinity Policy / registry `DevicePolicy`).
- Use **IoRing** (Windows 11+) or IOCP with batching; avoid per-request syscalls.
- Use High Performance / Ultimate Performance power plan to limit C-state jitter.

### macOS
- No core isolation or IRQ steering. Use QoS classes and thread affinity tags (hints only). Treat macOS as a correctness / Apple-silicon unified-memory testbed, not the interrupt-isolation benchmark.

### Mainframe "channel program" pattern
Describe a whole I/O sequence once and receive one completion, rather than one request per operation. Modern equivalents: io_uring linked SQEs (`IOSQE_IO_LINK`), batched submission, completion coalescing.

---

## 4. Cache control (instead of just more cache)

- **Partition L3 per pipeline stage:** Intel **CAT** (Resource Director Technology) / AMD **L3 QoS (PQoS)**. Check support with `pqos -d` (intel-cmt-cat) or `resctrl` (`/sys/fs/resctrl`). Also consider **Memory Bandwidth Allocation (MBA)** to cap noisy stages.
- **Intel DDIO:** PCIe DMA writes land directly in LLC — incoming data is already hot. Tune the number of DDIO ways; size input buffers so arriving data is consumed before eviction. (Opposite of the "RAM channel" idea, often better.)
- **Huge pages** (2 MB / 1 GB) for rings and working sets to eliminate TLB misses.
- **Data layout:** struct-of-arrays for hot fields; 64-byte (or 128-byte on Apple silicon) alignment/padding to prevent false sharing; keep per-stage state compact.
- **SPSC rings between stages sized to fit in L2** of the consumer core.
- **Software prefetch** a fixed distance ahead in stage loops; tune or disable hardware prefetchers if they pollute cache for streaming access.
- **Non-temporal stores** for any output not re-read soon (see Section 2).

---

## 5. Assembly-line (stagelanes) runtime rules

- One pinned thread per stage per core; no migration, no oversubscription.
- Stages connected by lock-free single-producer/single-consumer rings; batch handoff (process N items per dequeue) to amortize synchronization.
- No syscalls, allocations, or locks on the hot path. Pre-allocate everything at startup on the correct NUMA node.
- Keep a stage and its rings on the same NUMA node / L3 domain; cross-socket handoff only at deliberate boundaries.
- Backpressure via ring occupancy, not blocking.
- Instrument per-stage throughput, ring occupancy, and p50/p99/p99.9 latency.

---

## 6. Copy/data-movement offload (accelerator slot)

- Prefer on-platform DMA engines for bulk memory moves before reaching for a GPU: Intel **I/OAT (CBDMA)** on Xeon Scalable (check `dmesg | grep ioat`, Linux `dmaengine`), Intel **DSA** on Sapphire Rapids and newer, AMD **PTDMA** on EPYC.
- External PCIe options: DPU/SmartNIC (network + storage offload), FPGA (custom parsing/filtering), computational storage (filter at the drive).
- Peer-to-peer DMA (SSD ↔ accelerator without host RAM) where supported; note GPUDirect Storage is NVIDIA-only.
- **Future direction:** CXL memory/accelerators, allowing coherent shared memory regions owned by the device — the cleanest realization of "device-owned memory zone."

---

## 7. GPU role (bytelanes)

Good GPU work (batch, between stages):
- Pre-filter / parse raw blocks so the CPU only sees relevant records
- Checksums, hashing, compression/decompression, encryption
- Sorting, index building, deduplication
- Aggregation and telemetry reduction (feeds **diagsteno**: strain gauge / accelerometer processing — FFTs, filtering, RMS/peak, decimation)

Rules:
- Batch large enough to amortize launch + PCIe transfer; measure the crossover size.
- Use pinned (page-locked) host buffers and async queues; overlap transfer with compute (double-buffer on the GPU side too).
- Keep the GPU off the per-transaction critical path.
- Vendor-neutral API preferred: **Vulkan compute** (Rust: `wgpu` or `vulkano`), Metal on macOS. CUDA/ROCm only as optional backends.

---

## 8. Platform notes for the three test machines

| Machine | Role in plan | Notes |
|---|---|---|
| **Dell PowerEdge R440 — 2x Xeon Silver 4214 (12 cores each), Proxmox, no GPU, SATA SSDs only** | Shared first phase; assembly-line + channel-subsystem benchmark | Best platform for isolation work: SNC, CAT/MBA (verify with `pqos -d`), DDIO, I/OAT. Run benchmarks in a Linux VM with pinned vCPUs/NUMA passthrough or directly on host; note virtualization adds IRQ noise — prefer bare-metal-like pinning or a dedicated LXC/host run for final numbers. |
| **Ryzen 7 5700X desktop (AM4, A520 Mini-ITX), Windows, Radeon RX 7800 XT 16 GB (PCIe 3.0 x16), M.2 NVMe SSD** | stagelanes + paralanes testing; GPU (bytelanes) work | RX 7800 XT = RDNA3; bytelanes uses wgpu (Vulkan and DX12), which reproduce the CPU reference exactly (M3). Windows limits isolation (Section 3). Optionally dual-boot/WSL2 is *not* a substitute for Linux isolation tests. Check desktop Ryzen L3 QoS support before relying on it. |
| **MacBook Pro M5 Pro, 24 GB** | Third platform; ARM + unified memory | Unified memory removes CPU↔GPU PCIe copies — useful contrast for bytelanes design. Metal for GPU. No IRQ steering/core isolation; use for portability and ARM correctness (128-byte cache-line padding, `STNP`, memory ordering differences). |

---

## 9. Benchmark plan (run on the Dell first)

Same transaction workload, measure **throughput** and **p99 / p99.9 latency** at each step, changing one layer at a time:

1. **Baseline:** stock kernel/BIOS settings
2. **+ Isolation:** `isolcpus` / `nohz_full` / `rcu_nocbs`, IRQ steering to housekeeping cores, C-states limited
3. **+ Cache control:** CAT partitions per stage, huge pages, non-temporal output stores
4. **+ Zones & polled I/O:** SNC staging zones, io_uring SQPOLL/IOPOLL or SPDK, ping-pong buffers
5. **+ Copy offload:** I/OAT/DSA for bulk moves
6. **+ GPU stage** (Ryzen/RX 5700 or Mac): offload one batch transform; record batch-size crossover point

Record for each run: config, ops/sec, latency percentiles, `/proc/interrupts` deltas on worker cores, LLC miss rate (`perf stat -e LLC-load-misses`), memory bandwidth, ring occupancy per stage.

**Acceptance signal:** worker cores show ~zero interrupts during steady state, and each layer's contribution to throughput/p99 is quantified so later design choices are evidence-based.

---

## 10. Agent coordination notes

- Keep the three roles (CPU pipeline / channel I/O / GPU batch) behind clear interfaces so each agent can work on its platform independently.
- Shared contracts: buffer descriptor format, ownership states (free / device-owned / CPU-owned / done), ring API, and metrics schema — define these first and version them.
- Platform-specific code (isolation setup, DMA engines, GPU backend) lives behind feature flags / traits; core pipeline logic stays portable Rust.
- Every optimization lands with a benchmark result from Section 9, not just a claim.

---

## 11. Work for the Windows PC (agent B: Radeon RX 7800 XT, NVMe SSD)

*Added 2026-11-22 by agent A (Dell). The platform corrections in §8 above
follow `docs/machines.md`.*

The Dell has no GPU and no NVMe, and its guest cannot use CAT, DDIO or
I/OAT (see §12). These items need the Windows PC's GPU, NVMe SSD or
Windows itself. Each lands with a run file, as in §9.

### GPU (bytelanes-wgpu)
1. **Benchmark step 6 (§9): the GPU stage in the pipeline.**
   - Run diagsteno's feature stage through `Engine::new(<wgpu backend>)`
     inside the per-socket pipeline experiment
     (`experiments/per-socket-pipeline`), in place of the CPU reference.
   - Record the window-batch size at which the GPU beats the CPU, end to
     end. The FFT atlas (M5, M7) has the bare-op crossover; this is the
     in-pipeline one.
   - Check that features stay byte-identical to `features::reference`.
2. **§7 transfer rules in `bytelanes-wgpu`:**
   - pinned (page-locked) host buffers
   - asynchronous submit and poll, so `submit` never waits
   - double-buffering on the GPU side, so the copy of batch N+1 overlaps
     compute on batch N

   Measure each against the M5 runs.
3. **More GPU batch work from §7, prioritised for diagsteno:** decimation and
   RMS/peak reduction over many channels, and CRC-checked compression of
   sample batches before storage. Fewer bytes reaching the disk is the GPU
   help that actually targets the storage bottleneck the Dell measured.

### NVMe SSD (storage path)
4. **Durable-write latency on the NVMe.**
   - Record the drive model and whether it has power-loss protection.
   - Run diagsteno ingest (M7 gate command) with the store on the NVMe and
     compare acknowledgement delay with the Dell run file
     `atlas/runs/dell/2026-11-22-diagsteno-ingest.json`. There, flushes
     averaged about 85 ms and stalled for up to 8.7 s.
   - This tells us whether a fast flush device fixes the Dell's bottleneck
     before buying one.
5. **IoRing (Windows 11) or IOCP with batching for the store (§3):**
   write + flush + rename submitted as one batch, the "channel program"
   form. Compare with plain synchronous writes on the same NVMe.
6. **DirectStorage (research, not code yet).** It moves NVMe → GPU memory
   with GPU decompression on any DX12 GPU. It is the nearest thing to the
   "GPU does the disk I/O" idea available on AMD. wgpu does not expose it, so
   first report whether it can share buffers with a wgpu DX12 device.

### Windows isolation (stagelanes platform code)
7. **Benchmark step 2 (§9), Windows form:**
   - `SetThreadSelectedCpuSets` for stage threads, raised thread priority
   - per-device interrupt affinity (Interrupt Affinity Policy) for the NIC,
     NVMe and GPU, steered to 1–2 housekeeping cores
   - Ultimate Performance power plan

   Measure p50/p99/p99.9 and per-core interrupt rates with the same metrics
   as the Dell's step-1 run, so the two OSes compare directly.
8. **`stagelanes-pipe` (M6, §5):** the assembly-line runtime itself. It is
   built on the Ryzen per the plan, and the Dell repeats it on two sockets.
9. **Check AMD L3 QoS on the Ryzen 7 5700X** before relying on it. Desktop
   Zen 3 may not support it; record what Windows or the BIOS exposes.

Not for the Windows PC: Linux isolation tests (WSL2 is not a substitute),
AMD PTDMA (EPYC only), Intel CAT, DDIO and I/OAT.

## 12. Where each item stands on the Dell (agent A)

Checked inside the guest on 2026-11-22. The guest exposes:
- no CAT, MBA or resctrl
- no DMA engines and no cpuidle; the host controls C-states
- io_uring enabled
- 2 MB and 1 GB huge pages supported, none reserved
- the hardware PMU visible

| Item | Status on the Dell |
| --- | --- |
| §9 step 1 baseline with full metrics | In progress: `experiments/per-socket-pipeline` `--layout step-metrics` |
| §9 step 2: isolcpus, nohz_full, rcu_nocbs, IRQ steering, workqueue mask | Ready. Needs root in the guest and a guest reboot (Evan) |
| C-states and performance governor | Host only (Proxmox, Evan) |
| Huge pages for rings and pools | Ready with root; `stagelanes-mem` supports them |
| CAT and MBA | Host only, via `pqos` on the pinned host cores; support on the Silver 4214 must be checked with `pqos -d` |
| io_uring linked writes and SQPOLL for the store | Buildable in the guest |
| Non-temporal stores | Buildable, but needs `unsafe` intrinsics, so it belongs in stagelanes' `platform` modules |
| SNC zones | Not possible now: BIOS change plus evenly populated memory controllers (today 3+2 DIMMs) |
| NVMe IOPOLL and SPDK | Not possible: no NVMe |
| DDIO and I/OAT | Host only, and of little value while the guest's I/O is virtualised |
| GPU stage | Not possible: no GPU (see §11) |
