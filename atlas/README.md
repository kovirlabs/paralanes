# Atlas

Published measurements: CPU-versus-GPU crossover sizes for `bytelanes` ops
(atlas v1, milestone M5) and `stagelanes` pipeline latency runs (M6).
Measured so far: the Ryzen desktop with the Radeon RX 7800 XT, through Vulkan
and DX12 (see [`crossover.md`](crossover.md)).

## Method

- Every figure comes from a run of `bytelanes-bench` (ops) or
  `stagelanes-bench` (pipeline). No other numbers appear here.
- Each run writes one file, `runs/<machine>/<YYYY-MM-DD>-<op>.json` or
  `runs/<machine>/<YYYY-MM-DD>-pipeline.json`, which is never edited after it
  is committed. Other files:
  - `<YYYY-MM-DD>-commitlog.json` holds the transactional workload.
  - A suffix such as `-untuned` marks a run kept for comparison.
  - The benchmark adds `-2`, `-3` and so on rather than overwrite a file.
- One `bytelanes-bench` run file covers one op on the CPU reference and on
  every hardware GPU adapter (on Windows, Vulkan and DX12), so the
  comparison shares one data set, one build and one sitting.
- **How a point is measured:**
  - Each program goes through `ChannelBackend` submit and poll with one
    program in flight, so GPU times include upload, download and the
    result's CRC-32C.
  - Each point runs at least 1 s and at least 5 repetitions, after 3 warm-up
    repetitions.
  - Every repetition's completion (status, bytes, CRC) must equal the CPU
    reference's, or the run aborts.
- **What is recorded:** latency percentiles (min, p50, p90, p99, max, mean),
  throughput at the median, and CPU time spent on the work.
  - CPU time is the process's CPU time (all threads) minus the caller's time
    spinning while it waits for completion.
  - Windows updates these counters at scheduler ticks (about 15.6 ms), so
    CPU-time figures are means over the whole series.
- **The CPU reference** is `bytelanes::cpu` on one core, with hardware
  CRC-32C and AES instructions.
- **The two crossovers** (latency and CPU time) are defined in
  `crossover.md`.
- **Data** is generated from fixed seeds, so every machine measures the same
  bytes.
- Every run file records: machine, OS, kernel or build number, GPU driver
  version, wgpu backend (Vulkan, DX12 or Metal), GPU host link (PCIe
  generation and width, or unified memory), network link type and speed for
  runs across machines, Rust version and the exact command.
- Multi-socket runs also record memory per NUMA node (DIMM count and size)
  and which cores the guest was pinned to.
- When hardware changes, the affected runs are repeated and added as new
  files; old runs stay as the record of the old hardware.
- MacBook runs: plugged in, lid open, power mode recorded.
- Ryzen runs: no `stagelanes` pipeline benchmark running at the same time.
- A result where the GPU loses at every size is published as such.

`crossover.md` lists the crossover size per op per machine, each entry linked
to the run files it comes from.
