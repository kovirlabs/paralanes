# paralanes

Umbrella repo for the paralanes project: Rust software that gives commodity
hardware two mainframe traits. Transaction work runs as an assembly line of
pinned core stages, and GPUs act as a channel subsystem that moves and
transforms bulk data so the stage cores never touch it.

This repo holds the spec, the build plan, the measurements atlas, the network
join between machines, and the appliance image. The libraries live in sibling
repos (see Repo map).

Read `docs/PLAN.md` and `docs/SPEC.md` before any change, and
`docs/machines.md` for the hardware. The plan sets
milestone order and which machine does what. The spec is the technical source
of truth. Where they disagree, the plan is current; note the conflict in your
status file.

> DRAFT. Items marked TBD are open decisions for Evan.

## Repo map

| Repo | Role | Depends on | Licence |
| --- | --- | --- | --- |
| `paralanes` (this repo) | Spec, plan, atlas, network join, appliance image, all status files | All three below | GPL-3.0-only |
| `bytelanes` | Bulk data ops on any GPU through wgpu, each with a CPU reference. Owns the op format, which is the contract between all repos | Nothing above it | Apache-2.0 |
| `stagelanes` | Topology-aware stage runtime for Windows and Linux: pinning, rings, pinned memory, stage placement | Nothing above it | Apache-2.0 |
| `diagsteno` | The Diagnostic Stenographer: strain gauge and accelerometer signal processing, the demo workload | `bytelanes`, `stagelanes` | GPL-3.0-only |

Sibling repos are expected as checkouts next to this one (`../bytelanes`,
`../stagelanes`, `../diagsteno`). Use path dependencies during development;
switch to published versions only when Evan says so.

## What lives here

```
paralanes/
  CLAUDE.md
  docs/
    SPEC.md            technical spec
    machines.md        hardware inventory per machine
    PLAN.md            milestones, machines, gates
    landscape.md       M0 check of existing projects (go / no-go per repo)
    status/M<n>.md     one status file per milestone, all repos
  atlas/
    README.md          method: how every figure is measured and reported
    runs/<machine>/    raw benchmark output, one file per run
    crossover.md       published CPU-versus-GPU crossover sizes per op
  crates/
    paralanes-chand/   channel daemon on a GPU machine (spec: cf-chand)
    paralanes-net/     network backend with a Transport trait, TCP first (spec: cf-backend-net)
    paralanes-sched/   scheduler: batching, placement across paths, CPU fallback, failover (spec: cf-sched)
    paralanes-init/    PID 1 for the appliance image (spec: cf-init)
  image/               kernel config and unified kernel image build scripts
```

Milestones owned by this repo: M0 (setup and landscape check), M5 (atlas v1),
M9 (join and failover), M10 (appliance image). Other milestones only write
their status file here.

## Which agent are you

Three agents work in parallel, one per machine. Identify your machine first
(`uname -sm`, `lscpu` or `sysctl -a | grep machdep.cpu`, `systeminfo` on
Windows, `vulkaninfo --summary`) and act only as that agent. If the milestone
you were given is not assigned to your machine in `docs/PLAN.md`, stop and say
so.

| Agent | Machine | OS | Role in this repo |
| --- | --- | --- | --- |
| A | Dell PowerEdge R440, 2x Xeon Silver 4214 (12 cores each, 24 total), 160 GB ECC, no GPU | Linux guest on Proxmox, cores pinned, both NUMA nodes exposed | Runs the scheduler and drives M9 tests; builds the appliance image (M10) |
| B | Desktop, Ryzen 7 5700X (8 cores, AM4 Mini-ITX), Radeon RX 7800 XT 16 GB on PCIe 3.0 x16, 1 GbE | Windows | Runs `paralanes-chand`; contributes atlas runs. Record which wgpu backend is used (Vulkan or DX12) |
| C | MacBook Pro, M5 Pro, 24 GB unified memory | macOS, AArch64 | Runs `paralanes-chand`; contributes atlas runs. Benchmark plugged in, lid open, power mode recorded |

`paralanes-chand` must build and pass its tests on Windows and macOS.
`paralanes-init` and `image/` are Linux only.

## Hard rules

- **Agents never merge, tag or publish.** Prepare the pull request, changelog
  and version bump; Evan does the rest.
- **One milestone per session.** Stop at the gate. Do not start the next
  milestone unprompted.
- **The op format belongs to `bytelanes`.** Do not redefine it here. If this
  repo needs a change to it, write it under Requests in your status file and
  stop that task.
- **No unmeasured numbers.** A performance figure appears in a README, the
  atlas or a status file only with the run file that produced it. Every run
  file records machine, OS, kernel or build number, GPU driver version, wgpu
  backend, GPU host link (PCIe generation and width, or unified memory),
  network link type and speed for runs across machines, Rust version and the
  exact command.
- **A negative result is a result.** If the GPU loses to the CPU at every
  size, or topology-aware placement shows no gain, publish that plainly.
- **Results crossing the network are verified.** Every result returned by
  `paralanes-chand` carries a CRC that the receiver checks.
- **Measure, don't hard-code.** Thresholds, placement and link costs come
  from atlas run files and live measurements, never constants in code, so
  new hardware is used without code changes (spec R9).
- **Channel programs are idempotent.** The scheduler may resubmit once on
  another path after a failure; zero lost and zero duplicated completions is
  the M9 gate.
- **Data rule.** Use generated data or data Evan confirms he owns. Never
  commit machine data from an employer or customer.
- **`unsafe`** only in `paralanes-init` where a syscall wrapper needs it.
  Every block gets a `// SAFETY:` comment.
- **No `#[cfg(target_os)]`** outside a module named `platform` in each crate.
- **Look up current docs** before using wgpu, tokio, io_uring or any
  networking crate API. Do not write them from memory.
- **Do not write NVIDIA or CUDA code.** That path is deferred until NVIDIA
  hardware exists.
- **Licence compatibility.** This repo is GPL-3.0-only. Check the licence of
  every new dependency before adding it: it must be GPL-3.0 compatible (MIT,
  Apache-2.0, BSD, ISC, Zlib, MPL-2.0, LGPL/GPL-3.0 are fine; GPL-2.0-only,
  SSPL, BUSL and proprietary licences are not).
- Crates carry `publish = false` in the workspace until Evan removes it.

## Status files

Each milestone writes `docs/status/M<n>.md` in this repo, whichever repo the
work was in. Use this shape:

```markdown
# M<n>: <title>
Agent: <A|B|C>   Machine: <name>   Date: <YYYY-MM-DD>
## Built
## Gate results
<check> | pass/fail | command that produced it
## Open questions for Evan
## Requests for other agents
```

At the start of every session: rebase your branch on `main`, then read every
file in `docs/status/`.

## Branches

- `main` is Evan's. Never push to it.
- Work on `m<n>-<short-topic>` (for example `m9-failover`). One branch per
  milestone per repo.
- Small commits, one logical change each. Run fmt, clippy and tests before
  each commit.

## Commands

```
cargo fmt --all
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cargo test -p paralanes-chand            # Windows and macOS must pass
cargo build -p paralanes-init --target x86_64-unknown-linux-musl   # agent A only
```

Op benchmark runs for the atlas use `bytelanes-bench`; write its output to
`atlas/runs/<machine>/<YYYY-MM-DD>-<op>.json`. Pipeline runs use
`stagelanes-bench` and write `atlas/runs/<machine>/<YYYY-MM-DD>-pipeline.json`.
Do not edit run files after they are committed.

## Open decisions (TBD)

- GitHub owner for the four repos
- Remaining hardware details in `docs/machines.md`: Dell network and PCIe
  slots; MacBook network; the switch
- Size of agent A's Proxmox guest (spec assumes 20 cores, 10 per socket)
