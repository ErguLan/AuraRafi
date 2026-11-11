# Game interfaces

Game UI is authored with RafUI documents and presented through ApiGraphicBasic.
The primary authoring view edits interfaces directly over the scene, with the
document tree in Hierarchy and contextual element controls in Properties.
The **Interfaces** bottom tab is a launcher, not a second inspector or canvas.
The active session still owns its `ui_document_file`.
The player mounts independent instances before script `on_start`. No HUD,
camera, character, or gameplay behavior is injected into an empty document.

For engine contributors creating editor surfaces in Rust, use
[Authoring RafUI in Rust](RAF_UI_AUTHORING.md). That guide covers native
surface construction and action routing; the game API below cannot modify
editor panels. [RafUI](RAF_UI.md) contains the shared document and renderer
contracts.

## Create and preview

1. Open a Game project and use **Interfaces** in the viewport to edit the
   session's document. Insert a frame, label, image, button or text input from
   the authoring toolbar or Assets. New elements use white and neutral defaults;
   existing authored colors are preserved. This is scoped to game content and
   authoring chrome, not a change to the editor's global palette.
2. Select an element directly over the scene or in Hierarchy. Drag it or use
   any of its eight resize handles. Properties contains content, geometry and
   appearance; advanced identifiers, parenting, events and relative dimensions
   are collapsed initially. Values commit on Enter and update the preview.
3. Anchor presets preserve the current visual rectangle while changing its
   relationship to the parent. Relative dimensions and min/max bounds remain
   supported. A drag is one undo transaction. Escape cancels it, including
   anchored layouts; focus loss also cancels an unfinished gesture.
4. **Canvas** is an optional isolated view with landscape/portrait sizes,
   fit, zoom and scrolling. It uses the same document and renderer. View state,
   selection guides and editor handles are never saved into the game document.
   **Scene** returns to ordinary scene authoring; screen UI remains visible as
   a passive overlay without intercepting game-world selection.
5. **Assets → Create → Interfaces** creates a distinct blank interface asset.
   Double-clicking an existing `.ui.ron` opens that source for editing instead
   of inserting another copy. **Insert** explicitly copies it into the current
   document; **Insert in session** is the direct action while editing an asset.
   Returning to Scene restores the session document; dirty source
   changes are saved through the same conflict-checked persistence boundary.
6. Save, then Play. Starting or reloading Play also flushes pending interface
   changes. Stop discards runtime UI changes. Undo/redo in the designer retains
   sixteen document generations; disk publication keeps a recoverable previous
   file. Conflicting external changes are reported instead of overwritten.

Inspector fields retain separate drafts. Confirming one property does not erase
another unfinished value; Escape restores that field's latest document value.
Changing the selected element or source document starts a new editing scope.

Use an image asset's **Use in interface** action to populate the selected image
element or create one. Image keys use `assets://relative/path.png` or
`project://assets/relative/path.png`. The existing bounded asynchronous image
loader owns decoding, cancellation, and missing/error placeholders.

**Save to Assets** writes `assets/ui/<document-id>.ui.ron`. Inserting that asset
copies its tree, bindings, and stateful styles with new scoped IDs. Copies are
independent snapshots; editing the source does not automatically update them.

## Presentations

One document can declare several `UiMount` descriptors. Each becomes an isolated
runtime instance, identified by its mount ID (`main` by default).

| Kind | Behavior |
| --- | --- |
| `Screen` | Responsive overlay covering the game content area, excluding player chrome. |
| `CameraOverlay` | Screen overlay visible only while its referenced camera is active. |
| `CameraPlane` | A real textured plane transformed with its referenced active camera. |
| `Surface` | A plane transformed by its target entity, including parent transforms. |
| `Billboard` | A plane at its target entity that faces the active camera. |

Select a scene entity before choosing a targeted presentation: the inspector
captures its UUID. Surface authoring initially places the plane just beyond the
primitive's front face. Set offsets, rotations in degrees, physical size, and
canvas resolution explicitly as needed. Camera planes normally use a negative
local Z offset, such as `0,0,-1`. A missing target or camera produces no visual
association. Name references are accepted only when unambiguous; prefer UUIDs.

Physical presentations switch to the isolated content canvas. Their existing
runtime transforms, depth and interaction remain unchanged. This pass does not
add in-viewport 3D placement gizmos or render those planes inside the editor's
scene; inspect physical placement in Play. The live editor overlay is for
screen UI and camera-overlay content authoring.

World UI uses scene depth by default. `always_on_top` explicitly bypasses depth.
Physical UI interaction projects the pointer ray into the same rendered plane;
world occlusion uses the existing AABB collider queries, excluding its owner and
triggers. This is collider-level occlusion, not triangle-precise picking.
`interactive=false` leaves a presentation passive. Distance and visibility can
disable presentation without altering scene entities.

## Scripting

Rhai and Rust use the same client-scoped `ScriptContext` API. It only addresses
game-owned UI instances; it has no route to editor RafUI internals.

To try the example below:

1. Create a label and a button in the session interface. In their advanced
   properties, assign the element IDs `title` and `confirm`. Keep the default
   presentation instance ID `main`, or change the script to its actual ID.
2. Set the button's click-event field to `menu.confirm`. Save the interface.
3. Attach the Rhai script to a scene entity that runs in the active session,
   using a Local or Client role, then enter Play. The label changes to `Ready`;
   clicking the button hides that UI instance. A missing instance or element
   is an error, not a request to create an invisible default menu.

```rhai
fn on_start() {
    let title = ui_element("main", "title");
    ui_text(title, "Ready");
    ui_enabled(ui_element("main", "confirm"), true);
}

fn on_event(name, data) {
    if name == "menu.confirm" {
        ui_show("main", false);
    }
    if name == "ui.event" {
        // data.instance, data.element, data.name and data.value identify the source.
    }
}
```

Set a button's click-event field to `menu.confirm`. Input controls emit their
value keys and typed values. Every supported event also emits `ui.event` with a
structured source record, so cloned interfaces can be distinguished.

| Operation | Purpose |
| --- | --- |
| `ui_element(instance, id)` | Resolve an opaque element reference. |
| `ui_text`, `ui_visible`, `ui_enabled`, `ui_image` | Change an authored property. |
| `ui_create(instance, parent, id, kind)` | Add a frame, label, image, button, or input. |
| `ui_rect(element, x, y, width, height)` | Set an explicit logical rectangle; Rhai numeric arguments are floats. |
| `ui_remove(element)` | Remove an element. Re-resolve retained references after removal or renaming. |
| `ui_clone(source, new_id)`, `ui_close(id)`, `ui_show(id, bool)` | Manage local instances. |
| `ui_value(key, string)` | Update declared value subscriptions only when their value changes. |
| `ui_present(instance, mode, target)` | Switch presentation: `screen`, `camera`, `plane`, `surface`, `billboard`. |
| `ui_offset`, `ui_rotation`, `ui_size` | Adjust physical presentation in Rhai. |

Rust uses `ScriptContext::ui_mount` with a complete `UiMount`. Value bindings are
stored as `{key, node, property}` where property is `Text`, `Visible`, or
`Enabled`. Boolean subscriptions require the strings `true` or `false`.
Nodes provides matching UI text, image, visibility, enabled, instance visibility,
and value actions. They compile into these same Host API calls; **On Event**
receives UI events through the normal runtime event channel.

Rust integration code that already receives a `ScriptContext` can use the same
operations without exposing the underlying RafUI tree:

```rust
use raf_script::{ScriptContext, ScriptResult};

pub fn set_menu_ready(ctx: &mut ScriptContext<'_>) -> ScriptResult<()> {
    let title = ctx.ui_element("main", "title")?;
    let confirm = ctx.ui_element("main", "confirm")?;
    ctx.ui_text(&title, "Ready")?;
    ctx.ui_enabled(&confirm, true)
}
```

This is an integration helper to call from an existing Rust callback; saving a
standalone `.rs` file does not register or compile a new script automatically.
The example names refer to authored elements, not built-in engine UI.

Local and Client roles can use this API; Server rejects visual operations.
No multiplayer transport is added by this feature. Authoritative gameplay
remains in world logic. The future WASM/C++ boundary exposes typed
`HostOperation::GameUi(UiImport)` operations. An executor and guest-memory
adapter are still external prerequisites; no compiler or executor is installed.

## Agent, CLI, and MCP authoring

The native Agent tool pack exposes `ui.document.read`, `create`, `apply`, and
`undo` for interface requests. CLI/MCP use the same project-confined commands.
Read the active session's UI path from project/session metadata; reusable assets
may use `assets/ui/name.ui.ron`.

`ui.document.read` returns the document and its revision. `apply` and `undo`
require that revision. `apply` accepts at most 128 atomic edits: `insert`, `add`,
`patch`, `rename`, `remove`, `reparent`, `mount`, `bind`, and `rules`. `insert`
is the compact authoring operation:

```json
{
  "op": "insert", "parent": "root", "id": "title", "kind": "label",
  "text": "Ready", "rect": {"x": 16, "y": 16, "width": 240, "height": 32}
}
```

The command gateway retains its normal dry-run, confirmation, revision, and
idempotency contracts. Document undo restores the previous saved generation;
it is separate from scene undo. Runtime scripts never enter these authoring
commands. Unsaved visual edits and external document writes produce an explicit
conflict instead of silently merging unrelated changes.

## Ownership, limits, and validation

- `raf_ui`: version-4 document, migration defaults for older documents, anchored
  layout, edit transactions, subscriptions, and local instance state.
- `raf_editor`: visual authoring, Assets insertion, recoverable persistence,
  command adapters, and translations.
- `raf_script` / `raf_runtime`: shared client UI API and runtime ownership.
- `raf_player`: instance presentation, camera/entity transforms, input routing,
  and events.
- `ApiGraphicBasic`: retained UI compilation, GPU target creation, scene texture
  sampling, alpha/depth handling, and the existing CPU recovery compositor.

Limits are 4 MiB per saved document, 2,048 nodes per document, 32 instances,
256 value bindings per document, and a combined 4,194,304 physical canvas pixels
(16 MiB of RGBA targets). Individual physical canvases are at most 2,048 per
axis. CPU image stores share a 32 MiB allowance across live game UI instances;
graphics caches and script execution retain their independent limits.
Unused world-UI pipelines are initialized lazily. Unchanged targets are reused;
text-only changes do not restart image decoding. World UI never reads GPU pixels
back to the CPU on the hardware path.

Focused tests cover transactional document edits, serialization, stale handles,
canvas admission, responsive layout, and the Rhai/Nodes/event bridge. Native
visual acceptance, DPI, physical-plane picking, GPU/CPU appearance, and actual
frame pacing still require a real editor session and representative hardware.

The optional `RAF_INTERFACE_QA_DIR` environment variable enables PNG capture in
`raf_render/tests/game_ui_designer.rs`. That test imports the actual editor
surface and controller, renders with `CpuUiSurfaceHost`, and checks non-overlap,
unique element IDs, eight resize handles, and insert-menu bounds. The fixture
also covers drag/cancel/undo and source-asset editing. It does not substitute a mock
surface or require compiling the editor application. For an existing build cache:

```powershell
$env:RAF_INTERFACE_QA_DIR = Join-Path $PWD 'target_gnu/interface_qa'
cargo test --offline -p raf_render --test game_ui_designer -- --nocapture
```
> Note: a custom `--target-dir target_fix` (or any other) is forbidden. Use the default `target_gnu/`.

Inspect all generated PNG files in both themes and sizes. These captures prove
CPU layout and paint only; they do not prove native input, Play, GPU execution,
or frame pacing.
