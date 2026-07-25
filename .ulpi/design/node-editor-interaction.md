---
feature: Node Editor
binds_to: DESIGN.md
status: implementation-authorized
---

# Node Editor: Intent-First Workbench

## Design Read

The Node Editor is a live wiring desk, not a form builder: the graph remains
quiet until the user acts, then spatial feedback makes the next valid action
obvious without forcing a tool mode.

## Direction

Use the locked technical/utilitarian studio language as a **signal-routing
workbench**. The signature remains the thin orange active edge. Node category
color communicates type only; it never becomes page decoration. This rejects
generic white-card dashboards, oversized rounded controls, neon glows, and
fake motion.

## Ownership

| Region | Owner | Source of truth |
| --- | --- | --- |
| Flow rail, toolbar, node browser, inspector, status | RafUI retained surfaces | `NodeEditorPanel` view model |
| Nodes, pins, cables, selection, minimap | dedicated graph canvas | `NodeGraph` and canvas transform |
| Graph mutations, undo/redo, persistence | `NodeEditorPanel` and application persistence | existing graph/history model |

The shell must emit typed actions. It must never retain a second graph or a
decorative minimap. The canvas and minimap consume the same `NodeGraph`, zoom,
and offset.

## Interaction Contract

### Navigation

- Wheel zooms toward the cursor. `Ctrl + wheel` increases zoom velocity.
- Middle drag pans. `Shift + middle drag` constrains and accelerates horizontal
  pan.
- Holding `Space` temporarily pans with primary drag; release restores the
  current interaction without a persistent tool mode.
- Middle double click and `Home` frame the current flow. `F` frames selection.
  `Ctrl + 1` returns to 100% zoom around the visible canvas center.

### Selection and mutation

- Click selects one node. `Shift + click` adds; `Ctrl + click` removes.
- Empty primary drag creates a selection box. `Alt + drag` duplicates the
  dragged node with fresh node and pin IDs.
- Clicking a cable selects the connection. `Delete` removes selected nodes or
  the selected cable. `Ctrl + D` duplicates the selected nodes.
- `Ctrl + Z` undo; `Ctrl + Shift + Z` and `Ctrl + Y` redo.

### Connection and node creation

- Dragging from a port creates a live cable. Compatible ports receive a subtle
  outline; incompatible ports do not react.
- Dropping a cable on empty canvas opens the same searchable Add Node browser
  used by `Tab`, canvas double click, the Add Node button, and secondary click.
- Adding a node after an empty-canvas drop auto-connects the first compatible
  port when one exists. `Escape` or `Backspace` cancels the pending wire.
- The current graph model has no conversion node or segment model. Type
  conversion prompts and “remove last cable segment” remain explicitly out of
  scope until those domain types exist; the UI must not pretend otherwise.

### Scope honesty

Groups, comments that own nodes, variable Get/Set choice, asset/entity drops,
and execution pulses require domain data not currently present in `NodeGraph`.
They are not fabricated in this pass. The shell reserves no fake buttons for
them.

## Layout

At regular width the layout is:

```text
Flow rail | graph toolbar + canvas + minimap | Add Node / Inspector rail
```

- Flow rail: fixed 224px, search, flow list, new-flow command, and compact
  node library shortcuts.
- Graph center: a 42px retained toolbar, one authoritative canvas, and an
  overlay minimap in the lower right.
- Right rail: fixed 272px, searchable Add Node list above a read-only
  inspector. At narrow widths it collapses to Add Node only.
- Orange appears on the active flow, focus ring, primary Add Node command, and
  selected node outline. It is not used as an always-on fill.

## States

| State | Required response |
| --- | --- |
| empty flow | centered instruction naming `Tab`, double click, or secondary click |
| wire drag | live cable and compatible-port outline |
| pending add after wire | filtered browser plus automatic compatible connection on create |
| node selected | inspector shows name, category, description, and port count |
| cable selected | selected cable thickens using focus color; Delete is available |
| no selection | inspector states the next action without a modal |
| reduced/CPU path | same graph geometry and input semantics; no nonessential animation |

## Accessibility and keyboard

- All shell buttons have i18n labels, visible keyboard focus, and tooltips.
- Canvas shortcuts are active only while its region owns focus or pointer.
- Escape deterministically closes the browser or cancels an active wire.
- Color is never the only type signal: pins retain names and category labels.

## Acceptance Criteria

1. Shell panels use RafUI retained surfaces; the graph canvas remains
   renderer-owned and reads the real model.
2. Cursor-anchored zoom, middle/space pan, Home/F/Ctrl+1, multi-selection,
   box selection, Alt-duplicate, cable selection, and compatible-port wiring
   work against `NodeGraph`.
3. `Tab`, double click, secondary click, and a rail command open the same Add
   Node path and filtered list.
4. Every new static label exists in English and Spanish.
5. Node IDs and pin IDs remain unique after duplication; graph mutations enter
   the existing undo history once per completed gesture.

## Build Handoff

Target: Rust editor engineer working on RafUI and ApiGraphicBasic.

Implement exactly this spec against the locked design language. Do not replace
the graph with a generic retained card tree, do not add fake runtime behavior,
and do not redesign the existing NodeGraph domain model.
