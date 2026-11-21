//! paralanes-sched: the scheduler.
//!
//! Batching, placement across GPU paths by measured cost (link bandwidth and
//! round-trip time plus compute), CPU fallback below the measured crossover
//! size, path health and failover. No hard-coded thresholds. Programs are idempotent and may be
//! resubmitted once on another path.
//!
//! Status: stub. Built in M9 (spec: `cf-sched`).

#![forbid(unsafe_code)]
