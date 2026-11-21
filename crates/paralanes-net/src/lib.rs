//! paralanes-net: network backend.
//!
//! Submits channel programs to a `paralanes-chand` through a `Transport` trait.
//! TCP first, RDMA later.
//!
//! Status: stub. Built in M9 (spec: `cf-backend-net`).

#![forbid(unsafe_code)]
