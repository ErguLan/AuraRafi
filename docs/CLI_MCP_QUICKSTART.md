# Raf CLI and MCP Quickstart

This is the human-facing beta guide for using the editor as an attached
authoring host. It is intentionally limited to editable project documents:
Play, Stop, and Runtime are not available.

## 1. Build the CLI

From the repository root:

```text
cargo build -p raf_cli
```

The binary is `target/debug/raf.exe` on Windows (or `raf` on Unix-like
systems).

## 2. Inspect a project without opening the editor

These commands are headless and do not start a renderer:

```text
raf project info --project D:\Games\HorseDemo
raf project status --project D:\Games\HorseDemo
raf capabilities list --project D:\Games\HorseDemo
raf doctor --project D:\Games\HorseDemo
```

Headless inspection is safe to use in scripts and CI. At this beta stage it
does not claim to mutate a live scene while the editor is closed.

## 3. Attach to the open editor

Open the Game project in AuraRafi first. The editor publishes a short-lived
local endpoint at `.aura_rafi/agent_endpoint.json`. Then run:

```text
raf editors --json
raf attach status --json
raf attach capabilities --json
raf attach context --json
```

`raf editors` walks the recent-projects registry, probes each published
endpoint with a short loopback connect, and marks every editor `LIVE` or
`stale`. Each entry reports the project type (game/electronics) and the
editor state (hub/project) so agents pick the right command domain before
connecting. When exactly one editor is live, `raf attach` and
`raf mcp serve --attach` pick it automatically; no project path is required.
Pass `--project PATH` to target a specific editor when several are open.

`raf editors --wait[=SECONDS]` retries the scan every 400 ms (default budget
15 s) until a live editor appears, so automation may start before the engine.

Attached `engine.status` is domain-agnostic: it reports the editor state,
project type, entity count, and capabilities without touching a domain
executor. `game.batch` runs up to 512 game operations in one round trip, each
through the normal gateway (history and idempotency preserved).

Attached metadata commands are also domain-agnostic: `project info`,
`session list`, `workspace describe`, and `context` read the live project
without being routed into a Game or Electronics parser. `context` is the
preferred compact input for an external Agent: it includes the project,
active session, revision, supported commands, bounded workspace counts, and a
bounded Game scene outline when one is mounted.

For visual feedback, an attached Game editor can capture the rendered viewport
or a framed scene angle without entering Play mode:

```text
raf attach --project D:\Games\HorseDemo viewport capture --json
raf attach --project D:\Games\HorseDemo viewport capture --json --params '{"view":"isometric","target":"Player","width":960,"height":540}'
raf attach --project D:\Games\HorseDemo viewport capture --json --params '{"view":"front","frame":"scene"}'
```

For spatial planning, generate a metric top-down map with bounds, colliders,
grid and labels. The response includes the PNG plus structured entity, collider,
bounds, grid and label metadata:

```text
raf attach --project D:\Games\HorseDemo scene map --json --params '{"width":1200,"overlays":["bounds","colliders","grid","labels"],"grid":1.0}'
```

The same artifact is available through `raf_viewport_capture` with
`preset=top_down_map` and through the typed `raf_scene_top_down_map` MCP tool.

The response references a PNG saved inside the project at
`.aura_rafi/agent_artifacts/`. `view=current` reads the last rendered editor
frame; named angles render a read-only offscreen capture. The MCP adapter
returns the PNG as image content when the artifact is available, and the native
Agent attaches it to its next model turn automatically.

For layout perception without reading project files, query the semantic scene
map:

```text
raf attach --project D:\Games\HorseDemo scene spatial --json --params '{"root":"Store","check_collisions":true}'
```

The response contains bounded world-space extents, renderable entities and
conservative overlap pairs. Use the returned `next_cursor` for large scopes;
the command is read-only and does not alter the scene.

After a build, ask for a design audit so an agent can detect a blockout that
has objects but does not read as a place:

```text
raf attach --project D:\Games\HorseDemo scene design-audit --json --params '{"root":"Store","design_profile":"supermarket"}'
```

An audit failure is structured evidence for the next repair pass. It does not
modify the scene by itself.

For a Game foundation check that is stronger than geometry presence, call the
gameplay audit. It reports renderable geometry, camera/startup camera, authored
lighting, attached scripts, gameplay signals, enabled systems, and next actions:

```text
raf attach --project D:\Games\HorseDemo command project.gameplay_audit --json
```

Use `scene repair` only after an audit has identified concrete targets. It
accepts explicit `scene_update`, `scene_reparent`, `scene_create`, or
`scene_delete` operations and applies them atomically inside the requested
root; it never guesses geometry from prose:

```text
raf attach --project D:\Games\HorseDemo scene repair --json --params '{"root":"Store","operations":[{"name":"scene_update","params":{"target":"Wall_Back","transform":{"position":[0,2,-4]}}}]}' --confirm
```

The native Agent run can also be observed or cancelled from an attached host:

```text
raf attach --project D:\Games\HorseDemo task list --json
raf attach --project D:\Games\HorseDemo task events --since 0 --json
raf attach --project D:\Games\HorseDemo task cancel --id <task-id> --json
```

These task records are bounded and in-memory for the current editor process;
they are not durable jobs or resumable sessions. Cancellation is cooperative.

The handshake checks project identity, session, protocol version, and the
local session token. It uses loopback only; no public server is started.
Descriptors left behind by a crashed editor are reported as stale instead of
failing with a raw socket error, and are replaced the next time that project
opens.

## 4. Create a small scene safely

Use preview first, then confirm the same operation with an idempotency key:

```text
raf attach --project D:\Games\HorseDemo command game.add --json --params '{"primitive":"cube","name":"Stable","position":[0,0,0]}' --dry-run

raf attach --project D:\Games\HorseDemo command game.add --json --params '{"primitive":"cube","name":"Stable","position":[0,0,0]}' --confirm --idempotency-key stable-001
```

For an attached Game project, the response includes the new revision, a
structured scene diff, verification checks, and a scoped `undo_token`. To roll
back that exact edit:

```text
raf attach --project D:\Games\HorseDemo command transaction.undo --json --params '{"token":"<undo_token>"}' --confirm
```

The token is rejected if another scene edit, session switch, project change,
or attached revision has advanced the document. This prevents an AI from
undoing somebody else's work. Electronics attached mutations currently expose
revision and diff evidence but no scoped undo token.

## Modular scene authoring

For a modular authoring pass, prefer one atomic game.build request. For a real
place, set a design profile so an incomplete blockout is rejected before it
changes the scene. Create the envelope and circulation groups first, then the
floor, walls/roof and entrances, and only then repeated modules and details:

    raf attach --project D:\Games\HorseDemo command game.build --json --params '{"design_profile":"supermarket","groups":[{"name":"Structure"},{"name":"Products","parent":"Structure"}],"entities":[{"kind":"cube","name":"Floor","parent":"Structure","transform":{"position":[0,0,0],"scale":[4,0.2,2]},"color_rgba":[60,150,90,255]},{"kind":"cube","name":"Wall_Back","parent":"Structure","transform":{"position":[0,2,-4],"scale":[8,2,0.2]},"color_rgba":[180,180,180,255]},{"kind":"cube","name":"Entrance","parent":"Structure","transform":{"position":[0,1,4],"scale":[2,2,0.2]},"color_rgba":[80,120,160,255]},{"kind":"cube","name":"Aisle_Main","parent":"Structure","transform":{"position":[0,0.2,0],"scale":[1,0.1,6]},"color_rgba":[220,180,60,255]},{"kind":"cube","name":"Shelf_Left","parent":"Products","transform":{"position":[-2,1,0],"scale":[0.4,2,3]},"color_rgba":[120,90,50,255]}]}' --confirm --idempotency-key shelf-build-001

Supported profiles are `generic`, `real_world`, `building`, `supermarket`,
`parking`, and `outdoor`. The profile checks names, roles, stable keys and tags;
it does not invent missing geometry. If it rejects the request, add the missing
feature and preview again.

game.create_group and game.reparent are available when a build needs to be
assembled or reorganized in separate steps. Groups in game.build may reference
parents by name or path even when the child appears first. The attached response
reports the effective parent, transform, primitive, color, and live entity count.

For repeatable AI workflows, use `game.reconcile` with a `stable_key` on every
group and entity. Re-running the same desired state updates those nodes in place
instead of duplicating them:

    raf attach --project D:\Games\HorseDemo command game.reconcile --json --params '{"groups":[{"name":"Shelves","stable_key":"store.shelves"}],"entities":[{"kind":"cube","name":"Shelf_Left","stable_key":"store.shelf.left","parent":"key:store.shelves"}]}' --confirm --idempotency-key shelves-reconcile-001

### World Program: plan, preview and commit

A large world should be compiled as one declarative program instead of hundreds
of individual tool calls. The program supports built-in kits (`wall`, `floor`,
`door`, `shelf`, `checkout`, `street_lamp`, `tree`, `bench`, `crate`, `platform`,
`stairs`, `checkpoint`, `car`, `aisle`, `spawn`, `goal`), custom archetypes,
grids, paths, deterministic scatter, stable keys and design profiles:

```text
raf attach --project D:\Games\Harbor scene program --json --params @world.json
raf attach --project D:\Games\Harbor scene commit --json --params '{"plan_id":"plan_..."}' --confirm
```

`scene program` never mutates the scene. It returns a persistent `plan_id`, a
plan hash, a preview, design-audit evidence, conservative overlap evidence and
an optional `.aura_rafi/world/plans/<plan-id>.json` journal. `scene commit`
checks the scene fingerprint and applies the compiled plan in bounded chunks
(32 groups and 128 entities per chunk) so a multi-thousand-entity world is not
rejected by the interactive `game.build` limit. Every chunk is rolled back if a
later chunk fails, and the whole plan is still one command, one history entry
and one scoped undo token at the adapter boundary.
Repeating `scene commit` returns `replayed: true`; a keyed `mode: "reconcile"`
program updates stable keys in place instead of duplicating them.

The persistent `world.brief` keeps genre, camera, palette, scale metrics,
constraints and acceptance criteria across turns:

```text
raf attach --project D:\Games\Harbor scene brief --json
raf attach --project D:\Games\Harbor scene brief --json --params '{"action":"write","brief":{"mood":"night","metrics":{"aisle_width":2.4}}}' --confirm
```

`raf attach scene digest` returns one compact snapshot with counts, bounds,
cameras, component coverage, semantic roles and optional overlaps. The attached
CLI also supports `raf undo`, `raf redo`, `raf session use`, and
`raf project close` (which detaches without killing the editor).

## 5. Create and validate scripts

Scripting commands use the same attached editor endpoint and write only inside
the project:

```text
raf attach --project D:\Games\HorseDemo command script.create --json --params '{"lang":"rhai","name":"horse_controller"}' --confirm

raf attach --project D:\Games\HorseDemo command script.write --json --params '{"file":"scripts/horse_controller.rhai","content":"fn on_update(dt) { if is_key_pressed(\"w\") { owner().move_by(0.0, 0.0, 2.0 * dt); } }","overwrite":true}' --confirm

raf attach --project D:\Games\HorseDemo command script.list --json
raf attach --project D:\Games\HorseDemo command script.validate --json --params '{"file":"scripts/horse_controller.rhai"}' --confirm
```

`script.write` compiles the Rhai source before replacing the file, accepts only
project-relative `.rhai` paths, and restores the previous file if the write
fails. The external agent may also edit a project script directly through the
normal workspace tools. The engine command remains useful for project-scoped
creation, attachment, validation, and evidence.

### Large parameter payloads

`--params` remains inline JSON, but it also accepts `@FILE` and `-` for stdin.
`--params-file FILE` is the explicit form; both paths are bounded to 8 MiB.

```text
raf attach --project D:\Games\HorseDemo command script.write --json --params-file payload.json --confirm
raf attach --project D:\Games\HorseDemo command ui.document.apply --json --params @ui-payload.json --confirm
```

This avoids the Windows command-line length limit for complete script sources
and multi-edit interface documents. Parameter files are capped at 8 MiB; the
attached IPC frame itself remains bounded at 1 MiB, so split truly enormous
documents into multiple commands.

### Headless script smoke test

`script.run` executes one script against a cloned scene without entering Play.
The editor document is never mutated.

```text
raf attach --project D:\Games\HorseDemo command script.run --json --params '{"file":"scripts/player_controller.rhai","entity":"Player","ticks":120,"keys":["w","space"]}' --confirm
```

- `file` resolves like the other script commands.
- `entity` is optional. When present, that entity stays the behavior owner and
  its typed behavior parameters remain available. Without it, the script runs
  on a temporary `__script_run__` root.
- `ticks` (0..600) runs `on_update`, `on_fixed_update`, one bounded physics
  step and `on_late_update` per tick.
- `keys` (at most 16) supplies synthetic held keys; the first tick also reports
  them as just-pressed. Use names such as `w`, `space`, `shift`, `arrow_left`.
- The JSON result reports `status`, `owner`, `ticks`, `physics_steps`,
  `final_position`, logs and every runtime error.

This catches missing host functions and hook-level mistakes before a manual
Play test. It does not mount game UI instances, render frames, or replace the
native Play acceptance test.

### Locate the project root

```text
raf project path --json
```

The CLI prints the explicit `--project`/positional path when provided,
otherwise the single live attached editor's root. The MCP server exposes the
same value as the `raf_project_path` tool, including a clean path without the
Windows `\\?\` prefix.

## 6. Connect an MCP client

Use stdio so the MCP server exists only for the client session:

```text
raf mcp serve --attach D:\Games\HorseDemo
```

Codex, Claude Code, OpenCode, or another MCP client should launch that command
as a local stdio server. The MCP adapter is thin: it forwards to the same
command catalog and attached endpoint instead of implementing a second engine.
It exposes `raf_context` and `raf_capabilities` in addition to the generic
`raf_command`. It also exposes typed `raf_scene_digest`, `raf_scene_program`,
`raf_scene_commit`, `raf_scene_patch`, `raf_scene_cancel`, `raf_world_brief`,
`raf_world_recipe`, `raf_world_script`, `raf_scene_visual_diff`,
`raf_project_close`, `raf_session_use`, `raf_undo`, `raf_redo`,
`raf_viewport_capture` (including `preset=contact_sheet` and
`preset=top_down_map`), `raf_scene_top_down_map`, `raf_scene_visual_review`,
`raf_game_completion_audit`, `raf_lighting_*`, `raf_script_*` (including
`raf_script_run`), `raf_project_path`, and `raf_task_*` tools. Every tool
publishes read-only/destructive/idempotent annotations and an output schema;
attached results include readable text plus a structured
`EngineCommandResponse` with compact model-facing lines and raw detail fields kept
available to JSON callers. Viewport captures also return the PNG as MCP image content.

Headless `raf mcp serve` intentionally advertises only the tools the headless
host can execute (inspection, project/session lifecycle and observations).
Mutation, viewport and task tools appear only in attached mode, so a client does
not discover operations that can only fail.

Plan compilation returns a `pipeline` object with compile, simulate, budget,
overlap, design, navigation and assertion gates. World Program operations include
`group`, `place`, `grid`, `along`, `scatter`, `mirror`, `variant`, `update`,
`delete` and `assert`. `raf_world_recipe` captures a successful plan as a
reusable project recipe, `raf_world_script` compiles a sandboxed Rhai authoring
script that cannot touch files or Runtime (and stores the source under
`.aura_rafi/world_scripts/`), and `raf_scene_visual_diff` compares before/after
captures with an annotated PNG artifact.

Plans are cancellable before commit with `raf scene cancel` / `raf_scene_cancel`
and replaceable incrementally with `raf_scene_patch`. `raf open PATH` launches
the editor and waits for its live endpoint; `raf mcp serve --attach PATH --launch`
does both in one step. MCP exposes `raf_set_mode(inspect|plan|active)` to gate
mutations, `raf_project_open` now switches the attached editor project,
`raf_project_save` honors `--wait`/`wait:true`, and successful mutations emit
`notifications/resourcesUpdated`. Resources include `raf://brief`, `raf://plans`,
`raf://plan/{id}` and `raf://digest`. The MCP `session` parameter is consumed
before the requested command. Transient provider errors are retried, and
text-only model observations are persisted per project in
`.aura_rafi/model_capabilities.json`. In the Agent workspace, evidence cards now show real thumbnails;
the detail view opens a larger image and can pin a capture to the context so the
next run receives it as visual evidence.

A dedicated visual reviewer is available as `raf_scene_visual_review` /
`scene.visual_review`: it selects an enabled vision-capable provider/model,
inspects one to four capture artifacts and returns structured scores, findings
and suggested fixes. If no vision model is configured it returns
`status: "unavailable"`, `fallback: "vision_unavailable"` and leaves the
captures as project evidence. It never starts Play or Runtime.

## Safety rules

- Always inspect, preview, confirm, apply, and inspect the result.
- Send `expected_revision` and a stable `idempotency_key` for mutations.
- Treat a Game `undo_token` as scoped and short-lived; do not persist it as a
  project secret. Use the editor's normal Undo/Redo for Electronics until its
  snapshot bridge exposes the same token contract.
- Keep generated entities, files, and tool calls within explicit budgets.
- Do not request Play, Stop, Runtime, arbitrary shell access, or arbitrary host
  filesystem access through the engine tools.

See [AGENT_CLI_MCP_EXPANSION.md](AGENT_CLI_MCP_EXPANSION.md) for the architecture
and the v0.12 headless-core plan.
