# AuraRafi AI System

This document explains how the AI agent system works in AuraRafi, from the user
typing a message to the engine executing commands and returning results.

## Architecture Overview

```
User types message
  -> AgentPanel (ai_chat.rs)                         UI layer
    -> AgentRuntime (agent_runtime.rs)                State machine
      -> OpenAiClient (openai_client.rs)              HTTP to LLM API
        <- Response with tool calls or text
      -> AgentToolExecutor (agent_executor.rs)        Routes tools to handlers
        -> Command handlers (game, electronics, etc.)  Engine mutations
        <- CommandOutput
      -> Result formatted back to LLM or user
```

## Key Files

### Editor Side (`crates/raf_editor/`)

| File | Role |
|------|------|
| `src/panels/ai_chat.rs` | AgentPanel UI: left sidebar with sessions, message bubbles, model/mode selectors, input area |
| `src/agent_executor.rs` | AgentToolExecutor: converts tool calls to engine commands, builds tool definitions from catalog, sanitizes tool names |
| `src/app.rs` | Constructs the executor each frame (lines 1185-1217), wires AgentPanel + Settings panel |

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
| `src/ai.rs` | AgentMode, AiProvider, AiProviderConfig, AiModelShortcut types |
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

### 2. Non-blocking Runtime

The agent runtime runs HTTP requests on a background thread so the editor UI
never freezes:

1. `AgentRuntime::start_run()` pushes the user message and spawns a thread
   that calls the LLM API.
2. Each frame, `AgentRuntime::poll()` checks if the thread completed.
3. If the response contains tool calls:
   - **Active mode**: approved tools execute one per editor `poll()` frame,
     then the next API request starts automatically.
   - **Passive mode**: the UI shows the pending calls and waits for user
     approval before continuing.
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

### 3. Tool Name Sanitization

Some LLM providers (Cohere, etc.) reject tool names containing dots or special
characters. The system automatically sanitizes command names:

- `project.info` becomes `project_info`
- `workspace.read` becomes `workspace_read`
- Leading digits get an underscore prefix

A reverse map (`tool_name_map`) translates sanitized names back to original
command names before execution.

### 4. Settings Linkage

The Agent panel reads from the same `EngineSettings` as the Settings panel:

- `settings.default_ai_provider` -- which provider is active
- `settings.ai_providers` -- per-provider base URL, model, API key
- `settings.agent_mode` -- Passive (ask before commands) or Active (auto-execute)
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
