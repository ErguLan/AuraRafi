# Agent Setup Guide

This guide explains how to activate and use the AuraRafi Agent panel.

## What the Agent is

The Agent is a conversational AI interface inside the editor. It can read the
active project, create and modify scene objects, build electronic schematics,
run simulations, and manage scripts by calling the same slash commands that the
Console uses. It follows a bounded plan, executes through the shared command
kernel, and reports observable results without exposing private reasoning.

## Panel Layout

The Agent panel has two sections:

- **Left sidebar**: lists all chat sessions for the current project. Click to
  switch sessions, right-click to delete. The "New chat" button starts a fresh
  conversation.
- **Right content**: the conversation view with model/mode selectors at the
  top, scrollable message area, quick suggestion chips, and the text input.

The sidebar can be toggled open/closed with the `<` / `>` button.

## Activation steps

1. Open **Settings > AI Providers**.
2. Choose the provider you want to use:
   - **OpenRouter**: enter your API key and pick a model id (or add a model
     shortcut in the Agent panel).
   - **OpenAI**: enter the API key and model id.
   - **OpenRouter**: enter the API key and model id, or use a compatible
     gateway URL.
   - Puerto, GenAI and Claude remain readable as legacy provider values, but
     are not exposed as verified native editor transports in this release.
3. Set that provider as the default with **Set as default**.
4. Select the **Agent mode**:
   - **Inspect**: native project reads only; mutation tools are hidden.
   - **Plan**: reads run normally and mutations are previewed without changing the project.
   - **Active**: mutations run immediately through the shared command gateway.
5. (Optional) Open the **Agent** panel and click **+ Add model** to create
   shortcuts for the models you use most. These are saved globally.
6. Open or create a project.
7. Type a request in the Agent input and press **Send** or `Ctrl+Enter`.

## How tool calls work

Before the first provider request, the editor supplies a compact snapshot of
the active project, scene hierarchy, selection, assets, scripts, session and
revision. The model then calls contextual tools. Semantic Game tools map to
the canonical command gateway only at the final adapter, and return structured
`summary`, `data`, stable references, revision, diff and verification evidence.
Large mesh/debug payloads are kept out of the normal transcript.

## Preview and execution flow

1. Use **Inspect** when you only want the Agent to explain what is mounted.
2. Use **Plan** to review the resulting summaries and preview diffs.
3. Use **Active** when the request is ready to modify the project.

## Chat sessions

Each project has its own set of chat sessions, stored in `.ai/agent_history.ron`.
Sessions persist across editor restarts. Use the sidebar to switch between
conversations or start fresh ones.

## Tips

- Be specific: "create a red cube named Player at origin" works better than
  "add a cube".
- For electronics, ask the Agent to run `electronics.drc` or `electronics.simulate`
  after building a circuit.
- For large scenes, ask it to use prefabs and arrange them in a grid.
- The Agent cannot read arbitrary files outside the active project folder.

## Troubleshooting

- **No response**: verify the provider is enabled, the API key is set, and the
  base URL is reachable.
- **Model errors**: some providers require a specific model id format. Use the
  provider's documentation or add a shortcut with the exact id.
- **Tool call fails**: the command output is returned to the model; it may retry
  with corrected parameters.
- **Engine freezes**: the Agent runs HTTP requests on a background thread. If
  the editor still freezes, check that the API endpoint is responsive.
