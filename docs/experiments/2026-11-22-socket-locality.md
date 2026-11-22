# Socket locality on the Dell (experiment)

Agent: A   Machine: Dell PowerEdge R440 guest (VM 102)   Date: 2026-11-22

Not a milestone. Evan asked whether keeping work on one socket speeds up
processing on the Dell. The full answer for the stage pipeline is M6 (agent
B builds `stagelanes-pipe`; agent A repeats it on two sockets). This is an
early measurement with the pieces that exist today.

Every figure below comes from
`atlas/runs/dell/2026-11-22-socket-locality.json`: 3 repeats; the mean is
shown with its [min..max]. Code: `diagsteno` branch `exp-socket-locality`,
`experiments/socket_locality`.

## Setup first: the guest's sockets were not real sockets

Before this run, the VM's 24 vCPUs could run on any of the host's 48 CPUs,
and its two NUMA nodes were not bound to the host's. A socket experiment
then measures nothing. Fixed on 2026-11-22, and checked on the host after a
full stop and start:

- A Proxmox hookscript (`local:snippets/pin-102.sh`) pins guest vCPUs 0–11
  to host node 0's physical cores (CPUs 0, 2, …, 22) and vCPUs 12–23 to node
  1's (1, 3, …, 23). The hyperthread siblings are left to the host.
- `numa0`/`numa1` bind guest memory nodes to host nodes 0 and 1
  (`policy=bind`).

## Memory: local vs remote

| Threads | Read bandwidth, local | Read bandwidth, remote | Change |
| --- | --- | --- | --- |
| 1 | 12.1–13.1 GB/s | 7.9–8.1 GB/s | about −36% |
| 12 | 35.5–36.0 GB/s | 28.2–28.6 GB/s | about −21% |

| | Local | Remote | Change |
| --- | --- | --- | --- |
| Dependent-load latency (1 thread, 512 MiB, 4 KiB pages) | 138 ns | 212–216 ns | about +55% |

Reading the other socket's memory clearly costs: a third of one core's
bandwidth, a fifth of a full socket's, and half again on latency. Node 1
(2 DIMMs) streamed as fast as node 0 (3 DIMMs) with 12 threads, so neither
node's DIMM count limited this run.

## The diagsteno feature stage: placement barely matters

One producer copies 4096-sample windows to 10 workers running diagsteno's
feature reference (Q31 FFT and band energies).

| Layout | Windows/s | vs same socket |
| --- | --- | --- |
| Producer and workers on node 0 | 60,486 [60,388..60,606] | — |
| Producer on node 0, workers on node 1 | 59,733 [59,503..59,863] | −1.2% |
| Producer on node 0, 5 workers on each node | 63,727 [63,688..63,760] | +5.4% |
| Unpinned (OS scheduler) | 63,513 [63,272..63,805] | +5.0% |

Run-to-run spread is 0.1–0.3%, so these small differences are real. At
about 60,000 windows of 4096 samples per second, the stage handles 250
million samples/s on 10 cores. The M7 layout needs 851,200 samples/s, so
the feature stage needs well under one core.

What this says:

1. **This stage is compute-bound.** A 4096-point FFT and its working set fit
   in a core's cache. Each window is read from the producer's memory once
   (16 KB), then the core spends about 160 µs computing. So the remote-memory
   penalty above shows up only as −1.2%.
2. **Using both sockets beat packing one** (+5.4%). The likely reasons:
   - the turbo clock drops as more cores are active on one socket, so 5+5
     runs faster than 10+1
   - two L3 caches instead of one

   This was not verified: the guest cannot read the host's clock or turbo
   state.
3. **The unpinned scheduler did as well as the best pinned layout** for
   this workload, because it spreads the threads across both sockets.

## Conclusions for the design

- Socket-local memory matters for **memory-bound stages**: ingest copies,
  storage, ring handoffs of large batches, and any scan over big buffers.
  This is where stagelanes' per-socket memory (R1 and the M6 measurement)
  should pay off, by up to about a third of bandwidth.
- For **compute-bound stages** like the FFT features, spreading work across
  both sockets helps more than keeping it on one. Placement should come
  from measured stage profiles, not a fixed "one socket" rule. That agrees
  with spec R9 (measure, don't hard-code).
- On this machine today, the bottleneck for diagsteno is still storage
  `fsync` (M7: about 85 ms per 1 MiB write and fsync), not CPU or memory
  placement.

## Caveats

- This is a virtual machine. vCPUs are pinned 1:1 and memory is bound, but
  QEMU's I/O threads and the host's ZFS threads are not pinned.
- 4 KiB pages; huge pages would lower both latency figures.
- The guest sees QEMU's cache sizes, not the CPU's (4 MB L2 and 16 MB L3
  reported; the Silver 4214 has 1 MB and 16.5 MB).
- `perf bench numa mem` was tried first and showed no local/remote
  difference. Its loop is not bandwidth-bound, so it was set aside; its raw
  rows are in the run file.
