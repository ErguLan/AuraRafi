//! Viewport capture requests and PNG evidence capture.

use raf_ai::agent_runtime::AgentToolResult;
use serde_json::Value;

use crate::agent_artifacts::capture_viewport_artifact;
use crate::agent_context::resolve_target;
use crate::agent_scene_capture::{capture_scene_angle, SceneCaptureRequest};

use super::AgentToolExecutor;
/// Keep the most useful perception and semantic authoring tools inside the
/// provider budget. Low-level convenience commands remain reachable through
/// `scene_batch`, but must not displace the tools that let the model see scale,
/// bounds and structure before it acts.
pub(crate) fn capture_target_values(arguments: &Value) -> Vec<String> {
    if let Some(targets) = arguments.get("targets").and_then(Value::as_array) {
        return targets
            .iter()
            .filter_map(Value::as_str)
            .map(str::trim)
            .filter(|target| !target.is_empty())
            .take(64)
            .map(str::to_string)
            .collect();
    }
    arguments
        .get("target")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|target| !target.is_empty() && !target.eq_ignore_ascii_case("scene"))
        .map(|target| vec![target.to_string()])
        .unwrap_or_default()
}

pub(crate) fn capture_request(arguments: &Value, view: &str) -> SceneCaptureRequest {
    let number = |key: &str, default: f64| {
        arguments
            .get(key)
            .and_then(Value::as_f64)
            .unwrap_or(default)
    };
    SceneCaptureRequest {
        view: view.to_string(),
        yaw_deg: number("yaw_deg", 45.0) as f32,
        pitch_deg: number("pitch_deg", 30.0) as f32,
        distance: arguments
            .get("distance")
            .and_then(Value::as_f64)
            .filter(|distance| distance.is_finite() && *distance > 0.0)
            .map(|distance| distance as f32),
        padding: (number("padding", 1.15) as f32).clamp(1.0, 3.0),
        width: number("width", 960.0).clamp(128.0, 1_024.0) as u32,
        height: number("height", 540.0).clamp(128.0, 1_024.0) as u32,
        highlight_selection: arguments
            .get("highlight_selection")
            .and_then(Value::as_bool)
            .unwrap_or(true),
        label: arguments
            .get("label")
            .and_then(Value::as_str)
            .map(str::to_string),
    }
}

impl AgentToolExecutor<'_> {
    pub(super) fn execute_viewport_capture(
        &mut self,
        arguments: Value,
    ) -> Result<AgentToolResult, String> {
            let mut view = arguments
                .get("view")
                .and_then(Value::as_str)
                .unwrap_or("current")
                .trim()
                .to_ascii_lowercase();
            let frame_scene = arguments
                .get("frame")
                .and_then(Value::as_str)
                .is_some_and(|value| value.eq_ignore_ascii_case("scene"))
                || arguments
                    .get("target")
                    .and_then(Value::as_str)
                    .is_some_and(|value| value.eq_ignore_ascii_case("scene"));
            let requested_targets = capture_target_values(&arguments);
            let mut unresolved = Vec::new();
            let mut focus = Vec::new();
            if frame_scene {
                focus = self.scene.all_live_ids();
            } else {
                for target in &requested_targets {
                    match resolve_target(self.scene, self.selection, target) {
                        Some(id) => focus.push(id),
                        None => unresolved.push(target.clone()),
                    }
                }
            }
            if !requested_targets.is_empty() && focus.is_empty() && !frame_scene {
                let mut result = AgentToolResult::success(
                    "The requested capture targets were not found.",
                    serde_json::json!({
                        "error": {"code": "target_not_found", "targets": unresolved},
                        "available": false,
                    }),
                );
                result.ok = false;
                result.revision = Some(self.ledger.revision());
                return Ok(result);
            }
            let top_down_map = arguments
                .get("preset")
                .and_then(Value::as_str)
                .is_some_and(|preset| {
                    preset.eq_ignore_ascii_case("top_down_map")
                        || preset.eq_ignore_ascii_case("top_down")
                        || preset.eq_ignore_ascii_case("map")
                })
                || arguments
                    .get("top_down")
                    .and_then(Value::as_bool)
                    .unwrap_or(false);
            if top_down_map {
                let mut map_arguments = arguments.clone();
                if map_arguments.get("root").is_none() {
                    if let Some(target) = requested_targets.first() {
                        map_arguments["root"] = Value::String(target.clone());
                    }
                }
                let mut result = crate::agent_map_capture::capture_top_down_map(
                    self.scene,
                    self.project_info,
                    &self.selection.selected_nodes,
                    &map_arguments,
                    self.language,
                );
                result.revision = Some(self.ledger.revision());
                return Ok(result);
            }
            let contact_sheet = arguments
                .get("preset")
                .and_then(Value::as_str)
                .is_some_and(|preset| preset.eq_ignore_ascii_case("contact_sheet"))
                || arguments
                    .get("contact_sheet")
                    .and_then(Value::as_bool)
                    .unwrap_or(false);
            if contact_sheet {
                let Some(project) = self.project_info else {
                    let mut result = AgentToolResult::success(
                        "A contact sheet requires an active Game project.",
                        serde_json::json!({"ok": false, "error": "project_not_found"}),
                    );
                    result.ok = false;
                    result.revision = Some(self.ledger.revision());
                    return Ok(result);
                };
                let mut frames = Vec::new();
                let mut failures = Vec::new();
                for capture_view in ["isometric", "front", "right", "top"] {
                    let request = capture_request(&arguments, capture_view);
                    let capture = capture_scene_angle(
                        self.graphics.as_deref_mut(),
                        self.scene,
                        self.project_info,
                        &self.selection.selected_nodes,
                        &focus,
                        &request,
                        self.language,
                    );
                    if capture.ok {
                        if let Some(uri) = capture
                            .data
                            .get("artifact")
                            .and_then(Value::as_object)
                            .and_then(|artifact| artifact.get("uri"))
                            .and_then(Value::as_str)
                        {
                            frames.push(uri.to_string());
                        }
                    } else {
                        failures.push(format!("{capture_view}: {}", capture.summary));
                    }
                }
                let label = arguments
                    .get("label")
                    .and_then(Value::as_str)
                    .unwrap_or("Contact sheet");
                let mut result = match crate::agent_scene_capture::stitch_contact_sheet(
                    project,
                    &frames,
                    label,
                ) {
                    Ok(result) => result,
                    Err(error) => {
                        let mut result = AgentToolResult::success(
                            "The contact sheet could not be assembled.",
                            serde_json::json!({
                                "ok": false,
                                "frames": frames,
                                "failures": failures,
                                "error": error,
                            }),
                        );
                        result.ok = false;
                        result
                    }
                };
                if !failures.is_empty() {
                    result.warnings.extend(failures);
                }
                result.revision = Some(self.ledger.revision());
                return Ok(result);
            }
            let angle_requested = view != "current"
                || frame_scene
                || !focus.is_empty()
                || arguments.get("yaw_deg").is_some()
                || arguments.get("pitch_deg").is_some()
                || arguments.get("distance").is_some();
            let mut result = if !angle_requested {
                match self.graphics.as_deref_mut() {
                    Some(graphics) => {
                        let refresh_requested = arguments
                            .get("refresh")
                            .and_then(Value::as_bool)
                            .unwrap_or(true);
                        if refresh_requested {
                            graphics.request_frame(
                                raf_render::api_graphic_basic::FrameInvalidation::DOCUMENT
                                    | raf_render::api_graphic_basic::FrameInvalidation::UI,
                            );
                        }
                        let mut result =
                            capture_viewport_artifact(graphics, self.project_info, self.language);
                        result.data["refresh_requested"] = Value::Bool(refresh_requested);
                        result.data["capture_is_last_completed_frame"] =
                            Value::Bool(refresh_requested);
                        if refresh_requested {
                            result.warnings.push(
                                "A fresh viewport frame was requested; this synchronous tool call returns the last completed frame.".to_string(),
                            );
                        }
                        result
                    }
                    // The Hub workspace has no live viewport, but an offscreen
                    // capture still gives the Agent real visual evidence.
                    None => {
                        view = "isometric".to_string();
                        let request = capture_request(&arguments, &view);
                        capture_scene_angle(
                            self.graphics.as_deref_mut(),
                            self.scene,
                            self.project_info,
                            &self.selection.selected_nodes,
                            &focus,
                            &request,
                            self.language,
                        )
                    }
                }
            } else {
                if view == "current" {
                    view = "isometric".to_string();
                }
                let request = capture_request(&arguments, &view);
                capture_scene_angle(
                    self.graphics.as_deref_mut(),
                    self.scene,
                    self.project_info,
                    &self.selection.selected_nodes,
                    &focus,
                    &request,
                    self.language,
                )
            };
            if !unresolved.is_empty() {
                result.warnings.push(format!(
                    "Some capture targets were not found and were not framed: {}.",
                    unresolved.join(", ")
                ));
            }
            result.revision = Some(self.ledger.revision());
            return Ok(result);
    }
}
