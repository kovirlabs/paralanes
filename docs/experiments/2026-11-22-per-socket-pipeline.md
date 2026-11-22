# Per-socket pipeline on the Dell (experiment)

Agent: A   Machine: Dell PowerEdge R440 guest (VM 102)   Date: 2026-11-22

Not a milestone. Evan asked for diagsteno's ingest to be built as one
pipeline per socket, outside diagsteno. Code: `paralanes`
`experiments/per-socket-pipeline` (stagelanes pinning and rings, diagsteno
as a library). Every figure is from
`atlas/runs/dell/2026-11-22-per-socket-pipeline.json`: 3 repeats; mean
[min..max]. Same pinned VM setup as the socket-locality experiment
(`docs/experiments/2026-11-22-socket-locality.md`, paralanes#10).

Workload: 60 s of the 64-channel layout (51,072,000 samples), sent as fast
as ingest accepts it. Every batch is stored durably before it is
acknowledged. All 18 runs stored every frame and every feature window (0
missing).

## Results

Seconds until every frame was acknowledged, and until every feature window
was stored:

| Layout | Memory store: acked | Memory store: features | Disk store: acked | Disk store: features |
| --- | --- | --- | --- | --- |
| Baseline: diagsteno ingest, 1 pipeline, unpinned | 2.05 [1.91..2.13] | 4.01 [3.81..4.20] | 4.86 [4.74..5.07] | 6.35 [6.17..6.68] |
| Per socket, pinned | 1.36 [1.32..1.41] | 1.35 [1.32..1.40] | 10.16 [6.87..16.07] | 10.45 [7.30..16.22] |
| Per socket, unpinned | 1.40 [1.33..1.45] | 1.39 [1.32..1.44] | 21.46 [6.89..41.40] | 21.70 [7.16..41.55] |

As throughput, until all frames were acknowledged:

| Layout | Memory store | Disk store |
| --- | --- | --- |
| Baseline | 24.9 M samples/s | 10.5 M samples/s |
| Per socket, pinned | 37.7 M samples/s | 5.8 M samples/s |
| Per socket, unpinned | 36.5 M samples/s | 3.9 M samples/s |

The M7 target is 0.85 M samples/s. Every layout clears it by 4x or more.

## What it shows

**Without the disk (memory-backed store), per-socket is about 1.5x faster
to acknowledge and 3x faster to finish features.** Most of that is
parallelism, not placement:

- The baseline has one store thread and one feature thread. The per-socket
  layouts have two store threads and eight feature threads.
- Pinned vs unpinned isolates placement: **pinning and socket-local memory
  add about 3%** (1.36 s vs 1.40 s). That fits the socket-locality
  experiment: these stages are compute-bound, and the remote-memory penalty
  only shows up in memory-bound work.

**With the real disk, per-socket is slower and erratic.** That is a
negative result:

- Two shards issue twice as many fsyncs, from two independent stores on the
  same virtual disk.
- The shard workers also write a feature file every 256 rows, so many more
  small files are fsynced than in the baseline.
- On this storage (about 85 ms per 1 MiB write plus fsync, with
  multi-second stalls), more and smaller syncs make things worse.
  Throughput halves, and run-to-run spread grows to 6.9–41 s.
- Unpinned is worse still. Its variance is so large that the difference
  between pinned and unpinned on disk is not meaningful from 3 runs.

## Conclusions

1. **Sharding by socket is the right shape for CPU and memory work.** It
   scales the stages that were single-threaded, and pinning adds a small,
   steady gain on top.
2. **Do not split the storage path until each socket has its own fast,
   durable storage.** On one slow disk, per-shard stores multiply fsyncs.
   Until storage improves:
   - keep a single store stage, with group commit across shards
   - or give each socket its own power-loss-protected device (or a SLOG),
     and only then shard the store
3. **Batch feature writes like sample writes,** by time rather than by row
   count, so the number of fsyncs doesn't grow with the number of workers.
4. **Fix storage first.** It is still the limit on this machine:
   - per-socket pipelines reach 37.7 M samples/s into memory but only
     5.8 M samples/s to this disk
   - the baseline reaches 24.9 M and 10.5 M respectively

## Caveats

- Sender and ingest share the guest. The per-socket senders were pinned to
  their own socket's last core; the baseline sender was unpinned.
- The comparison with the baseline mixes parallelism and placement (see
  above). Only pinned vs unpinned per-socket isolates placement.
- This is a virtual machine. QEMU's I/O threads and the host's ZFS threads
  are not pinned, and the disk results depend on the host's ZFS state.

## Second run: one shared store with group commit

Following conclusions 2 and 3, a fourth layout, `per-socket-shared-store`,
keeps each shard's ingest and feature cores on its own socket, but:
- One store thread on socket 0 (where all I/O devices attach) writes both
  shards' frames as one durable file per 100 ms batch. That is one fsync per
  batch instead of one per shard (group commit). It then acknowledges each
  shard.
- One writer stores all feature rows once per second (by time, not row
  count).

All four layouts were rerun in the same session, 3 repeats each; every run
was complete. Run file: `atlas/runs/dell/2026-11-22-per-socket-pipeline-2.json`.

| Layout | Memory store: acked | Disk store: acked |
| --- | --- | --- |
| Baseline: diagsteno ingest | 2.05 [1.99..2.13] (24.9 M/s) | 7.38 [4.61..12.89] |
| Per socket, separate stores | 1.33 [1.32..1.35] (38.3 M/s) | 10.91 [6.78..18.99] |
| Per socket, unpinned | 1.37 [1.34..1.40] (37.2 M/s) | 10.12 [6.93..16.21] |
| Per socket, shared group-commit store | 2.27 [2.24..2.31] (22.5 M/s) | 9.03 [6.07..14.67] |

What it shows:

- **On disk, the shared store improved on separate per-socket stores on
  average** (9.03 s vs 10.91 s). But the disk became much noisier between
  the two sessions: the baseline ranged 4.61–12.89 s here, against
  4.74–5.07 s an hour earlier. The layouts' disk ranges overlap, so 3
  repeats cannot rank them. On this storage, disk results are dominated by
  the host's ZFS behaviour, not the pipeline's design.
- **In memory, the single store thread is now the bottleneck** (22.5 M
  samples/s, below the baseline). One core encodes Parquet for both shards,
  and it reads shard 1's frames from the other socket.
- **The pattern that would combine the gains:**
  - encode each shard's batch on its own socket, in parallel
  - hand only finished, encoded bytes to one commit stage
  - that stage writes them and issues one fsync for all

  Encoding stays parallel and local, and syncs stay shared. Not built yet.
- **Still true: fix the storage first.** With these disks no layout gives
  stable durable throughput. Every layout still clears the 0.85 M samples/s
  target by about 7x or more, even at its worst run.
