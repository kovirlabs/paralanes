# Atlas

Published measurements: CPU-versus-GPU crossover sizes for `bytelanes` ops
(atlas v1, milestone M5) and `stagelanes` pipeline latency runs (M6).
Nothing has been measured yet.

## Method (draft, finalised in M5)

- Every figure comes from a run of `bytelanes-bench` (ops) or
  `stagelanes-bench` (pipeline). No other numbers appear here.
- Each run writes one file, `runs/<machine>/<YYYY-MM-DD>-<op>.json` or
  `runs/<machine>/<YYYY-MM-DD>-pipeline.json`, which is never edited after it
  is committed.
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
