# Technical Canvases & Toolbar Architecture

This reference outlines the technical canvas design, dual-grid mathematical formulas, and 4-island toolbar modularization.

---

## 1. Blueprint Canvas Atmosphere

A technical workspace (Nodes, Electronics, Schematics, PCB) requires an environment that feels like a precision instrument rather than a void.

### Palette Formula
* **Background Fill**: Technical slate `#10141c` (`[16, 20, 28, 255]`).
* **Minor Grid (Fine)**:
  * Step: `24.0 px`.
  * Stroke: 1px width, color: `tokens.border` with alpha 26 (`0.10` opacity).
  * Skip lines where `index % 4 == 0` to prevent overdraw with major grid lines.
* **Major Grid (Primary)**:
  * Step: `96.0 px` ($4 \times 24\text{px}$).
  * Stroke: 1px width, color: `tokens.border` with alpha 68 (`0.27` opacity).
* **Intersection Highlights**: For EDA workspaces, render 2x2px coordinate dots or subtle crosshairs at major grid intersections.

---

## 2. The 4-Island Modular Toolbar Pattern

A toolbar should never stretch as a raw line of 12+ adjacent buttons. Cluster into 4 distinct functional islands:

```
+---------------------------------------------------------------------------------------------------------+
| [ ISLAND 1: DOMAIN ] | [ ISLAND 2: TOOLS ]     | [ ISLAND 3: VIEW & GRID ] | [ ISLAND 4: HISTORY & OPS ]|
| [ Schematic | PCB ]  | [ Select | Wire | Place]| [ Fit | Zoom | Grid ]     | [ Rotate | Undo | Redo ]   |
+---------------------------------------------------------------------------------------------------------+
```

### Island 1: Domain / Mode (Segmented Control)
* Clearly informs the user what space they are authoring in.
* Highlighted with active surface elevation and crisp accent indicator.

### Island 2: Editing Tools
* Direct interactive verbs (`Select (V)`, `Wire (W)`, `Place (P)`).
* The active tool maintains a subtle pressed elevation (`tokens.surface_raised` with `tokens.accent` border).

### Island 3: Viewport & Grid Controls
* Navigation aids (`Fit (F)`, `Zoom - / +`, `Toggle Grid (G)`, `Toggle Labels`).
* Never mixes with editing tools.

### Island 4: History & Manipulations
* Reversible state operations (`Rotate (R)`, `Undo (Ctrl+Z)`, `Redo (Ctrl+Y)`).
* Destructive buttons (`Delete (Del)`) illuminate with crimson warning borders on hover.

---

## 3. EDA Magnetic Wire Snapping & Ghost Previews

1. **Snap Tolerance**: Define `PIN_SNAP_TOLERANCE_SCREEN = 16.0 px`.
2. **Snapping Feedback**:
   * As the wire endpoint approaches within 16px of a terminal pin, snap the preview line vertex to the exact pin coordinate.
   * Shift the in-flight wire color from neutral amber to electric cyan (`[0, 220, 255, 230]`).
   * Render a circular target reticle (`pin-snap-target`) directly over the pin.
3. **Ghost Preview for Component Placement**:
   * When dragging a component from the library, do not render an opaque grey box.
   * Render the semi-transparent schematic symbol outline with terminal pin dots highlighted in green, snapping to the active grid step.
