# CPU throughput strategy on the Dell: steps 1 and 2

Agent: A   Machine: Dell PowerEdge R440 guest (VM 102)   Date: 2026-11-22

The benchmark plan of `docs/cpu-throughput-strategy.md` §9, one layer at a
time.
- Code: `experiments/per-socket-pipeline --layout step-metrics`.
- Run files: `atlas/runs/dell/2026-11-22-step1-baseline.json` and
  `2026-11-22-step2-isolation.json`. 3 repeats each.
- Pipeline: two per-socket shards (ingest, store and 4 feature workers per
  socket) on vCPUs 1–6 and 13–18. vCPUs 0 and 12 are housekeeping, and the
  senders run on 11 and 23.
- Store on `/dev/shm`, so disk noise stays out of CPU jitter. The order
  "store, then acknowledge" is kept.

| | Step 1: baseline | Step 2: + isolation |
| --- | --- | --- |
| Guest settings | stock | `isolcpus=managed_irq,domain` + `nohz_full` + `rcu_nocbs` on 1–11 and 13–23; interrupts and workqueues on 0 and 12 |
| **Interrupts per pipeline core per second** (real time) | 954–970 | **84–108 (−90%)** |
| **Timer ticks per pipeline core per second** (real time) | 865–867 | **4.7–9.5 (−99%)** |
| Interrupts per pipeline core per second (unpaced) | 2,241–2,616 | 1,312–1,739 (−40%) |
| Window compute p50 / p99 (real time) | 151–152 / 187–193 µs | 148 / 181–182 µs (−2.6% / −4%) |
| Window compute p99.9 / max (real time) | 216–247 / 248–427 µs | 226–250 / 350–1,336 µs (no gain) |
| Window compute max (unpaced) | 295 µs; **6.8 and 12.0 ms** in 2 of 3 runs | **7.0, 8.5 and 11.2 ms** in all 3 runs |
| Throughput (unpaced) | 36.8–37.2 M samples/s | 37.0–37.3 M samples/s (within noise) |
| Real-time target held, late ticks | yes, 0 | yes, 0 |

## What it shows

1. **Isolation works as designed on the guest.** The timer tick is gone
   from busy worker cores, and 90% of interrupts with it.
2. **It barely changes throughput or median latency** (−3 to −4% compute
   time at p50 and p99). This workload is compute-bound and was not
   interrupt-limited to begin with.
3. **The multi-millisecond compute spikes are not caused by the guest.**
   They remained with the guest's interrupts removed. The likely sources are
   on the host:
   - host tasks (QEMU I/O threads, ZFS, Proxmox daemons) can still run on
     the host cores the vCPUs are pinned to
   - VM exits
   - host power-state changes

   That makes the host the next layer to isolate (step 2b below).
4. **The remaining interrupts on pipeline cores are mostly self-inflicted.**
   In a 10 s run, per interrupt type over all pipeline cores:
   - function-call IPIs: 8,692
   - TLB shootdowns: 4,580
   - timer ticks: 1,334, mostly on the two ingest cores, which make a system
     call per socket read
   - IRQ work: 1,120

   The IPIs and shootdowns come from freeing large buffers. The allocator
   unmaps them, and the kernel then interrupts every core running the
   process. That is the strategy's §5 rule, "no allocations on the hot path,
   pre-allocate pools", showing its value: per-frame `Vec`s and Parquet
   buffers should come from pre-allocated pools (stagelanes `BlockPool`) and
   be reused, not freed.

## Next steps

- **Step 2b, host isolation (Proxmox; needs Evan):**
  - keep host tasks off host cores 0–23 by limiting the host's `system.slice`
    and `user.slice` to the hyperthread siblings 24–47
  - performance governor and limited C-states on 0–23

  Then rerun to see whether the millisecond spikes go.
- **In code (no root):**
  - pre-allocated, reused frame and batch buffers, to remove the TLB and IPI
    storms
  - batched socket reads on the ingest cores (fewer system calls, fewer
    ticks)
- **Step 3:** huge pages for rings and pools. They need root to reserve,
  and they also cut TLB-shootdown cost.
- **Not measurable here:** LLC misses. The guest's virtual PMU exposes only
  `cycles`. It needs a host-side `perf` run, or a VM CPU model that passes
  the full PMU.
