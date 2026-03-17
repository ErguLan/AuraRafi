# AuraRafi — Agent Context

You are working on **AuraRafi**, an open-source engine for both **video games** and **electronics projects** (schematics, PCB, simulation). It's built primarily in Rust with egui/eframe for the UI and wgpu for rendering.

## Project Structure

```
ProyectRaf/
├── editor/                  # Binary crate — launches the editor window
│   └── src/main.rs          # Entry point, icon loading, window config
├── crates/
│   ├── raf_core/            # Core types: Project, SceneGraph, Config, Events, Commands
│   ├── raf_editor/          # Editor UI (the big one)
│   │   └── src/
│   │       ├── app.rs       # Main app: loading screen, project hub, editor layout
│   │       ├── theme.rs     # Color tokens + theme application (dark/light)
│   │       ├── lib.rs       # Crate entry, re-exports AuraRafiApp
│   │       └── panels/      # All editor panels:
│   │           ├── viewport.rs       # 2D scene viewport (needs 3D rendering)
│   │           ├── node_editor.rs    # Visual scripting with connected nodes
│   │           ├── schematic_view.rs # Electronics schematic editor
│   │           ├── hierarchy.rs      # Scene tree
│   │           ├── properties.rs     # Selected entity properties
│   │           ├── console.rs        # Log output
│   │           ├── asset_browser.rs  # Asset management (basic)
│   │           ├── ai_chat.rs        # AI assistant panel (placeholder)
│   │           └── settings_panel.rs # Settings UI
│   ├── raf_render/          # Rendering (wgpu setup exists, actual rendering NOT implemented)
│   ├── raf_assets/          # Asset pipeline (structure only)
│   ├── raf_electronics/     # Electronic components, schematic, component library
│   ├── raf_nodes/           # Node graph system for visual scripting + executor
│   ├── raf_ai/              # AI integration (placeholder)
│   ├── raf_net/             # Networking (placeholder)
│   └── raf_hardware/        # Hardware: serial, sensors, actuators, robot, ML
├── docs/                    # ARCHITECTURE.md, CONTRIBUTING.md, GETTING_STARTED.md, ROADMAP.md
├── .cargo/config.toml       # GNU target config (no MSVC needed)
├── SETUP.md                 # How to build and run from source
├── CHANGELOG.md
└── README.md
```

## Tech Stack

- **Language**: Rust (edition 2021)
- **UI Framework**: egui 0.30 + eframe 0.30 (immediate mode GUI)
- **GPU**: wgpu 23 (Vulkan/DX12/Metal backend)
- **Math**: glam 0.29
- **Serialization**: serde + ron (human-readable config files)
- **Build target**: `x86_64-pc-windows-gnu` (no Visual Studio needed, uses MinGW)

## Current State (v0.1.0)

### What Works
- Full editor UI with loading screen, project hub, and main editor layout
- Project creation/loading/saving (ron serialization)
- Dark and light themes with warm orange accent (`#D4771A`)
- Node editor with bezier connections, drag-to-connect, palette
- Schematic editor with component library, wire drawing, electrical tests
- 2D viewport with grid, pan/zoom, tool selection (Q/W/E/R)
- Console panel with log filtering
- Properties panel for entity transforms
- Hierarchy panel for scene tree
- Spanish and English UI translations (partial)
- Custom window icon (metallic R logo)
- Subtle Yoll branding only on loading screen
- Simple Mode toggle (hides advanced UI)
- Target platform selector (Desktop/Mobile/Web/Cloud/Console)
- Magnet electronic component with field simulation
- Node graph executor (basic interpreter)
- Serial port communication protocol (structure)
- Sensor/Actuator/Robot/ML data models (structure)
- JLCPCB/PCBWay Gerber export structure (placeholder)
- Circuit sharing via RON serialization

### What Does NOT Work Yet (Priority Order)
1. **3D Rendering** -- `raf_render` has wgpu device setup but zero actual rendering. No meshes, no shaders, no lighting, no camera. The viewport is just a 2D grid. This is the #1 priority.
2. **Node Execution (partial)** -- Basic executor exists (walks flow chains, evaluates data pins, handles On Start/Print/If/Add) but does not yet run from the editor UI play button. Missing: variable nodes, loops, entity manipulation.
3. **Asset Pipeline** -- No importing, no thumbnail generation, no hot-reload. The asset browser panel is a shell.
4. **AI Integration** -- `raf_ai` and the chat panel are placeholders. No LLM provider is connected.
5. **Hardware I/O** -- `raf_hardware` has data models and protocols defined but no actual serial port communication (needs `serialport` crate).
6. **PCB 3D Layout** -- Schematic works, but no PCB board layout view. Gerber export blocked by this.
7. **Networking** -- `raf_net` is protocol definitions only.

## Architecture Principles

- **Command Bus**: All state mutations go through `raf_core::command::CommandBus` for undo/redo support
- **ECS-ready**: Scene uses `hecs` ECS, but components aren't fully utilized yet
- **Modular crates**: Each feature area is a separate crate for clean dependencies
- **Performance focused**: Target is to run on low-spec hardware. Use adaptive quality, LOD, efficient rendering.

## Key Design Decisions

- The editor uses **egui immediate mode** — no retained widget tree. Each frame redraws everything.
- Theme colors are **public constants** in `theme.rs`, used across all panels for consistency.
- The app has a state machine: `AppScreen` enum controls which screen is shown (Loading → ProjectHub → NewProject → Editor → Settings).
- Panel functions follow the pattern: `fn show(&mut self, ui: &mut egui::Ui)` — they take a mutable UI reference and draw themselves.
- Translations use inline `if is_es { "Spanish" } else { "English" }` checks. No i18n framework yet, but `fluent` crate is available.

## Building & Running

```bash
rustup default stable-x86_64-pc-windows-gnu  # one-time
cargo run -p aura_rafi_editor                 # that's it
```

## Branding

- **Yoll** credit appears ONLY on the loading screen, bottom, very subtle
- Website: yoll.site
- Don't make it invasive. Don't put it in every panel.

## Code Style

- Comments in English
- UI text supports English + Spanish
- Use the theme constants from `theme.rs`, don't hardcode colors
- Panels are standalone structs with `Default` implementation
- Keep functions focused, avoid 500+ line functions

## When Continuing This Project

1. Read `docs/ROADMAP.md` for planned milestones
2. Read `docs/ARCHITECTURE.md` for detailed crate descriptions
3. The viewport (`viewport.rs`) needs the most work — it's currently just a 2D grid
4. Check `Cargo.toml` workspace deps before adding new crates
5. Test with `cargo check` (fast) before `cargo run` (slow first time)
6. The `.cargo/config.toml` sets target-dir to `target_gnu` to avoid MSVC lock conflicts
