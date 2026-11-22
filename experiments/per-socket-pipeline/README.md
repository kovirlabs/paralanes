# per-socket-pipeline (experiment)

diagsteno's ingest rebuilt as one complete pipeline per socket on
stagelanes, compared with diagsteno's own single, unpinned ingest. Not a
milestone. diagsteno is used only as a library and is unchanged. The crate
is standalone (its own `[workspace]`), outside the paralanes build and CI. It
needs `bytelanes`, `diagsteno` and `stagelanes` checked out next to
`paralanes`.

```
cargo run --release -- --layout baseline            --store-dir DIR --seconds 60
cargo run --release -- --layout per-socket          --store-dir DIR --seconds 60
cargo run --release -- --layout per-socket-unpinned --store-dir DIR --seconds 60
```

| Layout | What runs |
| --- | --- |
| `baseline` | `diagsteno::ingest::Ingest`: one pipeline, plain threads, unpinned, one sender with all 64 channels |
| `per-socket` | Two shards. Shard `s` takes accelerometers and strain gauges `16s..16s+15` of each kind and runs only on socket `s`. Each shard has its own ingest core, store core, 4 feature cores (`--workers`), sender core, TCP port and store directory. Stages are joined by `stagelanes-ring` SPSC rings, and each thread allocates what it uses, so memory stays on its socket |
| `per-socket-unpinned` | The same two shards, unpinned, to separate the effect of sharding from the effect of pinning |
| `per-socket-shared-store` | CPU and memory work per socket, as in `per-socket`, but with ONE store thread on socket 0, where the I/O devices attach. It writes both shards' frames in one durable file per 100 ms batch (group commit: one fsync per batch instead of one per shard), then acknowledges each shard on its own connection. Feature rows from all workers go to one writer that stores them once per second (by time, not row count) |

Every layout keeps R8: a batch is stored (Parquet, fsync) before it is
acknowledged. Use a `--store-dir` on `/dev/shm` to take the disk out of the
measurement, and one on the real disk to include it.

Output: one JSON line per run. `all_acked_s` is the time until the sender
has every acknowledgement. `all_features_stored_s` is the time until every
feature window is stored.
