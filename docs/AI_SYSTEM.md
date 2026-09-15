# AuraRafi AI System

This document explains how the AI agent system works in AuraRafi, from the user
typing a message to the engine executing commands and returning results.

## Architecture Overview

The internal Agent, external CLI, and MCP expansion share one command kernel.
The game-first attached-mode direction, safety boundary, initial toolpack, and
v0.12 target are specified in
[`AGENT_CLI_MCP_EXPANSION.md`](AGENT_CLI_MCP_EXPANSION.md).

Human connection steps are in [`CLI_MCP_QUICKSTART.md`](CLI_MCP_QUICKSTART.md).
The reusable AI workflow is the project skill at
`.ai/skills/raf-game-authoring/SKILL.md`; it keeps scene and scripting work
attached, reversible, budgeted, and outside Play/Runtime.

```
User types message
  -> AgentSurfaceHost (agent_surface.rs)              Native UI layer
    -> AgentPanel + AgentRuntime                      Session/state machine
      -> OpenAiClient (openai_client.rs)              HTTP to LLM API
        <- Response with tool calls or text
      -> ProjectSnapshot + contextual Agent tool pack
      -> AgentToolExecutor (agent_executor.rs)        Native typed tool layer
        -> EngineCommandRequest -> CommandGateway -> domain kernel
        <- EngineCommandResponse + verification + diff
      -> Compact AgentToolResult formatted for LLM and UI
```

## Key Files

### Editor Side (`crates/raf_editor/`)

| File | Role |
|------|------|
| `src/panels/agent_surface.rs` | Retained native Agent surface: sidebar, transcript, model/mode controls, approvals and input |
| `src/panels/ai_chat.rs` | AgentPanel session/readiness model consumed by the native surface |
| `src/agent_context.rs` | ProjectSnapshot, scene hierarchy/query/inspect, asset/script catalog, health and verification reads |
| `src/agent_executor.rs` | Contextual semantic tool packs and typed Agent tool-call-to-CommandGateway adapter |
| `src/native_workbench.rs` | Owns the active native editor composition and connects the Agent surface to the editor runtime |
| `src/attached.rs` | Token-scoped loopback endpoint; queues external CLI/MCP requests onto the editor thread |

### AI Crate (`crates/raf_ai/`)

| File | Role |
|------|------|
| `src/agent_runtime.rs` | Non-blocking state machine: `start_run()` spawns HTTP on background thread, `poll()` advances the loop each frame |
| `src/openai_client.rs` | OpenAI-compatible HTTP client. Talks to any `/chat/completions` endpoint |
| `src/agent_history.rs` | Per-project session persistence as `.ai/agent_history.ron` |
| `src/agent_model_registry.rs` | User-defined model shortcuts (label -> provider + model_id) |
| `src/chat.rs` | ChatMessage, MessageRole, ChatPanel structs |
| `src/puerto.rs` | Bridge client for OpenClawd / OpenClaw gateways |
| `AGENT.md` | System prompt embedded into every LLM request |

### Core (`crates/raf_core/`)

| File | Role |
|------|------|
| `src/ai.rs` | Inspect/Plan/Active AgentMode, provider and model shortcut types |
| `locales/en.json` | English UI strings (agent_* keys) |
| `locales/es.json` | Spanish UI strings |

### Configuration (`assets/commands/`)

| File | Role |
|------|------|
| `catalog.json` | 40+ command definitions with parameters, domains, aliases |

## How it works

### 1. Panel Layout

The Agent panel is split into two areas:

- **Left sidebar**: lists chat sessions for the active project. Click to switch,
  right-click to delete. "New chat" starts a fresh session.
- **Right content**: header with model selector and mode toggle, scrollable
  message area, quick suggestion chips, and text input.

### Agent history rendering

The Agent runtime keeps the conversation available for the next model request,
but the retained UI renders the transcript through one continuous scroll
surface. `settings.agent_message_page_size` is a legacy persisted compatibility
field (default `24`, bounded to `20..64`); it is not a network page size and
must not be treated as a second history store.

The native Agent keeps history I/O in the runtime and outside the render loop.
The surface fingerprints only the visible message content needed for
invalidation, so a long tool-result history does not make every idle frame
proportional to stored output.

#### 2026-08-06 Agent FPS incident

The severe Agent-only FPS drop was caused by the retained bridge's GPU path:
it rendered a surface but failed to record the logical and physical sizes of
that completed presentation. The next frame therefore believed its target was
new and rebuilt/presented the Agent again, even while idle. Other tabs looked
stable because they did not execute that Agent bridge. The fix records both
sizes after a successful GPU render; resize, surface replacement, and actual
input still invalidate normally.

The session transition resets retained scroll, focus, hover, and pointer capture
state. This prevents a previous chat's transient interaction from being applied
to the next document. Do not replace this with a full history rebuild on every
frame, clear the text atlas during idle presentation, or move history
persistence into the render loop.

Retained surfaces also have explicit pointer ownership. A press that started
in the 3D viewport must not become an Agent drag merely because the cursor
crossed the Agent rectangle while the button remains held. The bridge filters
foreign button activity and only accepts a gesture that started inside the
surface or is already captured by that surface. This boundary is required for
camera orbit, pan, and other cross-surface drags.

The Agent surface key fingerprints only the visible portion of the newest
message card (900 characters plus the truncation boundary). It must not hash or
serialize an entire tool result every frame; full history remains available to
the runtime without making idle Agent frames proportional to stored output.

### 2. Non-blocking Runtime

The agent runtime runs HTTP requests on a background thread so the editor UI
never freezes:

1. `AgentRuntime::start_run()` pushes the user message and spawns a thread
   that calls the LLM API.
2. Each frame, `AgentRuntime::poll()` checks if the thread completed.
3. If the response contains tool calls, the runtime executes at most one per
   editor poll interval, preserving mutation order. Inspect runs reads only;
   Plan routes mutations to disposable preview state; Active applies them.
4. If the response is plain text, it is added to the message list and the runtime
   returns to `Done` state.

When a tool changes a scene, electronics document, or UI document, the editor
captures the prior document snapshot only for that tool-execution frame and
makes it available through the normal Undo/Redo stack. Agent-requested Undo
and Redo are queued back to the editor after the tool callback releases its
temporary document borrows.

Every request has bounded editor-side limits: a turn limit, a tool-call limit,
and a maximum tool-result size. Malformed tool arguments stay visible for
review but are never passed to the command executor. These guardrails keep an
ambitious generation request cooperative with the editor; they do not enable a
game runtime or autonomous in-game agent.

### 3. Native Agent perception and contextual tool packs

Before the first provider request for a submitted prompt, the workbench builds
a compact `ProjectSnapshot` from the mounted `SceneGraph`, selection, active
session, worker-backed `ProjectCatalog`, and shared revision ledger. The
snapshot is seeded as an ephemeral `project_summary` tool exchange in the
provider context. It is not duplicated inside the system prompt, persisted as
a fake chat message, or rendered as user text. The Agent therefore starts with
real project evidence while the conversation history remains truthful and
compact.

The pack is selected by project domain and prompt intent. A Game project never
receives Electronics tools. Normal questions receive read tools only;
authoring prompts receive semantic mutation tools. Observation tools are
bounded and paginated: `project_summary`, `scene_outline`, `scene_query`,
`scene_spatial_map`, `scene_check_overlaps`, `scene_diff`, `scene_design_audit`,
`scene_inspect`, `assets_catalog`, `assets_recommend`, `scripts_catalog`,
`project_health`, `scene_verify`, and
`game_validate_layout`.

Game mutations use `scene_create`, `scene_update`, `scene_delete`,
`scene_duplicate`, `scene_reparent`, `scene_snap`, `scene_arrange`,
`scene_instantiate_template`, and atomic `scene_batch`. The provider sees nested
transforms and stable target fields; only the final adapter flattens them for the
legacy domain handler. Repeated operations use explicit `count`, `axis`,
`spacing`, and `offset` fields instead of prompt-generated command text.

The scene authoring contract is shared by the native Agent, CLI, and MCP:
vectors are always flat `[x, y, z]`, invalid nested shapes return an actionable
structured diagnostic, and post-build verification can use `scene_diff` and
`scene_check_overlaps` before claiming completion.

### 4. Tool Name Sanitization

Some LLM providers (Cohere, etc.) reject tool names containing dots or special
characters. The system automatically sanitizes command names:

- `project.info` becomes `project_info`
- `workspace.read` becomes `workspace_read`
- Leading digits get an underscore prefix

A contextual route table maps the provider-safe name to either a native read or
a typed command route. It no longer reconstructs a slash command string.

### 5. Settings Linkage

The Agent panel reads from the same `EngineSettings` as the Settings panel:

- `settings.default_ai_provider` -- which provider is active
- `settings.ai_providers` -- per-provider base URL, model, API key
- `settings.agent_mode` -- Inspect, Plan (preview), or Active (apply)
- `settings.agent_model_shortcuts` -- user-defined model shortcuts
- `settings.language` -- UI language

Changes in either panel are reflected immediately because both operate on the
same `EngineSettings` reference. Agent panel saves changes to disk via the
`settings_changed` flag.

Implementation note: the panel refreshes the model shortcut registry before
building the runtime client config, so a selected shortcut resolves to the
correct provider/model before the next request is sent.

The runtime client is rebuilt only when its effective endpoint, model, key, or
generation settings change. Project history restores the saved active session
instead of creating a replacement session on every project open. System prompts
remain runtime-only: they are sent to the model but hidden from the visible
conversation and omitted from persisted history.

### Streaming responses

`Settings > AI > Stream assistant responses` controls the OpenAI-compatible
Server-Sent Events path and defaults to enabled. With it enabled, text deltas
are forwarded from the `agent-api-call` thread to the editor and the visible
assistant card grows while the provider is generating. Disabling it keeps the
same background request and waits for one complete response.

Streaming does not bypass tools. Tool-call fragments are accumulated by the
client until the response finishes, then converted into the same normal tool
calls used by passive approvals and active execution. This is important because
executing a partially received JSON argument would be unsafe. Providers that
do not support `stream: true` can be used with the toggle disabled.

### Provider Transport Boundary

The current tool loop uses the OpenAI-compatible chat-completions protocol.
OpenAI, OpenRouter, and any user-supplied compatible gateway can use it
directly. Default native endpoints that require a different request format are
shown as unavailable until an adapter exists or the user configures a
compatible gateway URL. This prevents a configured model from failing later
with a misleading request error.

### 5. Adding a New Command as a Tool

1. Add the command definition to `assets/commands/catalog.json`
2. Implement the handler in the appropriate domain module
3. Register the handler route in `agent_executor.rs` `dispatch_shared_command()`
4. The tool is automatically available to the agent on next restart

### 6. Provider Configuration

The current editor exposes only the transports verified for this release:
OpenRouter and OpenAI. Legacy provider values remain readable in old settings,
but are normalized out of the active editor configuration until a dedicated
adapter and live validation exist.

Each active provider is configured with:

- **Base URL**: API endpoint
- **Model**: model identifier string
- **API Key**: authentication token
- **Enabled**: toggle visibility in the agent panel

The agent uses the default provider's configuration. Model shortcuts let the
user create quick-select entries for frequently used models across any provider.

## Image Asset Worker

`asset.generate_image` starts a Python process only for that explicit command.
It uses `gpt-image-2` by default, reads `OPENAI_API_KEY` from the environment,
stores a PNG in `assets/generated/`, writes prompt/model metadata beside it,
and returns a job result through a staging file. It is not a resident service,
does not run every frame, and never stores credentials in the project.

`asset.generate_local_png` uses that same isolated process boundary but never
contacts a provider. It creates deterministic icon, badge, placeholder sprite,
or reference texture PNGs using only the prompt and a local palette. The agent
is instructed to choose it for editor icons, placeholders, and cheap visual
references; remote generation is reserved for assets that need actual visual
interpretation.

The editor polls the child process without blocking UI work. `asset.image_status`
and `asset.cancel_image` expose job control to the console and Agent tool loop.
The default endpoint is OpenAI Images; a compatible endpoint can be configured
through the worker environment without modifying the engine. Set
`RAF_PYTHON_EXECUTABLE` when Python is not on `PATH`; set
`RAF_AI_IMAGE_BASE_URL` or `RAF_AI_IMAGE_ENDPOINT` only for a compatible image
gateway.

## Electronics Diagnosis

For a circuit correctness question, the agent is instructed to call
`electronics.diagnose` before suggesting a change. The command returns the
pin-to-net topology, DRC findings (including dangling wire endpoints and
component-pin bypass shorts), component identities, and the available DC
simulation messages in one structured result. This prevents a reply based only
on counts of components, wires, or nets and gives the agent evidence to name
the exact affected components and connections.
