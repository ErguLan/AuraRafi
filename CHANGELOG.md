# Changelog

All notable changes to AuraRafi will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

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
- Electronic components (Resistor, Capacitor, LED)
- Schematic with auto-designator assignment
- Electrical connectivity test
- Component library with built-in parts
- Visual scripting nodes (On Start, On Update, Print, If, Add)
- Node graph with connections and validation
- AI tool registry with 6 default tools
- AI provider configuration (OpenRouter, OpenAI, GenAI, Claude)
- Network protocol definitions
- Project documentation (Architecture, Getting Started, Contributing, Roadmap)
