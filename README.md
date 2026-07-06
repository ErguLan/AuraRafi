- Built with AI-assisted development, but shaped through deliberate architectural decisions, iterative validation, and manual integration work.
- Thanks for taking the time to inspect the project and its current real state.


#  Rafi (Project)

Open-source hybrid engine for video games and electronic design.

Built in Rust for maximum performance and safety. Designed to run on low-end
hardware while scaling up to high-end systems.

This repository is in active development. Some systems are production-usable
inside the editor today, while others are intentionally marked as prepared,
experimental, or pending end-to-end runtime connection.

## Features

- **Hybrid Engine**: Game development (2D/3D) and electronic design in one tool
- **Performance Focused**: shared graphics runtime for Scene, Schematic, and PCB surfaces, using GPU hardware first when available and CPU software fallback for potato PCs
- **Visual Scripting**: No-code node editor with graph validation and basic executor
- **Runtime Foundations**: scene/runtime data, node persistence, and script-facing groundwork exist in the repo, while live game Play mode is currently staged during renderer/runtime consolidation
- **Electronics Design**: Schematic editor, simulation, DRC, exports, and synchronized PCB 2D workspace, unified under the same shared graphics runtime
- **Modern Editor**: Dark/light themes, modular panels, intuitive layout
- **AI-Ready**: Architecture prepared for AI agent integration via tool-calling when the runtime side matures
- **Modular**: 9 workspace crates plus the main editor binary, with clean domain boundaries
- **Multilingual**: English and Spanish support
- **Tested Modules**: Inline Rust tests are distributed across core, render, electronics, nodes, and AI crates

## Quick Start

```bash
# Install Rust
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh

# Clone and build
git clone https://github.com/AuraRafi/AuraRafi.git
cd AuraRafi
cargo check -p raf_editor

# Run the editor
cargo run -p aura_rafi_editor --release
```

If you are on Windows and use the GNU toolchain, see `SETUP.md` for the MinGW
step before running the editor.

What works today after launch:

- Game projects open with the scene editor, hierarchy, properties, asset browser, and a scene viewport routed through the shared graphics runtime (GPU when available, CPU fallback retained).
- Game runtime foundations remain in the repository, but live Play mode is currently temporarily disconnected while renderer/runtime truth is being stabilized.
- Electronics projects open with the schematic editor and can switch into PCB View for synchronized physical layout editing.
- Save/load, undo/redo, project persistence, DRC, DC simulation, SVG/BOM/netlist export, and localized UI are already integrated.
- Some roadmap systems are intentionally present as prepared architecture and are documented as such in the changelog and architecture docs.

## Architecture

| Crate | Purpose |
|-------|---------|
| `raf_core` | ECS, scene graph, command bus, events, configuration |
| `raf_render` | Shared graphics runtime for Scene/Schematic/PCB, CPU software fallback path, and prepared render abstraction/backends |
| `raf_editor` | Visual editor UI with egui/eframe |
| `raf_assets` | Asset importing, browsing, primitives |
| `raf_electronics` | Schematic editor domain, simulation, DRC, export pipeline, synchronized PCB layout model |
| `raf_nodes` | Visual node-based scripting, graph validation, executor |
| `raf_ai` | AI agent interface, tool registry, director and mesh-generation infrastructure |
| `raf_net` | Networking protocol stubs |
| `raf_hardware` | Serial, sensors, actuators, robotics and ML-facing hardware models |

Current implementation profile:

- The active editor surfaces use a shared graphics runtime: GPU hardware first when available, CPU software fallback retained for low-end compatibility.
- The game runtime is currently staged rather than fully active: the repository keeps scene/runtime foundations, but live Play mode is temporarily disconnected while renderer and runtime truth are consolidated.
- Electronics is currently the most end-to-end vertical: schematic, checks, simulation, export, and PCB 2D sync already live in the editor, unified under the same shared graphics runtime.
- The repository favors modular staging: some systems are usable now, others are intentionally prepared without being wired into final runtime flows yet.

## Documentation

- [Architecture](docs/ARCHITECTURE.md) - System design and crate overview
- [Getting Started](docs/GETTING_STARTED.md) - Installation and first steps
- [Contributing](docs/CONTRIBUTING.md) - How to contribute
- [Roadmap](docs/ROADMAP.md) - Planned features and milestones
- [Setup](SETUP.md) - Windows GNU toolchain setup and editor run notes
- [Changelog](CHANGELOG.md) - Actual feature progression by version

Documentation note:

- `README.md` gives a quick project-facing overview.
- `CHANGELOG.md` is the best source for what changed version by version.
- `docs/ARCHITECTURE.md` separates active systems from prepared infrastructure more precisely than the short README summary.

## Technology

- **Language**: Rust (2021 edition)
- **Rendering**: shared graphics runtime via `RenderRuntime` + `ApiGraphicBasic`, using GPU hardware execution when available and CPU software fallback when not; the scene viewport keeps a dedicated CPU scene path as the reference fallback implementation
- **UI**: egui + eframe
- **ECS**: hecs
- **Math**: glam (SIMD-optimized)
- **Serialization**: serde + RON
- **Localization**: JSON dictionaries through `raf_core::i18n::t()`
- **Testing Style**: Inline `#[test]` modules across workspace crates

## License

Soon...