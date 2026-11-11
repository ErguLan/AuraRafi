//! Offscreen, read-only scene capture for Agent visual verification.
//!
//! The capture never enters Play and never mutates the authored scene. It
//! frames either the current selection or the whole scene from an explicit
//! angle, then renders through the shared graphics runtime when one is
//! available or through the bounded CPU path otherwise.

use glam::Vec3;
use raf_ai::agent_runtime::AgentToolResult;
use raf_core::agent_context::world_bounds;
use raf_core::project::{GeometryDetailMode, Project, ProjectType, TextureQualityMode};
use raf_core::scene::{SceneGraph, SceneNodeId};
use raf_core::Language;
use raf_render::api_graphic_basic::{FrameInvalidation, SceneFrameCapture};
use raf_render::bridge::{GraphicsSurfaceKind, RenderRuntime};
use raf_render::camera::Camera;
use raf_render::scene_renderer::{RenderOptions, SceneRenderer};
use serde_json::json;
use std::path::Path;
use uuid::Uuid;

const MAX_CAPTURE_PIXELS: u64 = 16_777_216;
const MIN_CAPTURE_EDGE: u32 = 128;
const MAX_CAPTURE_EDGE: u32 = 1_024;

#[derive(Debug, Clone)]
pub(crate) struct SceneCaptureRequest {
    pub view: String,
    pub yaw_deg: f32,
    pub pitch_deg: f32,
    pub distance: Option<f32>,
    pub padding: f32,
    pub width: u32,
    pub height: u32,
    pub highlight_selection: bool,
    pub label: Option<String>,
}

impl Default for SceneCaptureRequest {
    fn default() -> Self {
        Self {
            view: "isometric".to_string(),
            yaw_deg: 45.0,
            pitch_deg: 30.0,
            distance: None,
            padding: 1.15,
            width: 960,
            height: 540,
            highlight_selection: true,
            label: None,
        }
    }
}

/// Capture selected entities or the whole scene from an authored angle.
pub(crate) fn capture_scene_angle(
    graphics: Option<&mut RenderRuntime>,
    scene: &SceneGraph,
    project: Option<&Project>,
    selection: &[SceneNodeId],
    focus: &[SceneNodeId],
    request: &SceneCaptureRequest,
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

    let (width, height) = bounded_size(request.width, request.height, graphics.is_some());
    let (camera, bounds, framed_ids) =
        match frame_camera(scene, selection, focus, request) {
            Ok(value) => value,
            Err(error) => {
                let mut result = failure(
                    language,
                    "agent.viewport_capture.no_frame",
                    "agent.viewport_capture.no_frame_detail",
                );
                result.details.push(error);
                return result;
            }
        };
    let highlight = request.highlight_selection && !selection.is_empty();
    let capture = match render_capture(
        graphics,
        scene,
        project,
        &camera,
        width,
        height,
        selection,
        highlight,
    ) {
        Ok(capture) => capture,
        Err(error) => {
            let mut result = failure(
                language,
                "agent.viewport_capture.no_frame",
                "agent.viewport_capture.no_frame_detail",
            );
            result.details.push(error);
            return result;
        }
    };
    let pixel_count = (capture.width as u64).saturating_mul(capture.height as u64);
    if pixel_count == 0 || pixel_count > MAX_CAPTURE_PIXELS {
        return failure(
            language,
            "agent.viewport_capture.invalid_size",
            "agent.viewport_capture.invalid_size_detail",
        );
    }
    let expected_len = pixel_count.saturating_mul(4) as usize;
    if capture.rgba8.len() != expected_len {
        let mut result = failure(
            language,
            "agent.viewport_capture.invalid_data",
            "agent.viewport_capture.invalid_data_detail",
        );
        result.details.push(format!(
            "Expected {expected_len} RGBA bytes, got {}.",
            capture.rgba8.len()
        ));
        return result;
    }

    let artifact_id = Uuid::new_v4().to_string();
    let relative_path = format!(".aura_rafi/agent_artifacts/viewport-{artifact_id}.png");
    if let Err(error) = save_capture(project, &relative_path, &capture) {
        let mut result = failure(
            language,
            "agent.viewport_capture.save_failed",
            "agent.viewport_capture.save_failed_detail",
        );
        result.details.push(error);
        return result;
    }

    let label = request
        .label
        .clone()
        .filter(|label| !label.trim().is_empty())
        .unwrap_or_else(|| {
            format!(
                "{} ({})",
                t(language, "agent.viewport_capture.angle_label"),
                request.view
            )
        });
    let uri = relative_path.replace('\\', "/");
    let mut result = AgentToolResult::success(
        t(language, "agent.viewport_capture.saved"),
        json!({
            "artifact": {
                "id": artifact_id,
                "kind": "image/png",
                "uri": uri,
                "label": label,
            },
            "width": capture.width,
            "height": capture.height,
            "view": request.view,
            "camera": {
                "position": [camera.position.x, camera.position.y, camera.position.z],
                "target": [camera.target.x, camera.target.y, camera.target.z],
                "distance": (camera.position - camera.target).length(),
                "fov_degrees": camera.fov,
            },
            "framed_entities": framed_ids.iter().map(|id| id.0).collect::<Vec<_>>(),
            "bounds": {
                "min": [bounds.0.x, bounds.0.y, bounds.0.z],
                "max": [bounds.1.x, bounds.1.y, bounds.1.z],
            },
            "visual_evidence": [{
                "file": uri,
                "width": capture.width,
                "height": capture.height,
                "label": label,
            }],
        }),
    );
    result.references.push(uri);
    result
        .warnings
        .push(t(language, "agent.viewport_capture.offscreen_note"));
    result
}

fn bounded_size(width: u32, height: u32, gpu: bool) -> (u32, u32) {
    let cap = if gpu { MAX_CAPTURE_EDGE } else { 640 };
    let width = width.clamp(MIN_CAPTURE_EDGE, cap);
    let height = height.clamp(MIN_CAPTURE_EDGE, cap);
    (width, height)
}

fn frame_camera(
    scene: &SceneGraph,
    selection: &[SceneNodeId],
    focus: &[SceneNodeId],
    request: &SceneCaptureRequest,
) -> Result<(Camera, (Vec3, Vec3), Vec<SceneNodeId>), String> {
    let mut framed_ids = if focus.is_empty() {
        selection.to_vec()
    } else {
        focus.to_vec()
    };
    let scene_wide = framed_ids.is_empty();
    if scene_wide {
        framed_ids = scene.all_live_ids();
    }
    let mut bounds: Option<(Vec3, Vec3)> = None;
    for id in &framed_ids {
        let node_bounds = if scene_wide {
            raf_core::scene::geometry::node_geometry_bounds(scene, *id)
        } else {
            world_bounds(scene, *id)
        };
        if let Some((min, max)) = node_bounds {
            bounds = Some(match bounds {
                Some((current_min, current_max)) => {
                    (current_min.min(min), current_max.max(max))
                }
                None => (min, max),
            });
        }
    }
    let Some((min, max)) = bounds else {
        return Err("No renderable bounds were found for the requested capture scope.".into());
    };
    let center = (min + max) * 0.5;
    let radius = ((max - min).length() * 0.5).max(0.5);
    let fov = 50.0f32;
    let auto_distance = radius / (fov.to_radians() * 0.5).tan() * request.padding.max(1.0);
    let distance = request
        .distance
        .map(|value| value.max(0.1))
        .unwrap_or(auto_distance)
        .max(0.5);
    let direction = view_direction(&request.view, request.yaw_deg, request.pitch_deg)?;
    let up = if direction.y.abs() > 0.95 {
        if direction.y > 0.0 {
            Vec3::Z
        } else {
            -Vec3::Z
        }
    } else {
        Vec3::Y
    };
    let camera = Camera {
        position: center + direction * distance,
        target: center,
        up,
        fov,
        near: (distance - radius * 2.0).max(0.05),
        far: (distance + radius * 8.0).max(100.0),
        ..Camera::default()
    };
    Ok((camera, (min, max), framed_ids))
}

fn view_direction(view: &str, yaw_deg: f32, pitch_deg: f32) -> Result<Vec3, String> {
    let direction = match view.to_ascii_lowercase().as_str() {
        "front" => Vec3::new(0.0, 0.0, 1.0),
        "back" => Vec3::new(0.0, 0.0, -1.0),
        "left" => Vec3::new(-1.0, 0.0, 0.0),
        "right" => Vec3::new(1.0, 0.0, 0.0),
        "top" => Vec3::new(0.0, 1.0, 0.0001),
        "bottom" => Vec3::new(0.0, -1.0, 0.0001),
        "isometric" => Vec3::new(1.0, 0.8, 1.0),
        "custom" => {
            let yaw = yaw_deg.to_radians();
            let pitch = pitch_deg.clamp(-89.0, 89.0).to_radians();
            Vec3::new(
                yaw.sin() * pitch.cos(),
                pitch.sin(),
                yaw.cos() * pitch.cos(),
            )
        }
        other => return Err(format!("Unknown capture view '{other}'.")),
    };
    Ok(direction.normalize())
}

fn render_capture(
    graphics: Option<&mut RenderRuntime>,
    scene: &SceneGraph,
    project: &Project,
    camera: &Camera,
    width: u32,
    height: u32,
    selected: &[SceneNodeId],
    highlight: bool,
) -> Result<SceneFrameCapture, String> {
    let mut renderer = SceneRenderer::new(width, height);
    renderer.set_asset_root(Some(project.path.join("assets")));
    let options = RenderOptions {
        environment_override: Some(*scene.environment()),
        show_grid_3d: false,
        selection_outline: highlight,
        primary_selected: selected.first().map(|id| id.0 as u64),
        geometry_detail: GeometryDetailMode::Efficient,
        texture_quality: TextureQualityMode::Adaptive,
        ..RenderOptions::default()
    };
    let background = [18, 22, 30, 255];
    let light_direction = Vec3::new(-0.5, 1.0, 0.3);
    let frame = renderer.build_frame(
        scene,
        camera,
        width as f32,
        height as f32,
        selected,
        background,
        light_direction,
        options,
        None,
    );
    if let Some(graphics) = graphics {
        graphics.activate_surface(GraphicsSurfaceKind::SceneViewport);
        let _ = graphics.render_scene_frame(&frame);
        let capture = graphics.capture_last_scene_rgba()?;
        // Restore the authored editor frame on the next compositor tick.
        graphics.request_frame(FrameInvalidation::DOCUMENT | FrameInvalidation::UI);
        return Ok(capture);
    }
    let pixels = renderer
        .render(
            scene,
            camera,
            width as f32,
            height as f32,
            selected,
            background,
            light_direction,
            options,
            None,
        )
        .to_vec();
    Ok(SceneFrameCapture {
        width,
        height,
        rgba8: pixels,
    })
}

fn save_capture(
    project: &Project,
    relative_path: &str,
    capture: &SceneFrameCapture,
) -> Result<(), String> {
    let absolute_path = project.path.join(Path::new(relative_path));
    if let Some(parent) = absolute_path.parent() {
        std::fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    }
    let Some(image) = image::RgbaImage::from_raw(capture.width, capture.height, capture.rgba8.clone())
    else {
        return Err("The capture RGBA buffer has an invalid size.".into());
    };
    image
        .save_with_format(&absolute_path, image::ImageFormat::Png)
        .map_err(|error| error.to_string())
}

fn failure(language: Language, summary_key: &str, detail_key: &str) -> AgentToolResult {
    let mut result = AgentToolResult::success(t(language, summary_key), json!({}));
    result.ok = false;
    result.details.push(t(language, detail_key));
    result
}

fn t(language: Language, key: &str) -> String {
    raf_core::i18n::t(key, language)
}

#[cfg(test)]
mod tests {
    use super::*;
    use raf_core::scene::Primitive;

    #[test]
    fn front_and_isometric_framing_target_the_selection() {
        let mut scene = SceneGraph::new();
        let hero = scene.add_root_with_primitive("Hero", Primitive::Cube);
        let far = scene.add_root_with_primitive("Far", Primitive::Cube);
        if let Some(node) = scene.get_mut(far) {
            node.position = Vec3::new(40.0, 0.0, 40.0);
        }
        let front = SceneCaptureRequest {
            view: "front".into(),
            ..SceneCaptureRequest::default()
        };
        let (camera, bounds, framed) =
            frame_camera(&scene, &[hero], &[hero], &front).expect("front camera");
        assert_eq!(framed, vec![hero]);
        assert!(camera.position.z > camera.target.z);
        assert!((camera.target - (bounds.0 + bounds.1) * 0.5).length() < 0.001);

        let isometric = SceneCaptureRequest {
            view: "isometric".into(),
            ..SceneCaptureRequest::default()
        };
        let (camera, _, _) =
            frame_camera(&scene, &[hero], &[hero], &isometric).expect("isometric camera");
        assert!(camera.position.x > camera.target.x);
        assert!(camera.position.y > camera.target.y);
        assert!(camera.position.z > camera.target.z);
    }

    #[test]
    fn whole_scene_frame_includes_every_renderable_node() {
        let mut scene = SceneGraph::new();
        scene.add_root_with_primitive("A", Primitive::Cube);
        let far = scene.add_root_with_primitive("B", Primitive::Cube);
        if let Some(node) = scene.get_mut(far) {
            node.position = Vec3::new(20.0, 0.0, 0.0);
        }
        let request = SceneCaptureRequest {
            view: "top".into(),
            ..SceneCaptureRequest::default()
        };
        let (_, bounds, framed) =
            frame_camera(&scene, &[], &[], &request).expect("scene camera");
        assert_eq!(framed.len(), 2);
        assert!(bounds.1.x - bounds.0.x > 10.0);
    }
}
