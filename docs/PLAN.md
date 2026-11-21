# Paralanes build plan: four repos, three agents

Nov 21, 2026 · @Evan Gress

The spec is split into four small open-source repos that are built in order and published one at a time, so there is something usable after the first few milestones instead of only at the end. The project is renamed **paralanes**, because 17 GitHub repos already use its working name, Channelforge.

## The four repos

| Repo | What it is | Gap it fills | Spec crates it absorbs |
| --- | --- | --- | --- |
| `bytelanes` | Rust library of bulk data operations (checksum, filter, decrypt; later parity and FFT) that run on any GPU through wgpu. Every op has a CPU reference and a measured size below which the CPU is used | Vendor-neutral GPU data ops. Today this exists almost only for NVIDIA | `cf-abi`, `cf-backend`, `cf-backend-wgpu`, `vectors/` |
| `stagelanes` | Rust runtime that reads each core's cache size, top frequency and socket, then pins pipeline stages to the cores that suit them, with lock-free batched handoff between stages. Runs on Windows and Linux | Topology-aware stage placement. Thread-per-core runtimes exist; none known that place stages by core type | `cf-ring`, `cf-mem`, `cf-arch`, `cf-pipe` |
| `diagsteno` | The Diagnostic Stenographer: scientific signal processing for strain gauge and accelerometer data. Ingest, FFT and band features on the GPU through `bytelanes`, anomaly scoring, storage, a query API. Stages run on `stagelanes` | Open, high-rate sensor data processing at the edge. Replaces the ledger as the demo workload | Replaces `cf-txn`; uses `cf-store` |
| `paralanes` | The umbrella: the spec, this plan, the network join (`cf-chand`, `cf-backend-net`, `cf-sched`), the appliance image, and the **atlas** of published CPU-versus-GPU crossover measurements | Published measurements; end-to-end proof | `cf-chand`, `cf-backend-net`, `cf-sched`, `cf-init`, `image/`, `docs/bench` |

A fifth name, `sievewarden`, is set aside for the SIEM analysis engine if you take that on later. It is not part of this plan.

The gap claims above are my judgement from memory. Milestone M0 includes a proper landscape check for each before code is written.

## Names

You chose these four on 21 Nov 2026. Each returned zero GitHub repositories with the name in their title and no crate on crates.io that day.

| Name | For | What the name says | GitHub repos with this name | crates.io |
| --- | --- | --- | --- | --- |
| `paralanes` | Umbrella | Parallel lanes: many streams of work side by side | 0 | Free |
| `stagelanes` | Stage placement runtime | Each pipeline stage gets its own lane of cores | 0 | Free |
| `bytelanes` | GPU data ops library | Bytes moving down lanes | 0 | Free |
| `diagsteno` | Strain and accelerometer processing | Diagnostic Stenographer, shortened for typing | 0 | Free |

A web search for `paralanes` and `stagelanes` found no project, product or company using either. `bytelanes` and `diagsteno` have not had a web search yet; M0 does it.

Free spares: `bulkchomp` for the bytes library, `signalsteno` or `diagnostic-stenographer` for the signal repo, and `sievewarden` held for a SIEM engine.

Rejected after checking:

| Name | Why |
| --- | --- |
| `channelforge` | 17 GitHub repos already use it |
| `bytelane` (singular) | One existing repo; use the plural |
| `byteflop` | An existing GitHub organisation and repo; "flop" also reads as failure |
| `bytechomp` | An existing project with 53 stars |
| `bytehaul` | 5 repos and an existing crate |
| `strainlog`, `accelog`, `vibrolog` | 2 to 5 existing repos each |
| `bytehone`, `gangmill`, `spindlescribe` | Free, but the words did not carry the right image |

Limits of the check: GitHub's search counts repos with the word in the repo name only; trademarks, domain names, npm and PyPI were not checked. A name is only yours once the repo exists and the first crate version is published, so M0 and M1 do that first.

## Milestones

Eleven milestones, each one Claude Code session on one machine, ending at a gate you review. Three of them end in a public release.

| # | Milestone | Repo | Machine and agent | Gate | Can run alongside |
| --- | --- | --- | --- | --- | --- |
| M0 | Claim names, set up repos, landscape check | All | You, then any agent | Four repos exist with licence, `CLAUDE.md`, the spec and this plan; a short written check of existing projects for each gap, with a go or no-go per repo | Nothing |
| M1 | Contract and CPU reference: op format, Checksum, Filter, Decrypt on CPU, golden vectors | `bytelanes` | Dell, agent A | Tests pass; miri clean; `vectors/` committed; version 0.0.1 published to hold the crate name | Nothing |
| M2 | Rings, pinned memory, topology detection, on Windows and Linux | `stagelanes` | Ryzen, agent B builds; Dell, agent A checks Linux and two sockets | Loom and miri clean; a `stagelanes topology` command prints the correct sockets, cache sizes and core classes on all three machines; 0.0.1 published | M4 |
| M3 | GPU ops on Windows with the Radeon RX 7800 XT | `bytelanes` | Ryzen, agent B | Reproduces `vectors/` exactly plus 10,000 property-test cases per op | M4 |
| M4 | GPU ops on Metal, plus a unified-memory buffer strategy | `bytelanes` | MacBook, agent C | Reproduces `vectors/` natively on AArch64; zero-copy result or a documented reason it is not possible in current wgpu | M2, M3 |
| M5 | Atlas v1: crossover size per op per machine, with method and raw data | `paralanes` | Ryzen and MacBook, agents B and C | Every number traces to a logged `bytelanes` benchmark run. **Release 1: `bytelanes` 0.1 and the atlas** | Nothing on the Ryzen |
| M6 | Stage pipeline: stages placed by topology, batched handoff | `stagelanes` | Ryzen, agent B builds and measures on bare metal; Dell, agent A repeats on two sockets | On a synthetic workload, p99 latency at 90% core load is at most 3x p99 at 50% on the Ryzen; the Dell run records the gain or loss from keeping each stage's memory on its own socket. **Release 2: `stagelanes` 0.1** | M4 |
| M7 | Signal ingest and features: stream ingest, an FFT op added to `bytelanes`, band features, storage | `diagsteno`, `bytelanes` | Dell, agent A; FFT op checked by B and C | A recorded synthetic strain and vibration dataset replays with features equal to the CPU reference; ingest holds the target channel count and sample rate with no dropped samples | Nothing |
| M8 | Scoring and query: anomaly score per channel, HTTP query API, export to an existing time-series database | `diagsteno` | Dell, agent A | Injected faults in the synthetic data are flagged; queries return stored features. **Release 3: `diagsteno` 0.1** | M9 |
| M9 | Join and failover: channel daemon on both GPU machines, network backend, scheduler | `paralanes` | All three | The Dell completes work on both GPU machines; unplug one and work moves to the other; unplug both and it moves to the CPU; zero lost, zero duplicated | M8 |
| M10 | Appliance image: Rust PID 1, minimal kernel, unified kernel image | `paralanes` | Dell, agent A | Boots in QEMU and runs `diagsteno` | Nothing |

The Ryzen desktop (Ryzen 7 5700X on an AM4 Mini-ITX board, Radeon RX 7800 XT 16 GB on PCIe 3.0 x16, Windows; details in `docs/machines.md`) is the main machine for `stagelanes` because it runs on bare metal, which gives cleaner latency numbers than the Dell's Proxmox guest. Three limits come with it:

- **Windows.** `stagelanes` needs Windows versions of core pinning, large pages and topology detection, and Windows cannot fully fence cores off from the operating system, so p99 figures will be noisier than on tuned Linux.
- **Eight identical cores, one socket.** The Ryzen can prove the pipeline and its latency behaviour, but not placement by core type. The Dell covers placement by socket. No machine in the set has mixed cache-heavy and frequency-heavy cores, so that claim stays untested until one does.
- **One machine, two jobs.** GPU benchmarks (M3, M5) and pipeline benchmarks (M2, M6) must not run at the same time on the Ryzen.

Later, with new hardware or a decision from you: faster links between machines (25 GbE, RDMA or similar), the NVIDIA direct NVMe-to-VRAM path, parity as an op, and `sievewarden`. Faster networking is not near-term but stays a test variable: the scheduler places work by measured link cost, and the M9 tests are re-run unchanged when a faster link arrives. New components are measured and used as they appear, without code changes (spec R9).

The spec stays the technical source of truth. Its earlier phases map onto these milestones: S0 is M1 and M2, B1 and B2 are M3 and M5, C1 and C2 are M4 and M5, A1 is M6, A2 is folded into M7, J1 and J2 are M9, and P is M10. Since the 21 Nov 2026 reconcile pass the spec uses these milestone numbers directly.

## Working method

You start sessions, answer questions and merge. Claude Code does everything between a prompt and a gate.

| Step | You | Claude Code |
| --- | --- | --- |
| 1. Start | Open Claude Code on the machine the milestone names and paste the milestone prompt | Identifies its machine, rebases its branch on `main`, reads the plan, the spec and the status files |
| 2. Build | Nothing; the session runs unattended | Writes code and tests on its own branch in small commits; looks up current library docs before using them |
| 3. Gate | Nothing | Runs the gate checks, writes `docs/status/M<n>.md` with results, open questions and requests to other agents, opens a pull request, then stops |
| 4. Review | Read the status file, answer its questions, merge the pull request if the gate passed | Nothing until the next prompt |
| 5. Release (M5, M6, M8 only) | Tag the version and publish the crate | Prepares the changelog and README; never tags or publishes |

Rules that hold across all four repos:

- **One milestone per session.** An agent that finishes early stops; it does not start the next milestone.
- **Agents never merge, tag or publish.** Those three actions are yours.
- **The op format in `bytelanes` is the contract.** After M1 it changes only through a pull request that passes on all three machines.
- **No unmeasured numbers.** A performance figure appears in a README or the atlas only with the benchmark run that produced it.
- **Synthetic or personally owned data only.** `diagsteno` is developed against generated strain and vibration data. Data from an employer's or customer's machines does not go into a public repo.
- **Parallel sessions touch different repos or different machines.** The last column of the milestone table says which pairs are safe.

If you have little time in a given week, the smallest useful unit is one milestone on one machine: paste one prompt, come back later, read one status file.

## Prompts

M0, run once after you create the four empty repos on GitHub:

```text
You are setting up four related Rust repos: paralanes, bytelanes, stagelanes,
diagsteno. docs/SPEC.md and docs/PLAN.md are in paralanes.

1. In each repo create: README.md (one paragraph, status "pre-alpha"),
   LICENSE files as chosen in docs/PLAN.md, a Cargo workspace or crate
   skeleton, CI that runs fmt, clippy and tests, and a CLAUDE.md.
2. Each CLAUDE.md starts from the one in paralanes and adds: the repo map
   from docs/PLAN.md, this repo's role, and the rule that agents never merge,
   tag or publish.
3. Landscape check: for each of the three gaps (vendor-neutral GPU data ops,
   topology-aware stage placement, open edge processing of strain gauge and
   accelerometer data), search for existing open-source projects. Write
   docs/landscape.md in paralanes: closest projects, what they lack, and a go
   or no-go for each repo. Cite what you read. If something already fills a
   gap, say so plainly.
4. Run a web search on the names bytelanes and diagsteno and report any
   project, product or company already using them.
5. Open one pull request per repo, write docs/status/M0.md, then stop.
```

Milestone prompt, the same shape for M1 to M10:

```text
Read CLAUDE.md, and in paralanes read docs/PLAN.md, docs/SPEC.md and every
file in docs/status/. Identify which machine this is and confirm it is the
one docs/PLAN.md names for milestone M<n>. If it is not, stop and say so.

Rebase your branch on main. Execute milestone M<n> only. Run its gate checks.
Write docs/status/M<n>.md: what was built, gate results with the commands
that produced them, open questions for Evan, and requests for other agents.
Open a pull request and stop. Do not merge, tag, publish or start M<n+1>.
```

Lines to add to each repo's `CLAUDE.md` beyond the umbrella version (done in the M0 scaffold):

```markdown
## Repo map
- paralanes: spec, plan, atlas, network join, appliance image.
- bytelanes: bulk data ops on GPU through wgpu, with CPU reference. Owns the
  op format, which is the contract between all repos.
- stagelanes: topology-aware stage runtime for Windows and Linux. Depends on
  nothing above it.
- diagsteno: the Diagnostic Stenographer. Strain gauge and accelerometer
  signal processing. Depends on bytelanes and stagelanes.

## Release rule
Agents prepare releases (changelog, README, version bump in a pull request).
Only Evan merges, tags and publishes.

## Data rule
Use generated data or data Evan confirms he owns. Never commit machine data
from an employer or customer.
```

## Decisions and risks

Decisions needed before M0:

- [ ] Which GitHub owner holds the repos: your personal account or an organisation?
- [x] Licence. Rust libraries are commonly dual-licensed MIT and Apache-2.0, which suits `bytelanes` and `stagelanes`. Pick the same or a copyleft licence for `diagsteno`. **Decided 21 Nov 2026:** Apache-2.0 for `bytelanes` and `stagelanes` (building-block libraries); GPL-3.0-only for `diagsteno` (end-user application) and `paralanes` (depends on `diagsteno`; GPL-3's installation-information clause covers the appliance image).
- [x] Which Ryzen 7 model is in the desktop, and does the RX 570 have 4 GB or 8 GB? Both go in the atlas with every figure. **Answered 21 Nov 2026:** Ryzen 7 5700X, and the GPU is a Radeon RX 7800 XT 16 GB, not an RX 570. Full details in `docs/machines.md`.
- [ ] For M7: target channel count, sample rate and sensor types for the synthetic strain and vibration data.
- [ ] For M7: storage format. Parquet files are the suggested default, because existing tools read them.

Risks:

| Risk | Effect | Mitigation |
| --- | --- | --- |
| Four repos is a lot for one person's spare time | Work stalls with nothing published | Releases are front-loaded: `bytelanes` and the atlas ship after M5, before the larger pieces start |
| A gap turns out to be filled already | Effort duplicates an existing project | M0 landscape check with a go or no-go per repo, before any code |
| A name is taken between today and M0 | Rename and rework | Create the repos and publish 0.0.1 crates in M0 and M1 |
| The Ryzen 7 5700X has fast built-in AES and CRC instructions, and the A520 board runs the GPU at PCIe 3.0 | The GPU may lose to the CPU on checksum and decrypt at every input size | That is a valid atlas result and is published as such; FFT and filter over large batches are the likelier GPU wins; record the PCIe link with every run |
| The join runs over 1 GbE | For most ops the Dell's own CPU beats a remote GPU, so M9 shows little offload | M9 is a correctness and failover gate; placement uses measured link cost; re-run M9 when faster links arrive |
| Windows adds latency noise | The M6 gate is harder to pass than on tuned Linux | Measure on a quiet system and report the distribution, not one number; the Dell repeats the run on Linux |
| No machine has mixed core types | `stagelanes` cannot prove placement by core type | State this in the README; prove placement by socket on the Dell; revisit when such a CPU is available |
| `diagsteno` overlaps general time-series databases | It looks like one more database | Position it as an edge ingest and feature engine that exports to them, which M8's export gate enforces |
| Benchmarks from a laptop and a virtual machine are noisy | Atlas numbers are not trusted | Publish method, raw data, hardware, OS and driver versions with every figure |
| Three agents drift apart | Integration fails at M9 | One contract in `bytelanes`, golden vectors, per-milestone status files, and you as the only merger |

## Sources

- [Channelforge: Rust GPU Channel Subsystem — Spec and Claude Code Plan](https://claude.ai/artifact/X4oUAgjzV3ofKCJmcjY8NL) — the technical spec this plan implements
- [GitHub repository search](https://github.com/search?type=repositories) — name counts, queried through the GitHub API on 21 Nov 2026
- [crates.io](https://crates.io) — exact-name lookups on 21 Nov 2026
- [SWARLog on SourceForge](https://sourceforge.net/projects/swarlog) — the near-collision that ruled out `swarflog`

Milestone gates and gap assessments are this plan's own proposals. Nothing here has been built or measured yet.
