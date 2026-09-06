//! Evidence artifacts produced by the native Agent.
//!
//! The module owns only artifact creation and serialization. It does not
//! choose a renderer or mutate project scene state; the render runtime is the
//! single source for the last Game viewport frame.

use std::path::Path;

use raf_ai::agent_runtime::AgentToolResult;
use raf_core::config::Language;
use raf_core::project::{Project, ProjectType};
use raf_render::bridge::{GraphicsSurfaceKind, RenderRuntime};
use serde_json::json;
use uuid::Uuid;

const MAX_CAPTURE_PIXELS: u64 = 16_777_216;

pub(crate) fn capture_viewport_artifact(
    graphics: &RenderRuntime,
    project: Option<&Project>,
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
    if graphics.snapshot().surface != GraphicsSurfaceKind::SceneViewport {
        return failure(
            language,
            "agent.viewport_capture.no_frame",
            "agent.viewport_capture.no_frame_detail",
        );
    }

    let capture = match graphics.capture_last_scene_rgba() {
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

    let Some(image) = image::RgbaImage::from_raw(capture.width, capture.height, capture.rgba8)
    else {
        return failure(
            language,
            "agent.viewport_capture.invalid_data",
            "agent.viewport_capture.invalid_data_detail",
        );
    };
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
    let backend = graphics
        .snapshot()
        .active_backend
        .map(|backend| format!("{backend:?}"));
    let mut result = AgentToolResult::success(
        t(language, "agent.viewport_capture.saved"),
        json!({
            "artifact": {
                "id": artifact_id,
                "kind": "image/png",
                "uri": uri,
                "label": t(language, "agent.viewport_capture.label"),
            },
            "width": capture.width,
            "height": capture.height,
            "backend": backend,
        }),
    );
    result.references.push(relative_path.replace('\\', "/"));
    result
        .details
        .push(t(language, "agent.viewport_capture.saved_detail"));
    result
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
