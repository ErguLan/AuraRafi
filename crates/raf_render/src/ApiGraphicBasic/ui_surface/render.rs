use std::sync::Arc;

use glam::{Mat4, Quat, Vec3};

use crate::api_graphic_basic::command_list::BasicCommandList;
use crate::api_graphic_basic::mesh::BasicMesh;
use crate::scene_renderer::{FrameStats, SceneRenderFrame};

use super::{
    UiFlow, UiHitRegion, UiNode, UiNodeKind, UiRect, UiStyle, UiStyleSheet, UiTextAtlasRequest,
    UiTextStyle, UiVisualState,
};

#[derive(Debug, Clone)]
pub struct UiSurfaceFrame {
    pub frame: SceneRenderFrame,
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
    pub interactive: bool,
    pub focusable: bool,
    pub disabled: bool,
    pub z_index: i16,
    pub style: UiStyle,
    pub text_style: Option<UiTextStyle>,
}

pub(super) fn build_surface_frame(
    root: &UiNode,
    style_sheet: &UiStyleSheet,
    visual_state: UiVisualState<'_>,
    width: u32,
    height: u32,
    clear_color: [u8; 4],
) -> UiSurfaceFrame {
    let mut commands = BasicCommandList::new();
    commands.clear(clear_color);
    let quad_id = commands.register_mesh(unit_quad_mesh());
    let root_rect = UiRect::new(0.0, 0.0, width as f32, height as f32);
    let mut boxes = Vec::new();
    let mut hit_regions = Vec::new();
    let mut text_requests = Vec::new();
    record_node(
        root,
        style_sheet,
        visual_state,
        root_rect,
        0.0,
        0,
        quad_id,
        &mut commands,
        &mut boxes,
        &mut hit_regions,
        &mut text_requests,
    );

    UiSurfaceFrame {
        frame: SceneRenderFrame {
            commands,
            view_proj: canvas_view_projection(0.0, width as f32, 0.0, height as f32),
            light_dir: Vec3::Z,
            width,
            height,
            stats: FrameStats::default(),
        },
        layout_boxes: boxes,
        hit_regions,
        text_requests,
    }
}

fn record_node(
    node: &UiNode,
    style_sheet: &UiStyleSheet,
    visual_state: UiVisualState<'_>,
    assigned_rect: UiRect,
    z: f32,
    parent_z_index: i16,
    quad_id: usize,
    commands: &mut BasicCommandList,
    boxes: &mut Vec<UiLayoutBox>,
    hit_regions: &mut Vec<UiHitRegion>,
    text_requests: &mut Vec<UiTextAtlasRequest>,
) {
    let rect = constrain_rect(&node.layout, node.layout.rect.unwrap_or(assigned_rect));
    let z_index = parent_z_index.saturating_add(node.layout.z_index);
    let draw_z = z - f32::from(z_index) * 0.0001;
    let style = style_sheet.resolve_with_state(node, visual_state);
    record_rect(
        commands,
        quad_id,
        rect,
        draw_z,
        apply_opacity(style.fill, style.opacity),
    );
    record_border(
        commands,
        rect,
        draw_z - 0.01,
        style.border_width,
        apply_opacity(style.border, style.opacity),
    );
    boxes.push(UiLayoutBox {
        id: node.id.clone(),
        kind: node.kind,
        text_key: node.text_key.clone(),
        tooltip_key: node.tooltip_key.clone(),
        rect,
        interactive: node.interactive,
        focusable: node.focusable,
        disabled: node.disabled,
        z_index,
        style: style.clone(),
        text_style: node.text_style,
    });
    hit_regions.push(UiHitRegion {
        id: node.id.clone(),
        kind: node.kind,
        rect,
        z_index,
        interactive: node.interactive,
        focusable: node.focusable,
        disabled: node.disabled,
    });
    if let Some(text_key) = node.text_key.as_ref() {
        let mut text_style = node
            .text_style
            .unwrap_or_else(|| UiTextStyle::body(style.text));
        text_style.color = apply_opacity(text_style.color, style.opacity);
        text_requests.push(UiTextAtlasRequest::new(
            node.id.clone(),
            text_key.clone(),
            text_style,
            rect.width,
        ));
    }

    let content = rect.shrink(node.layout.padding);
    match node.layout.flow {
        UiFlow::None => {
            for child in &node.children {
                let child_rect = child.layout.rect.unwrap_or(content);
                record_node(
                    child,
                    style_sheet,
                    visual_state,
                    child_rect,
                    draw_z - 0.02,
                    z_index,
                    quad_id,
                    commands,
                    boxes,
                    hit_regions,
                    text_requests,
                );
            }
        }
        UiFlow::Row => record_flow_children(
            node,
            style_sheet,
            visual_state,
            content,
            true,
            draw_z,
            z_index,
            quad_id,
            commands,
            boxes,
            hit_regions,
            text_requests,
        ),
        UiFlow::Column => record_flow_children(
            node,
            style_sheet,
            visual_state,
            content,
            false,
            draw_z,
            z_index,
            quad_id,
            commands,
            boxes,
            hit_regions,
            text_requests,
        ),
    }
}

fn record_flow_children(
    node: &UiNode,
    style_sheet: &UiStyleSheet,
    visual_state: UiVisualState<'_>,
    content: UiRect,
    row: bool,
    z: f32,
    parent_z_index: i16,
    quad_id: usize,
    commands: &mut BasicCommandList,
    boxes: &mut Vec<UiLayoutBox>,
    hit_regions: &mut Vec<UiHitRegion>,
    text_requests: &mut Vec<UiTextAtlasRequest>,
) {
    let flow_children = node
        .children
        .iter()
        .filter(|child| child.layout.rect.is_none())
        .collect::<Vec<_>>();
    let available_main = if row { content.width } else { content.height };
    let gap_count = flow_children.len().saturating_sub(1);
    let gap = node
        .layout
        .gap
        .max(0.0)
        .min(available_main / gap_count.max(1) as f32);
    let item_space = (available_main - gap * gap_count as f32).max(0.0);
    let main_sizes = resolve_flow_main_sizes(&flow_children, row, item_space);
    let mut cursor = if row { content.x } else { content.y };
    let mut flow_index = 0;

    for child in &node.children {
        let child_rect = if let Some(rect) = child.layout.rect {
            rect
        } else {
            let main = main_sizes[flow_index];
            flow_index += 1;
            let requested_cross = if row {
                if child.layout.basis[1] > 0.0 {
                    child.layout.basis[1].min(content.height)
                } else {
                    content.height
                }
            } else if child.layout.basis[0] > 0.0 {
                child.layout.basis[0].min(content.width)
            } else {
                content.width
            };

            let cross = constrain_dimension(
                requested_cross,
                if row {
                    child.layout.min_size[1]
                } else {
                    child.layout.min_size[0]
                },
                if row {
                    child.layout.max_size[1]
                } else {
                    child.layout.max_size[0]
                },
            );

            let rect = if row {
                UiRect::new(cursor, content.y, main, cross)
            } else {
                UiRect::new(content.x, cursor, cross, main)
            };
            cursor += main + gap;
            rect
        };
        record_node(
            child,
            style_sheet,
            visual_state,
            child_rect,
            z - 0.02,
            parent_z_index,
            quad_id,
            commands,
            boxes,
            hit_regions,
            text_requests,
        );
    }
}

/// Resolves one row or column track without allowing sibling rectangles to
/// overlap. Minimums are honored while the host has room; otherwise every
/// item compresses proportionally, which is the only physically valid result.
fn resolve_flow_main_sizes(children: &[&UiNode], row: bool, available: f32) -> Vec<f32> {
    if children.is_empty() {
        return Vec::new();
    }
    let axis = if row { 0 } else { 1 };
    let requested = children
        .iter()
        .map(|child| {
            child.layout.basis[axis]
                .max(child.layout.min_size[axis])
                .max(0.0)
        })
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

fn constrain_rect(layout: &super::UiLayout, rect: UiRect) -> UiRect {
    UiRect::new(
        rect.x,
        rect.y,
        constrain_dimension(rect.width, layout.min_size[0], layout.max_size[0]),
        constrain_dimension(rect.height, layout.min_size[1], layout.max_size[1]),
    )
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

fn record_rect(
    commands: &mut BasicCommandList,
    quad_id: usize,
    rect: UiRect,
    z: f32,
    color: [u8; 4],
) {
    if color[3] == 0 || rect.width <= 0.0 || rect.height <= 0.0 {
        return;
    }
    let transform = Mat4::from_scale_rotation_translation(
        Vec3::new(rect.width, rect.height, 1.0),
        Quat::IDENTITY,
        Vec3::new(rect.x, rect.y, z),
    );
    commands.draw_mesh(quad_id, transform, color);
}

fn record_border(
    commands: &mut BasicCommandList,
    rect: UiRect,
    z: f32,
    width: f32,
    color: [u8; 4],
) {
    if color[3] == 0 || width <= 0.0 || rect.width <= 0.0 || rect.height <= 0.0 {
        return;
    }
    let tl = Vec3::new(rect.x, rect.y, z);
    let tr = Vec3::new(rect.right(), rect.y, z);
    let br = Vec3::new(rect.right(), rect.bottom(), z);
    let bl = Vec3::new(rect.x, rect.bottom(), z);
    commands.draw_line(tl, tr, color, width, true, 0.0);
    commands.draw_line(tr, br, color, width, true, 0.0);
    commands.draw_line(br, bl, color, width, true, 0.0);
    commands.draw_line(bl, tl, color, width, true, 0.0);
}

fn unit_quad_mesh() -> Arc<BasicMesh> {
    Arc::new(BasicMesh::from_positions(
        &[
            Vec3::new(0.0, 0.0, 0.0),
            Vec3::new(1.0, 0.0, 0.0),
            Vec3::new(1.0, 1.0, 0.0),
            Vec3::new(0.0, 1.0, 0.0),
        ],
        &[0, 1, 2, 0, 2, 3],
    ))
}

fn canvas_view_projection(left: f32, right: f32, top: f32, bottom: f32) -> Mat4 {
    let width = (right - left).max(0.001);
    let height = (bottom - top).max(0.001);
    let scale_x = 2.0 / width;
    let scale_y = -2.0 / height;
    let translate_x = -(right + left) / width;
    let translate_y = (bottom + top) / height;

    Mat4::from_cols_array(&[
        scale_x,
        0.0,
        0.0,
        0.0,
        0.0,
        scale_y,
        0.0,
        0.0,
        0.0,
        0.0,
        1.0,
        0.0,
        translate_x,
        translate_y,
        0.0,
        1.0,
    ])
}
