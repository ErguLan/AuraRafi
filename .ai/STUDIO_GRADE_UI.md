# AuraRafi UI/UX Design Guidelines: Industrial Modernism

**Target Audience:** AI Agents, Contributors, and UI Developers.  
**Objective:** Keep AuraRafi visually professional, technically dense, modern, and lightweight. The engine should feel like a serious creation tool, not a toy, and never like a bloated main-process app that eats the whole machine.

## 1. Core Philosophy: Canvas First, UI Second
AuraRafi is a tool for building scenes, logic, electronics, and future runtime systems. The UI exists to support the work surface, not compete with it.

- **Canvas-first priority:** The viewport, schematic surface, and node graph are the stars. Panels, chrome, headers, and controls must stay visually subordinate.
- **Industrial modernism over decoration:** The interface should feel engineered, quiet, and intentional. Think precision instrument, not marketing website.
- **Potato-first discipline:** UI decisions must respect low-end hardware. No gratuitous blur layers, no giant nested panels, no expensive visual effects by default.
- **High density, low noise:** Keep information dense but readable. Prefer smaller typography, compact paddings, and stable alignment over oversized controls.
- **No cringe styling:** No emojis, no playful copy, no exaggerated gradients, no fake futuristic nonsense.

## 2. Language, i18n, and Text Rules
- **English is the source language for code and structure.** Panel names, comments, helper names, and internal UI concepts should be authored in English.
- **Do not strip i18n from the engine.** AuraRafi already uses JSON localization. New visible strings must go through translation keys, not hardcoded branching.
- **Never use inline language forks in UI code** like `if is_es { ... } else { ... }`. Use backend JSONs and `t("key", lang)`.
- **Text must be exact and technical:** Use words like `Hierarchy`, `Project Settings`, `Render Preset`, `Physics`, `Node Graph`, `Netlist`, `Visibility`.

## 3. Visual Character
- **Muted by default:** Most text, borders, inactive tabs, and helper content should sit in greys or low-contrast values.
- **Accent is scarce:** Orange or any accent color is for active selection, primary actions, live status, or focus points. Never flood the screen with it.
- **Flat depth, subtle separation:** Prefer soft borders, faint fills, and restrained framing. The goal is structure without weight.
- **White-space with intent:** Use spacing to separate semantic blocks, not to make the tool feel empty.

## 4. Typography Rules
- **Headers:** Use uppercase or small-cap feel with muted contrast. Avoid giant titles. Most section headers should live around `11.0` to `14.0`.
- **Body text:** Primary operational text usually lives around `11.0` to `12.0`.
- **Descriptions and metadata:** Use smaller and dimmer text. Metadata should never visually overpower editable values.
- **Contrast ladder:** Title > active value > normal value > secondary label > helper note.

## 5. Interaction Rules
- **Inline over modal when possible:** Rename in hierarchy should happen inline, not in disruptive center-screen dialogs, unless the action is truly destructive or multi-step.
- **Contextual actions near the target:** Per-item menus belong next to the item, not detached at the bottom of the panel or in unrelated screen zones.
- **Click-away should work:** Menus, transient controls, and popups must close naturally when the user clicks outside.
- **Stable drag behavior:** Resize, move, rotate, scale, and panel interactions must feel deterministic. No snapping back unless that behavior is explicit.
- **Shift should modify behavior, not replace baseline usability:** Default interactions should already feel correct. Modifiers are enhancements.

## 6. Layout Rules
- **Panels must earn their space:** If a panel is optional, allow it to collapse or hide. The main work area should expand to consume freed space.
- **Bottom workbench behavior:** Fixed panels like Console, Assets, AI Chat, Node Editor, and Project Settings should resize predictably and allow snap states when helpful.
- **Constraint oversized forms:** Long settings or inspector content should sit inside framed blocks with bounded width or clear structure.
- **Use separators as structure, not decoration:** `ui.separator()` plus measured spacing is enough most of the time.

## 7. Panel-Specific Rules

### Hierarchy
- Support true tree reading at a glance.
- Per-row actions should be local to the row.
- Grouping/folder semantics must be explicit, not fake cosmetics.
- Empty/group nodes should read as organizational objects, not broken geometry.

### Properties / Inspector
- Must feel like an inspector, not a random form dump.
- Group related fields into framed sections.
- Surface metadata that actually helps: parent, children, scripts, primitive type, visibility.
- Offer quick-reset actions where they reduce friction.

### Viewport / Scene Surface
- Grid must feel effectively infinite and stable around the camera target.
- Gizmos must match their visual affordance. If the user sees rotation rings, hit-testing must operate on rings, not arrows.
- Performance-heavy rendering remains opt-in.

### Project Settings
- Project settings are not engine-global settings.
- Per-project runtime/layout choices belong to the project model and should persist with the project.
- Examples: panel visibility, runtime preset, physics/audio flags, complements enablement, default scene.

## 8. Performance-Aware UI Rules
- **No heavy eye-candy by default.** Blur, bloom-like UI treatment, animated gradients, and oversized shadows are banned unless explicitly justified.
- **Avoid unnecessary allocations in hot UI paths.** Reuse state where practical.
- **Do not create giant always-on panels** that keep expensive content live when hidden.
- **Startup matters:** The first editor experience should load quickly and avoid front-loading non-essential systems.

## 9. Egui Implementation Patterns

### Tabs
Preferred pattern:
```rust
let text_color = if active { app_theme::ACCENT } else { MUTED_GRAY };
let btn = egui::Button::new(
  egui::RichText::new("Tab").color(text_color).size(13.0)
)
.fill(egui::Color32::TRANSPARENT)
.frame(false);
let response = ui.add(btn);
if active {
  ui.painter().line_segment([left_bottom, right_bottom], Stroke::new(2.0, app_theme::ACCENT));
}
```

### Standard Buttons
Preferred pattern:
```rust
let btn = egui::Button::new(
  egui::RichText::new("Refresh").size(11.0).color(MUTED_TEXT)
)
.fill(egui::Color32::from_rgb(34, 34, 38))
.stroke(egui::Stroke::new(1.0, BORDER))
.rounding(4.0);
```

### Inspector Cards
Preferred pattern:
```rust
egui::Frame::none()
  .fill(PANEL_FILL)
  .stroke(egui::Stroke::new(1.0, BORDER))
  .rounding(8.0)
  .inner_margin(12.0)
  .show(ui, |ui| {
    ui.label(egui::RichText::new("TRANSFORM").size(12.0).strong().color(MUTED_HEADER));
    ui.add_space(8.0);
    // Controls...
  });
```

## 10. Rules for Agents
If asked to modernize or clean the engine UI:

1. Preserve potato-mode performance and startup speed.
2. Keep user-facing strings translated through JSON keys.
3. Reduce noise before adding styling.
4. Prefer inline/contextual workflows over blocking modal flows.
5. Make panels, gizmos, and layout behavior match what the user visually expects.
6. Improve professional feel through structure, contrast hierarchy, spacing, and consistency, not through flashy decoration.
