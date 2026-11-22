# cpuid-qos (probe)

Item 9 of `docs/cpu-throughput-strategy.md` §11. Prints the CPUID bits and leaves for Intel RDT / AMD PQoS: leaf 7 PQM/PQE flags, leaves 0xF and 0x10 (monitoring and allocation enumeration) and AMD leaf 0x80000020. x86-64 only.

```text
cargo run --release
```
