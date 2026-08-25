# Hierarchy retained RafUI

Hierarchy is now a first-class editor surface backed by the existing
`SceneGraph`. Folders remain real `SceneNode` entries with `is_folder` and
`Primitive::Empty`, matching the historical scene format; there is no parallel
metadata tree.

## Ownership

- `hierarchy_model.rs` projects and virtualizes the scene tree.
- `hierarchy_surface.rs` declares RafUI nodes, styles, icons and semantic
  events.
- `hierarchy_surface_host.rs` owns search, expansion, selection modifiers,
  rename, context-menu state, copy/paste and drag/drop translation.
- `editor_viewport_app.rs` applies scene mutations, synchronizes viewport and
  Inspector selection, persists the scene and dispatches undo/redo. Inspector
  property edits remain semantic intents, including old audio, physics,
  variables, material and transform behavior.
- `inspector_surface.rs` and `inspector_surface_host.rs` expose the selected
  node through the same retained RafUI path.
- `scene_history.rs` keeps bounded scene snapshots for undo/redo. Continuous
  transform gestures are coalesced into one history entry.

## Rendering boundary

New Hierarchy and Inspector controls are authored as RafUI documents and
presented through ApiGraphicBasic. ApiGraphicBasic uses the configured WGPU
path, with its existing CPU fallback. The current native loop owns window-shell
and panel mounting; it does not define the new controls, layout, painting or
interaction rules outside RafUI.

## Connected behavior

- The Hierarchy chrome stays scene-focused: its local tabs are Hierarchy,
  Assets, World, Bookmarks and Search. Project-type selectors such as Games or
  Electronics do not appear above this panel.
- Selection supports click, Ctrl/Command-click, Shift range, blank-tree box
  selection, viewport sync and auto-reveal settings. The box gesture uses the
  retained tree layout rect and paints its feedback through ApiGraphicBasic.
- Rename, visibility, lock, create entity/folder, duplicate, delete, ungroup,
  focus and reparent are routed through application intents.
- Rows use semantic shared-atlas icons for folders, empty nodes and each
  built-in primitive instead of loading one image per entity.
- Drag/drop supports root, parent, before-sibling and after-sibling targets;
  multi-selection moves are one transaction and graph cycle checks remain
  authoritative. Copy/paste duplicates complete subtrees under a requested
  folder or parent. Before/after drop slots remain interactive hit targets but
  are visually transparent, so a drag gesture cannot add persistent orange
  bars to every row.
- Hierarchy and Inspector mutations, Agent scene mutations, console game
  mutations and viewport transform gestures participate in scene history.
- `Ctrl+Z`, `Ctrl+Y`, Edit menu commands, Agent editor actions and console
  `undo`/`redo` use the same history.
- Settings own Hierarchy presentation density, icon/visibility/lock columns,
  hidden filtering, selection reveal, parent expansion and transitions.
- Project Settings continue to own whether Hierarchy and Inspector panels are
  available; the View menu can reopen either panel after a local close.

## Retained invalidation

- Hierarchy rebuilds its flat projection only after a structural revision,
  filter, hidden-node setting or expansion change. Scrolling slices the cached
  rows with a small overscan instead of walking the entire scene each frame.
- The viewport render fingerprint is cached and includes roots, child order
  and visual node fields. Editor-only Inspector edits do not invalidate the
  Hierarchy projection.
- Inspector surface keys use a field hash rather than formatting a complete
  `SceneNode` debug string every frame.

Folders intentionally remain real historical `SceneNode` entries (`is_folder`
plus `Primitive::Empty`), so old scene files and runtime hierarchy semantics
stay compatible. They are not a second metadata-only tree.

Visual QA is intentionally a separate next phase. The code phase should be
validated first with compilation and targeted interaction tests, then with
large scenes, nested folders, multi-selection, drag/drop, undo/redo and both
GPU and CPU presentation paths.
