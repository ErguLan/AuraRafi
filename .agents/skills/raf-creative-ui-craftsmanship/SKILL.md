---
name: raf-creative-ui-craftsmanship
description: >-
  The premier, industry-leading design craftsmanship skill for AuraRafi and RafUI.
  The definitive creative design playbook for crafting AAA desktop-grade instruments,
  living node graphs, fluid canvases, and tactile developer tools with boundless
  creative freedom and zero bureaucratic friction.
---

# Raf Creative UI Craftsmanship

> **Status**: The premier, industry-leading design craftsmanship skill for AuraRafi and RafUI.
> **Philosophy**: Creative Instruments, Not Bureaucratic Forms.
> **Creative Freedom**: Empowering rather than restrictive. This is not a list of rigid prohibitions; it is a design playbook and collection of high-leverage heuristics, mathematical formulas, and tactile ergonomics designed for state-of-the-art desktop creativity and fluid interaction.

---

## 1. Creative Mindset: The UI as a Living Instrument

When you build an interface in AuraRafi, you are not laying out an administrative web form or a generic database table. You are handcrafting a **precision instrument** for engineers, game directors, and hardware inventors.

A great creative instrument follows three core sensations:
1. **Calm Presence**: When untouched, it doesn't shout. It sits in harmonious, deep tones that leave room for the user's mind to focus on their creations.
2. **Tactile Feedback**: The moment the mouse moves, clicks, or drags, the interface wakes up. Hovering illuminates surfaces smoothly, dragging draws elastic curves, and approaching a connector magnetically invites interaction.
3. **Boundless Expressiveness**: You have full freedom to innovate. If a design benefits from an organic bubble, a neon oscilloscope wave, an expressive HUD pill, or a custom spline handle, create it. Strive for beauty, fluidity, and elegance.

---

## 2. The 7 Pillars of Tactile Craftsmanship

These pillars are design superpowers and intuition guides, giving you principles to elevate any screen:

### Pillar 1: Surface Fluidity & Continuous Depth
* **The Insight**: When every row in a tree, list, or palette is boxed inside a 1px border, the UI feels like a rigid spreadsheet. 
* **The Creative Pattern**: Let rows float seamlessly over the panel surface. In resting state, eliminate heavy enclosing perimeter borders.
* **Tactile Lighting**:
  * On hover, illuminate the entire row with a soft whisper of surface elevation (`surface_raised` at 15–25% alpha).
  * Use vertical hairline depth guides (`alpha: 24–32`) to communicate nested hierarchy gently without boxing elements in.
  * For active items, illuminate with a soft tint and a crisp 2px vertical edge indicator on the left edge (`hierarchy-selection-edge`).

### Pillar 2: Island Modularization (Rhythm in Toolbars)
* **The Insight**: A flat bar of 15 contiguous buttons overwhelms the eye. Grouping controls by purpose gives the brain instant spatial landmarks.
* **The Creative Pattern**: Cluster tools into small, coherent **islands** with a thin hairline separator (1x18px) between them:
  * *Domain Island*: Mode or context switches (e.g., Schematic vs. PCB, Node Graph vs. Timeline).
  * *Tool Island*: Active creation verbs (Select, Wire, Place, Cut).
  * *View Island*: Navigation helpers (Fit, Zoom, Grid toggle).
  * *Ops Island*: History and transformations (Undo, Redo, Rotate, Clear).
* **Button Feel**: Keep tool icons crisp and compact (26x26px), giving the workspace maximum canvas area.

### Pillar 3: Atmospheric Technical Canvases
* **The Insight**: A viewport or node graph should feel like an authentic drafting table or blueprint bench, not an empty black void.
* **The Creative Pattern**:
  * Base tone: Deep technical slate (`#10141c` / `[16, 20, 28, 255]`).
  * **Dual-Density Blueprint Grid**: Combine a fine 24px minor grid (stroke alpha ~26) with a rhythmic 96px major grid (stroke alpha ~68). This gives users subconscious spatial orientation and smooth zoom reference.
  * Framing: A subtle sheet boundary or corner brackets gives schematic and node graphs the pride of an engineering blueprint.

### Pillar 4: Kinetic Ergonomics & Magnetic Affordances
* **The Insight**: Static interfaces feel unresponsive. Physical tools in the real world have weight, elasticity, and tactile snap.
* **The Creative Pattern**:
  * **Elastic Cables**: When dragging from any pin or port, render an active cubic Bezier spline that dynamically bends and stretches to the mouse in real time.
  * **Magnetic Proximity Snap**: When the cursor enters a 16–24px radius of a compatible port, snap the line vertex to the pin origin and bloom a glowing halo ring (`nodes-pin-dot-compatible`).
  * **Empathetic Dimming**: Gently dim incompatible target pins (to ~0.30 opacity) while dragging, immediately guiding the user to valid connections without error popups.
  * **Midpoint Sever Knots**: Place a small knot icon directly at the midpoint ($t = 0.5$) of cables. One click severs the connection cleanly without hunting for invisible hitboxes.

### Pillar 5: Fluid Desktop Context Menus
* **The Insight**: Great desktop context menus appear instantly, feel lightweight, and dismiss effortlessly.
* **The Creative Pattern**:
  * Clean container with a gentle 1px border and 6px rounded corners (`radius: 6.0`).
  * Borderless item rows that illuminate edge-to-edge on hover.
  * Organize options by natural semantic sections with horizontal hairline dividers.
  * Align hotkeys (`Ctrl+D`, `Del`, `F2`) cleanly to the right in muted typography.
  * Destructive options (Delete, Sever) softly warm up with a crimson tint on hover.
  * Natural dismissal on click-outside or `Escape`.

### Pillar 6: High-Density Information Elegance
* **The Insight**: Complex projects have lots of data. High density does not mean cramped; it means high signal-to-noise ratio.
* **The Creative Pattern**:
  * Prefer compact 32–36px rows with clear visual hierarchy: bold primary label, muted sub-label or badge.
  * Collapsible accordions with live item count badges (e.g. `LOGIC (4)`, `MATH (5)`).
  * Expressive inspector headers: a colored category pill, bold entity name, unique short hash, and status badge.

### Pillar 7: Harmonic Color & Accent Energy
* **The Insight**: Bright accent colors are high-energy visual light. When everything is bright orange or cyan, nothing stands out.
* **The Creative Pattern**:
  * Rest in tranquil, balanced neutrals.
  * Use vivid colors for domain identity and semantic data types:
    * *Events & Triggers*: Warm Amber `[245, 158, 11, 255]`
    * *Actions & Commands*: Electric Cobalt `[59, 130, 246, 255]`
    * *Logic & Flow*: Violet `[168, 85, 247, 255]`
    * *Math & Numbers*: Emerald `[16, 185, 129, 255]`
    * *Hardware & Signals*: Radiant Orange `[249, 115, 22, 255]`
    * *Data & Variables*: Cyan `[6, 182, 212, 255]`

---

## 3. Visual Scripting & Node Architecture Playbook

When designing or extending visual scripting canvases in `crates/raf_editor/src/panels/nodes_surface.rs` and `nodes_canvas.rs`:

```
+---------------------------------------------------------------------------------------------------+
| PALETTE RAIL (260px)    | CANVAS WORKBENCH (Fluid)                         | INSPECTOR RAIL (300px) |
|-------------------------+--------------------------------------------------+------------------------|
| [FLOWS ACCORDION]       | [FLOATING HUD: Select(V) | Pan(H) | Fit(F)]      | [NODE IDENTITY BANNER] |
| > Main Event Graph      | [STATUS: 8 nodes | 6 wires]      [Validate]      | Category: EVENT (Amber)|
|   + New Flow            |--------------------------------------------------| Title: On Update       |
|-------------------------| [IN-FLIGHT NOTICE: Click target pin / Esc]       | ID: #9f3b12            |
| [CATEGORIZED LIBRARY]   |                                                  | State: [ READY ]       |
| Search: [ Ctrl+F ... ]  |       +-----------------------+                  |------------------------|
| v EVENTS (4)            |       | EVENT: On Update      |                  | [DESCRIPTION CARD]     |
|   * On Start            |       |-----------------------|                  | Emitted every frame    |
|   * On Update           |       | (out) Delta Time o---======> In-Flight   | with frame duration.   |
|   * Key Press           |       +-----------------------+      Bezier Wire |------------------------|
| > ACTIONS (4)           |                                      (Cyan tip)  | [PROPERTIES (1)]       |
| > LOGIC (4)             |               +-----------------------+          | Key: delta_multiplier  |
| > MATH (5)              |               | MATH: Add             |          | Val: [ 1.0          ]  |
| > ELECTRONICS (4)       |               |-----------------------|          |------------------------|
|-------------------------|               | o (in) A [Magnet Glow]|          | [PIN CONNECTIONS]      |
| [Undo] [Redo] [Validate]|               | o (in) B [Dimmed]     |          | (out) Delta Time ->    |
|                         |               +-----------------------+          |       Add::A [Linked]  |
|                         |                                                  |------------------------|
|                         |                       [ZOOM HUD: - | 100% | + ]  | [Sever Wires] [Delete] |
+---------------------------------------------------------------------------------------------------+
```

### Proportional Geometry Baseline
* `NODE_WIDTH`: `196.0` (delivers generous typography space without swallowing the canvas).
* `NODE_HEADER_HEIGHT`: `34.0` (accommodates category glyph, title, and action icons).
* `NODE_PIN_ROW_HEIGHT`: `24.0` (comfortable hit target for mouse interactions).
* `NODE_RADIUS`: `8.0` (refined rounded rectangle geometry).
* `SPLINE_SEGMENTS`: `18` to `20` cubic Bezier steps for organic, silky-smooth wires.

---

## 4. Creative Heuristics for Engineers & AI Collaborators

When designing new interfaces, panels, or controls in AuraRafi:

1. **Be Ambitious and Tactile**: Don't settle for static grey boxes. If something can have a pleasant hover glow, a dynamic preview, an elastic curve, or a magnetic snap, add it.
2. **Respect the User's Focus**: Avoid noisy borders, screaming fills, or garish popups. Let typography, subtle elevations, and purposeful accents do the storytelling.
3. **Keep Information Clear and Dense**: Use 24–36px compact heights, clear icons, and hierarchical typography (bold titles, muted descriptions).
4. **Iterate with Freedom**: You are free to invent new layouts, creative badges, HUD widgets, or custom canvas elements. Tailor your work to make the user feel empowered and inspired.
5. **Prioritize for New Interfaces & Features**: Whenever creating **new interfaces, new features, or novel workflows**, prioritize this Creative Craftsmanship skill to unlock expressive invention, kinetic feedback, and fluid interaction. When refining standard system chrome, docks, or global token alignment, leverage Studio Grade UI. Both frameworks carry authoritative weight; if there is stylistic ambiguity, consult the user to confirm their preferred direction.
6. **Zero Competitor Mentions**: Never reference competing tools, engines, or external companies in code, comments, or documentation.
7. **Complete i18n Localization**: Every UI string must be registered in both `crates/raf_core/locales/en.json` and `crates/raf_core/locales/es.json`.
8. **One-Pass Implementation & Minimal Verification**: Program the entire feature end-to-end in a single direct pass without hesitation ("sin miedo al exito"), running minimal targeted tests only at the absolute conclusion.
9. **Never Touch Unrelated Errors**: If you encounter errors or broken code outside your task scope, leave them untouched as others may be working in parallel.
10. **Strict Modularity & Anti-God-Code**: AuraRafi is an advanced, complex creative desktop suite—never treat it like a simple web landing page where shortcuts and hardcoding are tolerated. Avoid spaghetti code, magic constants, and mammoth functions. Decompose logic into clean, composable subroutines and feel fully empowered to create new dedicated modular files (helpers, sub-surfaces, hosts, data models) whenever beneficial for long-term scalability.
