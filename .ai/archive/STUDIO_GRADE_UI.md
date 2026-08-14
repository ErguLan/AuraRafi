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

## 9. RafUI Implementation Patterns

### Structure

- Build a stable `UiDocument` from `UiNode`, `UiLayout`, semantic classes, and
  translation keys. Do not build persistent editor chrome from immediate-mode
  widget calls.
- Keep the visual tree declarative. The host carries focus, hover, scroll,
  text buffers, hit regions, menu placement, and typed action dispatch.
- Use one structural root, then explicit rails, command rows, canvas slots,
  inspectors, docks, and overlays. Do not nest empty cards merely to create
  spacing.

### Tabs, Buttons, And Inspector Sections

- Tabs are focusable buttons with a semantic active class. Use the active edge
  and focus token; do not enlarge the tab or recolor the whole workspace.
- Standard buttons have a fixed logical height, a stable icon/text layout,
  `4px` radius, and a semantic role. Only one primary action may use filled
  orange in a local command group.
- Inspector sections are grouped by domain data, not decorative cards.
  Structure the labels, controls, descriptions, and reset actions in a bounded
  grid that stacks on compact width.

### Menus

- A trigger emits `UiAction::OpenMenu`; the menu is `UiNodeKind::Menu` with an
  elevated z-index. Its host owns open state and keyboard focus.
- Close on Escape, outside primary/secondary click, accepted command, or a
  rebuild that removes the target. Clamp to surface bounds.
- Use a three-dot trigger only for a repeated item's local actions. Global
  commands belong in the shared command row.

### Transitional Egui Rule

Egui may bridge a remaining panel body while it migrates, but it must not gain
new permanent navigation, context-menu, canvas, docking, or renderer-ownership
logic. Move those responsibilities to RafUI/ApiGraphicBasic first.

## 10. Rules for Agents
If asked to modernize or clean the engine UI:

1. Preserve potato-mode performance and startup speed.
2. Keep user-facing strings translated through JSON keys.
3. Reduce noise before adding styling.

4. Make purposeful motion the default for perceived state changes: menu entry,
   selection, drag/reorder, docking, panel creation/removal, and layout changes
   should preserve spatial continuity with restrained shared RafUI tweens.
5. Keep animation state in hosts, use `UiTween` / `UiMotionSpec`, honor reduced
   motion, and reject decorative always-on effects. Review every animation with
   measured CPU/GPU frame time, allocations, texture/atlas work, and idle
   repaint cost before treating it as production-ready.
4. Prefer inline/contextual workflows over blocking modal flows.
5. Make panels, gizmos, and layout behavior match what the user visually expects.
6. Improve professional feel through structure, contrast hierarchy, spacing, and consistency, not through flashy decoration.
7. Read `docs/RAF_UI_AUTHORING.md` for every retained surface and
   `docs/APIGRAPHICBASIC.md` for every renderer/asset surface change.
8. Never fake CAD/scene content in a minimap, overlay, status metric, or
   preview. Derive it from the real document and active transform.

## 11. RafUI Frontier Quality Gate

The following are now core review gates, not optional panel polish:

1. Tooltips, menus, popovers, drag previews, and modals use a global RafUI
   overlay layer and never escape their owner surface by enlarging or abusing
   its clip rectangle.
2. Localized content uses `UiSizeMode::FitContent` or explicit min/max bounds;
   translated text is measured through the retained atlas before final paint.
3. Hover entry/exit is session state with monotonic time. Transitions use
   `UiTween`, are motivated, and stop requesting frames when settled.
4. Repeated visual language comes from `raf_ui::components`, not copied style
   literals in panel builders.
5. Logical layout and pointer coordinates remain stable across DPI. Physical
   target size and bounded raster scale come from `UiEnvironment`.
6. GPU and CPU hosts expose equivalent `UiSurfaceDiagnostics` so layout issues
   can be diagnosed without relying on a screenshot alone.

The design pre-flight remains locked to the technical/utilitarian RafUI
identity in `.ulpi/design/DESIGN.md`: active orange edge, restrained neutral
surfaces, no gradients, no glow, no decorative animation, and no fake product
data.

## 12. AuraRafi quality charter

The heart of AuraRafi UI is crispness, stability, hierarchy, truth, and
restraint. A visual defect that appears as sparkling points, crawling edges,
or changing icon silhouettes is classified as a rendering problem first:
pixel shimmer, subpixel jitter, temporal aliasing, texture bleeding, or
resampling blur. It is not solved by inflating controls or adding decoration.

Before approving a RafUI surface, compare static and interactive frames at
100%, 125%, 150%, and 200% DPI. Verify physical target allocation, pixel
snapping, sampling mode, atlas guard pixels, cache identity, and GPU/CPU parity.
