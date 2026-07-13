# Agent Setup Guide

This guide explains how to activate and use the AuraRafi Agent panel.

## What the Agent is

The Agent is a conversational AI interface inside the editor. It can read the
active project, create and modify scene objects, build electronic schematics,
run simulations, and manage scripts by calling the same slash commands that the
Console uses. The agent uses chain-of-thought reasoning: it plans, executes step
by step, and explains results in natural language.

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
   - **Puerto**: point to your OpenClawd / OpenClaw gateway URL.
   - **OpenAI / GenAI / Claude**: enter the API key and model id.
3. Set that provider as the default with **Set as default**.
4. Select the **Agent mode**:
   - **Passive**: every tool call stops and asks for approval.
   - **Active**: tool calls run immediately. Faster, but review the risk warning.
5. (Optional) Open the **Agent** panel and click **+ Add model** to create
   shortcuts for the models you use most. These are saved globally.
6. Open or create a project.
7. Type a request in the Agent input and press **Send** or `Ctrl+Enter`.

## How tool calls work

When the model decides to act, it calls one or more tools. Each tool maps to an
AuraRafi command. The engine executes the command and returns the output to the
model, which then decides the next step. Every command output is also logged to
the Console panel so you can inspect it.

The agent shows its thinking: before each tool call it explains what it is about
to do, and after execution it summarizes the result in natural language instead
of dumping raw JSON.

## Approval flow (Passive mode)

1. The Agent shows the list of commands it wants to run.
2. Click **Approve** to execute them and continue.
3. Click **Deny** to skip them and let the model react to the denial.

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
