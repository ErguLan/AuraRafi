use std::collections::HashMap;

use super::{
    UiAccessibilityRole, UiAlign, UiCompactMode, UiControl, UiFlow, UiHitRegion, UiJustify,
    UiLayout, UiNode, UiNodeKind, UiPositionMode, UiRect, UiSizeMode, UiStyle, UiStyleSheet,
    UiSurfaceMaterial, UiTextAtlasRequest, UiTextEditState, UiTextOverflow, UiTextStyle,
    UiVisualState,
};

/// Retained interaction/layout output. Visual geometry is compiled separately
/// into `UiSurfaceDrawList`, which is the sole paint payload for both CPU and
/// GPU UI compositors.
#[derive(Debug, Clone, PartialEq)]
pub struct UiSurfaceFrame {
    pub layout_boxes: Vec<UiLayoutBox>,
    pub hit_regions: Vec<UiHitRegion>,
    pub text_requests: Vec<UiTextAtlasRequest>,
    pub focus_order: Vec<String>,
    /// Window-space anchor retained for the global tooltip overlay. Keeping
    /// this separate from the trigger layout means fitting localized text
    /// cannot move the tooltip back on top of the icon.
    pub tooltip_anchor: Option<UiRect>,
    pub scroll_metrics: Vec<UiScrollMetrics>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct UiScrollMetrics {
    pub id: String,
    pub viewport_size: [f32; 2],
    pub content_size: [f32; 2],
    pub max_offset: [f32; 2],
}

#[derive(Debug, Clone, PartialEq)]
pub struct UiLayoutBox {
    pub id: String,
    pub kind: UiNodeKind,
    pub text_key: Option<String>,
    pub text_value: Option<String>,
    pub tooltip_key: Option<String>,
    pub tooltip_value: Option<String>,
    pub rect: UiRect,
    /// The node's paintable content box after layout padding. Presentation
    /// must use this for icons, text and carets instead of recreating padding
    /// with renderer-specific magic offsets.
    pub content_rect: UiRect,
    pub clip_rect: UiRect,
    pub interactive: bool,
    pub text_selectable: bool,
    pub focusable: bool,
    pub disabled: bool,
    pub invalid: bool,
    pub accessibility_label_key: Option<String>,
    pub accessibility_role: super::UiAccessibilityRole,
    pub accessibility_description_key: Option<String>,
    pub accessibility_expanded: Option<bool>,
    pub accessibility_checked: Option<bool>,
    pub accessibility_selected: Option<bool>,
    pub z_index: i16,
    pub width_mode: UiSizeMode,
    pub height_mode: UiSizeMode,
    pub style: UiStyle,
    pub material: UiSurfaceMaterial,
    pub text_style: Option<UiTextStyle>,
    pub text_overflow: UiTextOverflow,
    pub icon: Option<raf_ui::UiIcon>,
    pub control: UiControl,
    /// Snapshot of the focused text editor state used by presentation to draw
    /// a caret without coupling the draw list to the mutable control store.
    pub text_edit: Option<UiTextEditState>,
    pub ime_preedit: Option<String>,
}

pub(super) fn build_surface_frame(
    root: &UiNode,
    style_sheet: &UiStyleSheet,
    visual_state: UiVisualState<'_>,
    control_state: &super::UiControlState,
    width: u32,
    height: u32,
    _clear_color: [u8; 4],
    text_scale: f32,
) -> UiSurfaceFrame {
    build_surface_frame_with_intrinsic_sizes(
        root,
        style_sheet,
        visual_state,
        control_state,
        width,
        height,
        _clear_color,
        text_scale,
        &HashMap::new(),
    )
}

pub(super) fn build_surface_frame_with_intrinsic_sizes(
    root: &UiNode,
    style_sheet: &UiStyleSheet,
    visual_state: UiVisualState<'_>,
    control_state: &super::UiControlState,
    width: u32,
    height: u32,
    _clear_color: [u8; 4],
    text_scale: f32,
    intrinsic_sizes: &HashMap<String, [f32; 2]>,
) -> UiSurfaceFrame {
    let root_rect = UiRect::new(0.0, 0.0, width as f32, height as f32);
    let mut boxes = Vec::new();
    let mut hit_regions = Vec::new();
    let mut text_requests = Vec::new();
    let mut focus_order = Vec::new();
    let mut scroll_metrics = Vec::new();
    record_node(
        root,
        style_sheet,
        visual_state,
        control_state,
        root_rect,
        root_rect,
        0.0,
        0,
        &mut boxes,
        &mut hit_regions,
        &mut text_requests,
        &mut focus_order,
        &mut scroll_metrics,
        text_scale,
        intrinsic_sizes,
    );

    UiSurfaceFrame {
        layout_boxes: boxes,
        hit_regions,
        text_requests,
        focus_order,
        tooltip_anchor: None,
        scroll_metrics,
    }
}

fn effective_accessibility_role(node: &UiNode) -> UiAccessibilityRole {
    if node.accessibility_role != UiAccessibilityRole::Generic {
        return node.accessibility_role;
    }
    match &node.control {
        UiControl::TextInput(_) => UiAccessibilityRole::Textbox,
        UiControl::Toggle(_) => UiAccessibilityRole::Checkbox,
        UiControl::Range(_) => UiAccessibilityRole::Slider,
        UiControl::ColorPicker(_) => UiAccessibilityRole::Slider,
        UiControl::Select(_) => UiAccessibilityRole::Combobox,
        UiControl::None if node.kind == UiNodeKind::Button => UiAccessibilityRole::Button,
        UiControl::None if node.kind == UiNodeKind::Menu => UiAccessibilityRole::Menu,
        _ => UiAccessibilityRole::Generic,
    }
}

fn record_node(
    node: &UiNode,
    style_sheet: &UiStyleSheet,
    visual_state: UiVisualState<'_>,
    control_state: &super::UiControlState,
    assigned_rect: UiRect,
    parent_clip: UiRect,
    z: f32,
    parent_z_index: i16,
    boxes: &mut Vec<UiLayoutBox>,
    hit_regions: &mut Vec<UiHitRegion>,
    text_requests: &mut Vec<UiTextAtlasRequest>,
    focus_order: &mut Vec<String>,
    scroll_metrics: &mut Vec<UiScrollMetrics>,
    text_scale: f32,
    intrinsic_sizes: &HashMap<String, [f32; 2]>,
) {
    let resolved_layout = node.layout.resolved_for(assigned_rect.width);
    let has_text =
        node.text_key.is_some() || node.text_value.is_some() || node.control.text_input().is_some();
    let layout = if has_text
        && matches!(
            node.kind,
            UiNodeKind::Button
                | UiNodeKind::FloatingPanel
                | UiNodeKind::Label
                | UiNodeKind::Menu
                | UiNodeKind::Panel
                | UiNodeKind::TextInput
                | UiNodeKind::Toolbar
                | UiNodeKind::Tooltip
        ) {
        resolved_layout.with_text_safe_area(matches!(
            node.kind,
            UiNodeKind::Button | UiNodeKind::TextInput
        ))
    } else {
        resolved_layout
    };
    let rect = match layout.rect {
        Some(explicit_rect) if layout.position_mode == UiPositionMode::Absolute => constrain_rect(
            &layout,
            UiRect::new(
                assigned_rect.x + explicit_rect.x,
                assigned_rect.y + explicit_rect.y,
                explicit_rect.width,
                explicit_rect.height,
            ),
        ),
        Some(explicit_rect) => constrain_rect(&layout, explicit_rect),
        // Flexible flow tracks may be compressed by their parent. Fixed and
        // intrinsic tracks are kept at the size used to resolve their own
        // children, so a compressed parent cannot make a later sibling paint
        // over them.
        None => constrain_assigned_rect(&layout, assigned_rect, node, intrinsic_sizes),
    };
    let z_index = parent_z_index.saturating_add(layout.z_index);
    let style = style_sheet.resolve_with_state(node, visual_state);
    let accessibility_role = effective_accessibility_role(node);
    let accessibility_checked = node
        .accessibility_checked
        .or_else(|| node.control.toggle().map(|toggle| toggle.value));
    let accessibility_expanded = node
        .accessibility_expanded
        .or_else(|| node.control.select().map(|select| select.open));
    let accessibility_selected = node.accessibility_selected;
    let content_rect = rect.shrink(layout.padding);
    // Menus, popovers and floating panels are window-level visual layers even
    // when their declarative owner lives inside a scroll view. Let the popup
    // clip to its own bounds so an open dropdown is not cut off by an
    // inspector/list ancestor. The compositor still clamps to the target.
    let breaks_ancestor_clip = matches!(
        node.kind,
        UiNodeKind::Overlay | UiNodeKind::Menu | UiNodeKind::Tooltip | UiNodeKind::FloatingPanel
    );
    let node_clip = if breaks_ancestor_clip {
        rect
    } else {
        parent_clip
    };
    let text_edit = if visual_state.focused_id == Some(node.id.as_str()) {
        node.control
            .text_input()
            .map(|input| control_state.text_edit(input.value_key.as_str()))
            .or_else(|| {
                node.text_selectable.then(|| {
                    control_state.selectable_text_edit(
                        &node.id,
                        node.text_value.as_deref().unwrap_or_default(),
                    )
                })
            })
    } else {
        None
    };
    let ime_preedit = node.control.text_input().and_then(|input| {
        (visual_state.focused_id == Some(node.id.as_str()))
            .then(|| {
                control_state
                    .ime_preedit(input.value_key.as_str())
                    .to_string()
            })
            .filter(|value| !value.is_empty())
    });
    boxes.push(UiLayoutBox {
        id: node.id.clone(),
        kind: node.kind,
        text_key: node.text_key.clone(),
        text_value: node.text_value.clone(),
        tooltip_key: node.tooltip_key.clone(),
        tooltip_value: node.tooltip_value.clone(),
        rect,
        content_rect,
        clip_rect: node_clip,
        interactive: node.interactive,
        text_selectable: node.text_selectable,
        focusable: node.focusable,
        disabled: node.disabled,
        invalid: node.invalid,
        accessibility_label_key: node.accessibility_label_key.clone(),
        accessibility_role,
        accessibility_description_key: node.accessibility_description_key.clone(),
        accessibility_expanded,
        accessibility_checked,
        accessibility_selected,
        z_index,
        width_mode: layout.width_mode,
        height_mode: layout.height_mode,
        style: style.clone(),
        material: node.material,
        text_style: node.text_style,
        text_overflow: node.text_overflow,
        icon: node.icon,
        control: node.control.clone(),
        text_edit,
        ime_preedit,
    });
    if node.focusable && !node.disabled {
        focus_order.push(node.id.clone());
    }
    hit_regions.push(UiHitRegion {
        id: node.id.clone(),
        kind: node.kind,
        rect,
        clip_rect: node_clip,
        z_index,
        interactive: node.interactive,
        focusable: node.focusable,
        disabled: node.disabled,
    });
    let text_key = node
        .text_key
        .as_deref()
        .or_else(|| node.text_value.as_deref())
        .or_else(|| {
            node.control
                .text_input()
                .map(|input| input.value_key.as_str())
        });
    if let Some(text_key) = text_key {
        let mut text_style = node
            .text_style
            .unwrap_or_else(|| UiTextStyle::body(style.text))
            .scaled_for_ui(text_scale);
        if text_style.inherit_color {
            text_style.color = style.text;
        }
        text_style.color = apply_opacity(text_style.color, style.opacity);
        if let Some(input) = node.control.text_input() {
            if control_state.text(&input.value_key).is_empty()
                && control_state.ime_preedit(&input.value_key).is_empty()
                && input.placeholder_key.is_some()
            {
                text_style.color[3] = text_style.color[3].min(156);
            }
        }
        let icon_inset = node
            .icon
            .map(|icon| 6.0 + f32::from(icon.size.logical_pixels()) + 6.0)
            .unwrap_or(0.0);
        let available_text_width = content_rect.width - icon_inset;
        // A fit/max-content width must be measured from the natural line
        // before its final track exists. Measuring against the provisional
        // fallback track creates a feedback loop: the atlas wraps the label,
        // the wrapped slot becomes the intrinsic width, and the row remains
        // narrow forever. An authored max width still acts as the wrapping
        // boundary for intentionally constrained copy.
        let intrinsic_width = matches!(
            layout.width_mode,
            UiSizeMode::FitContent | UiSizeMode::MaxContent
        );
        let authored_text_max =
            layout.max_size[0] - layout.padding.left - layout.padding.right - icon_inset;
        let text_max_width = if intrinsic_width {
            if authored_text_max > 1.0 {
                authored_text_max
            } else {
                1024.0
            }
        } else if available_text_width > 1.0 {
            available_text_width
        } else {
            256.0
        };
        let request = UiTextAtlasRequest::new(
            node.id.clone(),
            text_key.to_string(),
            text_style,
            text_max_width,
        )
        .with_overflow(node.text_overflow)
        .with_single_line(
            node.control
                .text_input()
                .is_some_and(|input| !input.multiline)
                || matches!(node.kind, UiNodeKind::Button | UiNodeKind::Toolbar),
        );
        text_requests.push(request);
    }

    let content = content_rect;
    let child_clip = if breaks_ancestor_clip {
        rect
    } else if layout.overflow.clips_children() || node.control.scroll_axis().is_some() {
        parent_clip.intersection(content)
    } else {
        parent_clip
    };
    let mut child_content = content;
    if let Some(axis) = node.control.scroll_axis() {
        let offset = control_state.scroll_offset(&node.id);
        if axis.scrolls_horizontally() {
            child_content.x -= offset[0];
        }
        if axis.scrolls_vertically() {
            child_content.y -= offset[1];
        }
    }
    let flow = resolve_compact_flow(node, &layout, child_content, intrinsic_sizes);
    let mut flow_content = child_content;
    if let Some(axis) = node.control.scroll_axis() {
        if axis.scrolls_vertically() && flow == UiFlow::Column {
            flow_content.height = flow_content.height.max(intrinsic_flow_size_with_map(
                node,
                &layout,
                false,
                Some(intrinsic_sizes),
            ));
        }
        if axis.scrolls_horizontally() && flow == UiFlow::Row {
            flow_content.width = flow_content.width.max(intrinsic_flow_size_with_map(
                node,
                &layout,
                true,
                Some(intrinsic_sizes),
            ));
        }
        let measured_max_offset = [
            (flow_content.width - content.width).max(0.0),
            (flow_content.height - content.height).max(0.0),
        ];
        // A text-fit surface is laid out once provisionally before its atlas
        // measurements are available. Reuse the last authoritative extent in
        // that pass so a wheel tick or thumb pixel cannot clamp itself back to
        // the tiny provisional range. The intrinsic pass supplies the fresh
        // value and is allowed to shrink it when content really changed.
        let max_offset = if intrinsic_sizes.is_empty() {
            let previous = control_state.scroll_max_offset(&node.id);
            [
                measured_max_offset[0].max(previous[0]),
                measured_max_offset[1].max(previous[1]),
            ]
        } else {
            measured_max_offset
        };
        scroll_metrics.push(UiScrollMetrics {
            id: node.id.clone(),
            viewport_size: [content.width.max(0.0), content.height.max(0.0)],
            content_size: [
                flow_content.width.max(content.width),
                flow_content.height.max(content.height),
            ],
            max_offset,
        });
        append_scrollbar(
            &node.id,
            content,
            parent_clip,
            max_offset,
            control_state.scroll_offset(&node.id),
            z_index,
            visual_state,
            boxes,
            hit_regions,
        );
    }
    match flow {
        UiFlow::None => {
            for child in &node.children {
                let child_rect = explicit_child_container(child, child_content);
                record_node(
                    child,
                    style_sheet,
                    visual_state,
                    control_state,
                    child_rect,
                    child_clip,
                    z - 0.02,
                    z_index,
                    boxes,
                    hit_regions,
                    text_requests,
                    focus_order,
                    scroll_metrics,
                    text_scale,
                    intrinsic_sizes,
                );
            }
        }
        UiFlow::Row => record_flow_children(
            node,
            &layout,
            style_sheet,
            visual_state,
            control_state,
            flow_content,
            child_clip,
            true,
            z,
            z_index,
            boxes,
            hit_regions,
            text_requests,
            focus_order,
            scroll_metrics,
            text_scale,
            intrinsic_sizes,
        ),
        UiFlow::Column => record_flow_children(
            node,
            &layout,
            style_sheet,
            visual_state,
            control_state,
            flow_content,
            child_clip,
            false,
            z,
            z_index,
            boxes,
            hit_regions,
            text_requests,
            focus_order,
            scroll_metrics,
            text_scale,
            intrinsic_sizes,
        ),
        UiFlow::RowWrap => record_wrapped_children(
            node,
            &layout,
            style_sheet,
            visual_state,
            control_state,
            flow_content,
            child_clip,
            z,
            z_index,
            boxes,
            hit_regions,
            text_requests,
            focus_order,
            scroll_metrics,
            text_scale,
            intrinsic_sizes,
        ),
        UiFlow::Grid => record_grid_children(
            node,
            &layout,
            style_sheet,
            visual_state,
            control_state,
            flow_content,
            child_clip,
            z,
            z_index,
            boxes,
            hit_regions,
            text_requests,
            focus_order,
            scroll_metrics,
            text_scale,
            intrinsic_sizes,
        ),
    }
}

/// Flow children receive their computed rectangle. Absolute children instead
/// receive their containing content rectangle so `record_node` can resolve the
/// authored offset relative to that parent.
fn explicit_child_container(child: &UiNode, parent_content: UiRect) -> UiRect {
    match (child.layout.position_mode, child.layout.rect) {
        (UiPositionMode::Absolute, Some(_)) => parent_content,
        (_, Some(rect)) => rect,
        (_, None) => parent_content,
    }
}

const SCROLLBAR_HIT_WIDTH: f32 = 18.0;
const SCROLLBAR_VISUAL_WIDTH: f32 = 7.0;
const SCROLLBAR_EDGE_INSET: f32 = 4.0;
const SCROLLBAR_MIN_THUMB: f32 = 28.0;
const GENERATED_SCROLLBAR_PREFIX: &str = "__rafui.scrollbar.";

/// Adds the shared retained scrollbar affordance for vertical scroll views.
/// It is generated from measured scroll metrics rather than authored by each
/// panel, so the wheel, thumb drag and visual extent stay in one RafUI model.
fn append_scrollbar(
    scroll_id: &str,
    viewport: UiRect,
    parent_clip: UiRect,
    max_offset: [f32; 2],
    offset: [f32; 2],
    z_index: i16,
    visual_state: UiVisualState<'_>,
    boxes: &mut Vec<UiLayoutBox>,
    hit_regions: &mut Vec<UiHitRegion>,
) {
    if max_offset[1] <= f32::EPSILON || viewport.width <= 0.0 || viewport.height <= 0.0 {
        return;
    }
    let track_height = (viewport.height - SCROLLBAR_EDGE_INSET * 2.0).max(1.0);
    let hit_width = SCROLLBAR_HIT_WIDTH.min(viewport.width).max(1.0);
    let visual_width = SCROLLBAR_VISUAL_WIDTH.min(hit_width).max(1.0);
    let hit_x = (viewport.right() - hit_width).max(viewport.x);
    let visual_x = (viewport.right() - visual_width - SCROLLBAR_EDGE_INSET)
        .max(viewport.x)
        .min(hit_x + hit_width - visual_width);
    let track_hit = UiRect::new(
        hit_x,
        viewport.y + SCROLLBAR_EDGE_INSET,
        hit_width,
        track_height.min(viewport.height),
    );
    let track_visual = UiRect::new(visual_x, track_hit.y, visual_width, track_hit.height);
    let content_height = viewport.height + max_offset[1];
    let thumb_height = (track_hit.height * viewport.height / content_height)
        .clamp(SCROLLBAR_MIN_THUMB.min(track_hit.height), track_hit.height);
    let thumb_travel = (track_hit.height - thumb_height).max(0.0);
    let thumb_y = track_hit.y
        + thumb_travel * (offset[1].max(0.0) / max_offset[1].max(f32::EPSILON)).clamp(0.0, 1.0);
    let thumb_hit = UiRect::new(hit_x, thumb_y, hit_width, thumb_height);
    let thumb_visual = UiRect::new(visual_x, thumb_y, visual_width, thumb_height);
    let clip = parent_clip.intersection(viewport);
    let track_id = format!("{GENERATED_SCROLLBAR_PREFIX}{scroll_id}.track");
    let thumb_id = format!("{GENERATED_SCROLLBAR_PREFIX}{scroll_id}.thumb");
    let hovered = visual_state.hovered_id == Some(track_id.as_str())
        || visual_state.hovered_id == Some(thumb_id.as_str());
    let active = visual_state.active_id == Some(track_id.as_str())
        || visual_state.active_id == Some(thumb_id.as_str());
    let thumb_color = if active || hovered {
        [232, 133, 28, 255]
    } else {
        [102, 112, 126, 235]
    };
    push_scrollbar_box(
        boxes,
        hit_regions,
        track_id,
        track_visual,
        track_hit,
        clip,
        UiStyle {
            fill: if active || hovered {
                [51, 60, 72, 150]
            } else {
                [0, 0, 0, 0]
            },
            border: [0, 0, 0, 0],
            text: [0, 0, 0, 0],
            border_width: 0.0,
            radius: track_visual.width * 0.5,
            opacity: 1.0,
        },
        z_index.saturating_add(4),
    );
    push_scrollbar_box(
        boxes,
        hit_regions,
        thumb_id,
        thumb_visual,
        thumb_hit,
        clip,
        UiStyle {
            fill: thumb_color,
            border: if active || hovered {
                [255, 190, 92, 255]
            } else {
                [145, 154, 168, 220]
            },
            text: [0, 0, 0, 0],
            border_width: 1.0,
            radius: thumb_visual.width * 0.5,
            opacity: 1.0,
        },
        z_index.saturating_add(5),
    );
}

fn push_scrollbar_box(
    boxes: &mut Vec<UiLayoutBox>,
    hit_regions: &mut Vec<UiHitRegion>,
    id: String,
    visual_rect: UiRect,
    hit_rect: UiRect,
    clip_rect: UiRect,
    style: UiStyle,
    z_index: i16,
) {
    boxes.push(UiLayoutBox {
        id: id.clone(),
        kind: UiNodeKind::Panel,
        text_key: None,
        text_value: None,
        tooltip_key: None,
        tooltip_value: None,
        rect: visual_rect,
        content_rect: visual_rect,
        clip_rect,
        interactive: true,
        text_selectable: false,
        focusable: false,
        disabled: false,
        invalid: false,
        accessibility_label_key: None,
        accessibility_role: super::UiAccessibilityRole::Generic,
        accessibility_description_key: None,
        accessibility_expanded: None,
        accessibility_checked: None,
        accessibility_selected: None,
        z_index,
        width_mode: UiSizeMode::Fixed,
        height_mode: UiSizeMode::Fixed,
        style,
        material: UiSurfaceMaterial::Opaque,
        text_style: None,
        text_overflow: UiTextOverflow::Clip,
        icon: None,
        control: UiControl::None,
        text_edit: None,
        ime_preedit: None,
    });
    hit_regions.push(UiHitRegion {
        id,
        kind: UiNodeKind::Panel,
        rect: hit_rect,
        clip_rect,
        z_index,
        interactive: true,
        focusable: false,
        disabled: false,
    });
}

fn record_flow_children(
    node: &UiNode,
    parent_layout: &UiLayout,
    style_sheet: &UiStyleSheet,
    visual_state: UiVisualState<'_>,
    control_state: &super::UiControlState,
    content: UiRect,
    child_clip: UiRect,
    row: bool,
    z: f32,
    parent_z_index: i16,
    boxes: &mut Vec<UiLayoutBox>,
    hit_regions: &mut Vec<UiHitRegion>,
    text_requests: &mut Vec<UiTextAtlasRequest>,
    focus_order: &mut Vec<String>,
    scroll_metrics: &mut Vec<UiScrollMetrics>,
    text_scale: f32,
    intrinsic_sizes: &HashMap<String, [f32; 2]>,
) {
    let flow_children = node
        .children
        .iter()
        .filter(|child| child.layout.rect.is_none())
        .map(|node| FlowChild {
            layout: node.layout.resolved_for(content.width),
            intrinsic: intrinsic_sizes.get(&node.id).copied(),
            node,
        })
        .collect::<Vec<_>>();
    let available_main = if row { content.width } else { content.height };
    let gap_count = flow_children.len().saturating_sub(1);
    let base_gap = parent_layout
        .gap
        .max(0.0)
        .min(available_main / gap_count.max(1) as f32);
    let item_space = (available_main - base_gap * gap_count as f32).max(0.0);
    let main_sizes = resolve_flow_main_sizes(&flow_children, row, item_space, intrinsic_sizes);
    let occupied = main_sizes.iter().sum::<f32>() + base_gap * gap_count as f32;
    let (offset, gap) = resolve_justify(
        parent_layout.justify_content,
        available_main,
        occupied,
        base_gap,
        flow_children.len(),
    );
    let mut cursor = if row { content.x } else { content.y } + offset;
    let mut flow_index = 0;

    for child in &node.children {
        let child_rect = if child.layout.rect.is_some() {
            explicit_child_container(child, content)
        } else {
            let main = main_sizes[flow_index];
            let child_layout = &flow_children[flow_index].layout;
            flow_index += 1;
            let rect = aligned_flow_rect(
                cursor,
                main,
                content,
                row,
                parent_layout.align_items,
                child_layout,
            );
            cursor += main + gap;
            rect
        };
        record_node(
            child,
            style_sheet,
            visual_state,
            control_state,
            child_rect,
            child_clip,
            z - 0.02,
            parent_z_index,
            boxes,
            hit_regions,
            text_requests,
            focus_order,
            scroll_metrics,
            text_scale,
            intrinsic_sizes,
        );
    }
}

fn record_wrapped_children(
    node: &UiNode,
    parent_layout: &UiLayout,
    style_sheet: &UiStyleSheet,
    visual_state: UiVisualState<'_>,
    control_state: &super::UiControlState,
    content: UiRect,
    child_clip: UiRect,
    z: f32,
    parent_z_index: i16,
    boxes: &mut Vec<UiLayoutBox>,
    hit_regions: &mut Vec<UiHitRegion>,
    text_requests: &mut Vec<UiTextAtlasRequest>,
    focus_order: &mut Vec<String>,
    scroll_metrics: &mut Vec<UiScrollMetrics>,
    text_scale: f32,
    intrinsic_sizes: &HashMap<String, [f32; 2]>,
) {
    let gap = parent_layout.gap.max(0.0);
    let available = content.width;
    let mut rows = Vec::<Vec<FlowChild<'_>>>::new();
    let mut row_widths = Vec::<f32>::new();
    for child in node
        .children
        .iter()
        .filter(|child| child.layout.rect.is_none())
    {
        let layout = child.layout.resolved_for(content.width);
        let main = wrap_main_size(&layout, available, intrinsic_sizes.get(&child.id).copied());
        if rows.is_empty() {
            rows.push(Vec::new());
            row_widths.push(0.0);
        }
        let index = rows.len() - 1;
        let current_width = row_widths[index];
        let with_gap = if current_width > 0.0 { gap } else { 0.0 };
        if current_width > 0.0 && current_width + with_gap + main > available {
            rows.push(Vec::new());
            row_widths.push(0.0);
        }
        let index = rows.len() - 1;
        let current_width = row_widths[index];
        row_widths[index] = current_width + if current_width > 0.0 { gap } else { 0.0 } + main;
        rows[index].push(FlowChild {
            node: child,
            layout,
            intrinsic: intrinsic_sizes.get(&child.id).copied(),
        });
    }

    let mut layout_by_id = std::collections::HashMap::<&str, UiRect>::new();
    let mut y = content.y;
    for (row, occupied) in rows.iter().zip(row_widths.iter().copied()) {
        if y >= content.bottom() {
            break;
        }
        let row_height = row
            .iter()
            .map(|child| requested_cross_size(&child.layout, true, content.height))
            .fold(0.0_f32, f32::max)
            .max(1.0)
            .min((content.bottom() - y).max(0.0));
        let (offset, actual_gap) = resolve_justify(
            parent_layout.justify_content,
            available,
            occupied,
            gap,
            row.len(),
        );
        let mut x = content.x + offset;
        for child in row {
            let main = wrap_main_size(&child.layout, available, child.intrinsic);
            let rect = aligned_wrap_rect(
                x,
                y,
                main,
                row_height,
                parent_layout.align_items,
                &child.layout,
            );
            layout_by_id.insert(child.node.id.as_str(), rect);
            x += main + actual_gap;
        }
        y += row_height + gap;
        if y >= content.bottom() {
            break;
        }
    }

    for child in &node.children {
        let child_rect = if child.layout.rect.is_some() {
            explicit_child_container(child, content)
        } else {
            layout_by_id
                .get(child.id.as_str())
                .copied()
                .unwrap_or_else(|| UiRect::new(content.x, content.bottom(), 0.0, 0.0))
        };
        record_node(
            child,
            style_sheet,
            visual_state,
            control_state,
            child_rect,
            child_clip,
            z - 0.02,
            parent_z_index,
            boxes,
            hit_regions,
            text_requests,
            focus_order,
            scroll_metrics,
            text_scale,
            intrinsic_sizes,
        );
    }
}

fn record_grid_children(
    node: &UiNode,
    parent_layout: &UiLayout,
    style_sheet: &UiStyleSheet,
    visual_state: UiVisualState<'_>,
    control_state: &super::UiControlState,
    content: UiRect,
    child_clip: UiRect,
    z: f32,
    parent_z_index: i16,
    boxes: &mut Vec<UiLayoutBox>,
    hit_regions: &mut Vec<UiHitRegion>,
    text_requests: &mut Vec<UiTextAtlasRequest>,
    focus_order: &mut Vec<String>,
    scroll_metrics: &mut Vec<UiScrollMetrics>,
    text_scale: f32,
    intrinsic_sizes: &HashMap<String, [f32; 2]>,
) {
    let gap = parent_layout.gap.max(0.0);
    let min_width = parent_layout.grid.min_column_width.max(1.0);
    let automatic_columns = ((content.width + gap) / (min_width + gap)).floor() as u16;
    let columns = if parent_layout.grid.columns > 0 {
        parent_layout.grid.columns
    } else {
        automatic_columns.max(1)
    } as usize;
    let cell_width =
        ((content.width - gap * columns.saturating_sub(1) as f32) / columns as f32).max(0.0);
    let flow_children = node
        .children
        .iter()
        .filter(|child| child.layout.rect.is_none())
        .collect::<Vec<_>>();
    let row_height = parent_layout
        .grid
        .row_height
        .max(
            flow_children
                .iter()
                .map(|child| child.layout.basis[1].max(child.layout.min_size[1]))
                .fold(0.0_f32, f32::max),
        )
        .max(32.0);
    let mut grid_rects = std::collections::HashMap::<&str, UiRect>::new();
    for (index, child) in flow_children.iter().enumerate() {
        let column = index % columns;
        let row = index / columns;
        grid_rects.insert(
            child.id.as_str(),
            UiRect::new(
                content.x + column as f32 * (cell_width + gap),
                content.y + row as f32 * (row_height + gap),
                cell_width,
                row_height,
            ),
        );
    }
    for child in &node.children {
        let child_rect = if child.layout.rect.is_some() {
            explicit_child_container(child, content)
        } else {
            grid_rects
                .get(child.id.as_str())
                .copied()
                .unwrap_or_else(|| UiRect::new(content.x, content.bottom(), 0.0, 0.0))
        };
        record_node(
            child,
            style_sheet,
            visual_state,
            control_state,
            child_rect,
            child_clip,
            z - 0.02,
            parent_z_index,
            boxes,
            hit_regions,
            text_requests,
            focus_order,
            scroll_metrics,
            text_scale,
            intrinsic_sizes,
        );
    }
}

#[derive(Clone)]
struct FlowChild<'a> {
    node: &'a UiNode,
    layout: UiLayout,
    intrinsic: Option<[f32; 2]>,
}

fn resolve_compact_flow(
    node: &UiNode,
    layout: &UiLayout,
    content: UiRect,
    intrinsic_sizes: &HashMap<String, [f32; 2]>,
) -> UiFlow {
    if layout.flow != UiFlow::Row || layout.compact == UiCompactMode::None {
        return layout.flow;
    }
    let child_count = node
        .children
        .iter()
        .filter(|child| child.layout.rect.is_none())
        .count();
    let requested = node
        .children
        .iter()
        .filter(|child| child.layout.rect.is_none())
        .map(|child| {
            let child_layout = child.layout.resolved_for(content.width);
            intrinsic_sizes
                .get(&child.id)
                .map(|size| size[0])
                .unwrap_or_else(|| child_layout.basis[0].max(child_layout.min_size[0]))
        })
        .sum::<f32>();
    if requested + layout.gap.max(0.0) * child_count.saturating_sub(1) as f32 <= content.width {
        return layout.flow;
    }
    match layout.compact {
        UiCompactMode::None => UiFlow::Row,
        UiCompactMode::Wrap => UiFlow::RowWrap,
        UiCompactMode::Stack => UiFlow::Column,
        UiCompactMode::Auto => {
            let can_wrap = node
                .children
                .iter()
                .filter(|child| child.layout.rect.is_none())
                .all(|child| {
                    let child_layout = child.layout.resolved_for(content.width);
                    intrinsic_sizes
                        .get(&child.id)
                        .map(|size| size[0])
                        .unwrap_or_else(|| child_layout.min_size[0].max(child_layout.basis[0]))
                        <= content.width
                });
            if can_wrap {
                UiFlow::RowWrap
            } else {
                UiFlow::Column
            }
        }
    }
}

fn resolve_justify(
    justify: UiJustify,
    available: f32,
    occupied: f32,
    base_gap: f32,
    count: usize,
) -> (f32, f32) {
    let free = (available - occupied).max(0.0);
    match justify {
        UiJustify::Start => (0.0, base_gap),
        UiJustify::Center => (free * 0.5, base_gap),
        UiJustify::End => (free, base_gap),
        UiJustify::SpaceBetween if count > 1 => (0.0, base_gap + free / (count - 1) as f32),
        UiJustify::SpaceAround if count > 0 => {
            let spacing = free / count as f32;
            (spacing * 0.5, base_gap + spacing)
        }
        UiJustify::SpaceEvenly if count > 0 => {
            let spacing = free / (count + 1) as f32;
            (spacing, base_gap + spacing)
        }
        _ => (0.0, base_gap),
    }
}

fn aligned_flow_rect(
    cursor: f32,
    main: f32,
    content: UiRect,
    row: bool,
    parent_align: UiAlign,
    child_layout: &UiLayout,
) -> UiRect {
    let available_cross = if row { content.height } else { content.width };
    let cross = requested_cross_size(child_layout, row, available_cross);
    let alignment = child_layout.align_self.unwrap_or(parent_align);
    let actual_cross =
        if alignment == UiAlign::Stretch && child_layout.basis[if row { 1 } else { 0 }] <= 0.0 {
            available_cross
        } else {
            cross
        };
    let offset = align_offset(alignment, available_cross, actual_cross);
    if row {
        UiRect::new(cursor, content.y + offset, main, actual_cross)
    } else {
        UiRect::new(content.x + offset, cursor, actual_cross, main)
    }
}

fn aligned_wrap_rect(
    x: f32,
    y: f32,
    width: f32,
    row_height: f32,
    parent_align: UiAlign,
    child_layout: &UiLayout,
) -> UiRect {
    let requested = requested_cross_size(child_layout, true, row_height);
    let alignment = child_layout.align_self.unwrap_or(parent_align);
    let height = if alignment == UiAlign::Stretch && child_layout.basis[1] <= 0.0 {
        row_height
    } else {
        requested.min(row_height)
    };
    UiRect::new(
        x,
        y + align_offset(alignment, row_height, height),
        width,
        height,
    )
}

fn requested_cross_size(layout: &UiLayout, row: bool, available: f32) -> f32 {
    let axis = if row { 1 } else { 0 };
    let requested = if layout.basis[axis] > 0.0 {
        layout.basis[axis]
    } else {
        available
    };
    constrain_dimension(
        requested.min(available),
        layout.min_size[axis],
        layout.max_size[axis],
    )
}

fn align_offset(align: UiAlign, available: f32, size: f32) -> f32 {
    match align {
        UiAlign::Start | UiAlign::Stretch => 0.0,
        UiAlign::Center => ((available - size) * 0.5).max(0.0),
        UiAlign::End => (available - size).max(0.0),
    }
}

fn wrap_main_size(layout: &UiLayout, available: f32, intrinsic: Option<[f32; 2]>) -> f32 {
    let requested = intrinsic
        .map(|size| size[0])
        .unwrap_or_else(|| layout.basis[0].max(layout.min_size[0]));
    let fallback = (available * 0.5).max(1.0);
    constrain_dimension(
        if requested > 0.0 { requested } else { fallback },
        0.0,
        layout.max_size[0],
    )
    .min(available)
}

/// Resolves one row or column track without allowing sibling rectangles to
/// overlap. Flexible tracks compress when the host is smaller than the
/// requested content; fixed and intrinsic tracks remain rigid because their
/// descendants resolve against that authored size.
fn resolve_flow_main_sizes(
    children: &[FlowChild<'_>],
    row: bool,
    available: f32,
    intrinsic_sizes: &HashMap<String, [f32; 2]>,
) -> Vec<f32> {
    if children.is_empty() {
        return Vec::new();
    }
    let axis = if row { 0 } else { 1 };
    let requested = children
        .iter()
        .map(|child| {
            flow_main_requirement_with_map(
                child.node,
                &child.layout,
                row,
                child.intrinsic,
                Some(intrinsic_sizes),
            )
        })
        .collect::<Vec<_>>();
    let requested_total = requested.iter().sum::<f32>();
    if requested_total > available {
        if requested_total <= f32::EPSILON {
            return vec![available / children.len() as f32; children.len()];
        }
        let rigid = children
            .iter()
            .enumerate()
            .map(|(index, child)| {
                flow_track_is_rigid(&child.layout, axis).then_some(requested[index])
            })
            .collect::<Vec<_>>();
        let rigid_total = rigid.iter().flatten().sum::<f32>();
        if rigid_total > available {
            // The content itself is taller/wider than the host. Let the
            // parent overflow/scroll it instead of returning rectangles that
            // are smaller than the dimensions their children will resolve to.
            return requested;
        }
        let flexible_total = requested_total - rigid_total;
        if flexible_total <= f32::EPSILON {
            return requested;
        }
        let flexible_scale = (available - rigid_total).max(0.0) / flexible_total;
        return requested
            .iter()
            .enumerate()
            .map(|(index, size)| {
                if rigid[index].is_some() {
                    *size
                } else {
                    *size * flexible_scale
                }
            })
            .collect();
    }

    let mut sizes = requested;
    let mut remaining = available - requested_total;
    while remaining > f32::EPSILON {
        let eligible = children
            .iter()
            .enumerate()
            .filter_map(|(index, child)| {
                let maximum = child.layout.max_size[axis];
                let can_grow = child.layout.grow > 0.0
                    && (maximum <= 0.0 || sizes[index] < maximum.max(child.layout.min_size[axis]));
                can_grow.then_some((index, child.layout.grow))
            })
            .collect::<Vec<_>>();
        let total_grow = eligible.iter().map(|(_, grow)| *grow).sum::<f32>();
        if total_grow <= f32::EPSILON {
            break;
        }

        let mut consumed = 0.0;
        for (index, grow) in eligible {
            let maximum = children[index].layout.max_size[axis];
            let capacity = if maximum > 0.0 {
                (maximum.max(children[index].layout.min_size[axis]) - sizes[index]).max(0.0)
            } else {
                remaining
            };
            let increment = (remaining * (grow / total_grow)).min(capacity);
            sizes[index] += increment;
            consumed += increment;
        }
        if consumed <= f32::EPSILON {
            break;
        }
        remaining -= consumed;
    }
    sizes
}

fn flow_track_is_rigid(layout: &UiLayout, axis: usize) -> bool {
    let mode = if axis == 0 {
        layout.width_mode
    } else {
        layout.height_mode
    };
    match mode {
        UiSizeMode::Fixed => layout.basis[axis] > f32::EPSILON,
        UiSizeMode::FitContent | UiSizeMode::MinContent | UiSizeMode::MaxContent => true,
        UiSizeMode::Auto | UiSizeMode::Fill => false,
    }
}

/// Calculates the main-axis size a flow child needs before its parent applies
/// free-space growth. This gives scroll views a real content extent instead
/// of collapsing containers that only have flow children.
fn flow_main_requirement_with_map(
    node: &UiNode,
    layout: &UiLayout,
    row: bool,
    intrinsic: Option<[f32; 2]>,
    intrinsic_sizes: Option<&HashMap<String, [f32; 2]>>,
) -> f32 {
    flow_dimension_requirement_with_map(node, layout, row, intrinsic, intrinsic_sizes, true)
}

fn flow_cross_requirement_with_map(
    node: &UiNode,
    layout: &UiLayout,
    row: bool,
    intrinsic: Option<[f32; 2]>,
    intrinsic_sizes: Option<&HashMap<String, [f32; 2]>>,
) -> f32 {
    flow_dimension_requirement_with_map(node, layout, row, intrinsic, intrinsic_sizes, false)
}

fn flow_dimension_requirement_with_map(
    node: &UiNode,
    layout: &UiLayout,
    row: bool,
    intrinsic: Option<[f32; 2]>,
    intrinsic_sizes: Option<&HashMap<String, [f32; 2]>>,
    collapse_growing_main_axis: bool,
) -> f32 {
    let axis = if row { 0 } else { 1 };
    let explicit = layout.basis[axis].max(layout.min_size[axis]).max(0.0);
    if explicit > f32::EPSILON || (collapse_growing_main_axis && layout.grow > 0.0) {
        return explicit;
    }

    if let Some(intrinsic) = intrinsic.map(|size| size[if row { 0 } else { 1 }]) {
        return intrinsic.max(0.0);
    }
    let child_intrinsic = intrinsic_flow_size_with_map(node, layout, row, intrinsic_sizes);
    if child_intrinsic > f32::EPSILON {
        return child_intrinsic;
    }

    // Text leaves do not have children from which an intrinsic flow size can
    // be derived. Reserve a real track for them so a column does not paint
    // every label at the same y coordinate, and a row does not give a label a
    // zero-width wrapping box that rasterizes one character per line.
    if node.text_key.is_some() || node.text_value.is_some() || node.control.text_input().is_some() {
        return if row {
            let horizontal_padding = layout.padding.left + layout.padding.right;
            let icon_inset =
                if node.icon.is_some() && (node.text_key.is_some() || node.text_value.is_some()) {
                    6.0 + f32::from(node.icon.expect("icon checked").size.logical_pixels()) + 6.0
                } else {
                    0.0
                };
            (64.0 + horizontal_padding + icon_inset).max(layout.min_size[axis])
        } else {
            node.text_style
                .map(|style| style.line_height_px.max(18.0))
                .unwrap_or(18.0)
        };
    }

    0.0
}

fn intrinsic_flow_size_with_map(
    node: &UiNode,
    layout: &UiLayout,
    row: bool,
    intrinsic_sizes: Option<&HashMap<String, [f32; 2]>>,
) -> f32 {
    let main_axis = matches!(
        (layout.flow, row),
        (UiFlow::Row, true) | (UiFlow::Column, false)
    );
    let cross_axis = matches!(
        (layout.flow, row),
        (UiFlow::Row, false) | (UiFlow::Column, true)
    );
    if !main_axis && !cross_axis {
        return 0.0;
    }

    let children = node
        .children
        .iter()
        .filter(|child| child.layout.rect.is_none())
        .collect::<Vec<_>>();
    if children.is_empty() {
        return 0.0;
    }

    let padding = if row {
        layout.padding.left + layout.padding.right
    } else {
        layout.padding.top + layout.padding.bottom
    };
    let child_sizes = children.iter().map(|child| {
        let intrinsic = intrinsic_sizes.and_then(|sizes| sizes.get(&child.id).copied());
        if main_axis {
            flow_main_requirement_with_map(child, &child.layout, row, intrinsic, intrinsic_sizes)
        } else {
            flow_cross_requirement_with_map(child, &child.layout, row, intrinsic, intrinsic_sizes)
        }
    });
    let children_size = if main_axis {
        child_sizes.sum::<f32>() + layout.gap.max(0.0) * children.len().saturating_sub(1) as f32
    } else {
        child_sizes.fold(0.0, f32::max)
    };
    children_size + padding
}

fn constrain_rect(layout: &super::UiLayout, rect: UiRect) -> UiRect {
    UiRect::new(
        rect.x,
        rect.y,
        constrain_dimension(rect.width, layout.min_size[0], layout.max_size[0]),
        constrain_dimension(rect.height, layout.min_size[1], layout.max_size[1]),
    )
}

fn constrain_assigned_rect(
    layout: &super::UiLayout,
    rect: UiRect,
    node: &UiNode,
    intrinsic_sizes: &HashMap<String, [f32; 2]>,
) -> UiRect {
    UiRect::new(
        rect.x,
        rect.y,
        resolve_intrinsic_dimension(layout, node, rect.width, 0, intrinsic_sizes),
        resolve_intrinsic_dimension(layout, node, rect.height, 1, intrinsic_sizes),
    )
}

fn resolve_intrinsic_dimension(
    layout: &super::UiLayout,
    node: &UiNode,
    assigned: f32,
    axis: usize,
    intrinsic_sizes: &HashMap<String, [f32; 2]>,
) -> f32 {
    let mode = if axis == 0 {
        layout.width_mode
    } else {
        layout.height_mode
    };
    let requested = match mode {
        UiSizeMode::Fixed if layout.basis[axis] > 0.0 => layout.basis[axis],
        UiSizeMode::FitContent | UiSizeMode::MinContent | UiSizeMode::MaxContent => {
            intrinsic_node_dimension(node, layout, axis, intrinsic_sizes)
        }
        _ => assigned,
    };
    constrain_dimension_without_minimum(requested, layout.max_size[axis])
}

fn intrinsic_node_dimension(
    node: &UiNode,
    layout: &super::UiLayout,
    axis: usize,
    intrinsic_sizes: &HashMap<String, [f32; 2]>,
) -> f32 {
    let padding = if axis == 0 {
        layout.padding.left + layout.padding.right
    } else {
        layout.padding.top + layout.padding.bottom
    };
    // A retained flow owns both dimensions. Main-axis content is additive;
    // cross-axis content is the largest child. Without the cross-axis case a
    // fit-content row can reserve zero height while its children still paint,
    // allowing the next sibling to occupy the same vertical track.
    let child_extent = intrinsic_flow_size_with_map(node, layout, axis == 0, Some(intrinsic_sizes));
    if child_extent > 0.0 {
        return child_extent;
    }
    if let Some(size) = intrinsic_sizes.get(&node.id) {
        // The measured map already contains this node's authored/effective
        // padding. Adding it again would make the parent cursor advance by a
        // smaller amount than the child's final rect and overlap its sibling.
        return size[axis];
    }
    if node.text_key.is_some() || node.text_value.is_some() || node.control.text_input().is_some() {
        return if axis == 1 {
            node.text_style
                .map(|style| style.line_height_px.max(14.0))
                .unwrap_or(18.0)
                + padding
        } else {
            64.0 + padding
        };
    }
    layout.basis[axis].max(layout.min_size[axis]).max(0.0) + padding
}

fn constrain_dimension_without_minimum(value: f32, maximum: f32) -> f32 {
    let value = value.max(0.0);
    if maximum > 0.0 {
        value.min(maximum)
    } else {
        value
    }
}

fn constrain_dimension(value: f32, minimum: f32, maximum: f32) -> f32 {
    let minimum = minimum.max(0.0);
    let maximum = maximum.max(0.0);
    let value = value.max(minimum);
    if maximum > 0.0 {
        value.min(maximum.max(minimum))
    } else {
        value
    }
}

fn apply_opacity(mut color: [u8; 4], opacity: f32) -> [u8; 4] {
    color[3] = ((f32::from(color[3]) * opacity.clamp(0.0, 1.0)).round()) as u8;
    color
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn intrinsic_column_size_keeps_flow_rows_visible_inside_scroll_surfaces() {
        let section = UiNode::new("section", UiNodeKind::Panel)
            .with_layout(UiLayout {
                flow: UiFlow::Column,
                gap: 8.0,
                ..UiLayout::default()
            })
            .with_child(
                UiNode::new("first", UiNodeKind::Panel).with_layout(UiLayout::fixed(0.0, 46.0)),
            )
            .with_child(
                UiNode::new("second", UiNodeKind::Panel).with_layout(UiLayout::fixed(0.0, 66.0)),
            );

        assert_eq!(
            intrinsic_flow_size_with_map(&section, &section.layout, false, None),
            120.0
        );
    }

    #[test]
    fn fixed_flow_tracks_keep_their_authored_extent_when_the_host_is_shorter() {
        let surface = super::super::UiSurface::new(
            "fixed-overflow",
            super::super::StudioUiPalette::IndustrialDark,
            UiNode::new("root", UiNodeKind::Root)
                .with_layout(UiLayout::fill(UiFlow::Column))
                .with_child(
                    UiNode::new("first", UiNodeKind::Panel).with_layout(UiLayout::fixed(0.0, 80.0)),
                )
                .with_child(
                    UiNode::new("second", UiNodeKind::Panel)
                        .with_layout(UiLayout::fixed(0.0, 80.0)),
                ),
        );
        let frame = surface.build_frame(220, 100, [0, 0, 0, 255]);
        let first = frame
            .layout_boxes
            .iter()
            .find(|layout| layout.id == "first")
            .expect("first fixed track");
        let second = frame
            .layout_boxes
            .iter()
            .find(|layout| layout.id == "second")
            .expect("second fixed track");

        assert!(second.rect.y >= first.rect.bottom());
    }

    #[test]
    fn overflowing_vertical_scroll_view_builds_a_visible_manual_scrollbar() {
        let scroll = UiNode::scroll_view("list", super::super::UiScrollAxis::Vertical).with_layout(
            UiLayout {
                flow: UiFlow::Column,
                ..UiLayout::fixed(180.0, 80.0)
            },
        );
        let mut scroll = scroll;
        for index in 0..8 {
            scroll = scroll.with_child(
                UiNode::new(format!("item-{index}"), UiNodeKind::Panel)
                    .with_layout(UiLayout::fixed(0.0, 30.0)),
            );
        }
        let surface = super::super::UiSurface::new(
            "scrollbar",
            super::super::StudioUiPalette::IndustrialDark,
            UiNode::new("root", UiNodeKind::Root)
                .with_layout(UiLayout::fill(UiFlow::Column))
                .with_child(scroll),
        );
        let mut session = super::super::UiSurfaceSession::default();
        // Metrics are measured after the first layout; the following frame
        // paints the thumb using that retained extent and current offset.
        let _ = session.build_frame(&surface, 220, 100, [0, 0, 0, 255]);
        let frame = session.build_frame(&surface, 220, 100, [0, 0, 0, 255]);
        let track = frame
            .layout_boxes
            .iter()
            .find(|layout| layout.id == "__rafui.scrollbar.list.track")
            .expect("scrollbar track");
        let thumb = frame
            .layout_boxes
            .iter()
            .find(|layout| layout.id == "__rafui.scrollbar.list.thumb")
            .expect("scrollbar thumb");

        assert!(track.interactive);
        assert!(track.rect.width < 10.0);
        assert!(thumb.rect.height < track.rect.height);
        let track_hit = frame
            .hit_regions
            .iter()
            .find(|region| region.id == "__rafui.scrollbar.list.track")
            .expect("scrollbar track hit region");
        assert!(track_hit.rect.width > track.rect.width);
        assert!(frame
            .hit_regions
            .iter()
            .any(|region| { region.id == "__rafui.scrollbar.list.thumb" && region.interactive }));
    }

    #[test]
    fn fit_content_leaf_receives_an_intrinsic_text_box() {
        let surface = super::super::UiSurface::new(
            "fit-content",
            super::super::StudioUiPalette::IndustrialDark,
            UiNode::new("root", UiNodeKind::Root).with_child(
                UiNode::new("label", UiNodeKind::Label)
                    .with_text_key("label")
                    .with_layout(UiLayout::fit_content()),
            ),
        );
        let frame = surface.build_frame(320, 80, [0, 0, 0, 255]);
        let label = frame
            .layout_boxes
            .iter()
            .find(|layout| layout.id == "label")
            .expect("fit-content label");

        assert_eq!(label.width_mode, UiSizeMode::FitContent);
        assert!(label.rect.width >= 64.0);
        assert!(label.rect.height >= 18.0);
    }

    #[test]
    fn fit_content_text_is_not_measured_as_one_character_columns() {
        let surface = super::super::UiSurface::new(
            "fit-content-resolved",
            super::super::StudioUiPalette::IndustrialDark,
            UiNode::new("root", UiNodeKind::Root).with_child(
                UiNode::new("label", UiNodeKind::Label)
                    .with_text_key("label")
                    .with_layout(UiLayout::fit_content()),
            ),
        );
        let mut session = super::super::UiSurfaceSession::default();
        let frame =
            session.build_frame_with_resolved_text(&surface, 320, 80, [0, 0, 0, 255], |_| {
                "Natural width label".to_string()
            });
        let label = frame
            .layout_boxes
            .iter()
            .find(|layout| layout.id == "label")
            .expect("resolved fit-content label");
        let request = frame
            .text_requests
            .iter()
            .find(|request| request.node_id == "label")
            .expect("resolved label request");

        assert!(label.rect.width >= 40.0);
        assert!(label.rect.height < 30.0);
        assert!(request.max_width >= 1024.0);
    }

    #[test]
    fn icon_text_tracks_reserve_padding_and_icon_before_atlas_measurement() {
        let surface = super::super::UiSurface::new(
            "icon-text-track",
            super::super::StudioUiPalette::IndustrialDark,
            UiNode::new("root", UiNodeKind::Root)
                .with_layout(UiLayout::fill(UiFlow::Row))
                .with_child(
                    UiNode::new("tab", UiNodeKind::Button)
                        .with_text_key("tab")
                        .with_icon(super::super::UiIcon::new(super::super::UiIconId::Console))
                        .with_layout(UiLayout {
                            flow: UiFlow::Row,
                            padding: raf_ui::UiSpacing::xy(8.0, 0.0),
                            ..UiLayout::fixed(0.0, 28.0)
                        }),
                ),
        );
        let frame = surface.build_frame(320, 80, [0, 0, 0, 255]);
        let tab = frame
            .layout_boxes
            .iter()
            .find(|layout| layout.id == "tab")
            .expect("icon text tab");

        assert!(tab.rect.width >= 100.0);
    }

    #[test]
    fn fill_width_fit_height_rows_keep_text_on_one_measured_line() {
        let surface = super::super::UiSurface::new(
            "tree-row",
            super::super::StudioUiPalette::IndustrialDark,
            UiNode::new("root", UiNodeKind::Root)
                .with_layout(UiLayout::fill(UiFlow::Column))
                .with_child(
                    UiNode::new("row", UiNodeKind::Label)
                        .with_text_key("HELLO WORLD")
                        .with_layout(
                            UiLayout::fit_content()
                                .with_width_mode(UiSizeMode::Fill)
                                .with_height_mode(UiSizeMode::FitContent),
                        ),
                )
                .with_child(
                    UiNode::new("next", UiNodeKind::Label)
                        .with_text_key("Next row")
                        .with_layout(UiLayout::fixed(0.0, 18.0)),
                ),
        );
        let mut session = super::super::UiSurfaceSession::default();
        let frame =
            session.build_frame_with_resolved_text(&surface, 240, 80, [0, 0, 0, 255], |key| {
                key.to_string()
            });
        let row = frame
            .layout_boxes
            .iter()
            .find(|layout| layout.id == "row")
            .expect("tree row");
        let next = frame
            .layout_boxes
            .iter()
            .find(|layout| layout.id == "next")
            .expect("next row");
        let request = frame
            .text_requests
            .iter()
            .find(|request| request.node_id == "row")
            .expect("tree row text request");

        assert!(row.rect.width >= 230.0);
        assert!(row.rect.height < 30.0);
        assert!(next.rect.y >= row.rect.y + row.rect.height);
        assert!(request.max_width >= 230.0);
    }

    #[test]
    fn fit_content_column_reserves_intrinsic_height_for_text_children() {
        let surface = super::super::UiSurface::new(
            "fit-content-column",
            super::super::StudioUiPalette::IndustrialDark,
            UiNode::new("root", UiNodeKind::Root)
                .with_layout(UiLayout::fill(UiFlow::Column))
                .with_child(
                    UiNode::new("card", UiNodeKind::Panel)
                        .with_layout(UiLayout {
                            flow: UiFlow::Column,
                            ..UiLayout::fit_content()
                                .with_width_mode(UiSizeMode::Fill)
                                .with_height_mode(UiSizeMode::FitContent)
                        })
                        .with_child(
                            UiNode::new("card.label", UiNodeKind::Label)
                                .with_text_key("Card")
                                .with_layout(UiLayout::fit_content()),
                        )
                        .with_child(
                            UiNode::new("card.content", UiNodeKind::Label)
                                .with_text_key("Content")
                                .with_layout(UiLayout::fit_content()),
                        ),
                )
                .with_child(
                    UiNode::new("next", UiNodeKind::Label)
                        .with_text_key("Next")
                        .with_layout(UiLayout::fixed(0.0, 18.0)),
                ),
        );
        let mut session = super::super::UiSurfaceSession::default();
        let frame =
            session.build_frame_with_resolved_text(&surface, 320, 120, [0, 0, 0, 255], |key| {
                match key {
                    "Card" => "Assistant".to_string(),
                    "Content" => {
                        "A multiline tool result that must stay inside its card".to_string()
                    }
                    _ => key.to_string(),
                }
            });
        let card = frame
            .layout_boxes
            .iter()
            .find(|layout| layout.id == "card")
            .expect("fit-content card");
        let label = frame
            .layout_boxes
            .iter()
            .find(|layout| layout.id == "card.label")
            .expect("card label");
        let content = frame
            .layout_boxes
            .iter()
            .find(|layout| layout.id == "card.content")
            .expect("card content");
        let next = frame
            .layout_boxes
            .iter()
            .find(|layout| layout.id == "next")
            .expect("next row");

        assert!(card.rect.height >= label.rect.height + content.rect.height);
        assert!(content.rect.y >= label.rect.y + label.rect.height);
        assert!(next.rect.y >= card.rect.y + card.rect.height);
    }

    #[test]
    fn fit_content_row_reserves_cross_axis_height_for_its_tallest_child() {
        let surface = super::super::UiSurface::new(
            "fit-content-row-cross-axis",
            super::super::StudioUiPalette::IndustrialDark,
            UiNode::new("root", UiNodeKind::Root)
                .with_layout(UiLayout::fill(UiFlow::Column))
                .with_child(
                    UiNode::new("toolbar", UiNodeKind::Panel)
                        .with_layout(UiLayout {
                            flow: UiFlow::Row,
                            gap: 8.0,
                            ..UiLayout::fit_content()
                                .with_width_mode(UiSizeMode::Fill)
                                .with_height_mode(UiSizeMode::FitContent)
                        })
                        .with_child(
                            UiNode::new("toolbar.label", UiNodeKind::Label)
                                .with_layout(UiLayout::fixed(120.0, 24.0)),
                        )
                        .with_child(
                            UiNode::new("toolbar.action", UiNodeKind::Button)
                                .with_layout(UiLayout::fixed(44.0, 40.0)),
                        ),
                )
                .with_child(
                    UiNode::new("next", UiNodeKind::Panel).with_layout(UiLayout::fixed(0.0, 18.0)),
                ),
        );
        let frame = surface.build_frame(320, 120, [0, 0, 0, 255]);
        let toolbar = frame
            .layout_boxes
            .iter()
            .find(|layout| layout.id == "toolbar")
            .expect("fit-content row");
        let action = frame
            .layout_boxes
            .iter()
            .find(|layout| layout.id == "toolbar.action")
            .expect("tall row child");
        let next = frame
            .layout_boxes
            .iter()
            .find(|layout| layout.id == "next")
            .expect("following sibling");

        assert!(toolbar.rect.height >= action.rect.height);
        assert!(next.rect.y >= toolbar.rect.y + toolbar.rect.height);
    }

    #[test]
    fn absolute_child_offsets_are_relative_to_the_parent_content() {
        let root = UiNode::new("root", UiNodeKind::Root).with_child(
            UiNode::new("slider", UiNodeKind::Panel)
                .with_layout(UiLayout::absolute(UiRect::new(40.0, 20.0, 120.0, 40.0)))
                .with_child(
                    UiNode::new("slider.fill", UiNodeKind::Panel)
                        .with_layout(UiLayout::absolute(UiRect::new(4.0, 15.0, 72.0, 4.0))),
                ),
        );
        let frame = build_surface_frame(
            &root,
            &UiStyleSheet::default(),
            UiVisualState::default(),
            &super::super::UiControlState::default(),
            240,
            120,
            [0, 0, 0, 255],
            1.0,
        );
        let fill = frame
            .layout_boxes
            .iter()
            .find(|entry| entry.id == "slider.fill")
            .expect("absolute child is laid out");

        assert_eq!(fill.rect, UiRect::new(44.0, 35.0, 72.0, 4.0));
    }

    #[test]
    fn textual_column_children_receive_separate_vertical_tracks() {
        let root = UiNode::new("root", UiNodeKind::Root)
            .with_layout(UiLayout {
                flow: UiFlow::Column,
                gap: 2.0,
                ..UiLayout::fixed(240.0, 80.0)
            })
            .with_child(
                UiNode::new("first", UiNodeKind::Label)
                    .with_text_key("First")
                    .with_text_style(raf_ui::UiTextStyle::body([255, 255, 255, 255])),
            )
            .with_child(
                UiNode::new("second", UiNodeKind::Label)
                    .with_text_key("Second")
                    .with_text_style(raf_ui::UiTextStyle::body([255, 255, 255, 255])),
            );

        let frame = build_surface_frame(
            &root,
            &UiStyleSheet::default(),
            UiVisualState::default(),
            &super::super::UiControlState::default(),
            240,
            80,
            [0, 0, 0, 255],
            1.0,
        );
        let first = frame
            .layout_boxes
            .iter()
            .find(|entry| entry.id == "first")
            .expect("first text row exists");
        let second = frame
            .layout_boxes
            .iter()
            .find(|entry| entry.id == "second")
            .expect("second text row exists");

        assert!(second.rect.y > first.rect.y);
        assert!(first.rect.width > 1.0);
    }

    #[test]
    fn text_controls_receive_a_safe_area_without_changing_their_hit_rect() {
        let root = UiNode::new("root", UiNodeKind::Root).with_child(
            UiNode::new("button", UiNodeKind::Button)
                .with_text_key("<")
                .with_text_style(raf_ui::UiTextStyle::button([255, 255, 255, 255]))
                .with_layout(UiLayout::fixed(28.0, 30.0)),
        );
        let frame = build_surface_frame(
            &root,
            &UiStyleSheet::default(),
            UiVisualState::default(),
            &super::super::UiControlState::default(),
            80,
            48,
            [0, 0, 0, 255],
            1.0,
        );
        let button = frame
            .layout_boxes
            .iter()
            .find(|layout| layout.id == "button")
            .expect("button layout");

        assert_eq!(button.rect, UiRect::new(0.0, 0.0, 28.0, 30.0));
        assert_eq!(button.content_rect.x, 8.0);
        assert_eq!(button.content_rect.width, 12.0);
        assert_eq!(button.content_rect.y, 4.0);
        assert_eq!(button.content_rect.height, 22.0);
    }
}
