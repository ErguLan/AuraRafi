# Changelog

All notable changes to AuraRafi will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [0.1.1] - 2026-03-16

### Added

- **raf_hardware** crate: serial port abstraction, sensor data model, actuator commands, robot control interface, ML training data bridge
- **Node executor** (raf_nodes): walks flow chains, evaluates data pins, handles On Start/Print/If/Add nodes with safety step limit
- **Magnet** electronic component: N/S poles, field strength in Tesla, polarity, parse "Weak"/"Strong"/"Neodymium"/"0.5T"
- **Gerber export structure** for JLCPCB and PCBWay: manufacturer-specific layer definitions, placeholder until PCB 3D layout is built
- **Circuit sharing**: RON serialization/deserialization for schematics (share_circuit / load_shared_circuit)
- **Simple Mode** toggle in EngineSettings: hides advanced parameters (grid size, auto-save, multithreading, units) for beginners
- **Target Platform** selector: Desktop, Mobile, Web (WASM), Cloud/Streaming, Console - with display names and descriptions
- **Headless mode** flag in settings (structural, for future cloud/server rendering)
- **Responsive layout** flag in settings (structural, for future mobile/tablet UI)
- Settings panel: fully translated ES/EN, Simple Mode toggle at top, target platform dropdown with info labels
- Schematic editor: right-click context menu (rotate/delete/duplicate/edit value on components, delete on wires, wire mode/test on canvas)
- Schematic editor: drag-to-move components after placement
- Schematic editor: wire selection and deletion (point-to-segment hit testing)
- Schematic editor: component rotation (R key + context menu, rotation-aware pin rendering)
- Schematic editor: inline value editing via modal window
- Schematic editor: component duplication (Ctrl+D + context menu)
- Schematic editor: closeable test results overlay (Esc or click outside)
- Schematic editor: all UI text translated ES/EN via is_es flag from app settings

## [0.1.0] - 2026-03-01

### Added

- Initial project structure with 8 modular crates
- Entity Component System (ECS) with hecs integration
- Flat scene graph with parent-child hierarchy
- Command bus for undo/redo and AI tool-calling
- Event bus for decoupled module communication
- Engine configuration with theme, language, and render presets
- Project management (create, load, save, recent projects)
- Editor application with loading screen, project hub, and main editor
- Dark and light theme system with orange accent (#D4771A)
- Viewport panel with grid, axis lines, zoom/pan, and tool shortcuts
- Hierarchy panel with scene tree and entity selection
- Properties panel with transform editing
- Console panel with log filtering
- Asset browser panel with type filtering
- Visual node editor with drag-to-connect bezier connections
- Schematic view panel with component library and wire drawing
- AI chat panel structure (not yet functional)
- Settings panel (theme, language, quality, editor prefs)
- Asset importer with type detection
- 3D primitive types (Cube, Sphere, Cylinder, Plane)
- Electronic components (Resistor, Capacitor, LED) with SimModel
- Schematic with auto-designator assignment
- Electrical connectivity test
- Component library with built-in parts
- DC simulation engine (Modified Nodal Analysis, Gaussian elimination)
- Design Rule Check (6 rules: floating pins, missing value, isolated, unnamed nets, short circuit, LED without resistor)
- Netlist generation (union-find algorithm, rotation-aware pin positions)
- SVG export (styled, rotation-aware)
- BOM CSV export (grouped by value/footprint with quantities)
- Text netlist export
- Visual scripting nodes (On Start, On Update, Print, If, Add)
- Node graph with connections and validation
- Node graph compiler stub (validates connectivity)
- AI tool registry with 6 default tools
- AI provider configuration (OpenRouter, OpenAI, GenAI, Claude)
- Network protocol definitions
- Project documentation (Architecture, Getting Started, Contributing, Roadmap)
