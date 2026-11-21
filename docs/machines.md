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

## Dell PowerEdge R440 (agent A)

Recorded 21 Nov 2026 from the Proxmox host. Serial numbers and the service tag
are deliberately not recorded in this public repo.

| Part | Detail |
| --- | --- |
| System | Dell PowerEdge R440 (1U), board 04JN2K rev A06; BIOS 2.24.0 (dated 2 Apr 2025); UEFI boot with Secure Boot on |
| CPU | 2x Intel Xeon Silver 4214 (Cascade Lake), 12 cores / 24 threads each: 24 cores / 48 threads, 2 sockets, 2 NUMA nodes |
| CPU (Intel specification, per socket) | 2.2 GHz base, up to 3.2 GHz turbo, 16.5 MB L3, 85 W, DDR4-2400 maximum, 6 memory channels; AES-NI, SSE4.2 CRC32 and AVX-512 |
| GPU | None |
| Memory | 160 GB DDR4 registered ECC (multi-bit ECC), 5x 32 GB, all running at 2400 MT/s (the CPU's limit). Four are Samsung M393A4K40DB2-CVF (2933-rated); the one in A4 is a different 3200-rated module. 16 slots, 1 TB maximum |
| Memory per socket | CPU 1 (slots A1, A2, A4): 3 DIMMs, 96 GB. CPU 2 (slots B1, B2): 2 DIMMs, 64 GB. At most 3 of 6 memory channels are populated on CPU 1 and 2 of 6 on CPU 2 |
| Network | TBD (interfaces and link speeds not yet recorded) |
| PCIe slots | TBD (riser configuration not yet recorded) |
| Hypervisor | Proxmox VE 9.2.21, kernel 7.0.14-25-pve |
| Guest for agent A | Linux, host CPU type, cores pinned one-to-one, both NUMA nodes exposed, large-page backing; guest size TBD |

What this means for the design:

- **24 cores, not 20.** Earlier drafts assumed 10 cores per socket. The guest
  should not take all 24: Proxmox and other guests need some. The spec's
  starting split assumes a 20-core guest (10 per socket), leaving 2 cores per
  socket to the host. Pin one vCPU per physical core and leave the
  hyperthread siblings idle during benchmarks.
- **Unbalanced memory between sockets.** CPU 1 has 3 DIMMs and CPU 2 has 2,
  each on at most half of its 6 channels. Per-socket memory bandwidth will
  differ, which skews the M6 socket-local measurement, and the CPU 2 NUMA node
  has less memory for the guest. Every run file records the DIMMs per node.
  A sixth matching 32 GB module on CPU 2 would balance them at 3 and 3; full
  bandwidth needs 6 per socket.
- **Fast CPU baselines.** AES-NI, CRC32 and AVX-512 make the Dell's CPU
  reference strong. In M9 that is the local alternative every remote GPU path
  must beat over the network.
- **ECC memory.** The Dell's memory is ECC; the consumer GPUs' memory is not.
  The CRC on every GPU result covers that gap.
- **Secure Boot is on.** An appliance image booted on this hardware (M10 runs
  in QEMU first) would need signing or Secure Boot turned off.
- **Other guests are running.** The host had 63 GiB in use when recorded. Stop
  other guests during benchmarks.

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
| Dell | TBD (onboard ports not yet recorded) |
| MacBook | Tested on both wired Ethernet and Wi-Fi, recorded separately |
| Switch | TBD |

Faster networking (25 GbE, RDMA or similar) is not near-term. It stays in the
design as a test variable: see "Adapting to new hardware" in `SPEC.md`.
