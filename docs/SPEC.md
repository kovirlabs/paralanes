# Paralanes: Rust GPU Channel Subsystem — Spec and Claude Code Plan

Nov 21, 2026 · @Evan Gress

Paralanes is a Rust system layer that gives commodity hardware two mainframe traits: transaction cores organised as an assembly line, and a GPU acting as a channel subsystem for bulk data. It is proven on three machines, each developed by its own Claude Code agent and then joined over the network: a dual-Xeon Dell server for the assembly line, a Ryzen desktop with an AMD GPU as a discrete-GPU channel processor, and an M5 Pro MacBook Pro as a unified-memory channel processor.

## Names and repos

The project was drafted as Channelforge and is now **paralanes**. The work ships as four repos, and the `cf-` crate names used below are placeholders that map onto them.

| Repo | Role | Crates from this spec |
| --- | --- | --- |
| `paralanes` | Umbrella: spec, plan, measurements, network join, appliance image | `cf-chand`, `cf-backend-net`, `cf-sched`, `cf-init`, `image/` |
| `bytelanes` | Bulk data ops on any GPU, with a CPU reference | `cf-abi`, `cf-backend`, `cf-backend-wgpu`, `vectors/` |
| `stagelanes` | Stage placement runtime for Windows and Linux | `cf-ring`, `cf-mem`, `cf-arch`, `cf-pipe` |
| `diagsteno` | Diagnostic Stenographer: strain gauge and accelerometer signal processing; the demo workload in place of the ledger | Replaces `cf-txn`; uses `cf-store` |

The [Paralanes build plan](https://claude.ai/artifact/MbvLHC2yjx27zdoMiUHAjj) sets the milestone order and says which machine does what. Where it differs from the phase tables here, the plan is current: in particular, the stage pipeline is now built and measured on the Ryzen desktop first, with the Dell as the two-socket check.

## Feasibility

The idea works if GPUs take the I/O and bulk-data role, not the transaction role. Four corrections to the literal request shape the design:

| Literal ask | What is true | Design consequence |
| --- | --- | --- |
| GPUs do mainframe computing | GPUs are poor at branchy, conditional transaction logic; the source chat says the same | CPU cores keep the transaction logic. GPUs become channel processors: they read, decompress, checksum, decrypt, filter and score data in bulk |
| Rust firmware | No bare-metal GPU compute stack exists. Vendor compute needs an OS kernel driver. The Rust NVIDIA driver (Nova) boots the GPU's system processor on Turing through Blackwell but is not ready for end users | "Firmware" means an appliance image: UEFI boots a minimal Linux kernel whose only userspace is one Rust PID 1. It flashes and boots like firmware |
| GPU loads data into CPU caches | A GPU cannot write CPU caches (the source chat is right) | Results land in pinned hugepages that are NUMA-local to the requesting core; the core prefetches them |
| Mainframe level | The I/O offload property is reachable. Hardware seven-nines RAS is not | Target: transaction cores run at 90% load without latency collapse. Reliability comes from redundant GPU paths and checksums, in software |

The direct NVMe-to-GPU path exists today on NVIDIA: GPUDirect Storage has an NVMe P2PDMA mode on the upstream Linux NVMe driver. It needs a kernel newer than 6.2 and the open NVIDIA kernel driver 565.47 or later, and NVIDIA's guide lists it for x86-64 only. AArch64 therefore builds and runs from day one but uses a bounce-buffer path until that changes. The three-machine plan defers this direct path, because none of the machines has an NVIDIA GPU.

## Architecture

&#91;embedded content: data path · 2 control layers, 3 data-plane parts\]

A core pushes a channel program onto its ring and keeps working. Bulk bytes travel from NVMe to a GPU to pinned memory and never pass through the core; the core receives a pointer to the result.

## Three-machine plan

Each machine proves one part of the design alone; a network backend then joins them to prove the whole.

| Aspect | Dell server (track A) | Ryzen desktop (track B) | MacBook Pro (track C) |
| --- | --- | --- | --- |
| Hardware | Two Xeon Silver CPUs, 10 cores each; no GPU | Ryzen 7 on an AM5 board, one Radeon RX 570 | Apple M5 Pro, 24 GB unified memory |
| Target | x86-64 Linux guest on Proxmox | x86-64 Windows | AArch64 macOS |
| Role | Assembly line: stage pipeline, NVMe log, CPU reference backend | Channel processor with separate video memory: Vulkan backend | Channel processor with unified memory: the same backend through Metal |
| Proves alone | Latency holds at 90% core load with no GPU (R2); durability (R8) | GPU output equals the CPU reference (R7); the input size where the GPU starts to win | A native AArch64 run (R4); R7 on a second GPU vendor; what the ops cost when the copy to the GPU disappears |
| Proves joined | Schedules work across two GPU paths; fails over between them and then to the CPU (R3) | Serves channel programs to the Dell | Serves channel programs to the Dell |

The assembly line on the Dell gives each group of cores one stage and nothing else:

1. **Ingest:** parse and validate requests.
2. **Sequence:** one core assigns a global order, so later stages need no locks.
3. **Execute:** each core owns one partition of the data and runs its transactions serially.
4. **Log:** batch records to NVMe and flush.
5. **Reply:** send results.

A starting split of the 20 cores, to be tuned with `cf-bench`: 3 ingest, 1 sequence, 10 execute (5 per socket), 2 log, 2 reply, 2 for the OS and the channel scheduler. Stages hand off pointers in batches over `cf-ring` queues, and a partition's memory lives on the same socket as the core that owns it.

When joined, an I/O core on the Dell reads from NVMe and sends the bytes to a GPU machine; transaction cores still never touch payload. Two things stay unproven until new hardware arrives: the direct NVMe-to-VRAM path, and offload at PCIe speed rather than network speed. The Mac narrows the first gap by showing GPU work without the copy into video memory.

### Three agents

Each machine runs its own Claude Code agent on its own branch, and each agent may edit only what it owns.

| Agent | Machine | Branch | Owns |
| --- | --- | --- | --- |
| A | Dell | `track-a` | `cf-pipe`, `cf-store`, `cf-txn`, `cf-init`, `image/` |
| B | Ryzen | `track-b` | `cf-backend-wgpu` ops and shaders, `cf-chand` |
| C | MacBook | `track-c` | macOS support in `cf-mem` and `cf-arch`; the unified-memory buffer strategy in `cf-backend-wgpu` |

Rules that keep three agents from colliding:

- **S0 runs once.** One agent builds the shared phase and it is merged to `main` before the other two start.
- **The contract is frozen.** After S0, the shared crates (`cf-abi`, `cf-ring`, `cf-mem`, `cf-arch`, `cf-backend`, `cf-bench`, `vectors/`) change only on a `contract/<topic>` branch that passes tests on all three machines. You merge it; agents do not.
- **One seam in the shared GPU crate.** Agent B owns the ops and WGSL shaders. Agent C owns one module behind a `BufferStrategy` trait and does not edit shaders.
- **Golden vectors.** The CPU reference backend writes inputs and expected outputs to `vectors/`; both GPU machines must reproduce them exactly.
- **Separate status files.** Each agent writes only `docs/status/<track>.md` and `docs/bench/<track>.md`, so these never conflict. A request to another agent goes under a Requests heading in the writer's own status file.
- **Rebase at every phase start**, then read the other two status files before working.

## Requirements and non-goals

The numbers below are proposed targets, not measurements.

| ID | Requirement | How it is checked |
| --- | --- | --- |
| R1 | Transaction cores never copy bulk payload on the hot path | `cf-bench` counts payload bytes touched by CPU per transaction; it must not exceed the result size |
| R2 | Latency holds at high CPU load | p99 at 90% core utilisation is at most 3x p99 at 50% |
| R3 | The GPU path can fail without losing work | Take the GPU machine offline mid-load: work falls back to the CPU backend with zero lost and zero duplicated completions |
| R4 | One source tree for x86-64 and AArch64 | Builds for `x86_64-unknown-linux-musl` and `aarch64-unknown-linux-musl`; the channel side also runs natively on AArch64 macOS; arch code lives in one crate |
| R5 | Boots as an appliance | UEFI to serving in under 10 s, read-only root, A/B image update |
| R6 | GPU vendor is swappable | One `ChannelBackend` trait; CUDA, Vulkan (wgpu) and CPU backends |
| R7 | GPU results are trustworthy | Every op has a CPU reference; GPU output matches it byte for byte in property tests |
| R8 | Durability does not depend on a GPU | A write is acknowledged only after the NVMe flush of its log record; data leaving VRAM is checksum-verified |

Non-goals:

- s390x, z/OS or Hercules compatibility
- Running transaction logic on GPUs
- Writing a GPU driver or a `no_std` kernel from scratch
- Hardware-level RAS (lockstep cores, hot-swap CPUs)
- CXL memory pooling in v1; the memory crate leaves room for it

## Workspace layout

One Cargo workspace. The Track column says which machine exercises each crate; `unsafe` is allowed only where marked.

| Crate | Track | Kind | Owns | `unsafe` |
| --- | --- | --- | --- | --- |
| `cf-abi` | Shared | `no_std` lib | Channel program wire format, status codes, zero-copy structs | no |
| `cf-ring` | Shared | `no_std` lib | Lock-free rings: inter-stage queues and submission/completion pairs | yes |
| `cf-mem` | Shared | lib | Pinned memory pools; hugepages and NUMA placement on Linux, page-locked memory on macOS | yes |
| `cf-arch` | Shared | lib | All x86-64 and AArch64 specifics: core pinning, prefetch, cycle counters, CRC and AES intrinsics | yes |
| `cf-backend` | Shared | lib | `ChannelBackend` trait and the CPU reference backend | no |
| `cf-bench` | Shared | bin | Load generator, latency histograms, bytes-touched counter | no |
| `vectors/` | Shared | data | Golden inputs and expected outputs per op | n/a |
| `cf-pipe` | A | lib | The five-stage pipeline: stage threads, core and socket plan, batched handoff | no |
| `cf-store` | A | lib | NVMe access through io\_uring with `O_DIRECT`; write-ahead log | no |
| `cf-txn` | A | lib | Demo OLTP engine: a partitioned ledger with transfers | no |
| `cf-backend-wgpu` | B, C | lib, feature `wgpu` | Compute shaders in WGSL, run through Vulkan on the Ryzen and Metal on the Mac; a `BufferStrategy` trait with copying and unified-memory implementations | no |
| `cf-chand` | B, C | bin | Channel daemon on a GPU machine: receives programs, runs them on the local backend, returns results | no |
| `cf-backend-net` | Join | lib | Backend that submits channel programs to a `cf-chand` through a Transport trait: TCP first, RDMA later | no |
| `cf-sched` | Join | lib | Scheduler: batching, placement across GPU paths, CPU fallback below the size threshold, path health and failover | no |
| `cf-init`, `image/` | A | bin, build scripts | PID 1 and the appliance image | no |
| `cf-backend-cuda` | Deferred | lib, feature `cuda` | Direct NVMe-to-VRAM path; needs NVIDIA hardware | FFI only |

Track A crates use io\_uring, hugepages and NUMA calls and are built and tested on Linux only.

## Channel program interface

A core describes the data work as a short chain of commands, submits it, and moves on. This mirrors the mainframe's channel command words and its Start Subchannel instruction.

```rust
// cf-abi: no_std, #[repr(C)], zero-copy

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
}

#[repr(C, u8)]
pub enum Extent {
    None,
    Nvme { dev: DevId, lba: u64, blocks: u32 },
    Vram { buf: BufId },          // stays on the GPU
    Host { buf: HostBuf },        // pinned hugepage, NUMA-local
}

/// At most 16 commands. Intermediate extents must be Vram.
pub struct ChannelProgram { /* fixed-size array + len */ }

pub struct Completion { pub tag: Tag, pub status: Status, pub bytes: u32, pub crc: u32 }
```

```rust
// cf-backend
pub trait ChannelBackend: Send + Sync {
    fn capabilities(&self) -> Caps;           // ops supported, direct NVMe path or bounce
    fn submit(&self, prog: &ChannelProgram, tag: Tag) -> Result<(), Busy>;
    fn poll(&self, out: &mut [Completion]) -> usize;
    fn health(&self) -> PathState;            // Online, Degraded, Offline
}
```

```rust
// what a transaction core does
let tag = chan.start(&prog)?;      // ring push; no syscall, no lock, no allocation
// ... run other transactions ...
for c in chan.completions() {      // ring pop; busy-poll, no interrupt
    let row = c.host_buf();        // only the result bytes ever reach this core
}
```

Rules the scheduler enforces:

1. A whole program runs on one GPU; only the final `dst` may be `Host`.
2. Programs are batched per GPU up to a time or size limit, then dispatched together.
3. Work below a size threshold runs on the CPU reference backend, because a GPU round trip costs more than it saves on small inputs.
4. A program whose GPU goes `Offline` is resubmitted once on another path; programs are idempotent by construction, so a retry is safe.
5. Every `Host` result carries a CRC computed on the GPU and verified on the core.

## Phases

One shared phase, then three independent tracks run in parallel by three agents, then the join.

| Phase | Machine | Builds | Acceptance gate |
| --- | --- | --- | --- |
| S0. Shared contract | Dell | Workspace, `cf-abi`, `cf-ring`, `cf-mem`, CPU backend with Checksum, Filter, Decrypt; `cf-bench`; golden vectors | Tests pass; rings pass loom and miri; `vectors/` committed; merged to `main` |
| A1. Assembly line | Dell | `cf-pipe` with five stages pinned across both sockets; in-memory ledger | R2 passes; a test allocator and lock counter show zero allocations and zero locks on stage cores after startup |
| A2. Durability | Dell | `cf-store` write-ahead log on NVMe; `cf-txn` ledger | R8 passes; balances sum to zero after a kill-and-restart test |
| B1. GPU ops | Ryzen | `cf-backend-wgpu` with the three ops and the copying buffer strategy | Reproduces `vectors/` exactly, plus 10,000 property-test cases per op |
| B2. Crossover | Ryzen | GPU versus CPU benchmark per op across input sizes | Crossover size per op recorded in `docs/bench/b.md` |
| C1. Port | MacBook | macOS support in `cf-mem` and `cf-arch`; shared crates and `cf-backend-wgpu` running on Metal | S0 tests pass natively on AArch64; once B1 is merged, reproduces `vectors/` exactly |
| C2. Unified memory | MacBook | A `BufferStrategy` that avoids the copy to the GPU where current wgpu and Metal allow it; benchmark against the copying strategy | Copy versus no-copy cost and crossover size per op recorded in `docs/bench/c.md`. If current wgpu offers no zero-copy route, the status file says so with the evidence, and the gate still passes |
| J1. Join | All three | `cf-chand` on the Ryzen and the Mac; `cf-backend-net` and `cf-sched` on the Dell | The Dell pipeline completes channel programs on both GPU machines; every result passes its CRC check |
| J2. Failover | All three | Path health, resubmission, placement across two paths | R3 passes: unplug one GPU machine and work moves to the other; unplug both and it moves to the CPU; zero lost, zero duplicated |
| P. Appliance | Dell | `cf-init` as PID 1, kernel config, unified kernel image | R5 passes |
| Later | New hardware | Direct NVMe-to-VRAM path on NVIDIA, several GPUs in one box, RDMA links at 25 or 100 GbE between nodes, the `Score` op | Defined when started |

C1 depends on B1 only for its last gate item, so agent C starts the port as soon as S0 is merged.

Track A needs dedicated cores. The Dell runs Proxmox, so track A runs in a Linux guest with the host CPU type, cores pinned one-to-one, both NUMA nodes exposed and hugepage backing, with other guests stopped during benchmarks. On shared cores the R2 result means nothing. A2 needs a drive passed through to the guest.

## Claude Code setup

Three files drive the build: this spec exported as `docs/SPEC.md`, the `CLAUDE.md` below at the repo root, and a kickoff prompt per phase.

1. Create an empty repo and export this doc to Markdown as `docs/SPEC.md`.
2. Save the block below as `CLAUDE.md`.
3. Start Claude Code in the repo and paste the Phase S0 prompt.
4. At each gate, read `docs/status/`, answer its questions, then paste the next-phase prompt.

```markdown
# Paralanes

Rust system layer: transaction cores as an assembly line, a GPU as a
mainframe-style channel subsystem. Read docs/SPEC.md before any change.
It is the source of truth.

## Which agent are you
Three agents work in parallel, one per machine. Identify your machine first
(uname -sm, lscpu or sysctl, vulkaninfo) and act only as that agent.
- Agent A, Dell: dual Xeon Silver, 20 cores, two sockets, Linux guest on Proxmox, no GPU. Runs Phase S0.
  Branch track-a. Owns cf-pipe, cf-store, cf-txn, cf-init, image/.
- Agent B, Ryzen 7 desktop (AM5) with a Radeon RX 570, Windows. Branch track-b. Record which wgpu backend is used (Vulkan or DX12).
  Owns cf-backend-wgpu ops and WGSL shaders, cf-chand.
- Agent C, MacBook Pro M5 Pro, 24 GB, macOS AArch64. Branch track-c.
  Owns macOS support in cf-mem and cf-arch, and the unified-memory
  BufferStrategy module in cf-backend-wgpu. Does not edit shaders.

## Ownership rules
- Edit only what you own. Never push to main or to another track's branch.
- Shared crates (cf-abi, cf-ring, cf-mem, cf-arch, cf-backend, cf-bench,
  vectors/) are frozen after S0. If you need a change, describe it under
  Requests in your status file and stop that task; Evan opens a contract/
  branch. Agent C's macOS work in cf-mem and cf-arch is the one exception
  and must not change Linux behaviour.
- Write only docs/status/<your track>.md and docs/bench/<your track>.md.
- At the start of every phase: rebase on main, then read the other two
  status files.

## Hard rules
- Transaction cores never copy bulk payload. A new payload memcpy on the hot
  path needs a justifying comment and a cf-bench run.
- Every Op has a CPU reference in cf-backend::cpu. GPU backends must match it
  byte for byte: the files in vectors/ plus proptest.
- Stage cores take no locks and do not allocate or make syscalls after
  startup. Stages hand off pointers in batches over cf-ring.
- A data partition is owned by one execute core and its memory stays on that
  core's socket.
- `unsafe` only in cf-ring, cf-mem, cf-arch and backend FFI. Every block gets
  a `// SAFETY:` comment.
- No `#[cfg(target_arch)]` or `#[cfg(target_os)]` outside cf-arch and cf-mem.
- cf-abi and cf-ring stay `#![no_std]`.
- Never state a performance number that cf-bench did not measure on real
  hardware. Log runs in your bench file with machine, OS and driver versions.
  On the MacBook: plugged in, lid open, note the power mode.
- Look up current docs before using wgpu, Metal-related wgpu features or
  io_uring APIs. Do not write them from memory.
- Do not write cf-backend-cuda. It is deferred until NVIDIA hardware exists.

## Commands
- cargo fmt --all && cargo clippy --workspace --all-targets -- -D warnings
- cargo test --workspace                     # all machines
- cargo test --workspace --features wgpu     # Ryzen and MacBook
- cargo +nightly miri test -p cf-ring -p cf-abi
- RUSTFLAGS="--cfg loom" cargo test -p cf-ring --release

## Workflow
- One phase at a time. Do not start a phase until its predecessor's gate
  passes: S0 before A1, B1 and C1; A2, B2 and C2 before J1.
- If a step needs another machine, write the code and tests, mark them
  `#[ignore = "needs: <machine>"]`, note it in your status file and continue.
- Small commits, one logical change each. Run fmt, clippy and tests before each.
- Stop at the phase gate. Do not begin the next phase unprompted.
```

Phase S0 kickoff prompt, run once by one agent on a Linux machine and merged to main:

```text
Read CLAUDE.md and docs/SPEC.md. Execute Phase S0 only.

1. Create the Cargo workspace with every crate in the layout table. Crates
   owned by later phases get a lib.rs with a doc comment and nothing else.
2. Implement cf-abi, cf-ring, cf-mem, the CPU reference backend (Checksum,
   Filter, Decrypt) and cf-bench.
3. Generate the golden vectors into vectors/ from the CPU backend and add a
   test that replays them.
4. Write the S0 gate tests: unit and property tests, loom for cf-ring, miri
   for cf-ring and cf-abi.
5. Add CI that runs the commands in CLAUDE.md that need no GPU.
6. Update docs/status/s0.md, then stop. List every decision you need from me
   before A1, B1 and C1. Write no pipeline or GPU code.
```

Track prompt, the same shape for every later phase:

```text
Read CLAUDE.md and docs/SPEC.md. Identify which agent you are. Rebase your
track branch on main and read the other two status files. Execute Phase
<A1 | A2 | B1 | B2 | C1 | C2 | J1 | J2 | P> only. Stop at the gate and
update your status file.
```

For J1 and J2, run all three agents: B and C each build and run `cf-chand` on their machine, and A builds `cf-backend-net` and `cf-sched` and drives the test from the Dell.

## Risks and open questions

Decisions needed:

- [ ] Which workload defines "mainframe level" for you: the ledger demo here, or a real workload you have in mind?
- [ ] Is faster networking between nodes in scope? If so, 25 GbE cards with RDMA support are the smallest step that changes the J1 result.

Decided on 21 Nov 2026:

- The Dell runs the shared phase S0.
- The Dell runs Proxmox; track A runs in a pinned Linux guest.
- The Ryzen runs Windows; `cf-chand` and `cf-backend-wgpu` must build and test there.
- The MacBook is tested on both wired Ethernet and Wi-Fi; J1 records each separately.

Risks:

| Risk | Effect | Mitigation |
| --- | --- | --- |
| Three agents edit one repo | Merge conflicts and a drifting contract; J1 fails on integration | Branch and ownership per agent; shared crates frozen after S0; contract changes pass on all three machines; golden vectors |
| The join runs over the network, not PCIe | J1 proves the design end to end but its throughput says little about a single-box build | Report J1 as a correctness and failover result; take performance claims from A1, B2 and C2 separately |
| Current wgpu may not expose zero-copy buffers on Metal | C2 cannot show the unified-memory gain | C2 gate accepts a documented negative result; a Metal-native strategy can follow as a contract change |
| The MacBook is a laptop | Thermal throttling, sleep and battery mode make benchmarks noisy; it may be absent during J2 | Benchmark plugged in with the power mode recorded; the scheduler treats an absent Mac as a failed path, which is itself the R3 test |
| 24 GB is shared by CPU and GPU on the Mac | Large buffer pools starve the system | Cap pools in `cf-mem`; record pool size with each run |
| macOS has no hugepages, NUMA control or io\_uring | Track A cannot run on the Mac | The Mac covers the channel side only; the pipeline stays on the Dell |
| Stage handoffs cost a cache miss each | The pipeline is slower than one core doing everything | Few stages, batched handoffs, pointers only; A1 compares against a single-core baseline |
| Two sockets on the Dell | Cross-socket memory access erases the gain | Partition memory per socket; `cf-bench` reports remote-memory access counts |
| GPU round trip is slower than the CPU for small inputs | Latency gets worse, not better | B2 and C2 measure the crossover; the scheduler falls back to CPU below it |
| Consumer GPUs have no ECC memory | Silent corruption in results | CRC on every result leaving the GPU (rule 5) |
| The Dell's 1U chassis limits GPU options | A later single-box build may need different hardware | Check riser and power specifications before buying a GPU for it |

## Sources

- [Chat Summary: Mainframes vs. Modern Servers](https://docs.google.com/document/d/1B_K-BY-mZZTPvLNW9X4vl9Fnh6rmQ2_fOjTVenQ0lHI/edit) — the source conversation this spec builds on
- [NVIDIA: GDS support for NVMe using Linux PCI P2PDMA](https://docs.nvidia.com/deploy/gds-support-for-nvme/latest/pdf/GDS_Support_for_NVME.pdf) — kernel and driver requirements, x86-64 scope
- [NVIDIA: GPUDirect Storage release notes](https://docs.nvidia.com/gpudirect-storage/release-notes/index.html) — NVMe P2PDMA mode
- [NVIDIA: NVMe P2PDMA troubleshooting](https://docs.nvidia.com/gpudirect-storage/troubleshooting-guide/nvme-p2pdma-troubleshooting.html)
- [Phoronix: Nova driver status at LPC 2026](https://www.phoronix.com/news/Nova-Driver-LPC-2026)
- [Rust for Linux: Nova GPU driver](https://rust-for-linux.com/nova-gpu-driver)

Requirements, targets and the crate design are this spec's own proposals. No performance figure here has been measured.
