//! paralanes-net: network backend.
//!
//! Submits channel programs to a `paralanes-chand` through a `Transport` trait.
//! TCP first; faster transports such as RDMA arrive as more `Transport`
//! implementations. Link speed is measured, never assumed.
//!
//! Status: stub. Built in M9 (spec: `cf-backend-net`).

#![forbid(unsafe_code)]
