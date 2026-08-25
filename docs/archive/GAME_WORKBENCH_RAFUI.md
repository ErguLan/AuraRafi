# Game workbench RafUI

This document records the current Game workbench boundary after the interface
recovery pass.

## Frontend contract

Hierarchy, Search, Console, Assets and Nodes are retained RafUI surfaces. The
application coordinates them through typed intents; a surface never mutates a
scene, graph, project or filesystem directly. The native compositor only places
a completed RafUI surface during the window lifecycle.

The visual language is shared: dark industrial surfaces, compact neutral rows,
white and muted text, and orange only for focus, selection and primary actions.
World is visible in Hierarchy as an explicit Work in progress state until a
real world backend is connected. Project is indexed by Search instead of being
a permanent asset browser tab; Project Settings remains available in the
downbar as the project configuration surface.

## Restored behavior

- Hierarchy tabs expose Hierarchy, Assets, World, Bookmarks and Search.
- Camera bookmarks use the existing editor-camera persistence and three real
  slots.
- Search combines command names, scene entities, assets and project files.
- Assets search/filter state stays in the retained host; recursive discovery
  and refresh run through the worker-backed project catalog. The retained
  toolbar also restores the old create-script flow through the real
  `script.create` command and its Rust, Rhai and C++ templates. External files
  dropped over the Assets tab are copied by the catalog worker with collision
  suffixes, then indexed in the next immutable snapshot.
- Console keeps command history, autocomplete, severity filters, structured
  output and JSON disclosure while caching its retained surface by revision.
- Nodes load and save the session-scoped `nodes.ron` document and support graph
  switching, graph creation/deletion, node creation, selection, deletion,
  copy/paste, positioned node dragging, middle-button panning, zoom, a
  contextual creation palette, minimap selection, pin selection and typed pin
  connections through the existing `raf_nodes` runtime types. Connections are
  presented as retained orthogonal cable segments; the canvas does not claim a
  runtime evaluator for node types that have no executor.
- Game downbar tabs are Console, Assets, Project Settings, Nodes and Agent.
  Electronics uses Console, Assets, Project Settings, Agent, DRC and
  Simulation. Persisted layouts are sanitized against the active project type,
  so DRC and Simulation never appear in Game.
- The Game viewport exposes a retained overlay toolbar for Select, Move,
  Rotate, Scale, shading, grid, labels, 2D/3D view, focus and reset. Its hit
  region is excluded from camera navigation and transform input.
- Global Settings expose the restored viewport contracts: unlimited FPS,
  experimental theme amount, focus lock, W/S inversion, WASD speed, uniform
  scale and gizmo growth. Project Settings disable dependent controls and
  reject invalid scripting, depth, streaming and GPU-quality combinations at
  the application boundary.

## Performance guardrails

The UI bridge must not treat passive pointer movement as a reason to rebuild a
surface or upload a texture. Click, scroll, drag, keyboard and active tooltip
input remain explicit invalidation causes. Catalog discovery is worker-backed,
and retained surface documents are rebuilt only when their model revision or
visual state changes.

Any future hover animation must be isolated to a lightweight overlay or use a
bounded sampling policy. It must not restore per-pointer-move full-surface
renders in the Game viewport.

## Historical recovery rule

Historical files were used as behavior references through Git, including the
former `asset_browser.rs`, `console.rs` and `node_editor.rs` contracts in
commit `d6ed969`. Their working behavior was adapted to RafUI and
ApiGraphicBasic; the retired controls, layout and paint code were not
re-mounted.
