//! Dedicated visual review bridge.
//!
//! The active Agent model may be text-only. Captures still exist as project
//! artifacts, so the harness can ask a separately configured vision-capable
//! model to inspect them and return a structured critique. This module never
//! mutates the scene and never starts Play or Runtime.

use crate::chat_media::{image_part, ChatImage, MAX_IMAGES};
use crate::model_capabilities::{vision_support, VisionSupport};
use crate::openai_client::{OpenAiClient, OpenAiConfig, OpenAiMessage};
use crate::provider::{AiProvider, AiProviderConfig};
use raf_core::config::EngineSettings;
use serde_json::{json, Value};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver};
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

const REVIEW_JOB_TTL: Duration = Duration::from_secs(600);

struct ReviewJob {
    receiver: Receiver<Value>,
    started: Instant,
}

fn prune_jobs(jobs: &mut HashMap<String, ReviewJob>) {
    jobs.retain(|_, job| job.started.elapsed() < REVIEW_JOB_TTL);
}

fn review_jobs() -> &'static Mutex<HashMap<String, ReviewJob>> {
    static JOBS: OnceLock<Mutex<HashMap<String, ReviewJob>>> = OnceLock::new();
    JOBS.get_or_init(|| Mutex::new(HashMap::new()))
}

/// Start a non-blocking review on a worker thread. The Agent runtime polls
/// `poll_review` so a slow provider never blocks editor input or scene layout.
pub fn start_review(
    call_id: &str,
    reviewer: VisionReviewer,
    project_root: PathBuf,
    images: Vec<ChatImage>,
    brief: Value,
    focus: String,
) -> bool {
    if call_id.trim().is_empty() {
        return false;
    }
    let Ok(mut jobs) = review_jobs().lock() else {
        return false;
    };
    prune_jobs(&mut jobs);
    if jobs.contains_key(call_id) {
        return false;
    }
    let (sender, receiver) = mpsc::channel();
    let spawn = std::thread::Builder::new()
        .name("raf-vision-review".to_string())
        .spawn(move || {
            let data = review_images(
                Some(&reviewer),
                Some(&project_root),
                &images,
                &brief,
                &focus,
            );
            let _ = sender.send(data);
        });
    if spawn.is_err() {
        return false;
    }
    jobs.insert(
        call_id.to_string(),
        ReviewJob {
            receiver,
            started: Instant::now(),
        },
    );
    true
}

pub fn is_review_pending(call_id: &str) -> bool {
    let Ok(mut jobs) = review_jobs().lock() else {
        return false;
    };
    prune_jobs(&mut jobs);
    jobs.contains_key(call_id)
}

pub fn poll_review(call_id: &str) -> Option<Value> {
    let Ok(mut jobs) = review_jobs().lock() else {
        return None;
    };
    prune_jobs(&mut jobs);
    let job = jobs.get(call_id)?;
    match job.receiver.try_recv() {
        Ok(value) => {
            jobs.remove(call_id);
            Some(value)
        }
        Err(mpsc::TryRecvError::Empty) => None,
        Err(mpsc::TryRecvError::Disconnected) => {
            jobs.remove(call_id);
            Some(json!({
                "status": "error",
                "fallback": "vision_worker_disconnected",
                "reason": "The visual review worker ended without a response.",
            }))
        }
    }
}

const MAX_BRIEF_CHARS: usize = 8_192;
const MAX_REVIEW_TOKENS: u32 = 1_400;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VisionReviewer {
    pub provider: AiProvider,
    pub model: String,
    pub base_url: String,
    api_key: String,
}

impl VisionReviewer {
    fn from_config(config: &AiProviderConfig) -> Self {
        Self {
            provider: config.provider,
            model: config.model.clone(),
            base_url: config.base_url.clone(),
            api_key: config.api_key.clone(),
        }
    }

    pub fn provider_label(&self) -> String {
        self.provider.display_name().to_string()
    }
}

/// Pick a dedicated reviewer from the enabled provider cards. A known
/// vision-capable active model wins; otherwise any enabled known-capable model
/// is used. Unknown models are a last-resort attempt, never a hard failure.
pub fn select_reviewer(settings: &EngineSettings) -> Option<VisionReviewer> {
    let mut supported = Vec::new();
    let mut unknown = Vec::new();
    for config in &settings.ai_providers {
        if !config.enabled || !config.provider.is_editor_supported() {
            continue;
        }
        if config.provider == AiProvider::OpenAI && settings.agent_openai_use_plan {
            continue;
        }
        if config.base_url.trim().is_empty() || config.model.trim().is_empty() {
            continue;
        }
        let reviewer = VisionReviewer::from_config(config);
        let active = config.provider == settings.default_ai_provider;
        match vision_support(&config.model) {
            VisionSupport::Supported => supported.push((active, reviewer)),
            VisionSupport::Unknown => unknown.push((active, reviewer)),
            VisionSupport::Unsupported => {}
        }
    }
    supported.sort_by_key(|(active, _)| !*active);
    if let Some((_, reviewer)) = supported.into_iter().next() {
        return Some(reviewer);
    }
    unknown.sort_by_key(|(active, _)| !*active);
    unknown.into_iter().next().map(|(_, reviewer)| reviewer)
}

/// Ask the dedicated reviewer to inspect one to four project captures.
/// Every branch returns a structured JSON value so the Agent can distinguish
/// `reviewed`, `unavailable`, `unstructured` and `error` without guessing.
pub fn review_images(
    reviewer: Option<&VisionReviewer>,
    project_root: Option<&Path>,
    images: &[ChatImage],
    brief: &Value,
    focus: &str,
) -> Value {
    let Some(reviewer) = reviewer else {
        return json!({
            "status": "unavailable",
            "fallback": "vision_unavailable",
            "reason": "No enabled vision-capable model is configured. The captures remain available as project evidence.",
            "images": images.iter().map(|image| image.file.clone()).collect::<Vec<_>>(),
            "image_count": images.len(),
        });
    };
    let Some(root) = project_root else {
        return json!({
            "status": "unavailable",
            "fallback": "project_unavailable",
            "reason": "The visual reviewer needs an active project root to resolve capture artifacts.",
            "model": reviewer.model,
            "provider": reviewer.provider_label(),
            "images": images.iter().map(|image| image.file.clone()).collect::<Vec<_>>(),
            "image_count": images.len(),
        });
    };
    if images.is_empty() {
        return json!({
            "status": "invalid",
            "fallback": "no_images",
            "reason": "Provide one to four project-relative PNG capture paths.",
            "model": reviewer.model,
            "provider": reviewer.provider_label(),
        });
    }

    let brief_text = bounded_text(&brief.to_string(), MAX_BRIEF_CHARS);
    let focus = focus.trim();
    let user_text = format!(
        "Review the attached {} scene capture(s).\nFocus: {}.\nWorld brief: {}",
        images.len(),
        if focus.is_empty() {
            "composition, scale, lighting, readability and visible defects"
        } else {
            focus
        },
        brief_text
    );
    let mut parts = vec![json!({"type":"text","text": user_text})];
    let mut warnings = Vec::new();
    for image in images.iter().take(MAX_IMAGES) {
        match image_part(root, image) {
            Ok(part) => parts.push(part),
            Err(error) => warnings.push(format!("{}: {error}", image.file)),
        }
    }
    if parts.len() <= 1 {
        return json!({
            "status": "invalid",
            "fallback": "no_readable_images",
            "reason": "None of the supplied image references could be read as a project PNG.",
            "model": reviewer.model,
            "provider": reviewer.provider_label(),
            "warnings": warnings,
        });
    }

    let system = "You are a senior game environment and visual QA reviewer. \
Inspect the attached captures against the world brief. Be specific about composition, \
scale, lighting, readability, occlusion, traversal space and visible defects. \
Return ONLY valid JSON with this shape: \
{\"verdict\":\"pass|needs_work|fail\",\"summary\":\"short text\",\
\"scores\":{\"composition\":1-5,\"scale\":1-5,\"lighting\":1-5,\"readability\":1-5,\"gameplay_space\":1-5},\
\"findings\":[{\"severity\":\"info|warning|error\",\"message\":\"short text\",\
\"targets\":[\"entity ref or region\"],\"suggested_fix\":\"concrete action\"}]}. \
Do not invent entities that are not visible. Do not claim runtime behavior.";
    let messages = vec![
        OpenAiMessage {
            role: "system".to_string(),
            content: Value::String(system.to_string()),
            tool_calls: None,
            tool_call_id: None,
            reasoning_content: None,
            reasoning: None,
            reasoning_details: Vec::new(),
        },
        OpenAiMessage {
            role: "user".to_string(),
            content: Value::Array(parts),
            tool_calls: None,
            tool_call_id: None,
            reasoning_content: None,
            reasoning: None,
            reasoning_details: Vec::new(),
        },
    ];
    let client = OpenAiClient::new(OpenAiConfig {
        base_url: reviewer.base_url.clone(),
        model: reviewer.model.clone(),
        api_key: reviewer.api_key.clone(),
        max_tokens: MAX_REVIEW_TOKENS,
        reasoning_effort: String::new(),
        chatgpt_plan: false,
        chatgpt_registration: None,
        temperature: None,
        streaming: false,
    });
    match client.chat(&messages, None) {
        Ok(message) => {
            let text = message_text(&message.content);
            let mut data = parse_review_json(&text);
            let parsed_status = data
                .get("status")
                .and_then(Value::as_str)
                .unwrap_or("reviewed")
                .to_string();
            if let Some(object) = data.as_object_mut() {
                object.insert("status".to_string(), json!(parsed_status));
                object.insert("model".to_string(), json!(reviewer.model));
                object.insert("provider".to_string(), json!(reviewer.provider_label()));
                object.insert(
                    "images".to_string(),
                    json!(images.iter().map(|image| image.file.clone()).collect::<Vec<_>>()),
                );
                object.insert("image_count".to_string(), json!(images.len().min(MAX_IMAGES)));
                if !warnings.is_empty() {
                    object.insert("warnings".to_string(), json!(warnings));
                }
            }
            data
        }
        Err(error) => json!({
            "status": "error",
            "fallback": "vision_call_failed",
            "reason": "The dedicated vision model could not complete the review.",
            "error": error,
            "model": reviewer.model,
            "provider": reviewer.provider_label(),
            "images": images.iter().map(|image| image.file.clone()).collect::<Vec<_>>(),
            "image_count": images.len(),
            "warnings": warnings,
        }),
    }
}

fn parse_review_json(text: &str) -> Value {
    let cleaned = strip_code_fence(text);
    match serde_json::from_str::<Value>(&cleaned) {
        Ok(value) if value.is_object() => value,
        Ok(value) => json!({
            "status": "unstructured",
            "verdict": "unknown",
            "summary": "The reviewer returned JSON that was not an object.",
            "raw": value,
        }),
        Err(_) => json!({
            "verdict": "unknown",
            "summary": "The reviewer returned unstructured text; no visual score was accepted.",
            "status": "unstructured",
            "text": bounded_text(text, 4_096),
        }),
    }
}

fn strip_code_fence(text: &str) -> String {
    let trimmed = text.trim();
    let without_open = trimmed
        .strip_prefix("```json")
        .or_else(|| trimmed.strip_prefix("```"))
        .unwrap_or(trimmed);
    without_open
        .strip_suffix("```")
        .unwrap_or(without_open)
        .trim()
        .to_string()
}

fn message_text(content: &Value) -> String {
    match content {
        Value::String(text) => text.clone(),
        Value::Array(parts) => parts
            .iter()
            .filter_map(|part| {
                part.get("text")
                    .and_then(Value::as_str)
                    .or_else(|| part.as_str())
            })
            .collect::<Vec<_>>()
            .join("\n"),
        other => other.to_string(),
    }
}

fn bounded_text(text: &str, max_chars: usize) -> String {
    if text.chars().count() <= max_chars {
        return text.to_string();
    }
    let mut bounded = text.chars().take(max_chars).collect::<String>();
    bounded.push_str("... [truncated]");
    bounded
}

#[cfg(test)]
mod tests {
    use super::*;
    use raf_core::config::EngineSettings;

    fn settings_with_models(active: AiProvider, active_model: &str, other: AiProvider, other_model: &str) -> EngineSettings {
        let mut settings = EngineSettings::default();
        for config in &mut settings.ai_providers {
            config.enabled = false;
        }
        if let Some(config) = settings
            .ai_providers
            .iter_mut()
            .find(|config| config.provider == active)
        {
            config.enabled = true;
            config.model = active_model.to_string();
            config.base_url = "https://example.invalid/v1".to_string();
        }
        if active != other {
            if let Some(config) = settings
                .ai_providers
                .iter_mut()
                .find(|config| config.provider == other)
            {
                config.enabled = true;
                config.model = other_model.to_string();
                config.base_url = "https://example.invalid/v1".to_string();
            }
        }
        settings.default_ai_provider = active;
        settings
    }

    #[test]
    fn active_vision_model_is_preferred() {
        let settings = settings_with_models(
            AiProvider::OpenRouter,
            "openai/gpt-4o",
            AiProvider::OpenCode,
            "claude-sonnet-4-5-20250929",
        );
        let reviewer = select_reviewer(&settings).expect("reviewer");
        assert_eq!(reviewer.provider, AiProvider::OpenRouter);
        assert_eq!(reviewer.model, "openai/gpt-4o");
    }

    #[test]
    fn known_vision_model_wins_when_active_model_is_text_only() {
        let settings = settings_with_models(
            AiProvider::OpenRouter,
            "deepseek/deepseek-chat",
            AiProvider::OpenCode,
            "claude-sonnet-4-5-20250929",
        );
        let reviewer = select_reviewer(&settings).expect("reviewer");
        assert_eq!(reviewer.provider, AiProvider::OpenCode);
        assert!(reviewer.model.contains("claude"));
    }

    #[test]
    fn fallback_is_structured_when_no_vision_model_is_available() {
        let settings = settings_with_models(
            AiProvider::OpenRouter,
            "deepseek/deepseek-chat",
            AiProvider::OpenCode,
            "qwen/qwen3-coder",
        );
        assert!(select_reviewer(&settings).is_none());
        let result = review_images(None, None, &[], &json!({}), "");
        assert_eq!(result["status"], "unavailable");
        assert_eq!(result["fallback"], "vision_unavailable");
        assert!(result["reason"].as_str().is_some_and(|reason| reason.contains("vision-capable")));
    }

    #[test]
    fn missing_project_is_a_structured_fallback_before_network_access() {
        let reviewer = VisionReviewer {
            provider: AiProvider::OpenRouter,
            model: "openai/gpt-4o".to_string(),
            base_url: "https://example.invalid/v1".to_string(),
            api_key: "test".to_string(),
        };
        let result = review_images(
            Some(&reviewer),
            None,
            &[ChatImage {
                file: ".aura_rafi/agent_artifacts/front.png".to_string(),
                width: 960,
                height: 540,
            }],
            &json!({"genre":"retail"}),
            "entrance readability",
        );
        assert_eq!(result["status"], "unavailable");
        assert_eq!(result["fallback"], "project_unavailable");
        assert_eq!(result["model"], "openai/gpt-4o");
    }

    #[test]
    fn code_fenced_review_json_is_accepted() {
        let value = parse_review_json("```json\n{\"verdict\":\"pass\",\"summary\":\"ok\"}\n```");
        assert_eq!(value["verdict"], "pass");
        assert_eq!(value["summary"], "ok");
    }

    #[test]
    fn non_object_review_json_is_not_accepted_as_a_score() {
        let value = parse_review_json("[1,2,3]");
        assert_eq!(value["status"], "unstructured");
        assert_eq!(value["verdict"], "unknown");
    }
}
