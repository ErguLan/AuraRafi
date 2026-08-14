# Raf MCP reference

Start the adapter only for the client session:

```text
raf mcp serve --attach D:\Games\HorseDemo
```

Configure the external client to launch that command as a local stdio MCP
server. The adapter exposes command tools, capabilities, project resources,
and bounded workspace resources. It does not own a second scene model.

The MCP flow is:

```text
client -> MCP stdio -> raf mcp -> local attached handshake -> open editor
```

If no editor is open, use the headless CLI for supported inspection only. The
MCP adapter must report an explicit attachment error instead of pretending that
game mutations ran. Play, Stop, Runtime, public network listeners, and arbitrary
host shell access remain unavailable.
