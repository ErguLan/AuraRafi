# Mesh editing

Raf stores authored topology on the scene node itself (`SceneNode::editable_mesh`).
The renderer, the Tab Mode viewport editing session, the Console and the Agent all
mutate that same payload, so a shape edited by hand can be inspected or extended by
an Agent without exporting or re-importing anything.

## Tab Mode workflow

1. Select exactly one primitive entity or an entity with an authored mesh. Multiple
   selections and imported GLB/GLTF entities without an authored mesh are rejected.
2. Press **Tab** to enter mesh edit mode. A primitive is converted into editable
   triangle topology on entry; an entity that already has an authored mesh resumes
   that topology.
3. Press **1**, **2** or **3** to switch the selection mode between vertices, edges
   and faces. Click a component to select it; hold **Shift** to add to the current
   selection.
4. Drag a selected vertex to move it. Dragging commits one undo step when the
   pointer is released.
5. Double right-click a face to insert a new point on its surface; the face is
   split into three triangles and the new point becomes the active selection.
6. Press **Tab** to apply and leave mesh edit mode, or **Esc** to cancel and restore
   the scene state captured when Tab Mode was entered.
6. Press **Ctrl+S** or use the project Save action to persist the authored mesh. The
   `editable_mesh` payload is part of the scene document; reopening the project
   restores the same topology.

While Tab Mode is active the viewport owns the keyboard, so scene selection and
other editor commands are suspended until the session ends.

## Keys

| Key | Action |
| --- | --- |
| **Tab** | Enter Tab Mode with one editable entity selected; **Tab** again applies and exits. |
| **Esc** | Cancel the session and restore the entry snapshot. |
| **1 / 2 / 3** | Vertex / edge / face selection mode. |
| **Click**, **Shift+Click** | Select or extend the selection. |
| **Drag** | Move the selected vertices in the view plane (screen-right is +X, screen-up is +Y). |
| **Double right-click** | Insert a new movable point on the clicked face and split that triangle into three. |
| **Shift+X / Shift+Y / Shift+Z** | Constrain the drag to one local axis. |
| **G** | Clear the axis constraint and return to free dragging. |
| **A** | Select all components in the current mode. |
| **Backspace** | Clear the component selection. |
| **E** | Extrude the selected faces by 0.25 m along their average normal. |
| **I** | Inset the selected faces by 0.2 toward each face center. |
| **Ctrl+R** | Subdivide the selected faces. |
| **X** or **Delete** | Delete the selected faces; deleting the last face clears the editable mesh. |

Extrude, inset, subdivide and delete require a face selection; switch to face mode
with **3** first. Their status messages are shown in the viewport status line.

## Console and Agent command

`game.mesh` is the single index-based command surface. `target` accepts an ID,
UUID, name, path or stable key; it defaults to `selected`. Mutations go through the
same host history, revision and scene-diff path as other Game commands, and
topology edits select the edited entity.

```text
/game.mesh action=inspect target=Cube topology=true
/game.mesh action=enter target=Cube
/game.mesh action=set target=Cube vertices=[[0,0,0],[1,0,0],[0,1,0]] indices=[0,1,2]
/game.mesh action=move target=Cube vertices=[0] delta=[0,0.25,0]
/game.mesh action=scale target=Cube factor=[2,1,1] center=[0,0,0]
/game.mesh action=extrude target=Cube faces=[0] distance=0.25
/game.mesh action=inset target=Cube face=0 amount=0.2
/game.mesh action=subdivide target=Cube faces=[0]
/game.mesh action=delete target=Cube faces=[0]
/game.mesh action=revert target=Cube
/game.mesh action=apply target=Cube
```

The Agent native tool is `mesh_edit` and routes to `game.mesh`. It is advertised
for Game authoring and gameplay intent:

```json
{"action":"enter","target":"Cube"}
{"action":"inspect","target":"Cube","topology":true}
{"action":"extrude","target":"Cube","faces":[0],"distance":0.25}
```

Use `inspect` with `topology=true` before editing indices: positions and triangle
indices are local to the entity, while the entity transform still positions,
rotates and scales the result in the world.

## Actions

| Action | Meaning |
| --- | --- |
| `inspect` | Report `has_editable_mesh`, vertex/face counts and bounds. With `topology=true`, also return `positions` and `indices`. |
| `enter` | Convert the primitive into editable topology (or resume an existing mesh) and select the entity. |
| `set` | Replace the whole topology from `vertices` (`[x,y,z]` positions) and `indices` (flat triangle indices). |
| `move` | Move `vertices` by `delta`. Omit `vertices` to move every vertex. |
| `scale` | Scale `vertices` around `center` by `factor`. Omit `vertices` to scale every vertex. |
| `extrude` | Extrude `faces` by `distance` and create the border side walls. |
| `inset` | Inset `faces` by `amount` (0 ≤ amount < 1) and create the surrounding ring. |
| `subdivide` | Split `faces` into smaller triangles. |
| `delete` | Delete `faces`; an empty result clears `editable_mesh`. |
| `revert` | Remove `editable_mesh` and return to the parametric primitive. |
| `apply` | Validate the current mesh. This does not change the scene. |

`face=0` is a shortcut for `faces=[0]`. Invalid indices, empty topology and
non-finite values are rejected atomically; the previous mesh stays intact.

## Persistence and undo

- Authored topology is saved with the scene document, not in a separate file.
- Each completed Tab Mode drag or Console mutation is one host undo step.
- Host undo history is in memory and per editor session; it is not written into
  the project file.
- The Agent/CLI transaction ledger is also per session, retains at most 256
  records, and its undo tokens intentionally do not survive a process restart.
- `action=apply` reports validation only; it is not a commit or a file write.

## Limitations

- Imported GLB/GLTF static meshes are not editable yet. Tab Mode refuses them;
  the low-level command starts from the primitive fallback, so inspect first and
  do not run `action=enter` on an entity whose `mesh_asset` is set without an
  authored mesh. Rebuild from primitives instead.
- Topology is bounded to 32,768 vertices and 65,536 triangles. Operations that
  exceed the budget fail without applying a partial edit.
- The mesh is triangle-based and index-referenced. Primitive conversion welds
  vertices that share a position, so a cube corner moves all adjacent faces.
  The renderer derives flat per-face normals and planar box-projected UVs from
  the authored triangles; there is no polygon/quad, normal, UV or material-slot
  editing in this surface.
- Agent model results are context-bounded: `inspect topology=true` returns at
  most the first 64 vertex and triangle entries to the model even though the
  command itself returns the full mesh. Use Tab Mode for large meshes.
- Editing is local-space. The entity transform, pivot and parent hierarchy still
  apply unchanged to the mesh.
- `game.mesh` is deliberately not part of the Agent's observable-scene-change
  expectation: `inspect` and `apply` are read-only, and `revert` on a primitive is
  a valid no-op. Requiring a scene diff for every call would reject those valid
  invocations. Mutating actions still report `changed`, produce a scene diff and
  use the same reversal/undo path as other Game commands.
