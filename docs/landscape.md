# Landscape check (M0)

Searched 21 Nov 2026. Method: web searches, then the project pages, docs.rs pages and READMEs listed under Sources; GitHub figures (stars, last push, licence) come from the GitHub REST API and crate figures from the crates.io API, both queried that day. Where a figure is missing, it was not seen. Some project descriptions come from search-tool summaries of the cited page rather than a full read; those are marked "(summary)".

## bytelanes: vendor-neutral GPU data ops

The claim in PLAN.md is that vendor-neutral GPU data ops "exist almost only for NVIDIA". That is too strong. Cross-vendor GPU compute frameworks exist and are active. What is missing is a library of *data-path* ops (checksum, decrypt, filter) with a byte-exact CPU reference and a published, measured CPU-versus-GPU crossover.

| Project | Link | Licence | Activity | What it is |
| --- | --- | --- | --- | --- |
| CubeCL (tracel-ai) | github.com/tracel-ai/cubecl | MIT OR Apache-2.0 (crate); repo reports Apache-2.0 | 2,514 stars; pushed 21 Nov 2026; crate 0.12.0-pre.2 | Rust kernel language and runtime. Backends: wgpu (Vulkan, Metal, DX12), CUDA, ROCm, CPU. Built for Burn (ML). No CRC, AES or filter op library |
| vyre (santhreal) | crates.io/crates/vyre | MIT OR Apache-2.0 | 0.7.2, 14 Aug 2026; GitHub repo link returned 404 | GPU compute IR with a "standard operation library", wgpu and CUDA drivers, a `vyre-reference` CPU oracle and parity tests. Ships a CRC-32 op with a chunked GPU map-reduce plan (CRC-32, not confirmed CRC32C). Closest Rust overlap found |
| Kompute | github.com/KomputeProject/kompute | Apache-2.0 | 2,584 stars; pushed 15 Aug 2026; LF AI & Data sandbox | C++/Python framework on Vulkan. General compute (tensors, shaders); no data-path op library |
| ArrayFire | github.com/arrayfire/arrayfire | BSD-3-Clause | 4,914 stars; pushed 12 Nov 2026 | C++ array library with CUDA, oneAPI, OpenCL and CPU backends. Numerical ops and FFT; no Vulkan or Metal, no CRC or AES |
| VkFFT | github.com/DTolm/VkFFT | MIT | 1,791 stars; pushed 4 Apr 2026 | FFT on Vulkan, CUDA, HIP, OpenCL, Level Zero and Metal. Fills the FFT part of the gap |
| gpufft | crates.io/crates/gpufft | Apache-2.0 | 0.1.4, Oct 2026; 7 stars | Rust FFT wrapper: Vulkan through VkFFT, CUDA through cuFFT |
| wgsl-fft | docs.rs/crate/wgsl-fft | not checked | 0.4.4 (summary) | FFT in WGSL on wgpu; Stockham radix-4/2, Bluestein for other sizes (summary) |
| arrow-gpu (psvri) | github.com/psvri/arrow-gpu | Apache-2.0 | 32 stars; pushed 13 Jun 2026 | "WIP GPU implementation of apache arrow" on wgpu. Experimental |
| luma.gl GPU Dataframe | luma.gl/docs/api-reference/experimental/gpu-dataframe-operations | MIT (summary) | 2,500 stars (luma.gl repo) | Browser WebGPU filter, group, sort and join on Arrow input, compared with CPU references. Marked experimental; JavaScript, browser only |
| DirectStorage GPU decompression | devblogs.microsoft.com/directx/directstorage-1-1-now-available | proprietary runtime | 1.1 in 2022; 1.4 adds Zstandard (summary) | Vendor-neutral GPU decompression (GDeflate) on any DX12 SM 6.0 GPU. Windows only; decompression only |
| For contrast: nvCOMP, cuDF, GPUDirect Storage | docs.nvidia.com/cuda/nvcomp | nvCOMP proprietary; cuDF Apache-2.0 | nvCOMP docs repo archived | GPU CRC32, compression and decompression (nvCOMP), dataframe filter and I/O (cuDF), direct NVMe-to-GPU DMA (GDS). NVIDIA only |

No open-source GPU CRC32C or AES-256-CTR implementation on Vulkan, Metal or WGSL turned up. Published AES-CTR GPU code found is OpenCL (a 2015 thesis library) or CUDA. No published crossover measurements for CRC or AES on Vulkan or Metal turned up; general studies put GPU crossover between about 1K and 1M elements depending on the kernel (arXiv 2506.10092, arXiv 2609.24628).

**What they lack**

- An op library for the data path. The frameworks (CubeCL, Kompute, ArrayFire) are substrates; none ships CRC32C, AES-256-CTR and a columnar filter.
- A byte-exact CPU reference per op with golden vectors. vyre is the exception in intent; its source repo was unreachable, so its scope could not be checked.
- Measured crossover sizes per op per machine, published with the run files.
- For FFT, nothing is lacking: VkFFT covers Vulkan and Metal, and wgsl-fft covers wgpu.

**Verdict:** Go, with changes:
1. Reword the gap in PLAN.md and the README: "a vendor-neutral library of GPU data-path ops with CPU references and published crossover sizes", not "exists almost only for NVIDIA".
2. Before M7, benchmark wgsl-fft and VkFFT (through gpufft) against a home-grown WGSL FFT; build ours only if they cannot meet the op format or the CRC-on-GPU rule.
3. Track vyre. If its source reappears, compare its CRC and reference-oracle design before M1 is fixed.

## stagelanes: topology-aware stage placement

Every piece exists separately: topology discovery, core classification, pinning, lock-free rings, thread-per-core runtimes and hand-pinned pipelines. No project found reads the topology and then *places pipeline stages automatically* by cache size, frequency and socket.

| Project | Link | Licence | Activity | What it is |
| --- | --- | --- | --- | --- |
| hwloc | github.com/open-mpi/hwloc | BSD (per docs) | 746 stars; pushed 20 Nov 2026 | C library and `lstopo`: caches, NUMA, cpusets, CPU and memory binding. Linux and Windows. Discovery and binding, no placement policy |
| hwlocality | github.com/HadrienG2/hwlocality | MIT | 56 stars; 1.0.0-alpha.13, 30 Sep 2026 | Rust bindings for hwloc 2.x |
| gdt-cpus | github.com/gdt-tools/gdt-cpus-rs | MIT OR Apache-2.0 | 118 stars; 0.2606.1, 21 Jun 2026 | Rust: sockets, cores, cache hierarchy and L3 domains, NUMA IDs, P/E/LP-E classification, hard and soft pinning, priority. Windows, Linux, macOS (no affinity on Apple Silicon). Leaves placement to the caller; no queues |
| core_affinity | github.com/Elzair/core_affinity_rs | MIT/Apache-2.0 | 233 stars; 0.8.3, 1 Mar 2025 | Rust pin-to-core only; no topology |
| glommio | github.com/DataDog/glommio | Apache-2.0 OR MIT | 3,674 stars; 0.9.0, Mar 2024 | Thread-per-core runtime, Linux io_uring, `Placement::Fixed`. Apache Iggy calls it "pretty much unmaintained" |
| monoio | crates.io/crates/monoio | MIT OR Apache-2.0 | 0.2.4, Aug 2024 | Thread-per-core io_uring runtime. Iggy: "doesn't appear to be very actively maintained" |
| compio | github.com/compio-rs/compio | MIT | 1,964 stars; pushed 21 Nov 2026 | Thread-per-core runtime on IOCP, io_uring and polling; Windows and Linux. Active. No topology-aware placement |
| Seastar | github.com/scylladb/seastar | Apache-2.0 | 9,443 stars; pushed 17 Nov 2026 | C++ shard-per-core framework. Shards are uniform; no stage placement |
| disruptor-rs | github.com/nicholassm/disruptor-rs | MIT | 909 stars; 4.4.0, 8 Aug 2026 | Rust LMAX Disruptor ring with batching. Handoff only |
| DPDK eventdev | doc.dpdk.org/guides/prog_guide/eventdev.html | BSD-3-Clause | active | Pipeline stages as queues, one port per lcore, `rte_ring` handoff. Core roles set by hand with coremasks. Linux-first, packet-oriented |
| FastFlow | github.com/fastflow/fastflow | GPL-2.0 (repo) | 318 stars; pushed 14 Jun 2026 | C++ pipeline and farm patterns on lock-free SPSC queues. Placement not automatic (summary) |
| FutureSDR | github.com/FutureSDR/FutureSDR | Apache-2.0 | 487 stars; 0.10.0, 20 Oct 2026 | Rust flowgraph runtime with pluggable schedulers and wgpu buffers. Closest in shape; no topology-aware placement documented |

**What they lack**

- A placement step: measure each core (cache, top frequency, socket, NUMA node), then assign each stage to the cores that suit its profile. Today users set coremasks by hand (DPDK) or get uniform shards (Seastar, glommio, compio).
- Rings tied to placement: allocating the ring NUMA-local to the consumer stage.
- Windows and Linux both: glommio and monoio are Linux-only; DPDK is Linux-first.
- Heterogeneous cores as a placement input: gdt-cpus classifies P/E cores but leaves the decision to the caller.

**Verdict:** Go, with changes:
1. State the gap as "automatic stage-to-core placement", since manual pinning of pipeline stages is common.
2. Evaluate gdt-cpus and hwlocality for `stagelanes-arch` before writing topology discovery from scratch; keep our own measurement of top frequency if neither exposes it.
3. Use disruptor-rs as the baseline in `stagelanes-bench`; the ring has to beat or match it to justify itself.
4. See the name note below: a Nim concurrency runtime called `stagelane` appeared in Oct 2026.

## diagsteno: open edge processing of strain and accelerometer data

Open tools exist for each end: offline analysis (Python, MATLAB), real-time seismic networks, and hobby or vendor-tied edge nodes. None found does high-rate strain plus accelerometer ingest, GPU FFT and band features, anomaly scoring, a query API and TSDB export as one open edge service.

| Project | Link | Licence | Activity | What it is |
| --- | --- | --- | --- | --- |
| SeisComP | github.com/SeisComP/seiscomp | AGPL-3.0 (from v4) | active (summary) | Real-time seismic acquisition, archiving, QC, event detection; SeedLink protocol. Earthquake events, not condition features; no strain focus |
| Earthworm | isti.com/products/earthworm | open source | release Dec 2024 (summary) | Real-time seismic acquisition and processing; picking, location, magnitude. Same scope limit as SeisComP |
| OpenSHM (USGS) | earthquake.usgs.gov/monitoring/nsmp/buildings/faq.php | not stated | not stated | Building SHM on Earthworm: drift, modal parameters, wave travel time. Near real time, on-site server. No strain gauges mentioned |
| pyOMA2 | github.com/dagghe/pyOMA2 | MIT | 71 stars; pushed 22 Sep 2026 | Python operational modal analysis. Offline, batch |
| SHMTools (LANL) | lanl.gov/.../shm-data-sets-and-software.php | not checked | not checked | MATLAB routines: acquisition, feature extraction, classification. Needs MATLAB |
| ADI CN0549 stack | analog.com/en/resources/technical-articles/open-source-reusable-software-stack.html | open source (summary) | not checked | Condition-based monitoring stack, Kuiper Linux, MATLAB and Python. Tied to ADI hardware |
| freeFFT, viBscope, Memsio, CEDAS_acc | see Sources | various | small projects | Desktop or Raspberry Pi vibration tools; single-sensor, low rate |
| InfluxDB and Telegraf | influxdata.com/solutions/predictive-maintenance | MIT (open-source editions) | active | TSDB and collector. Stores features; does not compute FFT or band features |
| embedded-dsp (Rust) | docs.rs/crate/embedded-dsp | not checked | 0.5.1 (summary) | `no_std` DSP: FFT, PSD, wavelets; vibration spectrum example. Library, not a service |

Commercial products cover the full path (for example Dewesoft SHM), but are proprietary.

**What they lack**

- One open service that runs ingest, FFT, band features, anomaly scoring, storage and query at the edge for both strain and acceleration.
- GPU offload for many channels at high sample rates; all open tools found are CPU-only.
- Measured throughput per channel count and sample rate.

**Verdict:** Go. Two suggestions: accept or export SeedLink and miniSEED so diagsteno can sit beside SeisComP and Earthworm installations; and define the anomaly scores against pyOMA2 or SHMTools outputs on public SHM data sets (LANL publishes some) so results can be checked.

## Names

GitHub counts are from the repository search API (repo name, description and README, with stemming). crates.io is the exact-name endpoint (`/api/v1/crates/<name>`, 404 means free). The `kovirlabs` organisation's public repo list on 21 Nov 2026 contained none of the four names, so no hit below is ours.

| Name | GitHub repos | crates.io | Web findings |
| --- | --- | --- | --- |
| `bytelanes` | 0 with the name; 2 READMEs mention "bytelanes" in hardware text (an SDRAM module, a waveform viewer). Singular `bytelane`: 1 (`KojoFocus/bytelane-landing`, already known) | Free (`bytelane` also free) | Nothing. Nearest is the `bytelines` Rust crate (line reading), a different word |
| `diagsteno` | 0 | Free | Nothing. Results were medical pages on stenosis and unrelated firms (Diagenta, Diagenode) |
| `paralanes` | 0 with the plural name. Singular `paralane`: 3 (`paralane/paralane`, an organisation and empty-description repo pushed Nov 2025; `mjrzultan/paralane`; `nizqleo/paralane-dataset`, an autonomous-driving dataset site) | Free (`paralane` also free) | No software product found |
| `stagelanes` | 0 with the plural name. **Singular `stagelane`: `ghazalialshafi/stagelane`, "A macro based Concurrency library for Nim", 16 stars, created 30 Oct 2026, pushed 21 Nov 2026**, plus 4 repos built on it (an HTTP server, a Postgres client, a Redis client, a web server) | Free (`stagelane` also free) | Web search found no product; the Nim library is too new to be indexed |

The `stagelane` Nim runtime is a near-collision in the same domain (concurrency runtimes) and is growing an ecosystem. It does not block a Rust crate named `stagelanes`, but search results and conversation will mix them. Evan should decide whether to keep the name or switch; no spare name for the stage runtime is listed in PLAN.md.

## Sources

- https://crates.io/api/v1/crates/{bytelanes,diagsteno,paralanes,stagelanes,bytelane,paralane,stagelane} (all 404)
- https://api.github.com/search/repositories?q=bytelanes (and diagsteno, paralanes, stagelanes, stagelane, paralane, bytelane)
- https://api.github.com/users/kovirlabs/repos
- https://github.com/ghazalialshafi/stagelane (API metadata and README)
- https://docs.rs/crate/cubecl/latest ; https://github.com/tracel-ai/cubecl
- https://docs.rs/crate/vyre/latest ; https://docs.rs/crate/vyre/0.4.0/source/src/ops/hash/crc32.rs
- https://github.com/KomputeProject/kompute
- https://github.com/arrayfire/arrayfire ; https://www.intel.com/content/www/us/en/developer/articles/news/arrayfire-oneapi-september2022.html
- https://github.com/DTolm/VkFFT ; https://sc22.supercomputing.org/proceedings/tech_poster/tech_poster_pages/rpost143.html
- https://docs.rs/gpufft ; https://docs.rs/crate/wgsl-fft/latest/source/README.md
- https://github.com/psvri/arrow-gpu
- https://luma.gl/docs/api-reference/experimental/gpu-dataframe-operations
- https://devblogs.microsoft.com/directx/directstorage-1-1-now-available/
- https://docs.nvidia.com/cuda/nvcomp/index.html ; https://docs.nvidia.com/cuda/nvcomp/crc32.html ; https://docs.nvidia.com/cudf/26.10/cudf/io/io/
- https://hgpu.org/?p=15592 (OpenCL cryptographic library, AES-CTR)
- https://arxiv.org/pdf/2506.10092 ; https://arxiv.org/pdf/2609.24628 (GPU crossover studies)
- https://hwloc.readthedocs.io/ ; https://github.com/open-mpi/hwloc ; https://github.com/HadrienG2/hwlocality
- https://docs.rs/crate/gdt-cpus/latest
- https://github.com/Elzair/core_affinity_rs
- https://www.datadoghq.com/blog/engineering/introducing-glommio/ ; https://github.com/DataDog/glommio
- https://iggy.apache.org/blogs/2026/02/27/thread-per-core-io_uring/
- https://github.com/compio-rs/compio ; https://github.com/scylladb/seastar
- https://github.com/nicholassm/disruptor-rs
- https://doc.dpdk.org/guides/prog_guide/eventdev.html ; https://doc.dpdk.org/guides/sample_app_ug/eventdev_pipeline.html
- https://arxiv.org/pdf/1204.5402 (FastFlow tutorial) ; https://github.com/fastflow/fastflow
- https://github.com/FutureSDR/FutureSDR
- https://github.com/SeisComP/seiscomp ; https://www.seiscomp.de/doc/base/introduction.html
- https://isti.com/products/earthworm ; https://folkworm.ceri.memphis.edu/ew-doc/
- https://earthquake.usgs.gov/monitoring/nsmp/buildings/faq.php (OpenSHM)
- https://github.com/dagghe/pyOMA2 ; https://joss.theoj.org/papers/10.21105/joss.07656.pdf
- https://lanl.gov/projects/national-security-education-center/engineering/software/shm-data-sets-and-software.php
- https://www.analog.com/en/resources/technical-articles/open-source-reusable-software-stack.html
- https://pypi.org/project/freeFFT/0.0.8/ ; https://github.com/CoDeRgAnEsh/viBscope ; https://pmc.ncbi.nlm.nih.gov/articles/PMC9041184 ; https://doaj.org/article/eafb60c6d2d54d7babe808797bb203d5
- https://www.influxdata.com/solutions/predictive-maintenance/
- https://docs.rs/crate/embedded-dsp/0.5.1
- https://dewesoft.com/solutions/structural-health-monitoring
- https://github.com/whitfin/bytelines
