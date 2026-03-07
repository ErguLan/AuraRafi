# AuraRafi Roadmap

This document outlines the development roadmap for AuraRafi.

## Current Status: v0.1.0 (Foundation)

The engine foundation is in place with these working systems:

- [x] Cargo workspace with 8 modular crates
- [x] ECS (Entity Component System) via hecs
- [x] Scene graph with parent-child hierarchy
- [x] Command bus with undo/redo support
- [x] Event bus for decoupled communication
- [x] Project management (create, load, save)
- [x] Configuration system with RON persistence
- [x] Editor UI with panels (egui/eframe)
- [x] Theme system (dark/light + orange accent)
- [x] Visual node editor with connections
- [x] Schematic editor with component library
- [x] Console with log filtering
- [x] Asset browser with type filtering
- [x] AI tool registry structure
- [x] Networking protocol definitions

## v0.2.0 - Rendering (Planned)

The next milestone focuses on actual GPU rendering:

- [ ] wgpu render pass integration in editor viewport
- [ ] WGSL vertex and fragment shaders
- [ ] Basic mesh rendering (primitives: cube, sphere, plane, cylinder)
- [ ] Flat shading with single directional light
- [ ] Camera orbit controls in 3D viewport
- [ ] Grid rendering on GPU (replacing CPU painter)
- [ ] Wireframe mode toggle
- [ ] Color/material assignment per entity

## v0.3.0 - Editor Polish

- [ ] Drag-and-drop asset importing
- [ ] Asset thumbnail preview generation
- [ ] Context menus (right-click) throughout editor
- [ ] Keyboard shortcut customization
- [ ] Multi-entity selection
- [ ] Entity duplication (Ctrl+D)
- [ ] Scene serialization to RON files
- [ ] Auto-save implementation
- [ ] Hot-reload for assets using file watcher

## v0.4.0 - Visual Scripting

- [ ] Node graph execution engine
- [ ] Variable get/set nodes
- [ ] Loop nodes (For, While)
- [ ] Comparison nodes (>, <, ==, !=)
- [ ] Entity manipulation nodes (Spawn, Destroy, SetPosition)
- [ ] Input event nodes (Key Press, Mouse Click)
- [ ] Timer/delay nodes
- [ ] Node graph save/load

## v0.5.0 - Electronics

- [ ] Component rotation and mirroring
- [ ] Automatic net naming
- [ ] Design Rule Check (DRC)
- [ ] BOM (Bill of Materials) generation
- [ ] Gerber file export
- [ ] Additional components (transistors, ICs, connectors)
- [ ] Custom component creation
- [ ] PCB layout view (basic)

## v0.6.0 - Internationalization

- [ ] Fluent-based localization system
- [ ] Complete English translation file
- [ ] Complete Spanish translation file
- [ ] Community translation infrastructure
- [ ] RTL language support preparation

## v0.7.0 - Advanced Rendering

- [ ] PBR (Physically Based Rendering) materials
- [ ] Point and spot lights
- [ ] Shadow mapping
- [ ] Ambient occlusion (SSAO)
- [ ] Bloom post-processing
- [ ] Anti-aliasing (FXAA, MSAA)
- [ ] 2D sprite rendering
- [ ] Texture loading and UV mapping

## v0.8.0 - Game Runtime

- [ ] Separate game runtime binary
- [ ] Scene loading and initialization
- [ ] Game loop with fixed timestep
- [ ] Input handling system
- [ ] Basic collision detection
- [ ] Simple physics (gravity, velocity)
- [ ] Audio playback (wav, ogg)

## v0.9.0 - AI Integration

- [ ] LLM provider connection (API integration)
- [ ] Tool-calling execution pipeline
- [ ] Scene state context for AI
- [ ] AI-assisted entity creation
- [ ] AI-assisted debugging
- [ ] Chat history persistence

## v1.0.0 - Release

- [ ] Stability and performance optimization
- [ ] Complete documentation
- [ ] Example projects (game + electronics)
- [ ] Installer/packaging for Windows, macOS, Linux
- [ ] Website and community resources
- [ ] Contribution guidelines finalized

## Future (Post-1.0)

- Networking / multiplayer support
- Plugin system
- Marketplace for assets and components
- Ray tracing (RTX) rendering path
- VR/AR support
- Mobile target platforms
- Built-in code editor
- Processor/FPGA design tools
- Circuit simulation (SPICE-like)
