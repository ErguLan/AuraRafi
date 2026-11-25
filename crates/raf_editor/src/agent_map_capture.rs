//! Top-down, ornamental-free scene map capture.
//!
//! This is a planning and QA artifact, not a second viewport. It projects the
//! authored scene onto the XZ plane, draws world bounds, colliders, a metric
//! grid and a bounded set of labels, and returns the same structured metadata
//! that was used to draw the image.

use ab_glyph::{point, Font, FontArc, PxScale, ScaleFont};
use glam::Vec3;
use image::{Rgba, RgbaImage};
use raf_ai::agent_runtime::AgentToolResult;
use raf_core::agent_context::{
    display_name, display_path, resolve_target as resolve_agent_target, world_bounds,
};
use raf_core::project::{Project, ProjectType};
use raf_core::scene::{Primitive, SceneGraph, SceneNode, SceneNodeId};
use raf_core::Language;
use serde_json::{json, Value};
use std::path::Path;
use uuid::Uuid;

const MAP_FONT_BYTES: &[u8] = include_bytes!("../../raf_render/assets/fonts/Ubuntu-Medium.ttf");
const DEFAULT_WIDTH: u32 = 1_200;
const DEFAULT_HEIGHT: u32 = 1_200;
const MIN_EDGE: u32 = 256;
const MAX_EDGE: u32 = 2_048;
const PADDING: f32 = 48.0;
const MAX_LABELS: usize = 40;
const MAX_ENTITY_ROWS: usize = 512;
const MAX_COLLIDER_ROWS: usize = 256;

pub(crate) fn capture_top_down_map(
    scene: &SceneGraph,
    project: Option<&Project>,
    selection: &[SceneNodeId],
    arguments: &Value,
    language: Language,
) -> AgentToolResult {
    let Some(project) = project else {
        return failure(
            language,
            "agent.viewport_capture.no_project",
            "agent.viewport_capture.no_project_detail",
        );
    };
    if project.project_type != ProjectType::Game {
        return failure(
            language,
            "agent.viewport_capture.game_only",
            "agent.viewport_capture.game_only_detail",
        );
    }
    let root_target = arguments
        .get("root")
        .or_else(|| arguments.get("scope"))
        .and_then(Value::as_str);
    let root = root_target.and_then(|target| resolve_agent_target(scene, target));
    if root_target.is_some() && root.is_none() {
        let mut result = failure(
            language,
            "agent.viewport_capture.no_frame",
            "agent.viewport_capture.no_frame_detail",
        );
        result
            .details
            .push("The requested top-down map root was not found.".to_string());
        return result;
    }
    let scope = match root {
        Some(root) => collect_subtree(scene, root),
        None => scene.all_live_ids(),
    };
    let mut renderable = Vec::new();
    let mut aggregate: Option<(Vec3, Vec3)> = None;
    for id in &scope {
        let Some(node) = scene.get(*id) else {
            continue;
        };
        if node.is_folder || node.primitive == Primitive::Empty || !node.visible {
            continue;
        }
        let Some(bounds) = world_bounds(scene, *id) else {
            continue;
        };
        aggregate = Some(match aggregate {
            Some((min, max)) => (min.min(bounds.0), max.max(bounds.1)),
            None => bounds,
        });
        renderable.push((*id, bounds));
    }
    let Some((min, max)) = aggregate else {
        let mut result = failure(
            language,
            "agent.viewport_capture.no_frame",
            "agent.viewport_capture.no_frame_detail",
        );
        result
            .details
            .push("The requested map scope has no visible renderable entities.".to_string());
        return result;
    };
    let mut size = max - min;
    if size.x.abs() < 0.001 {
        size.x = 1.0;
    }
    if size.z.abs() < 0.001 {
        size.z = 1.0;
    }
    let width = integer_arg(arguments, "width", DEFAULT_WIDTH, MIN_EDGE, MAX_EDGE);
    let height = integer_arg(arguments, "height", DEFAULT_HEIGHT, MIN_EDGE, MAX_EDGE);
    let padding = number_arg(arguments, "padding", PADDING as f64) as f32;
    let padding = padding.clamp(16.0, (width.min(height) as f32) * 0.2);
    let scale = ((width as f32 - padding * 2.0) / size.x)
        .min((height as f32 - padding * 2.0) / size.z)
        .max(0.0001);
    let requested_overlays = arguments
        .get("overlays")
        .and_then(Value::as_array)
        .map(|overlays| {
            overlays
                .iter()
                .filter_map(Value::as_str)
                .map(|overlay| overlay.to_ascii_lowercase())
                .collect::<Vec<_>>()
        });
    let show = |name: &str| match requested_overlays.as_ref() {
        None => true,
        Some(overlays) => overlays
            .iter()
            .any(|overlay| overlay == "all" || overlay == name),
    };
    let grid_spacing = arguments
        .get("grid")
        .or_else(|| arguments.get("grid_spacing"))
        .and_then(Value::as_f64)
        .filter(|spacing| spacing.is_finite() && *spacing > 0.0)
        .map(|spacing| spacing as f32)
        .unwrap_or_else(|| nice_grid(size.x.max(size.z) / 16.0));
    let mut image = RgbaImage::from_pixel(width, height, Rgba([16, 17, 20, 255]));
    if show("grid") {
        draw_grid(&mut image, min, size, scale, padding, grid_spacing);
    }
    let mut entities = Vec::new();
    let mut colliders = Vec::new();
    let mut labels = Vec::new();
    let mut warnings = Vec::new();
    let font = FontArc::try_from_slice(MAP_FONT_BYTES).expect("bundled map font");
    for (id, bounds) in &renderable {
        let Some(node) = scene.get(*id) else {
            continue;
        };
        let rect = world_rect(*bounds, min, scale, padding, width, height);
        let selected = selection.contains(id);
        let is_collider = node.collider.collider_type != raf_core::scene::ColliderType::None;
        let fill = if is_collider && show("colliders") {
            [210, 116, 58, 46]
        } else {
            [node.color.r, node.color.g, node.color.b, 56]
        };
        fill_rect_alpha(&mut image, rect, fill);
        if show("bounds") {
            let outline = if selected {
                [255, 174, 72, 255]
            } else if is_collider && show("colliders") {
                [232, 128, 66, 230]
            } else {
                [116, 124, 138, 190]
            };
            stroke_rect(&mut image, rect, outline, if selected { 2 } else { 1 });
        }
        let reference = format!("entity:{}", node.uuid);
        let bounds_json = bounds_json(*bounds);
        let collider = collider_label(node);
        if entities.len() < MAX_ENTITY_ROWS {
            entities.push(json!({
                "ref": reference,
                "name": display_name(&node.name),
                "path": display_path(scene, *id),
                "kind": node.primitive.label(),
                "selected": selected,
                "collider": collider,
                "semantic_role": node.semantic_role,
                "tags": node.tags,
                "color_rgba": [node.color.r, node.color.g, node.color.b, node.color.a],
                "world_bounds": bounds_json,
                "screen_rect": [rect.0, rect.1, rect.2, rect.3],
            }));
        }
        if is_collider && colliders.len() < MAX_COLLIDER_ROWS {
            colliders.push(json!({
                "ref": reference,
                "type": collider,
                "bounds": bounds_json,
                "offset": [node.collider.offset.x, node.collider.offset.y, node.collider.offset.z],
            }));
        }
        if show("labels")
            && labels.len() < MAX_LABELS
            && should_label(node, *bounds, aggregate, selected)
        {
            let text = label_text(node);
            let (x, y) = (rect.0 + 3, rect.1 + 3);
            let label_color = if selected {
                [255, 196, 112, 255]
            } else {
                [226, 228, 234, 235]
            };
            draw_label(&mut image, &font, &text, x, y, label_color);
            labels.push(json!({
                "ref": reference,
                "text": text,
                "x": x,
                "y": y,
            }));
        }
    }
    let omitted_entities = renderable.len().saturating_sub(entities.len());
    if omitted_entities > 0 {
        warnings.push(format!(
            "Only {MAX_ENTITY_ROWS} of {} map entities were included in the structured metadata.",
            renderable.len()
        ));
    }
    let legend_lines = vec![
        "TOP-DOWN MAP (XZ)".to_string(),
        format!(
            "Entities: {}  Colliders: {}",
            renderable.len(),
            colliders.len()
        ),
        format!("Grid: {:.2} m  Scale: {:.2} px/m", grid_spacing, scale),
        format!("Extents: {:.1} x {:.1} m", size.x, size.z),
    ];
    draw_legend(&mut image, &font, &legend_lines);
    if requested_overlays.is_none() || show("grid") {
        draw_scale_bar(
            &mut image,
            &font,
            scale,
            padding,
            width,
            height,
            grid_spacing,
        );
    }
    let artifact_id = Uuid::new_v4().to_string();
    let relative_path = format!(".aura_rafi/agent_artifacts/top-down-{artifact_id}.png");
    let absolute_path = project.path.join(Path::new(&relative_path));
    if let Some(parent) = absolute_path.parent() {
        if let Err(error) = std::fs::create_dir_all(parent) {
            let mut result = failure(
                language,
                "agent.viewport_capture.save_failed",
                "agent.viewport_capture.save_failed_detail",
            );
            result.details.push(error.to_string());
            return result;
        }
    }
    if let Err(error) = image.save_with_format(&absolute_path, image::ImageFormat::Png) {
        let mut result = failure(
            language,
            "agent.viewport_capture.save_failed",
            "agent.viewport_capture.save_failed_detail",
        );
        result.details.push(error.to_string());
        return result;
    }
    let uri = relative_path.replace('\\', "/");
    let label = arguments
        .get("label")
        .and_then(Value::as_str)
        .unwrap_or("Top-down map");
    let overlays = ["bounds", "colliders", "grid", "labels"]
        .iter()
        .filter(|name| show(name))
        .copied()
        .collect::<Vec<_>>();
    let mut result = AgentToolResult::success(
        format!(
            "Top-down map saved with {} entities and {} colliders.",
            renderable.len(),
            colliders.len()
        ),
        json!({
            "mode": "top_down_map",
            "projection": "xz_top_down",
            "artifact": {
                "id": artifact_id,
                "kind": "image/png",
                "uri": uri.clone(),
                "label": label,
                "top_down_map": true,
            },
            "canvas": {
                "width": width,
                "height": height,
                "padding": padding,
                "scale_px_per_meter": scale,
            },
            "bounds": {
                "min": [min.x, min.y, min.z],
                "max": [max.x, max.y, max.z],
                "size": [size.x, size.y, size.z],
            },
            "grid": {
                "spacing": grid_spacing,
                "major_every": 5,
                "visible": show("grid"),
            },
            "overlays": overlays,
            "entities": entities,
            "colliders": colliders,
            "labels": labels,
            "omitted_entities": omitted_entities,
            "legend": legend_lines,
            "visual_evidence": [{
                "file": uri.clone(),
                "width": width,
                "height": height,
                "label": label,
            }],
        }),
    );
    result.references.push(uri.clone());
    result.artifacts.push(json!({
        "kind": "image/png",
        "uri": uri,
        "role": "top_down_map",
    }));
    result.warnings = warnings;
    result
}

fn collect_subtree(scene: &SceneGraph, root: SceneNodeId) -> Vec<SceneNodeId> {
    let mut result = Vec::new();
    let mut stack = vec![root];
    let mut visited = std::collections::HashSet::new();
    while let Some(id) = stack.pop() {
        if !visited.insert(id) || !scene.is_valid_node(id) {
            continue;
        }
        result.push(id);
        if let Some(node) = scene.get(id) {
            stack.extend(node.children.iter().copied());
        }
    }
    result
}

fn integer_arg(arguments: &Value, key: &str, default: u32, min: u32, max: u32) -> u32 {
    arguments
        .get(key)
        .and_then(Value::as_u64)
        .map(|value| value.clamp(min as u64, max as u64) as u32)
        .unwrap_or(default)
}

fn number_arg(arguments: &Value, key: &str, default: f64) -> f64 {
    arguments
        .get(key)
        .and_then(Value::as_f64)
        .filter(|value| value.is_finite() && *value > 0.0)
        .unwrap_or(default)
}

fn nice_grid(raw: f32) -> f32 {
    const STEPS: &[f32] = &[
        0.1, 0.25, 0.5, 1.0, 2.0, 5.0, 10.0, 20.0, 50.0, 100.0, 200.0, 500.0, 1_000.0,
    ];
    STEPS
        .iter()
        .copied()
        .find(|step| *step >= raw.max(0.05))
        .unwrap_or(1_000.0)
}

fn world_rect(
    bounds: (Vec3, Vec3),
    min: Vec3,
    scale: f32,
    padding: f32,
    width: u32,
    height: u32,
) -> (i32, i32, i32, i32) {
    let x0 = padding + (bounds.0.x - min.x) * scale;
    let y0 = padding + (bounds.0.z - min.z) * scale;
    let x1 = padding + (bounds.1.x - min.x) * scale;
    let y1 = padding + (bounds.1.z - min.z) * scale;
    (
        x0.round().clamp(0.0, width as f32 - 1.0) as i32,
        y0.round().clamp(0.0, height as f32 - 1.0) as i32,
        x1.round().clamp(0.0, width as f32 - 1.0) as i32,
        y1.round().clamp(0.0, height as f32 - 1.0) as i32,
    )
}

fn bounds_json(bounds: (Vec3, Vec3)) -> Value {
    json!({
        "min": [bounds.0.x, bounds.0.y, bounds.0.z],
        "max": [bounds.1.x, bounds.1.y, bounds.1.z],
        "size": [
            bounds.1.x - bounds.0.x,
            bounds.1.y - bounds.0.y,
            bounds.1.z - bounds.0.z,
        ],
    })
}

fn collider_label(node: &SceneNode) -> &'static str {
    match node.collider.collider_type {
        raf_core::scene::ColliderType::None => "none",
        raf_core::scene::ColliderType::Aabb => "aabb",
        raf_core::scene::ColliderType::ConvexHull => "convex_hull",
        raf_core::scene::ColliderType::MeshCollider => "mesh",
    }
}

fn label_text(node: &SceneNode) -> String {
    let mut text = display_name(&node.name);
    if text.is_empty() {
        text = "Entity".to_string();
    }
    if let Some(role) = node
        .semantic_role
        .as_deref()
        .filter(|role| !role.is_empty())
    {
        text = format!("{text} [{role}]");
    }
    text.chars().take(42).collect()
}

fn should_label(
    node: &SceneNode,
    bounds: (Vec3, Vec3),
    aggregate: Option<(Vec3, Vec3)>,
    selected: bool,
) -> bool {
    if selected {
        return true;
    }
    if node.collider.collider_type != raf_core::scene::ColliderType::None {
        return true;
    }
    let text = format!(
        "{} {} {}",
        node.name.to_ascii_lowercase(),
        node.semantic_role
            .as_deref()
            .unwrap_or("")
            .to_ascii_lowercase(),
        node.tags.join(" ").to_ascii_lowercase()
    );
    if ["spawn", "goal", "entrance", "checkpoint", "exit", "door"]
        .iter()
        .any(|marker| text.contains(marker))
    {
        return true;
    }
    if let Some((min, max)) = aggregate {
        let total = ((max.x - min.x) * (max.z - min.z)).max(0.001);
        let area = ((bounds.1.x - bounds.0.x) * (bounds.1.z - bounds.0.z)).max(0.0);
        return area / total > 0.06;
    }
    false
}

fn draw_grid(image: &mut RgbaImage, min: Vec3, size: Vec3, scale: f32, padding: f32, spacing: f32) {
    let color = [44, 47, 54, 255];
    let major = [66, 70, 80, 255];
    let start_x = (min.x / spacing).ceil() * spacing;
    let mut x = start_x;
    let mut index = 0usize;
    while x <= min.x + size.x + 0.001 {
        let px = (padding + (x - min.x) * scale).round() as i32;
        let c = if index % 5 == 0 { major } else { color };
        draw_line(image, px, 0, px, image.height() as i32 - 1, c);
        x += spacing;
        index += 1;
    }
    let start_z = (min.z / spacing).ceil() * spacing;
    let mut z = start_z;
    let mut index = 0usize;
    while z <= min.z + size.z + 0.001 {
        let py = (padding + (z - min.z) * scale).round() as i32;
        let c = if index % 5 == 0 { major } else { color };
        draw_line(image, 0, py, image.width() as i32 - 1, py, c);
        z += spacing;
        index += 1;
    }
}

fn draw_legend(image: &mut RgbaImage, font: &FontArc, lines: &[String]) {
    let line_height = 19.0;
    let height = (lines.len() as f32 * line_height + 16.0).ceil() as u32;
    let width = 330u32.min(image.width());
    fill_rect_alpha(
        image,
        (12, 12, 12 + width as i32, 12 + height as i32),
        [12, 14, 18, 225],
    );
    stroke_rect(
        image,
        (12, 12, 12 + width as i32, 12 + height as i32),
        [84, 92, 106, 255],
        1,
    );
    let mut y = 20.0;
    for (index, line) in lines.iter().enumerate() {
        let size = if index == 0 { 16.0 } else { 14.0 };
        draw_text(
            image,
            font,
            line,
            22,
            y as i32,
            size,
            if index == 0 {
                [255, 176, 74, 255]
            } else {
                [218, 222, 230, 255]
            },
        );
        y += line_height;
    }
}

fn draw_scale_bar(
    image: &mut RgbaImage,
    font: &FontArc,
    scale: f32,
    padding: f32,
    width: u32,
    height: u32,
    grid_spacing: f32,
) {
    let meters = if grid_spacing >= 1.0 {
        grid_spacing.round().max(1.0)
    } else {
        (1.0 / grid_spacing).max(1.0)
    };
    let bar = (meters * scale).clamp(20.0, (width as f32 - padding * 2.0).max(20.0));
    let x0 = (width as f32 - padding - bar).max(padding);
    let y = height as f32 - padding;
    draw_line(
        image,
        x0 as i32,
        y as i32,
        (x0 + bar) as i32,
        y as i32,
        [255, 176, 74, 255],
    );
    draw_line(
        image,
        x0 as i32,
        (y - 4.0) as i32,
        x0 as i32,
        (y + 4.0) as i32,
        [255, 176, 74, 255],
    );
    draw_line(
        image,
        (x0 + bar) as i32,
        (y - 4.0) as i32,
        (x0 + bar) as i32,
        (y + 4.0) as i32,
        [255, 176, 74, 255],
    );
    draw_text(
        image,
        font,
        &format!("{meters:.0} m"),
        x0 as i32,
        (y - 22.0) as i32,
        13.0,
        [236, 238, 244, 255],
    );
}

fn draw_label(image: &mut RgbaImage, font: &FontArc, text: &str, x: i32, y: i32, color: [u8; 4]) {
    let width = text_width(font, text, 12.0).ceil() as i32 + 8;
    let height = 17;
    fill_rect_alpha(image, (x, y, x + width, y + height), [12, 14, 18, 210]);
    draw_text(image, font, text, x + 4, y + 2, 12.0, color);
}

fn text_width(font: &FontArc, text: &str, size: f32) -> f32 {
    let scaled = font.as_scaled(PxScale::from(size));
    text.chars()
        .map(|character| scaled.h_advance(scaled.glyph_id(character)))
        .sum()
}

fn draw_text(
    image: &mut RgbaImage,
    font: &FontArc,
    text: &str,
    x: i32,
    y: i32,
    size: f32,
    color: [u8; 4],
) {
    let scaled = font.as_scaled(PxScale::from(size));
    let mut caret = x as f32;
    let baseline = y as f32 + scaled.ascent();
    for character in text.chars() {
        let glyph_id = scaled.glyph_id(character);
        if let Some(glyph) =
            font.outline_glyph(glyph_id.with_scale_and_position(size, point(caret, baseline)))
        {
            let bounds = glyph.px_bounds();
            glyph.draw(|dx, dy, coverage| {
                let px = bounds.min.x as i32 + dx as i32;
                let py = bounds.min.y as i32 + dy as i32;
                blend_pixel(image, px, py, color, (coverage * 255.0) as u8);
            });
        }
        caret += scaled.h_advance(glyph_id);
    }
}

fn fill_rect_alpha(image: &mut RgbaImage, rect: (i32, i32, i32, i32), color: [u8; 4]) {
    let (x0, y0, x1, y1) = ordered_rect(rect);
    for y in y0.max(0)..=y1.min(image.height() as i32 - 1) {
        for x in x0.max(0)..=x1.min(image.width() as i32 - 1) {
            blend_pixel(image, x, y, color, color[3]);
        }
    }
}

fn stroke_rect(image: &mut RgbaImage, rect: (i32, i32, i32, i32), color: [u8; 4], width: i32) {
    let (x0, y0, x1, y1) = ordered_rect(rect);
    for offset in 0..width.max(1) {
        draw_line(
            image,
            x0 - offset,
            y0 - offset,
            x1 + offset,
            y0 - offset,
            color,
        );
        draw_line(
            image,
            x0 - offset,
            y1 + offset,
            x1 + offset,
            y1 + offset,
            color,
        );
        draw_line(
            image,
            x0 - offset,
            y0 - offset,
            x0 - offset,
            y1 + offset,
            color,
        );
        draw_line(
            image,
            x1 + offset,
            y0 - offset,
            x1 + offset,
            y1 + offset,
            color,
        );
    }
}

fn ordered_rect(rect: (i32, i32, i32, i32)) -> (i32, i32, i32, i32) {
    (
        rect.0.min(rect.2),
        rect.1.min(rect.3),
        rect.0.max(rect.2),
        rect.1.max(rect.3),
    )
}

fn draw_line(image: &mut RgbaImage, mut x0: i32, mut y0: i32, x1: i32, y1: i32, color: [u8; 4]) {
    let dx = (x1 - x0).abs();
    let sx = if x0 < x1 { 1 } else { -1 };
    let dy = -(y1 - y0).abs();
    let sy = if y0 < y1 { 1 } else { -1 };
    let mut error = dx + dy;
    loop {
        blend_pixel(image, x0, y0, color, color[3]);
        if x0 == x1 && y0 == y1 {
            break;
        }
        let twice = error * 2;
        if twice >= dy {
            error += dy;
            x0 += sx;
        }
        if twice <= dx {
            error += dx;
            y0 += sy;
        }
    }
}

fn blend_pixel(image: &mut RgbaImage, x: i32, y: i32, color: [u8; 4], alpha: u8) {
    if x < 0 || y < 0 || x >= image.width() as i32 || y >= image.height() as i32 {
        return;
    }
    let alpha = ((alpha as u32 * color[3] as u32) / 255) as u8;
    if alpha == 0 {
        return;
    }
    let pixel = image.get_pixel_mut(x as u32, y as u32);
    let inverse = 255 - alpha as u32;
    for channel in 0..3 {
        pixel[channel] =
            ((color[channel] as u32 * alpha as u32 + pixel[channel] as u32 * inverse) / 255) as u8;
    }
    pixel[3] = 255;
}

fn failure(language: Language, summary_key: &str, detail_key: &str) -> AgentToolResult {
    let mut result = AgentToolResult::success(raf_core::i18n::t(summary_key, language), json!({}));
    result.ok = false;
    result.details.push(raf_core::i18n::t(detail_key, language));
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use raf_core::scene::collider::ColliderType;

    #[test]
    fn top_down_map_draws_bounds_colliders_grid_and_labels() {
        let root =
            std::env::temp_dir().join(format!("raf-top-down-{}", uuid::Uuid::new_v4().simple()));
        std::fs::create_dir_all(&root).unwrap();
        let project = Project::create("MapDemo", ProjectType::Game, &root).unwrap();
        let mut scene = SceneGraph::new();
        let floor = scene.add_root("Floor");
        if let Some(node) = scene.get_mut(floor) {
            node.primitive = Primitive::Plane;
            node.scale = Vec3::new(12.0, 1.0, 10.0);
        }
        let wall = scene.add_root("Wall");
        if let Some(node) = scene.get_mut(wall) {
            node.primitive = Primitive::Cube;
            node.position = Vec3::new(0.0, 1.0, 0.0);
            node.scale = Vec3::new(8.0, 2.0, 0.3);
            node.collider.collider_type = ColliderType::Aabb;
        }
        let crate_id = scene.add_root("Crate");
        if let Some(node) = scene.get_mut(crate_id) {
            node.primitive = Primitive::Cube;
            node.position = Vec3::new(2.0, 0.5, 2.0);
            node.scale = Vec3::splat(1.0);
            node.collider.collider_type = ColliderType::Aabb;
            node.tags = vec!["prop.crate".to_string()];
        }
        let result = capture_top_down_map(
            &scene,
            Some(&project),
            &[crate_id],
            &json!({"width": 512, "height": 512, "preset": "top_down_map"}),
            Language::English,
        );
        assert!(result.ok, "{:?}", result.details);
        assert_eq!(result.data["mode"], "top_down_map");
        assert_eq!(result.data["projection"], "xz_top_down");
        assert_eq!(result.data["grid"]["visible"], true);
        assert!(result.data["grid"]["spacing"].as_f64().unwrap_or(0.0) > 0.0);
        assert!(result.data["bounds"]["size"].as_array().is_some());
        let overlays = result.data["overlays"].as_array().expect("overlays");
        for overlay in ["bounds", "colliders", "grid", "labels"] {
            assert!(
                overlays.iter().any(|value| value == overlay),
                "missing overlay {overlay}"
            );
        }
        assert!(result.data["entities"].as_array().unwrap().len() >= 3);
        assert!(result.data["colliders"].as_array().unwrap().len() >= 2);
        assert!(!result.data["labels"].as_array().unwrap().is_empty());
        assert_eq!(result.data["canvas"]["width"], 512);
        assert_eq!(result.data["canvas"]["height"], 512);
        let uri = result.data["artifact"]["uri"]
            .as_str()
            .expect("artifact uri");
        let absolute = project.path.join(uri);
        assert!(absolute.exists());
        let image = image::open(absolute).unwrap();
        assert_eq!(image.width(), 512);
        assert_eq!(image.height(), 512);
        assert!(!result.artifacts.is_empty());
        assert!(result.data["visual_evidence"]
            .as_array()
            .is_some_and(|evidence| !evidence.is_empty()));
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn top_down_map_can_disable_overlays() {
        let root = std::env::temp_dir().join(format!(
            "raf-top-down-overlays-{}",
            uuid::Uuid::new_v4().simple()
        ));
        std::fs::create_dir_all(&root).unwrap();
        let project = Project::create("MapDemo", ProjectType::Game, &root).unwrap();
        let mut scene = SceneGraph::new();
        let floor = scene.add_root("Floor");
        if let Some(node) = scene.get_mut(floor) {
            node.primitive = Primitive::Plane;
            node.scale = Vec3::new(8.0, 1.0, 8.0);
        }
        let result = capture_top_down_map(
            &scene,
            Some(&project),
            &[],
            &json!({"overlays": ["bounds"], "width": 256, "height": 256}),
            Language::English,
        );
        assert!(result.ok);
        assert_eq!(result.data["grid"]["visible"], false);
        assert_eq!(result.data["labels"], json!([]));
        let _ = std::fs::remove_dir_all(root);
    }
}
