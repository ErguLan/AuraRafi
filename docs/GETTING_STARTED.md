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
   cargo build --release
   ```

3. Run the editor:
   ```bash
   cargo run -p aura_rafi_editor --release
   ```

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

## Editor Overview

### Menu Bar

- **File**: New project, save, settings, exit to hub
- **Edit**: Undo/redo operations
- **View**: Toggle grid, switch between Scene and Schematic views
- **Project**: Current project information

### Scene View (Game Projects)

The central viewport shows your game scene:

- **Pan**: Middle-mouse drag or right-mouse drag
- **Zoom**: Scroll wheel
- **Reset View**: Double-click
- **Tools**: Select (Q), Move (W), Rotate (E), Scale (R)

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

### Hierarchy Panel (Left)

Shows the scene tree structure:
- Click to select entities
- Collapsible groups for parent-child relationships
- "Add Entity" button at the bottom

### Properties Panel (Right)

Edit the selected entity:
- **Name**: Rename the entity
- **Transform**: Position (X, Y, Z), Rotation, Scale
- **Visibility**: Toggle visibility

### Bottom Panels (Tabbed)

- **Console**: Log messages with severity filters (Info, Warning, Error)
- **Assets**: Browse and filter project assets
- **Node Editor**: Visual scripting with connected nodes
- **AI Chat**: AI assistant interface (coming soon)

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
  scripts/          User scripts (future)
```

## Keyboard Shortcuts

| Shortcut | Action |
|----------|--------|
| Q | Select tool |
| W | Move tool |
| E | Rotate tool |
| R | Scale tool / Rotate component (schematic) |
| Delete | Remove selected entity/node/component/wire |
| Ctrl+D | Duplicate selected component (schematic) |
| Escape | Cancel current operation / Close overlay |
| Middle-drag | Pan viewport |
| Scroll | Zoom viewport |
| Double-click | Reset viewport |
| Right-click | Context menu (schematic) |

## Next Steps

- Explore the [Architecture](ARCHITECTURE.md) document to understand the codebase
- Check [Contributing](CONTRIBUTING.md) to help improve AuraRafi
- See the [Roadmap](ROADMAP.md) for planned features
