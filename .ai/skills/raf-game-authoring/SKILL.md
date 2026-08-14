---
name: raf-game-authoring
description: Use when an AI must inspect, create, modify, or validate an AuraRafi game scene or project script through the Raf CLI, attached editor endpoint, MCP adapter, or direct project files. Keep authoring reversible, project-scoped, evidence-backed, and limited to editing; never enable Play, Stop, or Runtime.
---

# Raf game authoring

Use this skill to turn a natural-language game-building request into small,
verifiable AuraRafi edits. Prefer the attached editor while a project is open:
it owns the live scene, session, persistence queue, and real undo history.
Use headless CLI commands for inspection and workspace work when the editor is
closed. Do not claim that a closed-editor mutation ran until the optional
headless core host is implemented.

## Workflow

1. Identify the project path, project type, active session, and current
   revision. Run `raf doctor`, `engine.status`, `project.info`, and
   `capabilities.list` as appropriate.
2. Inspect the scene, scripts, assets, and workspace bounds before planning a
   mutation. Keep generated entity/file counts within an explicit budget.
3. Break a complex request into canonical commands. Use `dry_run=true` for
   writes, show the proposed diff, and ask for or use explicit confirmation.
4. Send `expected_revision` and a stable `idempotency_key` for every write.
5. Apply the smallest useful batch. Record the returned transaction, revision,
   structured diff, verification checks, and `undo_token`.
6. Inspect the result again. If the exact revision is still active, consume an
   attached undo token with `transaction.undo` when rollback is requested.
7. Save an explicit checkpoint with `project.save` and report real remaining
   blockers. Never hide an unsupported runtime feature behind a placeholder.

## Choose the connection

- Open editor: `raf attach --project PATH ...` or `raf mcp serve --attach PATH`.
  Use this for live scene creation, entity transforms, sessions, and script
  attachment.
- Closed editor: use `raf project`, `raf capabilities`, `raf workspace`, and
  `raf command` only for supported headless inspection/workspace operations.
  Do not invent live scene state.
- Internal Agent: call the shared command kernel directly; do not spawn the
  CLI or route through MCP inside the editor.

## Authoring priorities

Prefer `game.add`, `game.select`, `game.rename`, `game.duplicate`,
`game.set_transform`, `game.move`, `game.rotate`, `game.scale`, and
`game.delete` for deterministic scene edits. Use `session.*` for isolated
worlds and `project.save` for checkpoints. Use `script.create`,
`script.attach`, `script.detach`, `script.list`, `script.validate`, and
`script.compile_nodes` for editor-safe scripting. `script.run` is an editor
authoring check only; it is not Runtime.

For a request such as a horse-riding simulator with procedural terrain, first
produce a bounded editable foundation: seeded groups, terrain markers,
paths/spawn points, available assets, and scripts. Report what still requires
future renderer or Runtime systems instead of pretending to generate them.

## Safety and evidence

Keep commands project-relative. Reject path traversal, arbitrary shell access,
public listeners, and undocumented filesystem writes. Treat an undo token as a
short-lived capability, not a secret or permanent backup. A token is valid only
for its issuing project, session, and revision; stale tokens must be rejected.

Read the focused reference when needed:

- [references/cli.md](references/cli.md) for command-line syntax;
- [references/mcp.md](references/mcp.md) for stdio MCP setup;
- [references/commands.md](references/commands.md) for game and scripting
  command contracts;
- [references/safety-and-transactions.md](references/safety-and-transactions.md)
  for previews, revisions, idempotency, undo, and evidence.
