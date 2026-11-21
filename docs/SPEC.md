# Paralanes: Rust GPU Channel Subsystem — Spec

Nov 21, 2026 · @Evan Gress · Reconciled with `docs/PLAN.md` on 21 Nov 2026 (see [Changes from the earlier draft](#changes-from-the-earlier-draft))

Paralanes is a Rust system layer that gives commodity hardware two mainframe traits: work cores organised as an assembly line of pinned stages, and a GPU acting as a channel subsystem for bulk data. It is proven on three machines, each developed by its own Claude Code agent and then joined over the network: a dual-Xeon Dell server, a Ryzen desktop with an AMD GPU as a discrete-GPU channel processor, and an M5 Pro MacBook Pro as a unified-memory channel processor. The demo workload is `diagsteno`, the Diagnostic Stenographer, which processes strain gauge and accelerometer data.

This document is the technical source of truth. [`docs/PLAN.md`](PLAN.md) sets the milestone order (M0 to M10) and says which machine does what; where the two disagree, the plan is current and the conflict is noted in a status file.

## Names and repos

The project was drafted as Channelforge and is now **paralanes**. The work ships as four repos. Earlier drafts used `cf-` placeholder crate names; the table maps them to the real crates.

| Repo | Role | Crates | Earlier `cf-` name | Licence |
| --- | --- | --- | --- | --- |
| `paralanes` | Umbrella: spec, plan, atlas of measurements, network join, appliance image | `paralanes-chand`, `paralanes-net`, `paralanes-sched`, `paralanes-init`, `image/` | `cf-chand`, `cf-backend-net`, `cf-sched`, `cf-init` | GPL-3.0-only |
| `bytelanes` | Bulk data ops on any GPU, with a CPU reference. Owns the op format | `bytelanes-abi`, `bytelanes`, `bytelanes-wgpu`, `bytelanes-bench`, `vectors/` | `cf-abi`, `cf-backend`, `cf-backend-wgpu`, the op half of `cf-bench` | Apache-2.0 |
| `stagelanes` | Topology-aware stage runtime for Windows and Linux | `stagelanes-ring`, `stagelanes-mem`, `stagelanes-arch`, `stagelanes-pipe`, `stagelanes-bench`, `stagelanes` (CLI) | `cf-ring`, `cf-mem`, `cf-arch`, `cf-pipe`, the pipeline half of `cf-bench` | Apache-2.0 |
| `diagsteno` | Diagnostic Stenographer: strain gauge and accelerometer signal processing; the demo workload | `diagsteno`, `diagsteno-store` | Replaces `cf-txn`; `cf-store` | GPL-3.0-only |

Dependencies point one way: `bytelanes` and `stagelanes` depend on neither each other nor anything above them; `diagsteno` uses both; `paralanes` uses all three.

## Feasibility

The idea works if GPUs take the I/O and bulk-data role, not the control role. Four corrections to the literal request shape the design:

| Literal ask | What is true | Design consequence |
| --- | --- | --- |
| GPUs do mainframe computing | GPUs are poor at branchy, conditional logic; the source chat says the same | CPU cores keep the control logic. GPUs become channel processors: they read, decompress, checksum, decrypt, filter and transform data in bulk |
| Rust firmware | No bare-metal GPU compute stack exists. Vendor compute needs an OS kernel driver. The Rust NVIDIA driver (Nova) boots the GPU's system processor on Turing through Blackwell but is not ready for end users | "Firmware" means an appliance image: UEFI boots a minimal Linux kernel whose only userspace is one Rust PID 1. It flashes and boots like firmware |
| GPU loads data into CPU caches | A GPU cannot write CPU caches (the source chat is right) | Results land in pinned large pages that are NUMA-local to the requesting core; the core prefetches them |
| Mainframe level | The I/O offload property is reachable. Hardware seven-nines RAS is not | Target: stage cores run at 90% load without latency collapse. Reliability comes from redundant GPU paths and checksums, in software |

The direct NVMe-to-GPU path exists today on NVIDIA: GPUDirect Storage has an NVMe P2PDMA mode on the upstream Linux NVMe driver. It needs a kernel newer than 6.2 and the open NVIDIA kernel driver 565.47 or later, and NVIDIA's guide lists it for x86-64 only. AArch64 therefore builds and runs from day one but uses a bounce-buffer path until that changes. The three-machine plan defers this direct path, because none of the machines has an NVIDIA GPU.

## Architecture

&#91;embedded content: data path · 2 control layers, 3 data-plane parts\]

A core pushes a channel program onto its ring and keeps working. Bulk bytes travel from storage or the network to a GPU to pinned memory and never pass through the core; the core receives a pointer to the result.

## Three-machine plan

Each machine proves one part of the design alone; a network backend then joins them to prove the whole. Milestone numbers refer to `docs/PLAN.md`.

| Aspect | Dell server (agent A) | Ryzen desktop (agent B) | MacBook Pro (agent C) |
| --- | --- | --- | --- |
| Hardware | PowerEdge R440: two Xeon Silver 4214 CPUs, 12 cores each; 160 GB ECC memory, unevenly split between sockets; no GPU | Ryzen 7 5700X (8 cores, one shared L3) on an AM4 Mini-ITX board; Radeon RX 7800 XT, 16 GB, on a PCIe 3.0 x16 link; 1 GbE | Apple M5 Pro, 24 GB unified memory |
| Target | x86-64 Linux guest on Proxmox | x86-64 Windows | AArch64 macOS |
| Role | Op format and CPU reference (M1); two-socket check of `stagelanes` (M2, M6); `diagsteno` (M7, M8); scheduler and test driver for the join (M9); appliance image (M10) | Main `stagelanes` machine on bare metal (M2, M6); discrete-GPU channel processor through Vulkan or DX12 (M3, M5) | Unified-memory channel processor through Metal (M4, M5) |
| Proves alone | Placement by socket: the gain or loss from keeping each stage's memory on its own socket; durability (R8) | Latency holds at 90% core load (R2); GPU output equals the CPU reference (R7); the input size where the GPU starts to win | A native AArch64 run (R4); R7 on a second GPU vendor; what the ops cost when the copy to the GPU disappears |
| Proves joined | Schedules work across two GPU paths; fails over between them and then to the CPU (R3) | Serves channel programs to the Dell | Serves channel programs to the Dell |

Full hardware details are in [`docs/machines.md`](machines.md). No machine in the set has mixed cache-heavy and frequency-heavy cores, so placement by core type stays untested until one does. On the Ryzen, GPU benchmarks (M3, M5) and pipeline benchmarks (M2, M6) must not run at the same time.

### The assembly line

`stagelanes-pipe` gives each group of cores one stage and nothing else. For `diagsteno` the stages are:

1. **Ingest:** receive sample frames, validate them and stamp arrival time.
2. **Sequence:** one core assigns a global order per channel, so later stages need no locks.
3. **Features:** each core owns one partition of the channels and submits FFT and band-feature channel programs for it; inputs below the crossover size run on the CPU reference.
4. **Score:** anomaly score per channel from its features.
5. **Store:** batch feature records to `diagsteno-store` and flush.

Stages are placed next to their I/O: on the Dell all network and storage devices attach to socket 0, so ingest and store run there (see `docs/machines.md`). Queries are served off the stage cores. Stages hand off pointers in batches over `stagelanes-ring` queues, and a partition's memory lives on the same socket as the core that owns it. The core split is tuned with `stagelanes-bench` per machine; on the Dell, assuming a 20-core guest (10 cores per socket, leaving 4 of the 24 to Proxmox), a starting split is 3 ingest, 1 sequence, 10 features (5 per socket), 2 score, 2 store, 2 for the OS and the channel scheduler.

When joined, an I/O core on the Dell sends bulk bytes to a GPU machine; stage cores still never touch payload. Two things stay unproven until new hardware arrives: the direct NVMe-to-VRAM path, and offload at PCIe speed rather than network speed. The Mac narrows the first gap by showing GPU work without the copy into video memory.

### Three agents

Each machine runs its own Claude Code agent. Each repo's `CLAUDE.md` holds the working rules; the ones that keep the agents from colliding are:

- **Milestone branches.** Work happens on `m<n>-<short-topic>`, one branch per milestone per repo. `main` is Evan's; only Evan merges, tags and publishes.
- **The contract is frozen after M1.** `bytelanes-abi` and `vectors/` change only through a pull request that passes on all three machines. An agent that needs a change writes it under Requests in its status file and stops that task.
- **One seam in the GPU crate.** Agent B owns the ops and WGSL shaders in `bytelanes-wgpu`. Agent C owns one module behind a `BufferStrategy` trait and does not edit shaders.
- **Golden vectors.** The CPU reference backend writes inputs and expected outputs to `vectors/`; both GPU machines must reproduce them exactly.
- **Status files.** Each milestone writes `docs/status/M<n>.md` in `paralanes`, whichever repo the work was in. Requests to another agent go under a Requests heading there.
- **Rebase at every session start**, then read every file in `docs/status/`.
- **Parallel sessions touch different repos or different machines.** The plan's milestone table says which pairs are safe.

### Adapting to new hardware

The machines will change: faster links between them, more GPUs, a board with PCIe 4.0, a CPU with mixed core types. The design treats hardware as a variable to measure, not a constant to code against, so new components are used as they appear.

1. **Inventory, then measure.** Each machine's hardware is recorded in `docs/machines.md`. `stagelanes topology` reports the CPU side; `paralanes-chand` reports its GPU, device memory, wgpu backend, memory model (discrete or unified) and host link when it joins.
2. **No hard-coded thresholds.** The CPU fallback size is per op, per machine and per path, taken from atlas run files, never a constant in code. A new GPU or a new link means new benchmark runs, not a code change.
3. **The scheduler measures links.** `paralanes-sched` measures bandwidth and round-trip time to each `paralanes-chand` when it joins and while it runs, and places work by estimated total cost (transfer both ways plus compute) against running locally. Over 1 GbE a remote GPU wins only on work with a high compute-to-byte ratio; on a faster link the same code sends more work out.
4. **Transports are pluggable.** `paralanes-net` hides the link behind a `Transport` trait. TCP comes first and needs no change for a faster Ethernet link; RDMA or other transports arrive as further implementations.
5. **Link speed is a recorded variable.** Every run that crosses machines records the link type and speed at each end. The M9 tests are written so they can be re-run unchanged when a faster link arrives, giving a direct comparison.
6. **More devices fit the same traits.** A second GPU in a box is another `ChannelBackend` path; a new GPU vendor is a new backend; a CPU with mixed core types shows up as core classes in `stagelanes-arch`.

## Requirements and non-goals

The numbers below are proposed targets, not measurements.

| ID | Requirement | How it is checked |
| --- | --- | --- |
| R1 | Stage cores never copy bulk payload on the hot path | `stagelanes-bench` counts payload bytes touched by CPU per work item; it must not exceed the result size |
| R2 | Latency holds at high CPU load | `stagelanes-bench`: p99 at 90% core utilisation is at most 3x p99 at 50% (M6 gate on the Ryzen) |
| R3 | The GPU path can fail without losing work | Take a GPU machine offline mid-load: work moves to the other GPU machine, then to the CPU backend, with zero lost and zero duplicated completions (M9 gate) |
| R4 | One source tree for x86-64 and AArch64, Linux, Windows and macOS | Builds for `x86_64-unknown-linux-musl` and `aarch64-unknown-linux-musl`; `stagelanes` and `paralanes-chand` also build and test on Windows; the channel side runs natively on AArch64 macOS; OS and arch code lives only in `platform` modules |
| R5 | Boots as an appliance | UEFI to serving in under 10 s, read-only root, A/B image update (M10) |
| R6 | GPU vendor is swappable | One `ChannelBackend` trait; a wgpu backend (Vulkan, DX12, Metal) and the CPU reference backend. A CUDA backend is deferred |
| R7 | GPU results are trustworthy | Every op has a CPU reference; GPU output matches it byte for byte on `vectors/` and in 10,000 property-test cases per op |
| R8 | Durability does not depend on a GPU | A record is acknowledged only after `diagsteno-store` has flushed it; a kill-and-restart test loses no acknowledged record; data leaving a GPU is checksum-verified |
| R9 | New hardware is used without code changes | Changing a GPU, adding one, or changing a link between machines needs only new benchmark runs: placement and CPU fallback come from measured values; M9 runs record link type and speed and can be re-run unchanged |

Non-goals:

- s390x, z/OS or Hercules compatibility
- Running branchy control logic on GPUs
- Writing a GPU driver or a `no_std` kernel from scratch
- Hardware-level RAS (lockstep cores, hot-swap CPUs)
- CXL memory pooling in v1; the memory crate leaves room for it
- NVIDIA or CUDA code until NVIDIA hardware exists

## Crate layout

`unsafe` is allowed only where marked, and every block gets a `// SAFETY:` comment.

| Crate | Repo | Kind | Owns | Built in | `unsafe` |
| --- | --- | --- | --- | --- | --- |
| `bytelanes-abi` | bytelanes | `no_std` lib | Op format: channel program wire format, status codes, zero-copy structs | M1 | no |
| `bytelanes` | bytelanes | lib | `ChannelBackend` trait and the CPU reference backend (`bytelanes::cpu`) | M1 | no |
| `bytelanes-wgpu` | bytelanes | lib | WGSL compute shaders through wgpu (Vulkan or DX12 on the Ryzen, Metal on the Mac); `BufferStrategy` trait with copying and unified-memory implementations | M3, M4 | no |
| `bytelanes-bench` | bytelanes | bin | GPU versus CPU benchmark per op across input sizes; writes atlas run files | M5 | no |
| `vectors/` | bytelanes | data | Golden inputs and expected outputs per op | M1 | n/a |
| `stagelanes-ring` | stagelanes | `no_std` lib | Lock-free rings: inter-stage queues and submission/completion pairs | M2 | yes |
| `stagelanes-mem` | stagelanes | lib | Pinned memory pools: large pages and NUMA placement on Linux and Windows, page-locked memory on macOS; pool sizes capped | M2 | yes |
| `stagelanes-arch` | stagelanes | lib | Topology detection (cache sizes, top frequency, socket, core class), core pinning, prefetch, cycle counters | M2 | yes |
| `stagelanes-pipe` | stagelanes | lib | Stage threads, core and socket plan, batched handoff | M6 | no |
| `stagelanes-bench` | stagelanes | bin | Synthetic load generator, latency histograms, bytes-touched counter, allocation and lock counters | M6 | no |
| `stagelanes` | stagelanes | bin | CLI: `stagelanes topology` | M2 | no |
| `diagsteno` | diagsteno | lib, bin | Ingest, band features, anomaly scoring, HTTP query API, export to a time-series database | M7, M8 | no |
| `diagsteno-store` | diagsteno | lib | Storage of samples and features with flush-before-acknowledge; format TBD, Parquet suggested | M7 | no |
| `paralanes-chand` | paralanes | bin | Channel daemon on a GPU machine: receives programs, runs them on the local backend, returns results with a CRC | M9 | no |
| `paralanes-net` | paralanes | lib | Backend that submits channel programs to a `paralanes-chand` through a `Transport` trait: TCP first, faster transports such as RDMA later | M9 | no |
| `paralanes-sched` | paralanes | lib | Scheduler: batching, placement across GPU paths by measured cost, link measurement, CPU fallback below the crossover size, path health and failover | M9 | no |
| `paralanes-init`, `image/` | paralanes | bin, build scripts | PID 1 and the appliance image; Linux only | M10 | syscall wrappers only |
| CUDA backend | deferred | lib | Direct NVMe-to-VRAM path; needs NVIDIA hardware | later | FFI only |

The CPU reference in `bytelanes` gets hardware CRC32C and AES speed from existing crates (`crc32c`, and RustCrypto's `aes` and `ctr`), which select the CPU instructions at runtime. `bytelanes` therefore needs nothing from `stagelanes-arch`, and the atlas compares the GPU against a CPU baseline that uses its hardware instructions.

## Channel program interface

A core describes the data work as a short chain of commands, submits it, and moves on. This mirrors the mainframe's channel command words and its Start Subchannel instruction. The sketch below is the starting point for M1; `bytelanes-abi` is the authority once it exists.

```rust
// bytelanes-abi: no_std, #[repr(C)], zero-copy

/// One step of a channel program.
#[repr(C)]
pub struct ChannelCmd {
    pub op: Op,
    pub flags: Flags,   // CHAIN = feed dst into the next command's src
    pub src: Extent,
    pub dst: Extent,
    pub arg: u64,       // op-specific: key slot, predicate id, model id
}

#[repr(u16)]
pub enum Op {
    Read, Write,
    Decompress, Compress,
    Checksum,           // CRC32C
    Decrypt, Encrypt,   // AES-256-CTR
    Filter,             // columnar predicate, returns matching rows
    Score,              // inline model inference (stretch)
    // Fft is added in M7 through a contract pull request
}

#[repr(C, u8)]
pub enum Extent {
    None,
    Nvme { dev: DevId, lba: u64, blocks: u32 },
    Vram { buf: BufId },          // stays on the GPU
    Host { buf: HostBuf },        // pinned large page, NUMA-local
}

/// At most 16 commands. Intermediate extents must be Vram.
pub struct ChannelProgram { /* fixed-size array + len */ }

pub struct Completion { pub tag: Tag, pub status: Status, pub bytes: u32, pub crc: u32 }
```

```rust
// bytelanes
pub trait ChannelBackend: Send + Sync {
    fn capabilities(&self) -> Caps;           // ops supported, direct NVMe path or bounce
    fn submit(&self, prog: &ChannelProgram, tag: Tag) -> Result<(), Busy>;
    fn poll(&self, out: &mut [Completion]) -> usize;
    fn health(&self) -> PathState;            // Online, Degraded, Offline
}
```

```rust
// what a stage core does
let tag = chan.start(&prog)?;      // ring push; no syscall, no lock, no allocation
// ... run other work ...
for c in chan.completions() {      // ring pop; busy-poll, no interrupt
    let row = c.host_buf();        // only the result bytes ever reach this core
}
```

Rules the scheduler enforces:

1. A whole program runs on one GPU; only the final `dst` may be `Host`.
2. Programs are batched per GPU up to a time or size limit, then dispatched together.
3. Work is placed by estimated cost from measured values: the atlas crossover per op and path, plus measured bandwidth and round-trip time for remote paths. Work below the crossover runs on the CPU reference backend, because a GPU round trip costs more than it saves on small inputs.
4. A program whose GPU goes `Offline` is resubmitted once on another path; programs are idempotent by construction, so a retry is safe.
5. Every `Host` result carries a CRC computed on the GPU and verified on the receiving core.

## Milestones and technical gates

`docs/PLAN.md` owns the milestone order, machines and release points. This table adds the technical detail behind each gate.

| # | Milestone | Technical gate detail |
| --- | --- | --- |
| M0 | Setup and landscape check | Four repos with licence, `CLAUDE.md`, CI; `docs/landscape.md` with a go or no-go per repo |
| M1 | `bytelanes` op format and CPU reference: Checksum, Filter, Decrypt; golden vectors | Unit and property tests pass; miri clean on `bytelanes-abi` and `bytelanes`; `vectors/` committed with a replay test |
| M2 | `stagelanes` rings, pinned memory, topology detection | Rings pass loom and miri; `stagelanes topology` prints the correct sockets, cache sizes and core classes on all three machines |
| M3 | GPU ops on the RX 7800 XT | Reproduces `vectors/` exactly plus 10,000 property-test cases per op; the wgpu backend used (Vulkan or DX12), driver version and PCIe link are recorded |
| M4 | GPU ops on Metal and a unified-memory `BufferStrategy` | Reproduces `vectors/` natively on AArch64; copy versus no-copy cost measured. If current wgpu offers no zero-copy route, the status file says so with the evidence, and the gate still passes |
| M5 | Atlas v1 | Crossover size per op per machine in `atlas/crossover.md`, each traced to a `bytelanes-bench` run file |
| M6 | Stage pipeline | R2 passes on the Ryzen; a test allocator and lock counter show zero allocations and zero locks on stage cores after startup; a single-core baseline is measured for comparison; the Dell run records the gain or loss from socket-local memory |
| M7 | `diagsteno` ingest and features; FFT op in `bytelanes` | A recorded synthetic dataset replays with features equal to the CPU reference; ingest holds the target channel count and sample rate with no dropped samples; R8 passes |
| M8 | `diagsteno` scoring and query | Injected faults are flagged; queries return stored features; export to a time-series database works |
| M9 | Join and failover | Work completes on both GPU machines with every result passing its CRC check; R3 passes. Every run records link type and speed; the MacBook is tested on wired Ethernet and on Wi-Fi, recorded separately |
| M10 | Appliance image | Boots in QEMU and runs `diagsteno`; R5 passes |
| Later | New hardware | Faster links between machines (25 GbE, RDMA or similar), re-running the M9 tests unchanged with link speed as the variable; a PCIe 4.0 board for the Ryzen's GPU; several GPUs in one box; the direct NVMe-to-VRAM path on NVIDIA; the `Score` op. Defined when started |

The Dell runs Proxmox, so agent A works in a Linux guest with the host CPU type, cores pinned one-to-one, both NUMA nodes exposed and large-page backing, with other guests stopped during benchmarks. On shared cores a latency result means nothing. R8 testing needs a disk or volume passed through to the guest. The Dell has no NVMe: storage sits behind a MegaRAID SAS-3 3108 controller, whose write-cache mode is recorded with every durability run.

If a step needs another machine, the agent writes the code and tests, marks them `#[ignore = "needs: <machine>"]`, notes it in the status file and continues.

## Claude Code setup

Each repo has its own `CLAUDE.md` with its crates, milestones, agents and rules. The milestone prompts are in `docs/PLAN.md`. At each gate, Evan reads `docs/status/M<n>.md`, answers its questions, merges if the gate passed, and pastes the next milestone prompt.

## Risks and open questions

Decisions needed:

- [ ] Remaining hardware details in `docs/machines.md`: drives and cache mode behind the Dell's RAID controller; whether a riser is fitted; MacBook network; the switch.
- [ ] Whether M9 runs over a dedicated link on the Dell's spare port `eno2`.
- [ ] Size of agent A's Proxmox guest (the spec assumes 20 cores, 10 per socket).
- [ ] For M7: channel count, sample rate and sensor types for the synthetic data; storage format.

Decided on 21 Nov 2026:

- The demo workload is `diagsteno`, in place of the earlier ledger.
- The Dell runs M1, the op format and CPU reference, which every other milestone builds on.
- The Ryzen is the main `stagelanes` machine; the Dell is the two-socket check.
- The Dell runs Proxmox; agent A works in a pinned Linux guest.
- The Ryzen runs Windows; `stagelanes`, `bytelanes-wgpu` and `paralanes-chand` must build and test there.
- The MacBook is tested on both wired Ethernet and Wi-Fi; M9 records each separately.
- Licences: Apache-2.0 for `bytelanes` and `stagelanes`; GPL-3.0-only for `diagsteno` and `paralanes`.
- The Ryzen desktop is a Ryzen 7 5700X with a Radeon RX 7800 XT 16 GB (details in `docs/machines.md`).
- Faster networking between machines is not near-term. It stays in the design as a test variable: the scheduler places work by measured link cost and the M9 tests are re-run when faster links arrive.
- The system adapts to the hardware present rather than to fixed values (see [Adapting to new hardware](#adapting-to-new-hardware)).

Risks:

| Risk | Effect | Mitigation |
| --- | --- | --- |
| Three agents across four repos | A drifting contract; M9 fails on integration | One contract in `bytelanes-abi`, frozen after M1; contract changes pass on all three machines; golden vectors; per-milestone status files; Evan as the only merger |
| The join runs over 1 GbE, not PCIe | M9 proves the design end to end, but for most ops the Dell's own CPU will beat a GPU on the other side of a 1 GbE link | Report M9 as a correctness and failover result; take performance claims from M5 and M6; place work by measured cost; re-run M9 unchanged when a faster link arrives |
| Current wgpu may not expose zero-copy buffers on Metal | M4 cannot show the unified-memory gain | M4 gate accepts a documented negative result; a Metal-native strategy can follow as a contract change |
| The MacBook is a laptop | Thermal throttling, sleep and battery mode make benchmarks noisy; it may be absent during M9 | Benchmark plugged in, lid open, power mode recorded; the scheduler treats an absent Mac as a failed path, which is itself the R3 test |
| 24 GB is shared by CPU and GPU on the Mac | Large buffer pools starve the system | Cap pools in `stagelanes-mem`; record pool size with each run |
| macOS has no large-page, NUMA or io_uring control | The pipeline cannot be measured meaningfully on the Mac | The Mac covers the channel side and `stagelanes topology` only; pipeline results come from the Ryzen and the Dell |
| Windows cannot fully fence cores off from the OS | p99 figures are noisier than on tuned Linux; the M6 gate is harder | Measure on a quiet system, report the distribution, and repeat on the Dell under Linux |
| Stage handoffs cost a cache miss each | The pipeline is slower than one core doing everything | Few stages, batched handoffs, pointers only; M6 compares against a single-core baseline |
| Two sockets on the Dell | Cross-socket memory access erases the gain | Partition memory per socket; `stagelanes-bench` reports remote-memory access counts |
| The Dell's memory is unbalanced: 3 DIMMs on CPU 1, 2 on CPU 2, each on at most half its channels | Per-socket bandwidth differs, skewing the M6 socket-local result; the CPU 2 node has less memory | Record DIMMs per NUMA node in every run file; report each socket separately; a sixth matching DIMM balances them at 3 and 3 |
| GPU round trip is slower than the CPU for small inputs | Latency gets worse, not better | M5 measures the crossover; the scheduler falls back to the CPU below it |
| The Ryzen has fast AES and CRC instructions, and its GPU sits on a PCIe 3.0 link | The GPU may lose to the CPU on checksum and decrypt at every input size, because moving the data costs more than computing on it | A loss is a valid atlas result; FFT and filter over large batches are the likelier GPU wins; record driver version and PCIe link with every run; the MacBook is the second GPU check |
| The Ryzen's Mini-ITX board has one PCIe slot, used by the GPU | A faster network card needs the M.2 slot through an adapter or a different board | Network upgrades start on the other machines or with a board change; tracked in `docs/machines.md` |
| Consumer GPUs have no ECC memory | Silent corruption in results | CRC on every result leaving the GPU (scheduler rule 5) |
| The Dell's 1U chassis limits GPU options | A later single-box build may need different hardware | Check riser and power specifications before buying a GPU for it |

## Changes from the earlier draft

The spec was drafted for a single workspace with a ledger demo and three track branches, then split into four repos by `docs/PLAN.md`. This pass brought it in line with the plan:

1. **Crate names.** The `cf-` placeholders are replaced by the real crate names throughout, with the mapping in [Names and repos](#names-and-repos).
2. **CRC and AES moved out of `cf-arch`.** `cf-arch` (now `stagelanes-arch`) held the CPU CRC and AES instructions, but the CPU reference that needs them is in `bytelanes`, which may not depend on `stagelanes`. The CPU reference now uses the `crc32c`, `aes` and `ctr` crates, which use the hardware instructions; `stagelanes-arch` keeps topology, pinning, prefetch and cycle counters.
3. **`cf-bench` split in two.** It served both the op benchmark and the pipeline benchmark. `bytelanes-bench` measures ops for the atlas; the new `stagelanes-bench` measures pipeline latency, bytes touched, allocations and locks (R1, R2).
4. **Ledger replaced by `diagsteno`.** `cf-txn` is gone. The assembly-line stages are rewritten for sensor data, and `cf-store`'s ledger write-ahead log becomes `diagsteno-store` with flush-before-acknowledge. R1, R2 and R8 refer to stage cores and stored records instead of transactions.
5. **Phases replaced by milestones.** The S0, A1–A2, B1–B2, C1–C2, J1–J2 and P phases are replaced by M0–M10 from the plan. The phases' technical gate detail is kept in [Milestones and technical gates](#milestones-and-technical-gates).
6. **Machine roles updated.** The Ryzen is now the main `stagelanes` machine and the Dell the two-socket check. The Dell runs M1 (formerly S0), and `stagelanes` targets Windows as well as Linux.
7. **Agent workflow.** Track branches (`track-a/b/c`), per-track ownership and `docs/status/<track>.md` are replaced by milestone branches (`m<n>-topic`) and `docs/status/M<n>.md`. `docs/bench/<track>.md` is replaced by the atlas run files.
8. **Embedded `CLAUDE.md` and S0 prompt removed.** Each repo now has its own `CLAUDE.md`, and the prompts live in `docs/PLAN.md`.
9. **Smaller fixes.** Hugepages became large pages, since Windows uses that term. The `ChannelBackend` list names the wgpu backends and defers CUDA. The scheduler's CPU fallback uses the crossover size from the atlas. The settled ledger-versus-real-workload question moved to Decided.

10. **Hardware and adaptability (later on 21 Nov 2026).** The Ryzen desktop is a Ryzen 7 5700X with a Radeon RX 7800 XT, not an AM5 board with an RX 570; hardware details moved to `docs/machines.md`. Added [Adapting to new hardware](#adapting-to-new-hardware) and R9: placement and CPU fallback come from measured values, and link speed is a recorded test variable. Faster networking is deferred, not dropped.
11. **Dell hardware (later on 21 Nov 2026).** The Dell is a PowerEdge R440 with two 12-core Xeon Silver 4214 CPUs (24 cores, not 20) and 160 GB of ECC memory split 96/64 GB between sockets. The 20-core stage split is kept as the starting guest size, leaving 4 cores to Proxmox, and the memory imbalance is added as a risk.

## Sources

- [Chat Summary: Mainframes vs. Modern Servers](https://docs.google.com/document/d/1B_K-BY-mZZTPvLNW9X4vl9Fnh6rmQ2_fOjTVenQ0lHI/edit) — the source conversation this spec builds on
- [NVIDIA: GDS support for NVMe using Linux PCI P2PDMA](https://docs.nvidia.com/deploy/gds-support-for-nvme/latest/pdf/GDS_Support_for_NVME.pdf) — kernel and driver requirements, x86-64 scope
- [NVIDIA: GPUDirect Storage release notes](https://docs.nvidia.com/gpudirect-storage/release-notes/index.html) — NVMe P2PDMA mode
- [NVIDIA: NVMe P2PDMA troubleshooting](https://docs.nvidia.com/gpudirect-storage/troubleshooting-guide/nvme-p2pdma-troubleshooting.html)
- [Phoronix: Nova driver status at LPC 2026](https://www.phoronix.com/news/Nova-Driver-LPC-2026)
- [Rust for Linux: Nova GPU driver](https://rust-for-linux.com/nova-gpu-driver)

Requirements, targets and the crate design are this spec's own proposals. No performance figure here has been measured.
