# AuraRafi Manual Commands

The Console can act as a small manual command runner before the full AI
tool-calling pipeline is connected.

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
- `/electronics.drc`, `/electronics.simulate`
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
a scene entity by name. `/script.run` is a one-shot test (Phase B).
`/script.compile_nodes` converts a node graph to Rhai source (Phase E).
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
