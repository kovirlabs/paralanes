//! paralanes-chand: channel daemon on a GPU machine.
//!
//! Receives channel programs, runs them on the local `bytelanes` backend and
//! returns results, each carrying a CRC the receiver checks. Must build and pass
//! its tests on Windows and macOS.
//!
//! Status: stub. Built in M9 (spec: `cf-chand`).

#![forbid(unsafe_code)]

fn main() {
    eprintln!("paralanes-chand: not implemented yet (M9)");
    std::process::exit(2);
}
