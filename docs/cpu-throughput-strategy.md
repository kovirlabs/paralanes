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
| **Dell server — dual Xeon Silver 10-core, Proxmox, no GPU** | Shared first phase; assembly-line + channel-subsystem benchmark | Best platform for isolation work: SNC, CAT/MBA (verify with `pqos -d`), DDIO, I/OAT. Run benchmarks in a Linux VM with pinned vCPUs/NUMA passthrough or directly on host; note virtualization adds IRQ noise — prefer bare-metal-like pinning or a dedicated LXC/host run for final numbers. |
| **Ryzen 7 AM5 desktop, Windows, Radeon RX 5700** | stagelanes + paralanes testing; GPU (bytelanes) work | RX 5700 = RDNA1 (gfx1010): not officially supported by ROCm → use **Vulkan compute**. Windows limits isolation (Section 3). Optionally dual-boot/WSL2 is *not* a substitute for Linux isolation tests. Check desktop Ryzen L3 QoS support before relying on it. |
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
