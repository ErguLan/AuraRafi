# Raf CLI reference

Build from the repository root with `cargo build -p raf_cli`. The binary is
`target/debug/raf.exe` on Windows.

## Headless inspection

```text
raf doctor --json
raf status --project PATH --json
raf project info PATH --json
raf capabilities --json
raf workspace describe PATH --json
```

## Attached editor

Open the Game project first. `raf editors --json` lists known projects, their
type (game/electronics), editor state (hub/project), and which editors are
live right now. When exactly one editor is live, the `--project` flag is
optional. `raf editors --wait` blocks (max 15 s by default) until an editor
appears:

```text
raf editors --json
raf editors --wait
raf attach status --json
raf attach capabilities --json
raf attach command engine.status --json
raf attach command game.batch --confirm --params '{"operations":[{"name":"game.add","params":{"primitive":"cube","name":"Stable"}},{"name":"game.move","params":{"name":"Stable","x":2}}]}'
raf attach command NAME --params JSON --dry-run
raf attach command NAME --params JSON --confirm --idempotency-key KEY
```

Attached `engine.status` is always safe: it reports editor state, project
type, entity count, and capabilities without touching a domain executor.

`--expected-revision N` protects against a newer manual edit. Attached command
responses are JSON when `--json` or `--ndjson` is selected.

## MCP

Run `raf mcp serve --attach PATH` to expose the same attached command kernel as
a local stdio server. Do not put the endpoint on a public interface.
