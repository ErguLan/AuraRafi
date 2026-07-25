use super::{
    UiAlign, UiCompactMode, UiControl, UiFlow, UiHitRegion, UiJustify, UiLayout, UiNode,
    UiNodeKind, UiPositionMode, UiRect, UiSizeMode, UiStyle, UiStyleSheet, UiTextAtlasRequest,
    UiTextStyle, UiVisualState,
};

/// Retained interaction/layout output. Visual geometry is compiled separately
/// into `UiSurfaceDrawList`, which is the sole paint payload for both CPU and
/// GPU UI compositors.
#[derive(Debug, Clone, PartialEq)]
pub struct UiSurfaceFrame {
    pub layout_boxes: Vec<UiLayoutBox>,
    pub hit_regions: Vec<UiHitRegion>,
    pub text_requests: Vec<UiTextAtlasRequest>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct UiLayoutBox {
    pub id: String,
    pub kind: UiNodeKind,
    pub text_key: Option<String>,
    pub tooltip_key: Option<String>,
    pub rect: UiRect,
    pub clip_rect: UiRect,
    pub interactive: bool,
    pub focusable: bool,
    pub disabled: bool,
    pub z_index: i16,
    pub width_mode: UiSizeMode,
    pub height_mode: UiSizeMode,
    pub style: UiStyle,
    pub text_style: Option<UiTextStyle>,
    pub control: UiControl,
}

pub(super) fn build_surface_frame(
    root: &UiNode,
    style_sheet: &UiStyleSheet,
    visual_state: UiVisualState<'_>,
    control_state: &super::UiControlState,
    width: u32,
    height: u32,
    _clear_color: [u8; 4],
) -> UiSurfaceFrame {
    let root_rect = UiRect::new(0.0, 0.0, width as f32, height as f32);
    let mut boxes = Vec::new();
    let mut hit_regions = Vec::new();
    let mut text_requests = Vec::new();
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
    );

    UiSurfaceFrame {
        layout_boxes: boxes,
        hit_regions,
        text_requests,
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
) {
    let layout = node.layout.resolved_for(assigned_rect.width);
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
        // A flow parent may intentionally compress a child below its requested
        // minimum when the surface is physically narrower than all tracks.
        // Re-applying the minimum here would make siblings overlap.
        None => constrain_assigned_rect(&layout, assigned_rect, node),
    };
    let z_index = parent_z_index.saturating_add(layout.z_index);
    let style = style_sheet.resolve_with_state(node, visual_state);
    boxes.push(UiLayoutBox {
        id: node.id.clone(),
        kind: node.kind,
        text_key: node.text_key.clone(),
        tooltip_key: node.tooltip_key.clone(),
        rect,
        clip_rect: parent_clip,
        interactive: node.interactive,
        focusable: node.focusable,
        disabled: node.disabled,
        z_index,
        width_mode: layout.width_mode,
        height_mode: layout.height_mode,
        style: style.clone(),
        text_style: node.text_style,
        control: node.control.clone(),
    });
    hit_regions.push(UiHitRegion {
        id: node.id.clone(),
        kind: node.kind,
        rect,
        clip_rect: parent_clip,
        z_index,
        interactive: node.interactive,
        focusable: node.focusable,
        disabled: node.disabled,
    });
    let text_key = node.text_key.as_deref().or_else(|| {
        node.control
            .text_input()
            .map(|input| input.value_key.as_str())
    });
    if let Some(text_key) = text_key {
        let mut text_style = node
            .text_style
            .unwrap_or_else(|| UiTextStyle::body(style.text));
        text_style.color = apply_opacity(text_style.color, style.opacity);
        let text_max_width = if rect.width > 1.0 { rect.width } else { 256.0 };
        text_requests.push(UiTextAtlasRequest::new(
            node.id.clone(),
            text_key.to_string(),
            text_style,
            text_max_width,
        ));
    }

    let content = rect.shrink(layout.padding);
    let child_clip = if layout.overflow.clips_children() || node.control.scroll_axis().is_some() {
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
    let flow = resolve_compact_flow(node, &layout, child_content);
    let mut flow_content = child_content;
    if let Some(axis) = node.control.scroll_axis() {
        if axis.scrolls_vertically() && flow == UiFlow::Column {
            flow_content.height = flow_content
                .height
                .max(intrinsic_flow_main_size(node, &layout, false));
        }
        if axis.scrolls_horizontally() && flow == UiFlow::Row {
            flow_content.width = flow_content
                .width
                .max(intrinsic_flow_main_size(node, &layout, true));
        }
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
) {
    let flow_children = node
        .children
        .iter()
        .filter(|child| child.layout.rect.is_none())
        .map(|node| FlowChild {
            layout: node.layout.resolved_for(content.width),
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
    let main_sizes = resolve_flow_main_sizes(&flow_children, row, item_space);
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
        let main = wrap_main_size(&layout, available);
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
        });
    }

    let mut layout_by_id = std::collections::HashMap::<&str, UiRect>::new();
    let mut y = content.y;
    for (row, occupied) in rows.iter().zip(row_widths.iter().copied()) {
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
            let main = wrap_main_size(&child.layout, available);
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
                .unwrap_or(content)
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
) {
    let gap = parent_layout.gap.max(0.0);
    let min_width = parent_layout.grid.min_column_width.max(1.0);
    let automatic_columns = ((content.width + gap) / (min_width + gap)).floor() as u16;
    let columns = parent_layout.grid.columns.max(automatic_columns).max(1) as usize;
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
                .unwrap_or(content)
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
        );
    }
}

#[derive(Clone)]
struct FlowChild<'a> {
    node: &'a UiNode,
    layout: UiLayout,
}

fn resolve_compact_flow(node: &UiNode, layout: &UiLayout, content: UiRect) -> UiFlow {
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
            child_layout.basis[0].max(child_layout.min_size[0])
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
                    child_layout.min_size[0].max(child_layout.basis[0]) <= content.width
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

fn wrap_main_size(layout: &UiLayout, available: f32) -> f32 {
    let requested = layout.basis[0].max(layout.min_size[0]);
    let fallback = (available * 0.5).max(1.0);
    constrain_dimension(
        if requested > 0.0 { requested } else { fallback },
        0.0,
        layout.max_size[0],
    )
    .min(available)
}

/// Resolves one row or column track without allowing sibling rectangles to
/// overlap. Minimums are honored while the host has room; otherwise every
/// item compresses proportionally, which is the only physically valid result.
fn resolve_flow_main_sizes(children: &[FlowChild<'_>], row: bool, available: f32) -> Vec<f32> {
    if children.is_empty() {
        return Vec::new();
    }
    let axis = if row { 0 } else { 1 };
    let requested = children
        .iter()
        .map(|child| flow_main_requirement(child.node, &child.layout, row))
        .collect::<Vec<_>>();
    let requested_total = requested.iter().sum::<f32>();
    if requested_total > available {
        if requested_total <= f32::EPSILON {
            return vec![available / children.len() as f32; children.len()];
        }
        return requested
            .iter()
            .map(|size| size * available / requested_total)
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

/// Calculates the main-axis size a flow child needs before its parent applies
/// free-space growth. This gives scroll views a real content extent instead
/// of collapsing containers that only have flow children.
fn flow_main_requirement(node: &UiNode, layout: &UiLayout, row: bool) -> f32 {
    let axis = if row { 0 } else { 1 };
    let explicit = layout.basis[axis].max(layout.min_size[axis]).max(0.0);
    if explicit > f32::EPSILON || layout.grow > 0.0 {
        return explicit;
    }

    let intrinsic = intrinsic_flow_main_size(node, layout, row);
    if intrinsic > f32::EPSILON {
        return intrinsic;
    }

    // Text leaves do not have children from which an intrinsic flow size can
    // be derived. Reserve a real track for them so a column does not paint
    // every label at the same y coordinate, and a row does not give a label a
    // zero-width wrapping box that rasterizes one character per line.
    if node.text_key.is_some() || node.control.text_input().is_some() {
        return if row {
            64.0_f32.max(layout.min_size[axis])
        } else {
            node.text_style
                .map(|style| style.line_height_px.max(18.0))
                .unwrap_or(18.0)
        };
    }

    0.0
}

fn intrinsic_flow_main_size(node: &UiNode, layout: &UiLayout, row: bool) -> f32 {
    let matching_flow = matches!(
        (layout.flow, row),
        (UiFlow::Row, true) | (UiFlow::Column, false)
    );
    if !matching_flow {
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
    let children_size = children
        .iter()
        .map(|child| flow_main_requirement(child, &child.layout, row))
        .sum::<f32>();
    children_size + layout.gap.max(0.0) * children.len().saturating_sub(1) as f32 + padding
}

fn constrain_rect(layout: &super::UiLayout, rect: UiRect) -> UiRect {
    UiRect::new(
        rect.x,
        rect.y,
        constrain_dimension(rect.width, layout.min_size[0], layout.max_size[0]),
        constrain_dimension(rect.height, layout.min_size[1], layout.max_size[1]),
    )
}

fn constrain_assigned_rect(layout: &super::UiLayout, rect: UiRect, node: &UiNode) -> UiRect {
    UiRect::new(
        rect.x,
        rect.y,
        resolve_intrinsic_dimension(layout, node, rect.width, 0),
        resolve_intrinsic_dimension(layout, node, rect.height, 1),
    )
}

fn resolve_intrinsic_dimension(
    layout: &super::UiLayout,
    node: &UiNode,
    assigned: f32,
    axis: usize,
) -> f32 {
    let mode = if axis == 0 {
        layout.width_mode
    } else {
        layout.height_mode
    };
    let requested = match mode {
        UiSizeMode::Fixed if layout.basis[axis] > 0.0 => layout.basis[axis],
        UiSizeMode::FitContent | UiSizeMode::MinContent | UiSizeMode::MaxContent => {
            intrinsic_node_dimension(node, layout, axis)
        }
        _ => assigned,
    };
    constrain_dimension_without_minimum(requested, layout.max_size[axis])
}

fn intrinsic_node_dimension(node: &UiNode, layout: &super::UiLayout, axis: usize) -> f32 {
    let padding = if axis == 0 {
        layout.padding.left + layout.padding.right
    } else {
        layout.padding.top + layout.padding.bottom
    };
    let child_extent = if axis == 0 && layout.flow == UiFlow::Row {
        node.children
            .iter()
            .map(|child| child.layout.basis[0].max(child.layout.min_size[0]))
            .sum::<f32>()
            + layout.gap.max(0.0) * node.children.len().saturating_sub(1) as f32
    } else if axis == 1 && layout.flow == UiFlow::Column {
        node.children
            .iter()
            .map(|child| child.layout.basis[1].max(child.layout.min_size[1]))
            .sum::<f32>()
            + layout.gap.max(0.0) * node.children.len().saturating_sub(1) as f32
    } else {
        0.0
    };
    if child_extent > 0.0 {
        return child_extent + padding;
    }
    if node.text_key.is_some() || node.control.text_input().is_some() {
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
            intrinsic_flow_main_size(&section, &section.layout, false),
            120.0
        );
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
}
