# Machines

The hardware inventory. Other docs refer to machines by name and point here,
so a hardware change is made in this file first. Every atlas run file also
records the machine details it ran on; when this file and a run file disagree,
the run file is what was measured.

Figures in this file are vendor specifications, not measurements.

## Ryzen desktop (agent B)

Recorded 21 Nov 2026.

| Part | Detail |
| --- | --- |
| CPU | AMD Ryzen 7 5700X (Zen 3 "Vermeer", 7 nm), 8 cores / 16 threads, 3.4 GHz base, up to 4.6 GHz boost, L2 4 MB, L3 32 MB, 65 W, socket AM4 |
| GPU | AMD Radeon RX 7800 XT (RDNA 3, Navi 32), ASRock board partner; 16 GB GDDR6, 256-bit, 19.5 Gbps (624 GB/s); 60 CUs / 3840 stream processors; 64 MB Infinity Cache; ~2124 MHz game, up to 2430 MHz boost (reference); 263 W board power (reference) |
| GPU driver | 32.0.32029.2010 (dated 17 Nov 2026) |
| GPU PCI ID | VEN_1002 DEV_747E SUBSYS_53261849 |
| Motherboard | Gigabyte A520I AC (Mini-ITX), AMD A520 chipset, socket AM4; BIOS AMI F23c (dated 17 Aug 2026) |
| Expansion | 1x PCIe 3.0 x16 (holds the GPU), 1x M.2 (PCIe 3.0 x4 NVMe), 4x SATA 6 Gb/s |
| Memory | 32 GB (2x 16 GB) G.Skill Trident Z RGB F4-3200C14-16GTZR, DDR4-3200, dual channel, XMP active, 14-14-14-34, 1.35 V; board maximum 64 GB |
| Network | Realtek Gigabit Ethernet; Intel Wi-Fi 5 (802.11ac) and Bluetooth |
| OS | Windows (build TBD; recorded in every run file) |

What this means for the design:

- **Host-to-GPU link is PCIe 3.0 x16.** The RX 7800 XT is a PCIe 4.0 card, but
  the A520 chipset runs the slot at PCIe 3.0. Copy-in and copy-out cost is
  bounded by that link, so it is recorded in every atlas run file. A board with
  PCIe 4.0 would change the crossover sizes and is an upgrade to re-measure.
- **One cache domain.** The 5700X has a single core complex: all 8 cores share
  one 32 MB L3. Eight identical cores, one socket: no placement by socket or
  core type can be shown here.
- **Fast CPU baselines.** Zen 3 has AES-NI and the SSE4.2 CRC32 instruction, so
  the CPU reference for Decrypt and Checksum is strong.
- **One PCIe slot, taken by the GPU.** A faster network card would need the
  M.2 slot through an adapter (PCIe 3.0 x4, displacing the NVMe drive) or a
  different board. Network upgrades on this machine are therefore later work.
- **1 GbE wired.** This is the link the M9 join runs over until faster
  networking arrives.

## Dell PowerEdge (agent A)

| Part | Detail |
| --- | --- |
| CPU | 2x Intel Xeon Silver, 10 cores each (models TBD) |
| GPU | None |
| Memory | TBD |
| Network | TBD |
| Chassis | 1U (limits future GPU options; check riser and power specifications before buying) |
| OS | Linux guest on Proxmox: host CPU type, cores pinned one-to-one, both NUMA nodes exposed, large-page backing |

## MacBook Pro (agent C)

| Part | Detail |
| --- | --- |
| SoC | Apple M5 Pro (core counts TBD) |
| Memory | 24 GB unified, shared by CPU and GPU |
| Network | Wi-Fi (standard TBD); wired Ethernet through an adapter (speed TBD) |
| OS | macOS, AArch64 |

Benchmark rule: plugged in, lid open, power mode recorded.

## Links between machines

| Link | Detail |
| --- | --- |
| Ryzen | 1 GbE wired; Wi-Fi 5 available |
| Dell | TBD |
| MacBook | Tested on both wired Ethernet and Wi-Fi, recorded separately |
| Switch | TBD |

Faster networking (25 GbE, RDMA or similar) is not near-term. It stays in the
design as a test variable: see "Adapting to new hardware" in `SPEC.md`.
