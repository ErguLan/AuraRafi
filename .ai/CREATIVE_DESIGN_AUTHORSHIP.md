# AuraRafi Creative Design Authorship & Instrument Craftsmanship

> **Status**: The definitive, living creative design authorship for AuraRafi and RafUI.
> **Philosophy**: Creative Instruments, Not Bureaucratic Forms. Total creative freedom, zero arbitrary limitations.
> **Associated Skill**: `.agents/skills/raf-creative-ui-craftsmanship/` (and `.ai/skills/raf-creative-ui-craftsmanship/`).

---

## 1. Dual Authority: Studio Foundation and Creative Instrument Craftsmanship

AuraRafi visual design is guided by two complementary, authoritative frameworks:

* **Studio Grade UI Foundation** (`.ai/STUDIO_GRADE_UI.md`): Establishes the global design system, token contracts, shell structure, accessibility, and restrained baseline consistency across all products.
* **Creative Instrument Authorship** (This document & `raf-creative-ui-craftsmanship`): Unlocks the tactile, desktop-grade craftsmanship layer—fluid ergonomics, elastic wire physics, magnetic snapping, modular toolbars, and expressive creative freedom.

Both frameworks carry full authority. While Studio Grade UI ensures architectural discipline, this authorship ensures that AuraRafi feels like a **living, pro-grade creative instrument** with rich responsiveness and tactile delight rather than a rigid administrative app.

### When to Use Which / Decision Rule:
* **Evaluate Context**: Use Studio Grade UI when implementing default system chrome, global settings dialogs, or enforcing foundational token palettes. Use Creative Instrument Craftsmanship when authoring **new interfaces**, creating **new features**, or designing interactive canvases, node editors, CAD drafting benches, and complex hierarchical navigators to unlock creative innovation and tactile ergonomics.
* **Ask the User When Uncertain**: If a task allows for either a restrained baseline treatment or an expressive kinetic design, the AI should thoughtfully evaluate the user's intent or ask the user which aesthetic direction they prefer.

---

## 2. Core Creative Values: Designing What Feels Alive

When designing an interface in RafUI, ask yourself: *"Does this feel like a fine physical instrument?"*

### A. Calm While Resting, Expressive While Active
* In resting state, let the interface breathe. Use tranquil, cohesive dark tones (`tokens.surface`, `tokens.surface_alt`, technical slate `#10141c`).
* The UI should not shout when you are not touching it.
* When you interact, it responds instantly: smooth hover illumination, crisp edge indicators, elastic spline bends, and inviting magnetic snap halos.

### B. High Information Density Without Visual Clutter
* High-grade creative instruments achieve high density through clean typography and spatial rhythm, not chunky cards.
* Standardize on compact 24–26px tool heights and 32–36px library rows.
* Pair bold primary titles (11.5–12px) with muted secondary subtitles or category tags (10px).
* Use count badges on accordions (e.g. `EVENTS (4)`, `LOGIC (4)`) so users know what is inside before expanding.

### C. Tactile Kinetic Affordance
* A digital wire should feel like an elastic physical cable. Dragging from a pin evaluates a cubic Bezier curve that stretches organically to the cursor.
* When approaching a valid connection target within 16–24px, the wire snaps magnetically to the pin and blooms a glowing halo.
* Incompatible pins gracefully dim, immediately guiding the user to valid connections without annoying modal error popups.
* A midpoint sever knot on wires gives an immediate, clear target to click when severing links.

---

## 3. High-Leverage UI Recipes in RafUI

These recipes are versatile patterns you can adapt, customize, and build upon.

### Recipe 1: Continuous Surface Rows (The Anti-Spreadsheet Pattern)
Instead of boxing every list item in a perimeter border:

```rust
// Resting state: borderless, floats smoothly over container
let row_style = UiStyle {
    fill: if is_selected {
        palette.tokens().surface_raised
    } else {
        [0, 0, 0, 0] // Transparent resting
    },
    border: [0, 0, 0, 0], // No spreadsheet borders
    border_width: 0.0,
    text: palette.tokens().text,
    radius: 4.0,
    opacity: 1.0,
};

// Selection indicator: crisp 2px vertical edge bar on the left
let left_edge_bar = UiNode::new("selection-edge", UiNodeKind::Panel)
    .with_layout(UiLayout::fixed(2.0, 24.0))
    .with_style(UiStyle {
        fill: palette.tokens().accent,
        border: [0, 0, 0, 0],
        border_width: 0.0,
        radius: 1.0,
        ..Default::default()
    });
```

### Recipe 2: The 4-Island Modular Toolbar
Instead of a flat, unstructured line of 15 buttons:

```
[ Domain: Schematic | PCB ]  |  [ Tools: Select | Wire | Place ]  |  [ View: Fit | Grid ]  |  [ Ops: Undo | Redo ]
```

* Group related tools into islands.
* Place 1px hairline dividers (`1.0 x 18.0 px` with `border` fill at alpha 0.4) between islands.
* Tool buttons: `26.0 x 26.0 px` with centered icons and hotkey tooltips.

### Recipe 3: Atmospheric Technical Canvas (Dual-Density Grid)
For schematic and visual scripting surfaces:
* Base slate fill: `#10141c` (`[16, 20, 28, 255]`).
* Minor grid: 24.0 px step, 1px width, alpha 26 (0.10 opacity).
* Major grid: 96.0 px step ($4 \times 24\text{px}$), 1px width, alpha 68 (0.27 opacity).
* Framed drafting sheet border to give the canvas the pride of an engineering blueprint.

### Recipe 4: Elastic Bezier Splines for Cables
Calculate tangent offsets dynamically:

```rust
let dx = ((target_x - source_x).abs() * 0.5).max(40.0 * zoom);
let p1 = (source_x + dx, source_y);
let p2 = (target_x - dx, target_y);

// Sample 18-20 segments using cubic Bernstein polynomials:
// B(t) = (1-t)^3 * P0 + 3*(1-t)^2 * t * P1 + 3*(1-t) * t^2 * P2 + t^3 * P3
```

### Recipe 5: Desktop-Grade Floating Context Menus
* Enclosing box: `tokens.surface` with 1px `tokens.border` and `radius: 6.0`. (No aggressive neon borders).
* Rows: borderless, full-width hover illumination with `tokens.surface_raised`.
* Grouping: 1px horizontal hairline dividers separating semantic categories.
* Hotkeys: Flush-right labels in `tokens.text_muted` (e.g. `Ctrl+D`, `Del`).
* Destructive actions: Illuminate with soft crimson on hover (`[180, 45, 45, 80]`).
* Dismissal: Closes naturally when clicking outside or pressing `Escape` (no redundant "Cancel" buttons).

---

## 4. Semantic Color Energy Budget

Color is meaning. Use bright accents with purpose:
* **Events & Triggers**: Warm Amber `[245, 158, 11, 255]`
* **Actions & Commands**: Electric Cobalt `[59, 130, 246, 255]`
* **Logic & Flow**: Vibrant Violet `[168, 85, 247, 255]`
* **Math & Numbers**: Emerald Green `[16, 185, 129, 255]`
* **Electronics & Signals**: Radiant Orange `[249, 115, 22, 255]`
* **Data & Variables**: High-Tech Cyan `[6, 182, 212, 255]`
* **Danger & Deletion**: Crimson `[239, 68, 68, 255]`

---

## 5. Creative Freedom & Empowerment Principle

As a developer or AI agent working on AuraRafi:
* **Never feel restricted by outdated or bureaucratic guidelines.**
* You are encouraged to propose and build innovative UI widgets: floating HUDs, zoom pills, mini-maps, waveform previews, radial menus where appropriate, interactive sliders, and animated transitions.
* If a visual problem asks for a creative solution, embrace it with taste, craft, and pride.
* Build tools that feel satisfying to use every single day.
