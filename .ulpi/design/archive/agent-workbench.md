---
project: ProyectRaf
feature: agent-workbench
binds_to: .ulpi/design/DESIGN.md
direction: calm command workbench
---

# Agent workbench

## Design read

The Agent should feel like a second pair of hands inside the editor: present,
legible, and quiet until it is planning or asking permission. The UI bets on a
visible activity trail and a strong active edge instead of a generic chatbot
layout.

## Primary flow

1. The user opens Agent from the top toolbar or the `Agent` downbar tab.
2. The left rail shows project-local chats. `New chat` is always the first
   action; the selected conversation uses the orange active edge.
3. The header exposes provider, model, permission mode, and configuration
   readiness without hiding them in a second screen.
4. The composer sends a request. `Send` morphs into `Stop` while the runtime is
   thinking or applying tools.
5. The activity strip communicates planning, tool execution, approval, and
   completion. It never renders private chain-of-thought text.
6. Passive mode presents tool calls as review cards. Active mode keeps the
   warning visible and executes approved-by-mode calls in order.

## States that must be visible

- Empty: short explanation plus contextual prompts for Game or Electronics.
- Ready: provider/model status is compact and affirmative.
- Thinking: animated active edge, elapsed activity label, disabled composer.
- Executing tools: one tool card changes state at a time; no frame-blocking UI.
- Awaiting approval: explicit command names, arguments, `Approve`, and `Deny`.
- Provider disabled, model missing, adapter required, and network error:
  actionable message with a Settings entry.
- Stopped: neutral status, preserved conversation, composer available again.
- Project switch: load the project's `.ai/agent_history.ron` and restore the
  previous chat selection.

## Component rules

- The internal chat rail is 220px at regular width, collapses to an icon rail,
  and becomes a full-width section below the narrow breakpoint.
- Conversation messages are left-aligned by role, with user messages using a
  restrained raised surface and Agent messages using the canvas surface plus
  the active edge. Tool results are nested cards, not fake messages.
- The activity strip is one row high when idle and expands only for tool
  details or approval. It uses motion to explain change, never as decoration.
- All controls have a semantic node id, a padded hitbox, a keyboard focus state,
  an accessibility label, and an i18n key or literal runtime value.
- Motion follows the locked 120ms feedback, 220ms dock transition, and 360ms
  layout transition. Reduced motion removes pulse and interpolation.

## Toolbar entry

The top application bar gets a compact `Agent` action between the project
menus and the flexible drag region. It opens/focuses the Agent tab and changes
its small status edge according to Ready, Thinking, Approval, or Error. The
toolbar remains useful when the downbar is collapsed.

## Backend boundary

`raf_ai` remains UI-independent. The editor owns a controller that projects
runtime/history/configuration into a snapshot and translates typed actions back
to the runtime. The retained surface does not own provider clients, project
mutation, persistence, or threads.

The executor receives the current editor command context and routes through the
existing command handlers. UI-document commands are optional capabilities; the
Agent must still work when no UI document is mounted.

## Performance acceptance

- No repaint loop while Agent is idle or an animation has settled.
- Runtime polling happens once per frame and tool mutations remain bounded to
  one tool call per poll.
- Thinking/activity motion is measured on GPU and CPU presentation paths.
- A stopped run cannot apply stale network results to the current conversation.

## Handoff acceptance

- Toolbar and downbar both focus the same Agent controller.
- Chat creation, selection, persistence, thinking, stop, approvals, and tool
  result states work with a headless runtime test.
- Provider settings round-trip through `EngineSettings` without exposing keys
  in labels or logs.
- Dark, light, compact, keyboard, GPU, and CPU presentations retain the same
  hierarchy and hitboxes.
