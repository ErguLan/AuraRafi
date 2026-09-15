# AuraRafi Manual Commands

The command handlers remain documented as domain infrastructure. The editor
Console is now mounted through the retained RafUI downbar; the former RafUI
Studio authoring surface remains removed and is not a hidden dependency.

## Activation

Commands require two switches:

- Global editor switch: Settings -> Enable manual command console.
- Project switch: Project Settings -> Enable console commands.

When both are enabled, the Console shows a `User1` input row and a Send button.
Normal text is logged as a user message. Text that starts with `/` is parsed as
a command.

## Command kernel and external agents

The Console is not the owner of command behavior. RafUI, the Console, a CLI,
and an MCP endpoint must produce the same `EngineCommandRequest` and receive
an `EngineCommandResponse` with `changed`, data, warnings, diff, and undo
state. The contract lives in `raf_core::command_protocol`; the editor
boundary adapts it through `raf_editor::commands::gateway`.

The initial transport is newline-delimited JSON over stdio with a 1 MiB frame
limit. Attached editor mode currently uses a token-scoped loopback TCP stream;
the same frames are ready for Unix sockets and Windows named pipes,
so an agent can talk to an open interface or to a headless process without
importing a Console screen. Real execution still requires a project and a
domain executor; the protocol does not fake mutations.

This surface does not activate Play, Stop, or Runtime. Scene, asset, and CAD
commands remain document-editing operations and their responses must be able
to enter the existing undo history.

### Headless CLI and MCP adapters

The `raf` binary is a UI-independent adapter over the same core protocol. It
can be used by Codex, Claude Code, OpenCode or another local harness without
starting the editor renderer:

```text
raf doctor --json
raf status --json
raf capabilities search terrain --json
raf project create --name Demo --parent ./projects --type game --confirm --json
raf project info ./projects/Demo --json
raf serve                         # JSONL EngineCommandRequest/Response
raf mcp serve                     # MCP JSON-RPC over stdio
raf attach --project ./projects/Demo status --json
raf attach --project ./projects/Demo command game.add --params '{"primitive":"cube"}' --confirm --json
raf mcp serve --attach ./projects/Demo # MCP against the open editor
```

`--dry-run`, `--expected-revision`, `--idempotency-key` and `--confirm` are
available on command requests where relevant. Mutations return a revision,
diff, verification status and undo token when the host can provide one. The
headless adapter currently exposes project/workspace/capability inspection and
project creation/opening; domain mutation commands require an attached editor
executor. Play, Stop and Runtime are deliberately not exposed.

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
| `/session.rename session=<name-or-uuid> name=<name>` | Renames a session without changing its storage identity. |
| `/session.duplicate source=<name-or-uuid> name=<name>` | Copies session documents into a new session. |
| `/session.remove session=<name-or-uuid>` | Removes a non-active registry entry and retains files for recovery. |
| `/ui.document.describe` | Describes the active session's empty-or-authored UI document. |
| `/ui.node.add id=<id> kind=<kind> parent=root text_key=<i18n-key>` | Adds a user-authored UI node. |
| `/ui.node.remove id=<id>` | Removes a non-root UI node. |
| `/ui.document.set_space space=screen|world|camera` | Chooses where that user UI renders. |
| `/ui.document.bind_camera camera=<key>` | Links a document to a camera by reference, not hierarchy ownership. |
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
- `/transaction.undo token=<undo-token>` (attached CLI/MCP only; requires
  `confirm=true` and the exact issuing revision)
- `/project.info`, `/project.save`
- `/scene.outline`, `/scene.query`, `/scene.spatial_map`, `/scene.check_overlaps`,
  `/scene.diff`, `/scene.design_audit`, `/scene.inspect`, `/scene.verify`
- `/assets.catalog`, `/assets.inspect`, `/assets.recommend`
- `/workspace.read`, `/workspace.search`

Games:

- `/game.add`, `/game.select`, `/game.rename`
- `/game.delete`, `/game.duplicate`
- `/game.set_transform`, `/game.move`, `/game.rotate`, `/game.scale`
- `/game.color`, `/game.arrange_grid`
- `/game.generate_prefab`, `/game.snap`, `/game.update`, `/game.batch`
- `/game.create_group`, `/game.reparent`, `/game.build`, `/game.reconcile`, `/game.repair`
- `/game.describe_scene`, `/game.focus`

The native Agent uses a semantic tool layer over these commands. Its compact
tools are `project_summary`, `scene_outline`, `scene_query`,
`scene_spatial_map`, `scene_check_overlaps`, `scene_diff`, `scene_design_audit`,
`scene_inspect`, `selection_get`, `assets_catalog`, `assets_recommend`,
`asset_inspect`, `scripts_catalog`, `project_health`, and `scene_verify`.
Authoring tools such as `scene_build`, `scene_reconcile`, `scene_repair`, `scene_create_group`, `scene_create`,
`scene_update`, `scene_reparent`, `scene_delete`, `scene_duplicate`,
`scene_snap`, `scene_arrange`, `scene_instantiate_template`, and `scene_batch` are only
advertised when the project domain and prompt require them. `scene_build`
creates groups before entities, accepts child groups in any order, and accepts
group references by name or path;
`scene_batch` accepts 1-512 ordered operations and commits atomically. Repeated
duplicates and template instances accept `count`, `axis`, `spacing`, `offset`,
and `parent`, so a repeated layout does not require a hand-written list of
nearly identical calls.
`scene_reconcile` requires a unique `stable_key` for every desired group/entity;
rerunning the same payload updates those nodes in place and preserves
unmentioned nodes. `game.build` and `game.reconcile` accept an optional
`design_profile` (`generic`, `real_world`, `building`, `supermarket`, `parking`,
or `outdoor`). Real-world profiles reject an incomplete envelope before the
atomic mutation, so a blockout cannot silently be reported as a finished place.

`game.add`, `game.update`, and `game.set_transform` accept the legacy scalar
fields (`x`, `y`, `z`, `rx`, `ry`, `rz`, `sx`, `sy`, `sz`) and the native Agent
shape (`transform.position`, `transform.rotation_deg`, `transform.scale`).
Colors can be sent as `color=#RRGGBB`, `color=#RRGGBBAA`, or a 3/4-channel
`color_rgba` array. The command kernel preserves these values and rejects
malformed vectors or channels instead of silently falling back to defaults.
`scene_verify` can receive an optional `expected` object with `primitive`,
`transform`, and `color_rgba`; verification fails when the effective scene
state does not match it. Mutation results expose the effective entity values,
including local and world position, parent, path, primitive, scale, and color.

`scene.spatial_map` is the bounded layout perception command. It returns the
visible renderable entities in a scope, conservative world-space AABBs, the
aggregate extents of the inspected scope, and optional overlap pairs. Use
`root=<uuid|ref|name|path>` to avoid unrelated legacy scene roots and follow
`next_cursor` when the result is paginated. It reports overlap evidence; it
does not mutate or automatically move the scene.

`scene.check_overlaps` is the focused repair companion. It reports intersecting
world-space bounds, penetration, the smallest separating axis, and a deterministic
`suggested_snap` for the next mutation. `scene.snap` can then place an entity on
the grid, floor, or another entity's surface. `scene.diff` reports retained
created/updated/deleted references since a revision, which lets the Agent explain
what a long build actually changed without rereading the whole scene.

`assets.recommend` ranks imported assets for an authoring intent such as
`shelf`, `wall`, or `store`. It is deliberately transparent and heuristic: the
result includes the matched tokens and scene references, so the Agent can choose
between an existing asset and a procedural primitive instead of scanning the
workspace's internal metadata.

`scene.design_audit` is a read-only semantic check for real-world authoring.
It infers candidates for `floor`, `enclosure`, `entrance`, `circulation`,
`roof`, `primary_modules`, and `details` from names, tags, roles and stable
keys. Pass `design_profile=supermarket` or `design_profile=parking` to select
the matching envelope automatically, or pass `required_features=[...]` when
the requested place has a specific contract. Treat a failed audit as a repair
request rather than a completed build.

`scene.repair` applies explicit operations from an audit or spatial inspection
as one atomic batch inside an optional root scope. It never infers geometry
from prose, so the Agent must inspect the reported target and send concrete
update/reparent/create/delete operations.

Attached Agent observation and task commands:

- `/viewport.capture` captures the last rendered Game viewport to a bounded PNG
  artifact under `.aura_rafi/agent_artifacts/`. It never enters Play mode and
  requires the native Game viewport to have rendered a frame.
- `/task.list`, `/task.get id=<task-id>`, `/task.events since=<sequence>`, and
  `/task.cancel id=<task-id>` expose the native Agent run lifecycle to an
  attached CLI/MCP client. Task progress is in-memory and bounded to the
  editor process; it is cooperative cancellation, not durable resume.

The native Agent receives the same task snapshot in its status strip while a
run is thinking, executing tools, or waiting for approval. `task.cancel` only
requests cancellation at a safe runtime boundary; it does not kill an
in-flight provider network request.

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

For external authoring setup and Windows examples, see
`docs/CLI_MCP_QUICKSTART.md` and the reusable `.ai/skills/raf-game-authoring/`
skill.

## Agent Scene Build

The native Agent exposes scene_build, scene_reconcile, scene_create_group, and
scene_reparent for modular authoring. scene_build creates groups first and
entities second in one atomic operation, while scene_reconcile applies a keyed
desired state without duplicating existing nodes. scene_batch accepts 1-512
ordered operations.
Nested transform and color objects remain structured through the gateway.
Every vector uses the same flat shape, for example `position: [x, y, z]`;
`[[x, y, z]]` is rejected with an actionable shape diagnostic. Reparenting,
duplication, snapping, and template instantiation are first-class kernel
operations, so the native Agent, CLI, and MCP share the same behavior.

Prefer this payload for generated structures:

    {
      "groups": [
        {"name": "Structure"},
        {"name": "Products", "parent": "Structure"}
      ],
      "entities": [
        {
          "kind": "cube",
          "name": "Floor",
          "parent": "Structure",
          "transform": {
            "position": [0, 0, 0],
            "rotation_deg": [0, 0, 0],
            "scale": [4, 0.2, 2]
          },
          "color_rgba": [60, 150, 90, 255]
        }
      ]
    }

Mutation postconditions compare the effective primitive, parent, transform, and
color. A mismatch is reported as a failed tool result so the Agent inspects and
repairs instead of claiming success.

## Output Contract

Every command returns:

- a human-readable title
- detailed lines for the Console card
- a machine-readable JSON payload
- a `changed` flag

Observation responses may also include `artifacts`, `references`, `revision`,
and bounded metrics. The human-facing Agent card should show the summary first;
raw command lines, large geometry data, and full JSON remain expandable details.
The CLI human output and the Agent/MCP model-facing text use the same compact
line filter: renderer dumps such as `Command executed:`, mesh vertices, and
duplicated raw JSON are omitted from the summary while the structured response
remains available to callers that explicitly request details.

While a run is waiting on a provider or executing tools, the Agent surface shows
the current semantic activity, completed/total tools, turn and elapsed time. The
activity is a projection of runtime state, not a fake message, and stops with
the run so a stalled-looking panel remains diagnosable.

Editor-owned mutating commands push an undo snapshot before the document change
is recorded. Attached/headless adapters record the revision and semantic diff,
but never advertise an undo token they cannot restore. Mutations mark the
active project dirty but do not save automatically; use `/project.save` or
`/project.checkpoint` for an explicit checkpoint.

Game outputs include entity ids, names, transform, color, primitive type, mesh
counts and local bounds. Electronics outputs include designators, ids, values,
footprints, rotations, pin local/world positions and nets. PCB outputs include
board size, placement, trace points, layers and airwire routing details.

## File Safety

`/workspace.read` and `/workspace.search` only read inside the active project
folder. They do not read the whole AuraRafi repository unless the active project
itself is intentionally located there.

Normal workspace search skips engine/Agent metadata such as `.ai`,
`.aura_rafi`, `.codex`, `.grok`, `agent_history.ron`, and
`agent_endpoint.json`. Pass `include_internal=true` only when an explicit
diagnostic needs those files. Scene, asset, and script perception should use
the native Agent tools instead of workspace search.

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

The former RafUI Studio preview command was removed with the Studio surface.
It will only return after the new authoring workflow is designed and connected
again deliberately.
