# Windows isolation, steps 1 and 2 of the strategy benchmark (experiment)

Agent: B   Machine: Ryzen 7 5700X desktop, Windows 11 (bare metal)   Date: 2026-11-22

Item 7 of the Windows PC list in `docs/cpu-throughput-strategy.md` §11: the
Windows form of benchmark step 2, measured against a Windows step-1
baseline. It uses the same instrument as the Dell's step-1 run (agent A's
`experiments/per-socket-pipeline --layout step-metrics`), adapted:

- **One shard.** The Ryzen has one socket, so one shard runs the full
  64-channel layout on one logical CPU per physical core (the first SMT
  thread):
  - core 0: housekeeping;
  - cores 1 and 2: ingest and store;
  - cores 3 to 6: feature workers;
  - core 7: the sender.

  The pipeline CPUs are OS CPUs 2, 4, 6, 8, 10 and 12.
- **Interrupt counts.** Windows has no `/proc/interrupts`, so `typeperf`
  samples the Processor counters every second (`src/irq.rs`): interrupts
  per second (including the clock; Windows has no separate local-timer
  count), DPCs queued per second, and % DPC and % interrupt time.
- **Store on the NVMe** (WD_BLACK SN770). The Dell used `/dev/shm`, so
  acknowledgement latency is not comparable between the machines. The
  interrupt and compute metrics are.

Every figure is from the run files below (3 real-time and 3 unpaced runs
each; mean [min..max]). Scripts: `windows-step.ps1`, `summarize-step.ps1`
and `windows-irq-affinity.ps1` in `experiments/per-socket-pipeline`.

## Results (real-time runs: 60 s of the 64-channel layout at its real rate)

| Run | Interrupts/s per pipeline core | FFT window p50 | p99 | p99.9 | max |
| --- | --- | --- | --- | --- | --- |
| Dell step 1 (Linux VM, stock) | 961 (866 local timer) | 152 µs | 189 µs | 231 µs | 322 µs |
| Ryzen step 1 (stock, Balanced) | 696 | 78 µs | 127 µs | 396 µs | 525 µs |
| Ryzen step 2a (+ Ultimate Performance plan) | 692 | 78 µs | 123 µs | 399 µs | 594 µs |
| Ryzen step 2b (2a + time-critical priority for pipeline threads) | 694 | 79 µs | 137 µs | 461 µs | 1,137 µs |

Run files: `atlas/runs/ryzen/2026-11-22-step1-baseline.json`,
`…-step2a-power-plan.json`, `…-step2b-priority.json`; Dell:
`atlas/runs/dell/2026-11-22-step1-baseline.json`.

What it shows:

1. **The Ryzen computes a window twice as fast, but its tail is much
   wider.**
   - On the Ryzen, p99.9 is 5.1× p50; on the Dell, 1.5×.
   - That is the jitter the strategy targets: a window that should take
     78 µs sometimes takes 400 to 600 µs.
2. **The power plan does nothing measurable** (step 2a). The tail is the
   same within the run-to-run spread.
3. **Raising pipeline threads to time-critical does not help** (step 2b);
   the tail is slightly worse.
   - So the tail is not ordinary pre-emption by other programs' threads,
     which priority would have removed.
   - That leaves:
     - interrupts and DPCs on the pipeline cores (about 690/s each, and
       91 DPCs/s);
     - the SMT sibling of each pipeline core, which Windows still gives to
       other programs.
4. **A likely interrupt source:**
   - The NVMe controller's affinity policy is 5
     (`IrqPolicySpreadMessagesAcrossAllProcessors`): its completion
     interrupts are spread over every CPU, pipeline cores included.
   - In this benchmark the store writes to that NVMe, so the pipeline's
     own storage I/O interrupts the pipeline.
   - Step 2c tests this.
5. **Unpaced runs** (the same data as fast as possible) show the same
   pattern with larger tails: window p99.9 is about 790 to 830 µs, and the
   max reaches 1.3 to 26 ms. The per-core interrupt rates rise to 860 to
   950/s.

## Step 2c: interrupt affinity (needs a reboot; not run yet)

`windows-irq-affinity.ps1 -Apply` sets every present PCI device's
documented interrupt affinity policy:
- `DevicePolicy = 4` (`IrqPolicySpecifiedProcessors`);
- `AssignmentSetOverride = 0x3`, which is CPUs 0 and 1, the housekeeping
  core and its SMT sibling.

It saves the previous values first, and `-Revert` restores them exactly.
`-Show` lists the current policies and changes nothing. Both apply and
revert need an elevated shell and a reboot, so Evan runs them. Then step 2c
reruns `windows-step.ps1` with the same arguments as 2b.

Not possible on Windows: true core isolation (no `isolcpus`), moving the
clock interrupt off a core, and keeping other programs off the pipeline
cores' SMT siblings without disabling SMT in the BIOS. Disabling SMT would
test the sibling explanation directly (point 3 above).

## stagelanes change

`stagelanes-arch` gained `set_current_thread_priority(Priority::{Normal,
High, TimeCritical})`, used here through `--priority`:
- Windows: `SetThreadPriority`.
- Linux: nice -10, or `SCHED_FIFO` 50 (both need `CAP_SYS_NICE`).
- macOS: Normal only.

It is on stagelanes branch `exp-windows-isolation`.
