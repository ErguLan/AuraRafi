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
executor. `game.batch` runs up to 64 game operations in one round trip, each
through the normal gateway (history and idempotency preserved).

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

The response includes the new revision, a structured scene diff, verification
checks, and an `undo_token`. To roll back that exact attached edit:

```text
raf attach --project D:\Games\HorseDemo command transaction.undo --json --params '{"token":"<undo_token>"}' --confirm
```

The token is rejected if another edit, session switch, or project change has
advanced the document. This prevents an AI from undoing somebody else's work.

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

## Safety rules

- Always inspect, preview, confirm, apply, and inspect the result.
- Send `expected_revision` and a stable `idempotency_key` for mutations.
- Treat `undo_token` as scoped and short-lived; do not persist it as a project
  secret.
- Keep generated entities, files, and tool calls within explicit budgets.
- Do not request Play, Stop, Runtime, arbitrary shell access, or arbitrary host
  filesystem access through the engine tools.

See [AGENT_CLI_MCP_EXPANSION.md](AGENT_CLI_MCP_EXPANSION.md) for the architecture
and the v0.12 headless-core plan.
