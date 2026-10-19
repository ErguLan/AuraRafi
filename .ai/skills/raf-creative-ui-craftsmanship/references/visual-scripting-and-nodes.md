# Visual Scripting & Node Graphs Reference

This reference details the card geometry, Bezier curve mathematics, and magnetic pin affordances for AuraRafi's Nodes system.

---

## 1. Card Geometry vs. The Circle Anti-Pattern

### Why Circular Nodes Fail
* Early prototypes attempted 136px circular orbs. Circles waste 60% of their screen area, cannot legibly display multi-line pin names, and force connector dots into awkward radial angles.

### AAA Card Geometry
* **Width**: `196.0 px` at 1.0 zoom (scales proportionally down to `0.55` zoom minimum).
* **Header**: `34.0 px` height.
  * Tinted category background (`[category_color, 44]`) with a 1px bottom divider line.
  * Category icon (`16x16 px`) in category accent color.
  * Bold node title (`11.5 px`).
  * Close/Delete button (`18x18 px`) with `UiIconId::Close` in `tokens.text_muted`.
* **Pin Rows**: `24.0 px` height per row.
  * Maximum of input or output pin counts defines card height.
  * Inputs on the left, Outputs on the right.
  * Pin connectors placed exactly on the card perimeter (-connector_size/2 on left, card_width - connector_size/2 on right).
* **Padding & Radius**: `radius: 8.0`, bottom padding `8.0 px`.

---

## 2. In-Flight Elastic Wires & Bezier Spline Math

When dragging from any pin, an active cubic Bezier spline must follow the pointer.

### Spline Tangent Formulation
Given source point $P_0 = (x_0, y_0)$ and target point $P_3 = (x_3, y_3)$:

$$\Delta x = \max(|x_3 - x_0| \times 0.5, 40.0 \times \text{zoom})$$

If dragging from an **Output** pin:
$$P_1 = (x_0 + \Delta x, y_0), \quad P_2 = (x_3 - \Delta x, y_3)$$

If dragging from an **Input** pin:
$$P_1 = (x_0 - \Delta x, y_0), \quad P_2 = (x_3 + \Delta x, y_3)$$

### Curve Evaluation
Sample the curve into 18–20 line segments using Bernstein polynomials:

$$B(t) = (1-t)^3 P_0 + 3(1-t)^2 t P_1 + 3(1-t) t^2 P_2 + t^3 P_3$$

Draw each segment as a high-density quad (`height = 2.5 - 3.0 px`).

---

## 3. Magnetic Affordance & Proximity Snapping

When a wire is being dragged or a source pin is active (`pending_pin.is_some()`):

1. **Self Pin**: The source pin glows with `tokens.accent_hot` and a white core border (`border_width: 2.5`).
2. **Compatible Target Pins**:
   * Evaluated via `raf_nodes::types_compatible(source_type, target_type) && source_kind != target_kind && source_node != target_node`.
   * Glows with a bright white fill, `tokens.accent_hot` border, and an outer halo (`nodes-pin-dot-compatible`).
   * Tooltip updates dynamically to `"Connect to <PinName> (<Type>)"`.
3. **Incompatible Pins**:
   * Opacity drops to `0.25 - 0.30`.
   * Label text dims to alpha 75.
   * Prevents user confusion by immediately communicating invalid targets.

---

## 4. Connection Severing: Midpoint Knot

Never force users to pixel-hunt an invisible 1px Bezier stroke to disconnect wires.
* At $t = 0.5$ along the evaluated curve, paint a small circular knot (`14.0 x 14.0 px`) with a subtle `Close` icon.
* On hover, the knot elevates and turns crimson/accent.
* Clicking fires `nodes.disconnect.<connection_id>`.

---

## 5. Library Accordions with Real-Time Counters

Organize the left palette into category accordions:
* Group: `Events`, `Actions`, `Logic`, `Math`, `Electronics`, `Variables`.
* Each category header displays:
  * Category glyph.
  * Localized category name.
  * Node count badge: `(4)`.
  * Chevron indicator (`ChevronDown` / `ChevronRight`).
* Search input (`nodes.search`) automatically filters and expands relevant accordions.
