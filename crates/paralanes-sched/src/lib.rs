//! paralanes-sched: the scheduler.
//!
//! Batching, placement across GPU paths, CPU fallback below the measured
//! crossover size, path health and failover. Programs are idempotent and may be
//! resubmitted once on another path.
//!
//! Status: stub. Built in M9 (spec: `cf-sched`).

#![forbid(unsafe_code)]
