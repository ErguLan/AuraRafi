# Desktop Interaction Laws for RafUI

This reference codifies the desktop ergonomics and design standards required across all AuraRafi panels.

---

## 1. Context Menus: The Native Desktop Standard

Amateur web interfaces treat context menus as stacked button columns with heavy borders. Professional desktop environments follow these ergonomic standards:

### Container Geometry & Styling
* **No Outer Accent Border**: The container must use `border: tokens.border` with `border_width: 1.0` and `radius: 6.0`. Never wrap a menu in `tokens.accent` or warm orange.
* **Surface Elevation**: Use `tokens.surface` or `tokens.surface_raised` with subtle drop shadow if supported.
* **Dismissal**: Do not include a "Cancel" item. Menus dismiss when the user clicks outside or presses `Escape`.

### Menu Row Hierarchy
* **Full-Width Hover**: Each item is borderless in resting state. On hover, the row illuminates completely with `surface_raised` (`radius: 3.0`).
* **Semantic Grouping via Dividers**: Separate categories (Edit, View/Nav, Structure, Danger) with 1px horizontal dividers:
  ```rust
  fn menu_separator(id: &str, palette: StudioUiPalette) -> UiNode {
      UiNode::new(id, UiNodeKind::Panel)
          .with_layout(UiLayout::fixed(0.0, 1.0).with_width_mode(UiSizeMode::Fill))
          .with_style(UiStyle {
              fill: palette.tokens().border,
              border: [0, 0, 0, 0],
              text: palette.tokens().text,
              border_width: 0.0,
              radius: 0.0,
              opacity: 0.6,
          })
  }
  ```
* **Flush-Right Keyboard Shortcuts**: Place shortcuts (e.g. `Ctrl+D`, `Del`, `F2`) in a separate label right-aligned with `tokens.text_muted`. Never concatenate shortcuts inside the primary action label string.
* **Destructive Actions**: Delete rows must illuminate with a soft crimson background on hover (`[180, 45, 45, 80]`), providing instant danger affordance without visual aggression in resting state.

---

## 2. Tree Hierarchies & Nested Lists

### The Spreadsheet Anti-Pattern
Placing `border: tokens.border` and `border_width: 1.0` on every tree row creates closed rectangular boxes at every indentation level. This makes the hierarchy look like an unformatted HTML table or an Excel sheet.

### The Correct Tree Implementation
1. **Resting Rows**: `border_width: 0.0` and transparent fill. The tree floats smoothly over the panel background.
2. **Depth Guides**: Draw vertical hairline guides at each nesting level with low opacity (`alpha: 26 - 32`). Guides connect children to parents without boxing them in.
3. **Hover**: Illuminate the entire row row-width with `tokens.surface_raised` at low opacity.
4. **Active Selection**:
   * Subtle surface tint across the row.
   * A 2px high-contrast vertical accent bar (`hierarchy-selection-edge`) on the far-left edge of the selected row.
5. **Stateful Icon Visibility**:
   * Eye (visibility) and Lock icons should rest in `tokens.text_muted` or reveal on hover.
   * If an entity is hidden or locked, its icon remains prominently lit so the user can diagnose state at a glance.

---

## 3. Segmented Controls and Tab Bars

* **No Chunky Framed Tabs**: Tabs must not look like thick orange boxes.
* **Pill Track Architecture**:
  * Track background: `tokens.surface_alt` with `radius: 4.0`.
  * Active segment: Elevated pill with `tokens.surface_raised`, crisp 1px subtle border, and bold typography.
  * Inactive segments: Transparent, muted text, smooth hover elevation.
