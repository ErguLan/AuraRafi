# Agent, CLI, and MCP Expansion

Status: design direction for the game-first stabilization cycle. This document
does not activate Play, Stop, Runtime, or autonomous background execution.

## Mission

AuraRafi needs one lightweight agent harness that can be used from three
surfaces without duplicating engine behavior:

- the retained Agent inside the editor;
- the `raf` CLI used by a developer or automation;
- the `raf mcp serve` adapter used by Codex, Claude Code, OpenCode, or another
  compatible client.

The immediate purpose is practical: build a serious game while the engine is
still stabilizing. Manual authoring and AI-assisted authoring will exercise the
same project, expose missing workflows, and turn real game blockers into engine
fixes. The Agent is a co-developer of editable project documents, not an
in-game runtime system.

## Product Strategy: Game-First Stabilization

The next cycle combines two activities:

1. Stabilize RafUI, sessions, scripting, viewport behavior, saving, and undo.
2. Build a real game manually and through agent tools against those same
   systems.

Every problem found while producing the game must be classified before the
engine grows:

- engine defect: fix the shared system;
- missing authoring capability: add a reusable command/tool;
- game-specific content: keep it in the private game project;
- future runtime feature: document it, but do not pull Runtime into this phase.

This prevents the engine from expanding through hypothetical features while
still allowing the game to drive useful command coverage.

## Current Baseline

The foundation already exists:

- `assets/commands/catalog.json` is the canonical command catalog;
- `EngineCommandRequest` and `EngineCommandResponse` are transport-neutral;
- the internal Agent can execute editor domain commands;
- `raf` can inspect, create, and open projects without Egui or WGPU;
- `raf serve` exposes bounded JSONL over stdio;
- `raf mcp serve` exposes MCP tools and resources over stdio;
- requests already carry confirmation, dry-run, revision, transaction,
  idempotency, session, and execution-budget metadata.

The historical missing link was attached mutation: an external CLI or MCP
process could not forward a command such as `game.add` to the editor instance
that owns the live scene document and undo history.

The baby bridge is now implemented as the next stabilization slice: the
editor publishes `.aura_rafi/agent_endpoint.json`, accepts a token-scoped
handshake, and queues requests onto the UI thread. The endpoint owns no scene
and no renderer state. Attached scene mutations now return a scoped undo token
backed by the editor's real `SceneHistory`; the token is rejected after a
project, session, or revision change. The remaining gate is a smoke project
against a running editor and visual proof of response/diff/undo behavior; no
Runtime is required for that gate.

## Beta operating modes

The early beta has two explicit modes:

- **Attached authoring (current):** open the project in the editor, then use
  `raf attach` or `raf mcp serve --attach`. This is the complete path for live
  scene creation and scripting because the editor owns the loaded documents and
  undo history.
- **Optional headless core (future):** when the editor is closed, a manually
  activated `raf host`/MCP process may load the project and expose the same
  command kernel without RafUI, Egui, or a renderer. It is not a resident
  service: the user starts it for a task, it advertises a local endpoint, and
  it exits or unloads when the task is complete. Resource budgets and an
  explicit opt-in keep potato hardware responsive.

The current headless CLI already supports project inspection and workspace
operations. It intentionally does not pretend that live scene mutations have
run while the editor is closed; that host is a v0.12 expansion after scene,
session, and scripting persistence are stable.

## One Kernel, Three Adapters

```text
Internal Agent ------------------------------+
                                              |
RafUI / Console ------------------------------+--> Command kernel
                                              |      -> project documents
External Codex / Claude / OpenCode            |      -> scene and assets
  -> MCP stdio -> raf mcp serve -> local IPC -+      -> scripts and sessions

CLI user -> raf -> local IPC -----------------+
```

The internal Agent should call the command endpoint directly. Sending it
through MCP or spawning the CLI would add serialization, process, and latency
cost with no benefit. External agents use CLI or MCP, but reach the same command
catalog, permission rules, revision checks, and result contract.

## Early Attached Mode: Baby Vertical Slice

This slice may be implemented before v0.12 once the current RafUI migration is
stable enough. It is intentionally smaller than the full v0.12 harness.

1. The editor owns a project-scoped local command endpoint.
2. The first transport is a loopback TCP listener (`127.0.0.1` with a random
   port). It is local-only and never becomes a public network server by
   default. Windows named pipes and Unix sockets can replace the transport
   behind the same frames later without changing the command kernel.
3. The editor publishes a short-lived discovery record under `.aura_rafi/`
   containing protocol version, project identity, process identity, endpoint,
   and a random session token. Secrets never enter project source control.
4. `raf attach --project <path>` performs a handshake and reads capabilities,
   active session, document revision, and allowed command domains.
5. `raf attach --project <path> command ...` forwards an
   `EngineCommandRequest` to the editor and
   returns the real `EngineCommandResponse` as human text, JSON, or NDJSON.
6. `raf mcp serve --attach <project>` uses that same connection. If no editor
   is attached, it remains headless and does not pretend that mutations ran.
7. Disconnects, stale revisions, changed sessions, and editor shutdown return
   explicit recoverable errors.

The attached bridge must not know about Egui. Its host belongs beside the
editor application state and command gateway, so replacing the window host
does not require replacing CLI or MCP.

## Initial Game Toolpack

The first external mutation allowlist should cover editable game production,
not every engine subsystem.

### Inspect

- project metadata, active session, capabilities, and revision;
- scene summary, hierarchy, selected entities, transforms, and bounds;
- assets, materials, scripts, diagnostics, and dirty/save state;
- bounded workspace read and search inside the project.

### Modify

- create, select, rename, duplicate, delete, and group entities;
- batch transforms, colors, material references, and parent relationships;
- create reusable prefabs from primitives or existing entity groups;
- import or generate project-scoped assets through existing asset workers;
- create, attach, edit, and validate Rhai or C++ scripts through the scripting
  host boundary;
- create/open/duplicate project sessions and save explicit checkpoints.

### Complex but still editor-only

- deterministic terrain generation from seed and bounded parameters;
- roads, paths, vegetation regions, spawn markers, and repeated prop layouts;
- character assembly from available assets, components, colliders, and scripts;
- material assignment and asset relinking;
- scene-wide validation and repair proposals.

Complex tools should be plans composed from smaller canonical commands. The
response must identify every subcommand and resulting entity or artifact. A
large request should not become one opaque `make_game` mutation.

## Transaction and Verification Contract

Agent quality comes from evidence and recovery, not only model intelligence.
Every mutating plan should follow this sequence:

```text
inspect -> plan -> preview -> confirm -> apply -> inspect result -> validate
```

Required behavior:

- expected revision prevents writing over a newer manual edit;
- idempotency keys prevent accidental duplicate terrain, entities, or assets;
- a composite transaction groups related commands into one visible operation;
- the editor owns real undo/redo integration and returns a valid undo token only
  when rollback is actually available. In the current attached slice,
  `transaction.undo` consumes that token only at the exact issuing revision;
- structured diffs list created, modified, deleted, and relinked objects;
- verification returns checks, warnings, failures, and artifact references;
- budgets bound tool calls, elapsed time, generated entities, filesystem reads,
  and result size for potato hardware.

Long tasks must be cooperative and cancellable. They should yield progress
events and avoid holding the UI thread, but durable MCP tasks and resume support
remain part of the full v0.12 harness.

## Example: Horse-Riding Simulator Request

For a request such as "create a horse-riding simulator with procedural
terrain," the Agent should not claim to finish an unsupported game runtime. It
should instead produce a useful editable foundation:

1. inspect the project, scene, asset inventory, and available script APIs;
2. create a bounded terrain plan with seed, size, density, and entity budget;
3. preview the scene diff and estimated resource cost;
4. create terrain chunks, paths, spawn markers, environment groups, and named
   placeholders or available character assets;
5. generate or attach editor-safe movement/camera scripts only through the
   scripting Host API;
6. validate hierarchy, missing assets, script syntax, bounds, and budget;
7. return a checkpoint, structured evidence, and the remaining manual or future
   runtime blockers.

That is already valuable for production and engine stabilization without
turning on Play or Runtime.

## Implementation Order

### Pull forward when RafUI migration permits

- attached local endpoint and handshake;
- project/session scoping and capability negotiation;
- external read tools plus a small game mutation allowlist;
- real editor-owned transactions, scoped undo tokens, diffs, and verification;
- CLI and MCP connection presets for external agents;
- one deterministic golden game project for regression testing.

### Complete in v0.12

- resumable/cancellable task protocol and progress streaming;
- optional manually activated headless core host for closed-editor authoring,
  with renderer-free startup, project locks, budgets, and explicit shutdown;
- game-first skills and toolpacks for terrain, characters, materials, animation,
  particles, prefabs, and validation as their engine systems become real;
- evidence and artifact resources consumable by both MCP and the Agent UI;
- richer policy profiles, command budgets, audit logs, checkpoints, and replay;
- optional ACP support only if AuraRafi later becomes a visual client for full
  external coding agents rather than merely exposing engine tools.

## Explicitly Out of Scope Now

- Play, Stop, game Runtime, runtime simulation, or autonomous in-game agents;
- remote unauthenticated command servers;
- direct access to engine internals from scripts or agents;
- arbitrary host filesystem or shell access through engine tools;
- an Egui-specific CLI/MCP bridge;
- pretending that unsupported PBR, animation, particles, or physics features
  were created successfully.

The success criterion for the early slice is simple: an external agent can
attach to an open project, inspect it, make a reversible document edit, verify
the result, and return control to the developer without freezing the editor or
starting Runtime.
