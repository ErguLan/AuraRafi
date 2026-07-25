# AuraRafi Manual Commands

The Console is a manual command runner that uses the same domain handlers as
the AI tool-calling pipeline.

For the reusable UI authoring and quality contract behind these commands, see
[RafUI Studio](../.ulpi/design/rafui-studio.md). Its preview command can run
inside the editor Console or as a read-only process from Windows CMD.

## Activation

Commands require two switches:

- Global editor switch: Settings -> Enable manual command console.
- Project switch: Project Settings -> Enable console commands.

When both are enabled, the Console shows a `User1` input row and a Send button.
Normal text is logged as a user message. Text that starts with `/` is parsed as
a command.

## Syntax

Supported forms:

```text
/game.add primitive=cube name="Player Start" x=0 y=1 z=0
/electronics.add_part kind=resistor value=10k x=100 y=100
/game.add {"primitive":"sphere","name":"Orb","x":2}
```

Tab autocompletes command names. Arrow Up and Arrow Down navigate command
history. `/` alone maps to `/help`.

## Sessions And Generated Assets

| Command | Purpose |
| --- | --- |
| `/session.list` | Lists the active project session registry. |
| `/session.create name=<name> kind=world|interface|electronics` | Creates an isolated session. |
| `/session.open session=<name-or-uuid>` | Activates a session after preserving dirty work. |
| `/session.duplicate source=<name-or-uuid> name=<name>` | Copies session documents into a new session. |
| `/session.remove session=<name-or-uuid>` | Removes a non-active registry entry and retains files for recovery. |
| `/ui.document.describe` | Describes the active session's empty-or-authored UI document. |
| `/ui.node.add id=<id> kind=<kind> parent=root text_key=<i18n-key>` | Adds a user-authored UI node. |
| `/ui.node.remove id=<id>` | Removes a non-root UI node. |
| `/ui.document.set_space space=screen|world|camera` | Chooses where that user UI renders. |
| `/ui.document.bind_camera camera=<key>` | Links a document to a camera by reference, not hierarchy ownership. |
| `/rafui.studio.preview format=text|json dpi=1.0|1.25|1.5|2.0 theme=dark|light` | Prints the RafUI Studio recipe, density, and diagnostic preview. |
| `/asset.generate_image prompt="..." name=<asset> size=square|landscape|portrait transparent=true|false` | Starts the isolated remote image worker with `gpt-image-2` by default. |
| `/asset.generate_local_png prompt="..." name=<asset> style=icon|badge|sprite|texture` | Starts the inexpensive local procedural PNG worker; no API key or network required. |
| `/asset.image_status job=<uuid>` | Reads an image generation job. |
| `/asset.cancel_image job=<uuid>` | Stops a running image generation job. |

## Domains

Commands are intentionally separated by project type:

- `shared`: available in any project.
- `game`: only valid in Game projects.
- `electronics`: only valid in Electronics projects, including PCB commands.

Game commands mutate `SceneGraph`. Electronics commands mutate `Schematic`.
PCB commands mutate `PcbLayout`. This keeps game objects and circuit documents
separate.

## Important Commands

Shared:

- `/help`, `/commands`, `/describe`
- `/history`, `/clear`
- `/undo`, `/redo`
- `/project.info`
- `/workspace.read`, `/workspace.search`

Games:

- `/game.add`, `/game.select`, `/game.rename`
- `/game.delete`, `/game.duplicate`
- `/game.set_transform`, `/game.move`, `/game.rotate`, `/game.scale`
- `/game.color`, `/game.arrange_grid`
- `/game.generate_prefab`, `/game.describe_scene`, `/game.focus`

Prefab examples include `kind=platform`, `kind=tower`, `kind=gate` and
`kind=boat`.

Electronics:

- `/electronics.add_part`, `/electronics.wire`
- `/electronics.set_value`, `/electronics.rotate`
- `/electronics.delete`, `/electronics.select`
- `/electronics.generate_circuit`, `/electronics.autolayout`
- `/electronics.drc`, `/electronics.diagnose`, `/electronics.simulate`
- `/electronics.netlist`, `/electronics.bom`, `/electronics.describe`

PCB:

- `/pcb.sync`, `/pcb.route_airwire`, `/pcb.set_board`
- `/pcb.move`, `/pcb.rotate`, `/pcb.describe`

Script (shared, all project types):

- `/script.create`, `/script.attach`, `/script.detach`
- `/script.list`, `/script.validate`, `/script.run`
- `/script.compile_nodes`

Script commands manage `.rhai` and `.cpp` files in `assets/scripts/`.
`/script.create` writes a template file. `/script.attach` binds a file to
a scene entity by name. The shared Rhai runtime session now exists in
`raf_script`; `/script.run` executes `on_start` once against a cloned scene so
the editor document is not mutated.
`/script.compile_nodes` is prepared for the future node-runtime connection and
does not activate a product runtime.
See `docs/SCRIPTING_SYSTEM.md` for the full scripting architecture.

## Output Contract

Every command returns:

- a human-readable title
- detailed lines for the Console card
- a machine-readable JSON payload
- a `changed` flag

Mutating commands push an undo snapshot before the document change is recorded.
They mark the active project dirty but do not save automatically.

Game outputs include entity ids, names, transform, color, primitive type, mesh
counts and local bounds. Electronics outputs include designators, ids, values,
footprints, rotations, pin local/world positions and nets. PCB outputs include
board size, placement, trace points, layers and airwire routing details.

## File Safety

`/workspace.read` and `/workspace.search` only read inside the active project
folder. They do not read the whole AuraRafi repository unless the active project
itself is intentionally located there.

Limits:

- read defaults to 64 KiB, max 256 KiB
- search defaults to 80 results, max 250
- search scans at most 4000 files
- common binary/build folders are skipped

## Extending Commands

1. Add the command definition to `assets/commands/catalog.json`.
2. Use a domain: `shared`, `game`, or `electronics`.
3. Add parameters, defaults, examples and description keys.
4. Add the Rust handler in the matching module:
   - `crates/raf_editor/src/commands/game.rs`
   - `crates/raf_editor/src/commands/electronics.rs`
   - `crates/raf_editor/src/commands/script.rs`
   - `crates/raf_editor/src/commands/workspace.rs`
5. Route the new canonical name from the module `execute(...)` match.
6. Return `CommandOutput::changed(...)` only when real document state changed.
7. Run `cargo check -p raf_editor`.

Do not put new command logic directly into the Console UI. The Console should
collect input and render output; command behavior belongs in the command
modules so agents and external callers can reuse the same path later.

## RafUI Studio from Windows CMD

The editor executable exposes the same read-only preview without opening the
window:

```powershell
cargo run -p aura_rafi_editor -- --rafui-studio-preview
cargo run -p aura_rafi_editor -- --rafui-studio-preview format=json dpi=1.25 theme=dark
cargo run -p aura_rafi_editor -- --rafui-command "/rafui.studio.preview format=text dpi=2 theme=light"
tools\rafui-studio.cmd format=text dpi=1.25 theme=dark
```

For a built executable, replace `cargo run -p aura_rafi_editor --` with the
path to `aura_rafi_editor.exe`. The process prints the title, human-readable
preview lines, and the machine-readable JSON payload, then exits.

The current external path is intentionally read-only because a second process
does not own the live editor session. A future mutation bridge must add an
explicit IPC/session endpoint, command authentication, undo ownership, and
atomic persistence before allowing external writes. Internal Console commands
already use the same canonical command names and typed handlers.
