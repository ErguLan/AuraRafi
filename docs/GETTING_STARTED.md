# Getting Started with AuraRafi

This guide walks you through setting up and using AuraRafi for the first time.

## Installation

### From Source

1. Install Rust via [rustup.rs](https://rustup.rs):
   ```bash
   curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
   ```

2. Clone and build:
   ```bash
   git clone https://github.com/AuraRafi/AuraRafi.git
   cd AuraRafi
   cargo check -p raf_editor
   ```

3. Run the editor:
   ```bash
   cargo run -p aura_rafi_editor --release
   ```

4. On Windows with the GNU toolchain, complete the MinGW step described in `../SETUP.md` if you have not done it yet.

## First Launch

When you first launch AuraRafi, you will see:

1. **Loading Screen** - A brief splash screen while the engine initializes
2. **Project Hub** - The main starting point

## Creating a Project

### Game Project

1. From the Project Hub, click **Create Game**
2. Enter a project name and choose a save location
3. Click **Create Project**
4. The editor opens with a default scene containing:
   - Scene Root
   - Main Camera (positioned at 0, 5, 10)
   - Directional Light

### Electronics Project

1. From the Project Hub, click **Create Electronics**
2. Enter a project name and choose a save location
3. Click **Create Project**
4. The editor opens in **Schematic View** with:
   - An empty schematic canvas
   - The component library sidebar
   - Save/load support through `schematic.ron`
   - A secondary **PCB View** workspace available from the top switch row and the View menu

## Editor Overview

### Menu Bar

- **File**: New project, save, settings, exit to hub
- **Edit**: Undo/redo, duplicate, delete, select all
- **View**: Toggle grid, switch between Scene, Schematic, and PCB views depending on project type
- **Project**: Current project information and per-project runtime/render settings
- **Help**: Shortcuts reference and version/status information

### Scene View (Game Projects)

The central viewport shows your game scene in 2D or 3D:

- **Orbit Camera**: Left-mouse drag (3D mode)
- **Pan**: Middle-mouse drag
- **Zoom**: Scroll wheel
- **Reset View**: Double-click
- **Tools**: Select (Q), Move (W), Rotate (E), Scale (R)
- **Render Style**: Cycle with Z key (Solid+Wire / Wireframe / Solid Only)
- **2D/3D Toggle**: Click buttons at top-center
- **Change Color**: Use the color picker in the Properties panel (right side)
- **Change Shape**: Use the primitive dropdown in Properties > Shape
- **Multi-select**: Shift+Click in hierarchy/viewport, Ctrl+A for all entities
- **Edit Mode**: Tab toggles Object / Vertex mode foundations

### Play Mode

Game projects now have an editor-integrated runtime slice:

- Click **Run Game** to enter Play mode
- The engine clones the current scene into a temporary runtime scene
- Saved node graphs from `nodes.ron` execute `On Start` and `On Update`
- Attached `.rhai` behaviors can read input, access `self` / `parent` / paths, set variables, move entities, trigger audio, and react to trigger overlaps
- Simple rigid body physics, gravity, damping, and trigger-only collider checks run on the runtime copy
- Click **Stop Game** to leave Play mode and return to the edit document unchanged

### Schematic View (Electronics Projects)

The schematic editor for circuit design:

- **Place Component**: Select from the library sidebar, click on canvas
- **Draw Wire**: Click "Draw Wire" button or use context menu, click two points
- **Select**: Click on a component or wire
- **Move Component**: Click and drag a selected component
- **Rotate**: Select component, press R or use right-click menu
- **Duplicate**: Select component, press Ctrl+D or use right-click menu
- **Edit Value**: Right-click a component and select "Edit Value"
- **Delete**: Select and press Delete, or use right-click menu
- **Right-click Menu**: Context-sensitive options for component/wire/canvas
- **Pan**: Middle-mouse drag or right-mouse drag (when not on component)
- **Zoom**: Scroll wheel
- **Electrical Test**: Click the button to run DRC checks
- **Simulation**: Use the build/run action on electronics projects to run DC simulation and report voltages/currents in the console
- **Save Flow**: Ctrl+S saves the project and keeps the PCB layout synchronized from the schematic

### PCB View (Electronics Projects)

The PCB editor is a physical 2D board workspace synchronized from the schematic:

- **Switch View**: Use the top workspace switch or View menu
- **Select**: Click components, traces, or airwires
- **Move Components**: Drag placements on the board canvas
- **Route**: Route selected airwires into traces with the Route tool
- **Outline**: Draft or close the board outline in the Outline tool
- **Layers**: Inspect top/bottom copper assignments in the properties panel
- **Persistence**: The board document is saved as `pcb_layout.ron`
- **Sync Model**: Schematic remains the electrical source of truth; PCB preserves manual physical placement while refreshing nets/components on save

### Hierarchy Panel (Left)

Shows the scene tree structure:
- Click to select entities
- Collapsible groups for parent-child relationships
- "Add Entity" button at the bottom

For electronics projects, this panel changes role:

- In Schematic View it lists components and wiring-related document data
- In PCB View it lists board, components, traces, and airwires

### Properties Panel (Right)

Edit the selected entity:
- **Name**: Rename the entity
- **Transform**: Position (X, Y, Z), Rotation, Scale
- **Visibility**: Toggle visibility

In electronics workspaces, the same area becomes a domain-specific inspector:

- Schematic: component values, footprint, orientation, selection context
- PCB: component placement, copper layer, trace width, lock state, board status, missing footprints

### Bottom Panels (Tabbed)

- **Console**: Log messages with severity filters (Info, Warning, Error)
- **Assets**: Browse and filter project assets
- **Node Editor**: Visual scripting with connected nodes
- **AI Chat**: AI assistant interface structure (UI exists, provider/runtime integration is still pending)

## Node Editor

The visual scripting system allows logic creation without code:

1. Switch to the **Node Editor** tab at the bottom
2. **Right-click** on the canvas to open the Add Node palette
3. Choose a node type (On Start, Print, If Branch, etc.)
4. **Drag** from an output pin to an input pin to connect nodes
5. **Click** a node header to select it
6. Press **Delete** to remove the selected node

### Node Types

- **Events**: On Start, On Update (triggers)
- **Actions**: Print (output to console)
- **Logic**: If Branch (conditional flow)
- **Math**: Add (arithmetic operations)

The node executor is now wired into editor Play mode through `nodes.ron`, executing `On Start` and `On Update` entries from saved graphs.

## Script Behaviors

Game entities can also attach external `.rhai` scripts from the Assets/Behaviors workflow.

- Attach a `.rhai` file from `assets/scripts/`
- Use `fn on_start(ctx)`, `fn on_update(ctx)`, and `fn on_trigger_enter(ctx, other_path)` as entry points
- Runtime context exposes entity path/name, parent path/name, custom variables, simple movement helpers, audio triggers, and keyboard state
- `.rs`, `.cpp`, and `.lua` files can still be tracked in the editor, but direct in-editor execution currently targets `.rhai`

## Settings

Access settings from the menu bar or Project Hub:

- **Simple Mode**: Toggle to hide advanced parameters (ideal for beginners)
- **Theme**: Dark, Light, or System
- **Font Size**: 10-24px
- **UI Scale**: 0.5x-2.0x
- **Language**: English or Spanish (all UI text translates)
- **Target Platform**: Desktop, Mobile, Web (WASM), Cloud/Streaming, Console
- **Render Quality**: Potato (0) to High (3)
- **FPS Limit**: 15-240
- **VSync**: On/Off
- **Grid**: Visibility, snap, size (hidden in Simple Mode)
- **Auto-save**: Interval in seconds (hidden in Simple Mode)
- **Units**: Metric or Imperial (hidden in Simple Mode)

Settings are saved to your system config directory.

## Project Files

Each project creates this directory structure:

```
MyProject/
  project.ron       Project metadata
  assets/           Imported assets (images, models, audio)
  scenes/           Scene files
   scripts/          User scripts / native experiments
```

Additional persisted documents depend on project type:

- Game projects save editor scene data as `scene.ron` and node graphs as `nodes.ron`
- Electronics projects save `schematic.ron` and `pcb_layout.ron`

## Keyboard Shortcuts

| Shortcut | Action |
|----------|--------|
| Q | Select tool |
| W | Move tool |
| E | Rotate tool |
| R | Scale tool / Rotate component (schematic) |
| Z | Cycle render style (Solid / Wire / Fill) |
| Tab | Toggle Object / Vertex edit mode |
| Ctrl+S | Save current project/document |
| Ctrl+Z / Ctrl+Y | Undo / Redo |
| Delete | Remove selected entity/node/component/wire |
| Ctrl+D | Duplicate selected component (schematic) |
| Ctrl+A | Select all supported items in the active workspace |
| Escape | Cancel current operation / Close overlay |
| Left-drag | Orbit camera (3D viewport) |
| Middle-drag | Pan viewport |
| Scroll | Zoom viewport |
| Double-click | Reset viewport |
| Right-click | Context menu (schematic) |

## Next Steps

- Explore the [Architecture](ARCHITECTURE.md) document to understand the codebase
- Check [Contributing](CONTRIBUTING.md) to help improve AuraRafi
- See the [Roadmap](ROADMAP.md) for planned features
- Read `../CHANGELOG.md` to compare roadmap intent against implemented milestones
