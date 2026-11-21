# paralanes

**Status: pre-alpha.** Nothing here is built or measured yet.

Rust software that gives commodity hardware two mainframe traits. Transaction work runs as an assembly line of pinned core stages, and GPUs act as a channel subsystem that moves and transforms bulk data so the stage cores never touch it. This umbrella repo holds the spec ([docs/SPEC.md](docs/SPEC.md)), the build plan ([docs/PLAN.md](docs/PLAN.md)), the atlas of measured CPU-versus-GPU crossover sizes, the network join between machines, and the appliance image.

Part of the paralanes project: `paralanes` (umbrella: spec, plan, atlas,
network join, appliance image), `bytelanes` (GPU data ops), `stagelanes`
(stage placement runtime) and `diagsteno` (strain and vibration processing).

## Licence

GNU General Public License v3.0 only. See [LICENSE](LICENSE).
