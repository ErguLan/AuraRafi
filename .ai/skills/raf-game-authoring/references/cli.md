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

Open the Game project first. Then use:

```text
raf attach --project PATH status --json
raf attach --project PATH capabilities --json
raf attach --project PATH command NAME --params JSON --dry-run
raf attach --project PATH command NAME --params JSON --confirm --idempotency-key KEY
```

`--expected-revision N` protects against a newer manual edit. Attached command
responses are JSON when `--json` or `--ndjson` is selected.

## MCP

Run `raf mcp serve --attach PATH` to expose the same attached command kernel as
a local stdio server. Do not put the endpoint on a public interface.
