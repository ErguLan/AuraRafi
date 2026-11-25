//! Image generation and vision-review decisions.

use raf_ai::agent_runtime::{AgentToolResult, ToolExecutionMode};
use serde_json::Value;

use super::AgentToolExecutor;

impl AgentToolExecutor<'_> {
    pub(super) fn execute_asset_generation(
        &mut self,
        call_id: &str,
        name: &str,
        arguments: &Value,
        mode: ToolExecutionMode,
    ) -> Result<AgentToolResult, String> {
        self.asset_tools.execute(
            call_id,
            name,
            arguments,
            mode,
            self.project.as_ref().map(|project| project.root.as_path()),
            self.asset_studio,
            self.image_provider.as_ref(),
            &self.image_model,
            self.language,
        )
    }

    pub(super) fn execute_visual_review(
        &mut self,
        call_id: &str,
        arguments: &Value,
    ) -> Result<AgentToolResult, String> {
        let project_root = self.project.as_ref().map(|project| project.root.clone());
        let mut paths = arguments
            .get("images")
            .and_then(Value::as_array)
            .map(|images| {
                images
                    .iter()
                    .filter_map(Value::as_str)
                    .map(str::to_string)
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        if let Some(image) = arguments.get("image").and_then(Value::as_str) {
            paths.insert(0, image.to_string());
        }
        paths.truncate(raf_ai::chat_media::MAX_IMAGES);
        let mut images = Vec::new();
        let mut warnings = Vec::new();
        if let Some(root) = project_root.as_deref() {
            for path in &paths {
                let relative = path.replace('\\', "/");
                let safe = !relative.starts_with('/')
                    && !relative.contains(':')
                    && relative
                        .split('/')
                        .all(|component| !matches!(component, "" | "." | ".."))
                    && relative.ends_with(".png");
                if !safe {
                    warnings.push(format!("Ignored unsafe image reference: {path}"));
                    continue;
                }
                match ::image::image_dimensions(root.join(&relative)) {
                    Ok((width, height)) => images.push(raf_ai::chat_media::ChatImage {
                        file: relative,
                        width,
                        height,
                    }),
                    Err(error) => warnings.push(format!("Could not read {path}: {error}")),
                }
            }
        }
        let brief = arguments
            .get("brief")
            .cloned()
            .or_else(|| {
                project_root.as_ref().and_then(|root| {
                    std::fs::read(root.join(".aura_rafi/world/brief.json"))
                        .ok()
                        .and_then(|bytes| serde_json::from_slice::<Value>(&bytes).ok())
                })
            })
            .unwrap_or(Value::Null);
        let focus = arguments.get("focus").and_then(Value::as_str).unwrap_or("");
        // A dedicated reviewer performs a network call. Start it on a
        // worker thread and let the Agent runtime poll for the result, so
        // the editor never freezes while a provider is thinking.
        if let (Some(reviewer), Some(root)) = (self.vision_reviewer.as_ref(), project_root.as_ref())
        {
            let queued = raf_ai::vision_bridge::start_review(
                call_id,
                reviewer.clone(),
                root.clone(),
                images.clone(),
                brief.clone(),
                focus.to_string(),
            );
            if queued || raf_ai::vision_bridge::is_review_pending(call_id) {
                let mut result = AgentToolResult::success(
                    "Visual review queued.",
                    serde_json::json!({
                        "state": "queued",
                        "image_count": images.len(),
                        "model": reviewer.model,
                        "provider": reviewer.provider_label(),
                    }),
                );
                result.revision = Some(self.ledger.revision());
                result.verification = Some(serde_json::json!({"status": "pending"}));
                return Ok(result);
            }
        }
        let data = raf_ai::vision_bridge::review_images(
            self.vision_reviewer.as_ref(),
            project_root.as_deref(),
            &images,
            &brief,
            focus,
        );
        let mut result = visual_review_tool_result(data, self.ledger.revision());
        if !warnings.is_empty() {
            result.warnings.splice(0..0, warnings);
        }
        return Ok(result);
    }
}

pub(super) fn visual_review_tool_result(data: Value, revision: u64) -> AgentToolResult {
    let status = data
        .get("status")
        .and_then(Value::as_str)
        .unwrap_or("error")
        .to_string();
    let summary = match status.as_str() {
        "reviewed" => "Visual review completed.".to_string(),
        "unavailable" => {
            "Visual review unavailable; captures remain saved as project evidence.".to_string()
        }
        "unstructured" => {
            "The vision reviewer returned unstructured text; no score was accepted.".to_string()
        }
        "invalid" => "The visual review request had no readable capture.".to_string(),
        _ => "The visual review call failed.".to_string(),
    };
    let images = data
        .get("images")
        .and_then(Value::as_array)
        .map(|images| {
            images
                .iter()
                .filter_map(Value::as_str)
                .map(str::to_string)
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    let mut warnings = data
        .get("warnings")
        .and_then(Value::as_array)
        .map(|warnings| {
            warnings
                .iter()
                .filter_map(Value::as_str)
                .map(str::to_string)
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    if status == "unavailable" {
        warnings.push(
            "No enabled vision-capable model is configured; the captures were not interpreted."
                .to_string(),
        );
    }
    let mut result = AgentToolResult::success(summary, data);
    result.ok = !matches!(status.as_str(), "error" | "invalid");
    result.partial = matches!(status.as_str(), "unavailable" | "unstructured");
    result.revision = Some(revision);
    result.references = images.clone();
    result.artifacts = images
        .iter()
        .map(|image| {
            serde_json::json!({
                "kind": "image/png",
                "uri": image,
                "role": "vision_review_input",
            })
        })
        .collect();
    result.verification = Some(serde_json::json!({
        "status": match status.as_str() {
            "reviewed" => "reviewed",
            "unavailable" | "unstructured" => "not_checked",
            _ => "failed",
        },
        "vision_status": status,
        "image_count": images.len(),
    }));
    result.warnings = warnings;
    result
}
