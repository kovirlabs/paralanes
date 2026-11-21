//! paralanes-init: PID 1 for the appliance image. Linux only.
//!
//! `unsafe` is allowed here only where a syscall wrapper needs it; every block
//! gets a `// SAFETY:` comment.
//!
//! Status: stub. Built in M10 (spec: `cf-init`).

fn main() {
    eprintln!("paralanes-init: not implemented yet (M10)");
    std::process::exit(2);
}
