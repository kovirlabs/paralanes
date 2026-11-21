# Atlas

Published CPU-versus-GPU crossover measurements for `bytelanes` ops. Nothing
has been measured yet; atlas v1 is milestone M5.

## Method (draft, finalised in M5)

- Every figure comes from a run of the `bytelanes-bench` binary. No other
  numbers appear here.
- Each run writes one file, `runs/<machine>/<YYYY-MM-DD>-<op>.json`, which is
  never edited after it is committed.
- Every run file records: machine, OS, kernel or build number, GPU driver
  version, wgpu backend (Vulkan, DX12 or Metal), Rust version and the exact
  command.
- MacBook runs: plugged in, lid open, power mode recorded.
- Ryzen runs: no `stagelanes` pipeline benchmark running at the same time.
- A result where the GPU loses at every size is published as such.

`crossover.md` lists the crossover size per op per machine, each entry linked
to the run files it comes from.
