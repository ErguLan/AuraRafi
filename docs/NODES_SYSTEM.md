# Nodes authoring system

This document describes the visual-node authoring path that is active in the
native editor. It is intentionally narrower than the future scripting/runtime
architecture: editing a graph is available now; executing that graph from
Play is not part of this contract.

## Ownership and modules

The persisted model lives in `crates/raf_nodes`:

- `node.rs` defines `Node`, typed `NodePin`, categories, and persisted
  `NodeProperty` values.
- `graph.rs` owns nodes and connections, checked connection rules, and graph
  diagnostics. `connect` remains a compatibility primitive; editor mutations
  use `try_connect`.
- `catalog.rs` is the single built-in node catalog and factory for Game and
  Electronics authoring. Stable slugs and translation keys prevent the editor
  from maintaining a second list of node types.
- `compiler.rs` currently validates the graph and returns compatibility message
  summaries plus structured diagnostics. The editor localizes those diagnostics;
  the compiler does not produce executable code or start a runtime.

The native editor presentation lives in `crates/raf_editor`:

- `panels/nodes_surface.rs` declares the retained RafUI surface: palette,
  search, canvas, circular node bodies, connector dots, wires, and Inspector.
- `panels/nodes_canvas.rs` contains canvas geometry, grid, port, and wire paint
  helpers. It emits ordinary RafUI quads; it is not a second renderer.
- `panels/nodes_catalog.rs` maps domain slugs to icons, category keys, pin keys,
  and other presentation details.
- `panels/nodes_surface_host.rs` keeps transient search, property drafts, and
  the two-click pending-pin interaction out of the declarative surface.
- `nodes_history.rs` provides a bounded authoring-only undo/redo stack.
- `native_editor_runtime.rs` owns graph selection, drag commits, property
  changes, checked connections, validation state, and history.
- `native_editor_commands.rs` translates retained UI commands into those
  runtime methods. There is no direct mutation path from a RafUI node.

`native_project_controller.rs` persists the active session graph in
`nodes.ron` alongside the scene document. Loading a project restores the
graph; loading a different session replaces it and clears transient history.

## Current interaction contract

- Add a node from the searchable palette.
- Select a node with the pointer, keyboard focus, or Enter.
- Drag a node to reposition it. The movement is one undoable document change,
  not one history entry per pointer frame.
- Select one output and one input connector dot to create a connection. Direction,
  duplicate links, occupied inputs, node self-links, and incompatible types are
  rejected by `NodeGraph::try_connect`.
- Click a rendered wire to disconnect it.
- Edit declared node properties in the right Inspector and press Enter or
  leave the control through its submit action to commit the value.
- Use Delete on a focused node, Ctrl+Z/Ctrl+Y for node history, and the
  Inspector/toolbar Validate action for diagnostics.
- The graph canvas supports independent horizontal and vertical scrolling and
  retained zoom controls. Nodes show only their identity and connector dots;
  pin names, types, and editable values remain in the right Inspector. Long
  labels use an explicit overflow policy.

All visible labels, categories, node names, pin names, types, descriptions,
and authoring messages use `raf_core/locales/en.json` and `es.json`. Runtime
values such as a user-entered property or UUID are literal values, not
translation keys.

## Scope boundary

The Nodes document is authoring state. This pass does not add or advertise:

- Play/Stop integration or a game runtime loop;
- scene-mutating node execution;
- a separate script editor;
- a second graph document format or a parallel renderer;
- a new widget toolkit.

`raf_nodes::executor`, `raf_script`, and Host API documentation describe
prepared or future execution layers. They must not be read as proof that the
native Nodes surface currently executes graphs.

## Extension rules

When adding a node, add its factory and stable descriptor in `raf_nodes`, give
its editable values `#[serde(default)]` compatibility through the existing
property field, add both locale entries, and expose it through the shared
catalog. Keep UI-only mapping in `raf_editor`. New graph mutations belong in
`NativeEditorRuntime` and its command translation; do not add an alternate
surface-local mutation path.
