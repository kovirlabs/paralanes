# Roadmap: projects that build on paralanes

Date: 2026-11-22. Status: DRAFT for Evan. Nothing here changes `PLAN.md`;
the milestones M0 to M10 stay as they are. This file lists the projects that
would use `stagelanes` and `bytelanes` beyond `diagsteno`, why each fits, and
where each stands.

Outside facts marked "(checked)" come from a web search on 2026-11-22.
Those marked "(memory)" were not checked and need a landscape check, like
M0's, before work starts.

## What the measurements say a good fit looks like

Every figure is from a run file in `atlas/runs/`; see `atlas/crossover.md`
and `docs/experiments/`.

| Finding | Figure | Source |
| --- | --- | --- |
| Pinned stages keep their tail under load | p99 10.6 µs at 50% load, 11.1 µs at 90%; 0 allocations on stage cores (0.5 µs parse + 4 µs work, 4 workers) | `runs/ryzen/2026-11-22-pipeline-3.json` (M6, not yet committed) |
| The GPU wins on compute-dense work with a small result | FFT about 18× faster at 64 MiB with 40 to 50× less CPU time, against a scalar one-core reference | `runs/ryzen/2026-11-22-fft-2.json` |
| In a pipeline the GPU frees cores | 3 of 4 feature cores freed for 2.4 ms more median latency, from 16 windows per batch | `runs/ryzen/2026-11-22-gpu-feature-stage-realtime.json` |
| Chained work that returns only indices wins | Decrypt then filter, 1M records: 1.81 ms against 4.63 ms | `runs/ryzen/2026-11-22-commitlog-2.json` |
| The GPU loses where the CPU has an instruction | CRC-32C, AES and plain filter never win on latency; 8 CPU threads 37.9 M txn/s against 14.1 M | `runs/ryzen/2026-11-22-stream.json` |
| Each GPU call has a fixed cost | 0.26 to 0.4 ms | `runs/ryzen/2026-11-22-checksum-2.json` |
| Durable storage is the limit | 1 MiB write plus flush: 84.9 ms (Dell SATA on ZFS), 1.34 ms (NVMe) | `runs/ryzen/2026-11-22-diagsteno-ingest-nvme.json` |
| One shared commit stage beats a store per socket | Per-socket stores doubled fsyncs and halved disk throughput | `runs/dell/2026-11-22-per-socket-pipeline.json` |
| Remote memory costs memory-bound stages only | −36% bandwidth, +55% latency; −1.2% for the FFT stage | `runs/dell/2026-11-22-socket-locality.json` |
| Windows cannot isolate a core | p99.9 is 5.1× the median; power plan, priority and interrupt routing did not help | `runs/ryzen/2026-11-22-step2c-irq-affinity.json` |

So a project fits `stagelanes` when its work is staged, per item, and a few
microseconds each. It fits `bytelanes` when the work is compute-dense per
byte, arrives in batches, and returns far less than it reads.

## Projects

| # | Project | Repo | Uses | Status |
| --- | --- | --- | --- | --- |
| 1 | Security event decode-and-match engine | `rulelanes` | stagelanes | M0 and M1 done 2026-11-22 (`../rulelanes/docs/status/`); M2 not started |
| 2 | Many-channel sensor reducer | `diagsteno` (extended) | bytelanes, stagelanes | Proposed; recommended next |
| 3 | Flow collector with a network anomaly engine | `flowlanes` | stagelanes; bytelanes FFT for beaconing | M0 and M1 done 2026-11-22 (`../flowlanes/docs/status/`); M2 (UDP receive) not started |
| 4 | Group-commit journal | crate in `stagelanes` or its own repo (TBD) | stagelanes | Proposed |
| 5 | Exact-integer transforms for cryptography | ops in `bytelanes` | bytelanes | Fringe; research first |
| 6 | Evidence-grade signal features | `diagsteno` | bytelanes | Fringe |
| 7 | Nanopore adaptive sampling on raw signal | new | diagsteno, bytelanes | Fringe; needs hardware |
| 8 | Software radio blocks | contributions to FutureSDR | bytelanes, stagelanes | Fringe |

The names `rulelanes` and `flowlanes` are free on crates.io (checked). Evan
confirmed both on 2026-11-22.

### 1. `rulelanes`: security event decode-and-match engine

- **What:** reads Wazuh-format decoders and rules, and later Sigma rules,
  and runs parse → decode → match → write as pinned stages.
- **Why it fits:** this is the M6 pipeline shape. Wazuh's `analysisd` drops
  events when its queues fill, and Falco's main event stream is
  single-threaded with an open proposal to change that (both checked).
- **How it is checked:** `wazuh-logtest` on a real Wazuh manager in Docker
  is the oracle, as the CPU reference is for bytelanes. The engine must
  produce the same rule id, level and fields for the syntax it supports.
- **GPU:** none on the live path; CPU SIMD regex is the bar. One later
  research item: re-scanning archives when a new rule arrives, which has
  the batch, small-result shape. That needs a new multi-pattern match op in
  `bytelanes` (a contract change) and is unmeasured.
- **Risks:** Wazuh 5.0 moves normalisation into a new engine (checked), so
  the 4.x syntax is the target for now. Wazuh is GPL-2.0-only (checked in
  its v4.14.10 LICENSE); the engine reads rules the user supplies and
  bundles none.
- Spec: `../rulelanes/docs/SPEC.md`.

### 2. Many-channel sensor reducer

- **What:** `diagsteno` generalised beyond strain and vibration: fibre-optic
  acoustic sensing, neural probes, grid phasor and power quality, seismic.
- **Why it fits:** the only domain where the GPU won on latency. Ingest
  already reaches 24.9 to 38.3 M samples/s into memory on the Dell
  (`runs/dell/2026-11-22-per-socket-pipeline-2.json`). A 384-channel probe
  at 30 kHz is about 11.5 M samples/s (memory).
- **Gap:** Kilosort4 supports only NVIDIA GPUs (checked). For fibre sensing,
  Python tools (xdas) and a 2026 real-time framework paper exist (checked),
  so the gap is a byte-exact GPU edge reducer, not the whole stack.
- **Needs:** the RMS/peak and decimation ops (designed in
  `docs/experiments/2026-11-22-windows-pc-items.md`, item 3; a contract
  change), a Linux IIO source, and SeedLink/miniSEED export (suggested in
  `docs/landscape.md`).

### 3. `flowlanes`: flow collector with a network anomaly engine

- **What:** receives NetFlow v5, v9 and IPFIX, decodes and enriches flow
  records on pinned stages, and runs detectors over them: scans, new
  talkers, unusual volume, beaconing.
- **Why it fits:** small records at high rates through parse, enrich,
  aggregate. Beaconing detection is a periodicity test, so it can reuse the
  `bytelanes` FFT.
- **Where it plugs in:** Wazuh first. Wazuh has no NetFlow module; it takes
  network device data through syslog and JSON logs (checked). `flowlanes`
  writes JSON events and ships a Wazuh decoder and rule pack, which is the
  "Wazuh NetFlow module".
- **Why not Security Onion as the base:** it already collects NetFlow
  through the Elastic integration (checked), it is a whole distribution that
  wants its own machine, and its strength is packet capture with Zeek and
  Suricata. The gap is larger on Wazuh. The output stays neutral (JSON
  lines, syslog), so Security Onion or Elastic can read it later.
- **Zabbix:** a later output for traffic metrics, not the place for
  security detections.
- **Test source:** the UniFi gateway offers flow export as version 10
  (IPFIX), 9 or 5; which is the default was not confirmed
  (`../flowlanes/docs/unifi.md`). Real captures from the home network are
  never committed.
- Spec: `../flowlanes/docs/SPEC.md`.

### 4. Group-commit journal

- **What:** encode each shard's batch on its own socket in parallel, hand
  finished bytes to one commit stage, one fsync for all.
- **Why:** the per-socket experiment named this pattern and it is not built.
  Payment switches, telecom charging and broker persistence need it.
  `rulelanes` and `flowlanes` can use it for their outputs.
- **Risk:** ledger databases cover part of this (memory).

### 5. Exact-integer transforms for cryptography (fringe)

- The Q31 FFT butterfly has the structure of a number-theoretic transform.
  ML-DSA's modulus fits in 23 bits (memory), so it works in native 32-bit
  shader integers.
- WebGPU and CUDA implementations exist (checked). What was not found is a
  conformance-tested library with a CPU reference and published crossover
  sizes.
- Unmeasured. Start with one op and a crossover run.

### 6. Evidence-grade signal features (fringe)

- GPU output is byte-identical to the CPU reference on AMD and Apple GPUs.
  That suits audited condition monitoring and reproducible ML
  preprocessing, such as bioacoustic spectrograms.

### 7. Nanopore adaptive sampling on raw signal (fringe)

- Thousands of channels at kHz rates with a real-time decision per channel
  is diagsteno's shape (memory). It needs sequencer hardware and vendor API
  access.

### 8. Software radio blocks (fringe)

- `docs/landscape.md` found FutureSDR closest in shape. A bytelanes FFT
  block and stagelanes placement as a scheduler would be contributions, not
  a new project.

### 9. One-box monitor (direction, not started)

Evan's aim (2026-11-22): one server that does what Wazuh, Security Onion
and Zabbix do between them. He does not run Wazuh yet.

| Job | Done today by | Piece here |
| --- | --- | --- |
| Log and host security rules | Wazuh | `rulelanes` |
| Network behaviour | Security Onion (Zeek, Suricata, packet capture) | `flowlanes`, from flow records only |
| Availability and performance | Zabbix | Nothing yet; the `flowlanes` M6 metrics output is the nearest |
| The box itself | Three separate installs | The M10 appliance image (`paralanes-init`, `image/`) |

- **Decided direction (Evan, 2026-11-22): reuse, don't rebuild.** The
  stack runs Wazuh, Zabbix, Suricata and Zeek as they are, side by side in
  containers, and this project builds only what joins them: the installer
  and configuration, `rulelanes`, `flowlanes`, and later one screen.
  - Licences (checked 2026-11-22): Zabbix is AGPL-3.0 from 7.0; Wazuh
    4.14.10 is GPL-2.0-only; Security Onion's own components are Elastic
    License 2.0, so its glue code is not reused. Suricata and Zeek are open
    source (GPL-2.0 and BSD, from memory; verify before use). Separate
    containers that exchange logs do not combine these licences.
  - Order: Zabbix beside the running Wazuh first (works on the Ryzen PC).
    Suricata and Zeek need a Linux host with a mirror port from the UniFi
    switch, which Evan can arrange; Docker Desktop on Windows cannot
    capture from the physical network card.
- What would otherwise be missing: packet and payload inspection (flow records carry no
  payload), device polling such as SNMP and agent checks, storage and
  search, and a dashboard. Each is large.
- The first step is small: `rulelanes` and `flowlanes` writing into the one
  Wazuh server now running in Docker, so there is one place to look.
- Decide after `rulelanes` M2 and `flowlanes` M4 whether to keep Wazuh as
  the store and screen, or build those too.

## What the data argues against

- GPU checksums, encryption, erasure coding or dedup hashing for storage
  and backup. The CPU has instructions for all of them.
- GPU columnar filtering for databases. The crossover was marginal at 64 to
  256 MiB.
- Anything on Windows that needs a p99.9 under 100 µs.
- Any design that calls the GPU once per item.

## Order

1. `rulelanes` and `flowlanes` M0 and M1 (done 2026-11-22, agent B's
   machine), then M2 of each. The Ryzen desktop is off from 9:30pm to 9am,
   so an always-on collector needs another host.
2. Sensor reducer: the lowest-risk use of the GPU results.
3. Group-commit journal, when `rulelanes` or `flowlanes` needs durable
   output.
4. Fringe items only after a landscape check and one measured run each.

## Open decisions for Evan

Decided by Evan on 2026-11-22:

- The names `rulelanes` and `flowlanes`.
- The UniFi gateway will export IPFIX to the Ryzen desktop on UDP 2055.
- Flow data from Evan's network is never committed; it is used only to
  measure false-alert rates.
- `rulelanes` is GPL-3.0-only; `flowlanes` stays Apache-2.0.
- The `flowlanes` Wazuh pack uses rule IDs 100500 to 100599. Evan's only
  other custom rules are his UniFi ones (100010 to 100399).

Still open:

- GitHub owner for `rulelanes` and `flowlanes`; both are local repos with
  no remote.
- Whether their status files and run files stay in their own repos (the
  specs assume so) or move here like the other repos'.
- Both depend on `stagelanes-pipe`, which is on the unmerged branch
  `m6-stage-pipeline` (stagelanes PR #3, open).
