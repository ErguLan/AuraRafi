- The most of code has been developed by AI through the use of Yoll Ide, VS Code, Antigravity and Powershell
- Thanks for at least see the project


# AuraRafi

Open-source hybrid engine for video games and electronic design.

Built in Rust for maximum performance and safety. Designed to run on low-end
hardware while scaling up to high-end systems.

## Features

- **Hybrid Engine**: Game development (2D/3D) and electronic design in one tool
- **Performance Focused**: Adaptive quality from "Potato" to "High" presets
- **Visual Scripting**: No-code node editor for creating game logic
- **Electronics Design**: Schematic editor with component library and electrical tests
- **Modern Editor**: Dark/light themes, modular panels, intuitive layout
- **AI-Ready**: Architecture prepared for AI agent integration via tool-calling
- **Modular**: 8 independent crates with clean interfaces
- **Multilingual**: English and Spanish support

## Quick Start

```bash
# Install Rust
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh

# Clone and build
git clone https://github.com/AuraRafi/AuraRafi.git
cd AuraRafi
cargo build --release

# Run the editor
cargo run -p aura_rafi_editor --release
```

## Architecture

| Crate | Purpose |
|-------|---------|
| `raf_core` | ECS, scene graph, command bus, events, configuration |
| `raf_render` | GPU rendering via wgpu (Vulkan/Metal/DX12) |
| `raf_editor` | Visual editor UI with egui/eframe |
| `raf_assets` | Asset importing, browsing, primitives |
| `raf_electronics` | Schematic editor, component library, PCB design |
| `raf_nodes` | Visual node-based scripting (no-code) |
| `raf_ai` | AI agent interface and tool registry |
| `raf_net` | Networking protocol stubs |

## Documentation

- [Architecture](docs/ARCHITECTURE.md) - System design and crate overview
- [Getting Started](docs/GETTING_STARTED.md) - Installation and first steps
- [Contributing](docs/CONTRIBUTING.md) - How to contribute
- [Roadmap](docs/ROADMAP.md) - Planned features and milestones

## Technology

- **Language**: Rust (2021 edition)
- **GPU**: wgpu (Vulkan, Metal, DirectX 12)
- **UI**: egui + eframe
- **ECS**: hecs
- **Math**: glam (SIMD-optimized)
- **Serialization**: serde + RON

## License

Dual-licensed under MIT and Apache 2.0. See LICENSE files for details.
