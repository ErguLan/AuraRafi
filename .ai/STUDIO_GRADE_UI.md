# AuraRafi UI/UX Design Guidelines: Studio-Grade Aesthetic

**Target Audience:** AI Agents, Contributors, and UI Developers.  
**Objective:** Maintain a professional, industry-standard ("Studio-Grade") interface for the AuraRafi engine, heavily inspired by modern creative suites (Unreal Engine 5, Blender, Anvil, VS Code). 

## 1. Core Philosophy: The "Lean" Aesthetic
A professional engine UI rarely screams for attention. It exists to highlight the user's content (the canvas, the viewport, the node graph), not itself.
- **Subtlety over vibrancy:** Avoid large blocks of saturated colors. Highlight states should be subtle, relying on lines, glows, or muted background shifts rather than blinding primary colors.
- **Maximum Information Density (Lean):** Menus, hierarchies, and properties panels should pack information neatly. We use smaller fonts (around `size(11.0)` to `size(13.0)`) and condensed paddings.
- **No cringe/vibe-coding:** **ABSOLUTELY NO EMOJIS.** Interfaces should read like technical instruments. Use exact, descriptive technical words (e.g., "Hierarchy", "Compile", "Netlist") instead of playful metaphors.
- **English-Only by Default:** To maintain an industry-standard look, core engine layout and structure should be strictly in English. (i18n should rely on backend JSONs if ever needed, but do not clutter UI code with `if is_es { "Hola" } else { "Hello" }`).

## 2. Typography & Text Styling
- **Headers:** Keep them understated. Instead of giant text, use capitalization, slight spacing, and low contrast. Example: `egui::RichText::new("ENGINE SETTINGS").size(14.0).strong().color(gray)`. This approach mimics the "Extralight" modern industrial feel.
- **Muted Colors for Inactive Text:** Unselected items, descriptions, and labels should usually be a muted gray or dark gray, not fully white or black. Only the active elements get high contrast or the engine's Accent color.
- **Font Sizes:** Base fonts shouldn't exceed 14px outside of the main project hub titles. Toolbars and filters should use `11.0px` or `12.0px`.

## 3. UI Controls & Interactive Elements
- **Tabs (Tab Bars):** Avoid painting boxes around tabs. Professional software uses underlined text or subtle background shifts for active tabs. 
  - *Pattern:* `egui::Button::new("Tab").fill(TRANSPARENT)` with a manually drawn `egui::Stroke` line below the active tab.
- **Buttons:** 
  - Standard buttons should have a low-contrast background (`rgb(34, 34, 38)`) and thin borders, not standard generic bumps.
  - "Primary" Action buttons (like Create, Compile, Save) can use the `app_theme::ACCENT` (usually the industrial orange), but keep them small and properly bordered (`rounding(4.0)`). 
- **Toggles / Segmented Controls:** Use "Pill" designs. For filter bars ("All", "Errors", "Warnings"), use custom buttons with a dark grey background when active, instead of native `selectable_label` which defaults to a giant generic box.
- **Color Pickers/Presets:** Make them rounded rectangles with low-profile borders, not generic circles. Alignment is key.

## 4. Layout & Spacing
- **Separators:** Use `ui.separator()` paired with `ui.add_space(8.0)` to create clean structural divisions without clutter.
- **Constraint Content:** Large configurations or settings forms shouldn't stretch infinitely. Wrap them in a `ui.set_max_width(540.0)` or similar constraint inside a `CentralPanel`, surrounded by an elegant framed inner margin `ui.group()` or `egui::Frame` with subtle background and stroke.
- **Alignment:** Right-align final action buttons (Save, Cancel, Create) at the bottom of forms. Left-align labels. Values should line up.

## 5. Egui-Specific Refactoring Cheat Sheet
Instead of this:
```rust
if ui.selectable_label(active, "Big Tab").clicked() { ... }
```
Do this:
```rust
let text_color = if active { app_theme::ACCENT } else { MUTED_GRAY };
let btn = egui::Button::new(egui::RichText::new("Clean Tab").color(text_color)).frame(false);
let response = ui.add(btn);
if active {
    // Draw underline
    ui.painter().line_segment([left_bottom, right_bottom], Stroke::new(2.0, app_theme::ACCENT));
}
```

Instead of this:
```rust
ui.heading("Settings");
ui.button("Close");
```
Do this:
```rust
ui.label(egui::RichText::new("SETTINGS").size(14.0).color(MUTED_GRAY).strong());
// Add framing and spacing...
ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
    ui.add_sized([90.0, 30.0], egui::Button::new("Close").rounding(4.0));
});
```

## Summary for Agents
If asked to "modernize", "clean up", or "make it look studio-grade": 
1. Strip all Spanish/i18n boilerplate. 
2. Remove emojis. 
3. Drop font sizes and switch generic default buttons to subtle, muted custom frames. 
4. Shrink and right-align action buttons. 
5. Add elegant visual hierarchy using `RichText` color grading rather than size.
