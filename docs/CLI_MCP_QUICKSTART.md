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

For visual feedback, an attached Game editor can expose the last rendered
viewport without entering Play mode:

```text
raf attach --project D:\Games\HorseDemo viewport capture --json
```

The response references a PNG saved inside the project at
`.aura_rafi/agent_artifacts/`. This is an observation of the current rendered
editor frame, not a new render request. The editor must have rendered the Game
viewport at least once.

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

## 5. Create and validate scripts

Scripting commands use the same attached editor endpoint and write only inside
the project:

```text
raf attach --project D:\Games\HorseDemo command script.create --json --params '{"lang":"rhai","name":"horse_controller"}' --confirm

raf attach --project D:\Games\HorseDemo command script.list --json
raf attach --project D:\Games\HorseDemo command script.validate --json --params '{"file":"scripts/horse_controller.rhai"}' --confirm
```

The external agent may also edit a project script directly through the normal
workspace tools. The engine command remains useful for project-scoped
creation, attachment, validation, and evidence.

## 6. Connect an MCP client

Use stdio so the MCP server exists only for the client session:

```text
raf mcp serve --attach D:\Games\HorseDemo
```

Codex, Claude Code, OpenCode, or another MCP client should launch that command
as a local stdio server. The MCP adapter is thin: it forwards to the same
command catalog and attached endpoint instead of implementing a second engine.
It exposes `raf_context` and `raf_capabilities` in addition to the generic
`raf_command`. It also exposes typed `raf_viewport_capture` and
`raf_task_*` observation tools; attached results include readable text plus a
structured `EngineCommandResponse` with compact model-facing lines and raw
detail fields kept available to JSON callers.

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
