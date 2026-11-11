//! Agent runtime - planning, tool-calling loop, and event generation.
//!
//! The runtime is non-blocking: HTTP requests run on a background thread
//! so the UI stays responsive. The caller calls `poll()` each frame to
//! advance the state machine.

use std::collections::HashSet;
use std::sync::{mpsc, Arc, Mutex};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use crate::chat::{ChatMessage, MessageRole, ProviderReasoning};
use crate::openai_client::{
    FunctionCall, OpenAiClient, OpenAiConfig, OpenAiMessage, OpenAiTool, StreamDelta, ToolCall,
};
use chrono::Utc;
use raf_core::{AgentTaskEvent, AgentTaskId, AgentTaskManager, AgentTaskSnapshot, AgentTaskStatus};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use uuid::Uuid;

/// Whether a tool observes state or can mutate the open project.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AgentToolKind {
    Read,
    Mutation,
}

/// Execution policy selected by the editor for the current Agent run.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToolExecutionMode {
    /// Only native read tools may run.
    Inspect,
    /// Reads run normally; mutations execute against a disposable preview.
    Preview,
    /// Mutations are applied to the active project.
    Apply,
}

/// Provider-neutral result returned by every native Agent tool.
///
/// The compact JSON representation is sent back to the model and persisted in
/// chat history. Renderer/debug details stay in `details` and are intentionally
/// omitted when they would only duplicate `data`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AgentToolResult {
    pub ok: bool,
    pub summary: String,
    #[serde(default, skip_serializing_if = "Value::is_null")]
    pub data: Value,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub references: Vec<String>,
    #[serde(default)]
    pub changed: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub revision: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub diff: Option<Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub verification: Option<Value>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub warnings: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub details: Vec<String>,
    #[serde(default)]
    pub preview: bool,
}

impl AgentToolResult {
    pub fn success(summary: impl Into<String>, data: Value) -> Self {
        Self {
            ok: true,
            summary: summary.into(),
            data,
            references: Vec::new(),
            changed: false,
            revision: None,
            diff: None,
            verification: None,
            warnings: Vec::new(),
            details: Vec::new(),
            preview: false,
        }
    }

    fn model_content(&self, max_chars: usize) -> String {
        let mut compact = self.clone();
        compact.data = compact_json_value(compact.data, 6, 64, 2_048);
        if let Some(diff) = compact.diff.take() {
            compact.diff = Some(compact_json_value(diff, 5, 48, 1_024));
        }
        compact.details.truncate(12);
        compact.details = compact
            .details
            .into_iter()
            .map(|line| truncate_chars(line, 320))
            .collect();
        let encoded = serde_json::to_string(&compact).unwrap_or_else(|error| {
            format!(
                "{{\"ok\":false,\"summary\":\"Agent tool result serialization failed: {error}\"}}"
            )
        });
        if encoded.chars().count() <= max_chars {
            return encoded;
        }
        let fallback = Self {
            ok: compact.ok,
            summary: truncate_chars(compact.summary, max_chars.saturating_sub(120)),
            data: serde_json::json!({"truncated": true}),
            references: compact.references.into_iter().take(16).collect(),
            changed: compact.changed,
            revision: compact.revision,
            diff: None,
            verification: compact.verification,
            warnings: Vec::new(),
            details: Vec::new(),
            preview: compact.preview,
        };
        serde_json::to_string(&fallback).unwrap_or_else(|_| "{\"ok\":false}".to_string())
    }
}

/// Executor for tools invoked by the agent.
pub trait ToolExecutor {
    fn revision(&self) -> Option<u64> {
        None
    }
    fn preview_for_review(
        &mut self,
        _id: &str,
        _name: &str,
        _arguments: Value,
    ) -> Option<AgentToolResult> {
        None
    }
    /// An asynchronous tool has started, but has not produced its final result.
    fn is_pending(&self, _call_id: &str) -> bool {
        false
    }

    /// Poll existing work without re-executing the tool or issuing another request.
    fn poll_pending(&mut self, _call_id: &str) -> Option<Result<AgentToolResult, String>> {
        None
    }

    fn kind(&self, _name: &str) -> AgentToolKind {
        AgentToolKind::Mutation
    }

    fn execute(
        &mut self,
        call_id: &str,
        name: &str,
        arguments: Value,
        mode: ToolExecutionMode,
    ) -> Result<AgentToolResult, String>;

    fn describe(&self, name: &str) -> String {
        format!("Execute {}", name)
    }
}

/// A tool call waiting for approval or execution.
#[derive(Debug, Clone)]
pub struct PendingToolCall {
    pub id: String,
    pub name: String,
    pub arguments: Value,
    /// Set when the provider did not return valid JSON for this call. The
    /// request remains visible, but it is never sent to the editor executor.
    pub argument_error: Option<String>,
    pub approved: Option<bool>,
    pub result: Option<String>,
    pub expected_revision: Option<u64>,
    pub review_preview: Option<AgentToolResult>,
}

impl PendingToolCall {
    pub fn new(id: impl Into<String>, name: impl Into<String>, arguments: Value) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
            arguments,
            argument_error: None,
            approved: None,
            result: None,
            expected_revision: None,
            review_preview: None,
        }
    }
}

/// Runtime status.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AgentStatus {
    /// Agent finished or not started.
    Done,
    /// Waiting for user to approve/deny tool calls.
    AwaitingApproval,
    /// Waiting for the LLM API response.
    Thinking,
    /// Applying one approved tool call at a time on the editor thread.
    ExecutingTools,
    /// Hit an error.
    Error,
}

/// Lightweight live activity projected by native UI, CLI, and MCP clients.
/// Elapsed time is derived on demand and is not emitted as a task event every
/// second, keeping the shared lifecycle stream bounded.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentActivitySnapshot {
    pub status: AgentStatus,
    pub elapsed_seconds: u64,
    pub current_tool: Option<String>,
    pub completed_tools: usize,
    pub pending_tools: usize,
    pub total_tools: usize,
    pub turn: usize,
}

impl AgentStatus {
    /// Whether the agent currently owns the input flow.
    pub fn blocks_input(&self) -> bool {
        matches!(
            self,
            Self::Thinking | Self::ExecutingTools | Self::AwaitingApproval
        )
    }

    /// Whether the editor must keep presenting frames without user input.
    /// Awaiting approval blocks submission but is event-driven, so it must not
    /// keep the whole editor in a continuous repaint loop.
    pub fn needs_continuous_frame(&self) -> bool {
        matches!(self, Self::Thinking | Self::ExecutingTools)
    }
}

/// Events emitted for UI observation.
#[derive(Debug, Clone)]
pub enum AgentEvent {
    MessageAdded(ChatMessage),
    ToolCallsRequested(Vec<PendingToolCall>),
    ToolResult { id: String, result: String },
    StatusChanged(AgentStatus, Option<String>),
    TaskProgress(AgentTaskSnapshot),
}

/// Shared cell for passing background-thread results back to the runtime.
struct PendingResponse {
    result: Arc<Mutex<Option<Result<OpenAiMessage, String>>>>,
    stream_rx: mpsc::Receiver<StreamDelta>,
    _handle: JoinHandle<()>,
    tools: Vec<OpenAiTool>,
    execution_mode: ToolExecutionMode,
}

/// A queued sequence of tool calls that must preserve editor mutation order.
struct PendingToolExecution {
    tools: Vec<OpenAiTool>,
    execution_mode: ToolExecutionMode,
}

/// The Agent runtime with non-blocking design.
pub struct AgentRuntime {
    pub client: OpenAiClient,
    pub messages: Vec<ChatMessage>,
    pub pending_calls: Vec<PendingToolCall>,
    pub status: AgentStatus,
    pub last_error: Option<String>,
    pub events: Vec<AgentEvent>,
    max_turns: usize,
    turn_count: usize,
    /// Optional per-run tool-call budget. `None` is the user-facing
    /// unlimited mode; the turn guard still protects the editor from a
    /// provider that loops forever without producing a final answer.
    max_tool_calls: Option<usize>,
    tool_calls_executed: usize,
    max_tool_result_chars: usize,
    tool_execution_interval: Duration,
    last_tool_execution: Option<Instant>,
    run_started_at: Option<Instant>,
    pending: Option<PendingResponse>,
    pending_tool_execution: Option<PendingToolExecution>,
    /// Ephemeral provider context loaded through a native read tool before a
    /// run starts. It is sent once to the provider and is intentionally not
    /// persisted as a fake chat exchange.
    request_context: Vec<OpenAiMessage>,
    /// Project-scoped PNG evidence produced by observation tools. It is sent
    /// once on the next provider request and never persisted as user content.
    evidence_images: Vec<crate::chat_media::ChatImage>,
    pub media_root: Option<std::path::PathBuf>,
    streaming_message_index: Option<usize>,
    message_revision: u64,
    task_manager: AgentTaskManager,
    task_id: Option<AgentTaskId>,
    task_event_cursor: u64,
    review_mutations: bool,
}

impl AgentRuntime {
    pub fn new(config: OpenAiConfig) -> Self {
        Self {
            client: OpenAiClient::new(config),
            messages: Vec::new(),
            pending_calls: Vec::new(),
            status: AgentStatus::Done,
            last_error: None,
            events: Vec::new(),
            max_turns: 128,
            turn_count: 0,
            max_tool_calls: None,
            tool_calls_executed: 0,
            max_tool_result_chars: 16 * 1024,
            // Give the editor a presentation frame between consecutive
            // scene/asset mutations requested by the model.
            tool_execution_interval: Duration::from_millis(40),
            last_tool_execution: None,
            run_started_at: None,
            pending: None,
            pending_tool_execution: None,
            request_context: Vec::new(),
            evidence_images: Vec::new(),
            media_root: None,
            streaming_message_index: None,
            message_revision: 0,
            task_manager: AgentTaskManager::new(),
            task_id: None,
            task_event_cursor: 0,
            review_mutations: false,
        }
    }

    pub fn clear(&mut self) {
        let had_messages = !self.messages.is_empty();
        self.cancel_background();
        self.messages.clear();
        self.pending_calls.clear();
        self.status = AgentStatus::Done;
        self.last_error = None;
        self.events.clear();
        self.turn_count = 0;
        self.tool_calls_executed = 0;
        self.last_tool_execution = None;
        self.run_started_at = None;
        self.pending_tool_execution = None;
        self.request_context.clear();
        self.evidence_images.clear();
        self.streaming_message_index = None;
        self.cancel_active_task();
        if had_messages {
            self.bump_message_revision();
        }
    }

    pub fn set_system_prompt(&mut self, prompt: impl Into<String>) {
        let prompt = prompt.into();
        if let Some(first) = self.messages.first_mut() {
            if first.role == MessageRole::System {
                if first.content == prompt {
                    return;
                }
                first.content = prompt;
                self.bump_message_revision();
                return;
            }
        }
        self.messages.insert(0, ChatMessage::system(&prompt));
        self.bump_message_revision();
    }

    /// Seed the next provider request with the result of a native read tool.
    ///
    /// The project snapshot is therefore represented as an actual tool
    /// exchange instead of being duplicated in the system prompt or rendered
    /// as a user-visible chat message. The context is consumed by the first
    /// request of the run and is not persisted in the chat history.
    pub fn set_initial_tool_context(
        &mut self,
        name: impl Into<String>,
        arguments: Value,
        result: AgentToolResult,
    ) {
        let call_id = format!("context-{}", Uuid::new_v4());
        let arguments = serde_json::to_string(&arguments).unwrap_or_else(|_| "{}".to_string());
        let assistant = OpenAiMessage {
            role: "assistant".to_string(),
            content: serde_json::json!(" "),
            tool_calls: Some(vec![ToolCall {
                id: call_id.clone(),
                call_type: "function".to_string(),
                function: FunctionCall {
                    name: name.into(),
                    arguments,
                },
            }]),
            tool_call_id: None,
            reasoning_content: None,
            reasoning: None,
            reasoning_details: Vec::new(),
        };
        let tool = OpenAiMessage {
            role: "tool".to_string(),
            content: Value::String(result.model_content(self.max_tool_result_chars)),
            tool_calls: None,
            tool_call_id: Some(call_id),
            reasoning_content: None,
            reasoning: None,
            reasoning_details: Vec::new(),
        };
        self.request_context = vec![assistant, tool];
    }

    /// Start a new agent run. The user message is added immediately and a
    /// background thread is spawned for the first API call. Returns immediately.
    /// Call `poll()` each frame to advance the state machine.
    pub fn start_run(
        &mut self,
        user_message: impl Into<String>,
        tools: &[OpenAiTool],
        execution_mode: ToolExecutionMode,
    ) {
        self.start_run_with_images(user_message, Vec::new(), tools, execution_mode);
    }

    pub fn start_run_with_images(
        &mut self, user_message: impl Into<String>, images: Vec<crate::chat_media::ChatImage>,
        tools: &[OpenAiTool], execution_mode: ToolExecutionMode,
    ) {
        let content = user_message.into();
        self.cancel_background();
        self.pending_calls.clear();
        self.pending_tool_execution = None;
        self.last_error = None;
        self.events.clear();
        self.turn_count = 0;
        self.tool_calls_executed = 0;
        self.last_tool_execution = None;
        self.run_started_at = Some(Instant::now());
        self.streaming_message_index = None;
        self.evidence_images.clear();
        self.start_task();
        self.repair_orphaned_tool_call_history();
        let mut user = ChatMessage::user(&content);
        user.images = images;
        self.push_message(user);
        self.spawn_next_request(tools, execution_mode);
        self.sync_task();
    }

    /// Sets limits for one editor request. `max_tool_calls = None` keeps the
    /// tool-call budget unlimited for power users while `max_turns` remains a
    /// high emergency guard against a provider loop.
    pub fn set_run_limits(
        &mut self,
        max_turns: usize,
        max_tool_calls: Option<usize>,
        max_tool_result_chars: usize,
    ) {
        self.max_turns = max_turns.max(1);
        self.max_tool_calls = max_tool_calls.map(|limit| limit.max(1));
        self.max_tool_result_chars = max_tool_result_chars.max(256);
    }

    /// Controls the minimum interval between editor-thread tool mutations.
    /// A zero duration is useful for deterministic headless callers.
    pub fn set_tool_execution_interval(&mut self, interval: Duration) {
        self.tool_execution_interval = interval;
    }

    /// Poll the runtime to advance the state machine. Must be called each frame.
    /// `executor` is only needed when tool calls must be executed (active mode).
    /// Returns the current `AgentStatus`.
    pub fn poll(&mut self, executor: Option<&mut dyn ToolExecutor>) -> AgentStatus {
        let status = match self.status {
            AgentStatus::Thinking => self.check_pending(executor),
            AgentStatus::ExecutingTools => self.execute_next_tool(executor),
            AgentStatus::AwaitingApproval | AgentStatus::Done | AgentStatus::Error => {
                self.status.clone()
            }
        };
        self.sync_task();
        status
    }

    /// Check if the background thread is done and process the response.
    fn check_pending(&mut self, executor: Option<&mut dyn ToolExecutor>) -> AgentStatus {
        const MAX_DELTAS_PER_POLL: usize = 32;
        if self.pending.is_none() {
            self.status = AgentStatus::Done;
            return AgentStatus::Done;
        }
        let mut stream_ended = false;
        for _ in 0..MAX_DELTAS_PER_POLL {
            match self.pending.as_ref().unwrap().stream_rx.try_recv() {
                Ok(StreamDelta::Text(text)) => self.append_stream_delta(&text),
                Ok(StreamDelta::Reasoning(text)) => self.append_reasoning_delta(&text),
                Err(mpsc::TryRecvError::Empty) => break,
                Err(mpsc::TryRecvError::Disconnected) => { stream_ended = true; break; }
            }
        }
        if !stream_ended { return AgentStatus::Thinking; }
        let pending = self.pending.as_ref().unwrap();

        let result_lock = pending.result.lock().unwrap();
        let result = match result_lock.as_ref() {
            Some(r) => r.clone(),
            None => return AgentStatus::Thinking, // still running
        };
        drop(result_lock);

        // Thread is done; take ownership of the pending data.
        let PendingResponse {
            tools,
            execution_mode,
            ..
        } = self.pending.take().unwrap();

        match result {
            Ok(message) => self.handle_assistant_message(message, executor, &tools, execution_mode),
            Err(error) => {
                let error = friendly_provider_error(error);
                self.last_error = Some(error.clone());
                self.status = AgentStatus::Error;
                let error_message = format!("Error: {}", error);
                if let Some(index) = self.streaming_message_index.take() {
                    let updated = if let Some(message) = self.messages.get_mut(index) {
                        message.content = error_message;
                        message.tool_calls = None;
                        message.reasoning = None;
                        true
                    } else {
                        false
                    };
                    if updated {
                        self.bump_message_revision();
                    }
                } else {
                    self.push_message(ChatMessage::assistant(&error_message));
                }
                self.emit(AgentEvent::StatusChanged(AgentStatus::Error, Some(error)));
                AgentStatus::Error
            }
        }
    }

    /// Process an assistant message (text or tool calls).
    fn handle_assistant_message(
        &mut self,
        message: OpenAiMessage,
        executor: Option<&mut dyn ToolExecutor>,
        tools: &[OpenAiTool],
        execution_mode: ToolExecutionMode,
    ) -> AgentStatus {
        let reasoning_text = message.reasoning_content.clone().or(message.reasoning.clone())
            .unwrap_or_else(|| message.reasoning_details.iter().filter_map(|detail|
                detail.get("text").or_else(|| detail.get("summary")).and_then(Value::as_str)
            ).collect::<Vec<_>>().join(""));
        let completed_reasoning = (!reasoning_text.is_empty() || !message.reasoning_details.is_empty())
            .then(|| ProviderReasoning {
                endpoint: self.client.config.base_url.clone(),
                model: self.client.config.model.clone(),
                transport: self.client.config.reasoning_transport(),
                text: reasoning_text,
                details: message.reasoning_details.clone(),
            });
        let text = match &message.content {
            Value::String(text) => text.clone(),
            Value::Null => String::new(),
            other => other.to_string(),
        };

        if let Some(tool_calls) = message.tool_calls.clone() {
            self.upsert_assistant_message(
                text,
                Some(serde_json::to_value(&tool_calls).unwrap_or_else(|_| serde_json::Value::Null)),
            );
            self.set_last_reasoning(completed_reasoning);

            if let Some(max_tool_calls) = self.max_tool_calls {
                let remaining_calls = max_tool_calls.saturating_sub(self.tool_calls_executed);
                if tool_calls.len() > remaining_calls {
                    let error = format!(
                        "The agent requested {} tool calls, but this run has a remaining budget of {}.",
                        tool_calls.len(),
                        remaining_calls
                    );
                    let requested_calls = tool_calls.len();
                    // Keep the provider conversation well-formed even when the
                    // model proposes a plan larger than the remaining budget.
                    // An assistant tool-call message must be followed by one
                    // tool result per call; otherwise a later "continue" sends
                    // an orphaned tool-call message and providers reject it with
                    // errors such as "tool call result does not follow tool call".
                    self.pending_calls = tool_calls
                        .into_iter()
                        .map(|call| {
                            let mut pending = PendingToolCall::new(
                                call.id,
                                call.function.name,
                                Value::Null,
                            );
                            pending.result = Some(
                                serde_json::json!({
                                    "ok": false,
                                    "summary": "Tool call rejected because the run budget was exceeded.",
                                    "error": {
                                        "code": "tool_call_budget_exceeded",
                                        "message": error.as_str(),
                                        "remaining_calls": remaining_calls,
                                        "requested_calls": requested_calls
                                    }
                                })
                                .to_string(),
                            );
                            pending
                        })
                        .collect();
                    self.flush_pending_as_tool_messages();
                    self.pending_calls.clear();
                    self.last_error = Some(error.clone());
                    self.status = AgentStatus::Error;
                    self.push_message(ChatMessage::assistant(&error));
                    self.emit(AgentEvent::StatusChanged(AgentStatus::Error, Some(error)));
                    return AgentStatus::Error;
                }
            }

            let pending: Vec<PendingToolCall> = tool_calls
                .into_iter()
                .map(
                    |call| match serde_json::from_str(&call.function.arguments) {
                        Ok(arguments) => {
                            PendingToolCall::new(call.id, call.function.name, arguments)
                        }
                        Err(error) => {
                            let mut pending = PendingToolCall::new(
                                call.id,
                                call.function.name,
                                serde_json::Value::Null,
                            );
                            pending.argument_error =
                                Some(format!("Tool arguments are not valid JSON: {error}"));
                            pending
                        }
                    },
                )
                .collect();

            self.pending_calls = pending.clone();
            self.emit(AgentEvent::ToolCallsRequested(pending));

            if executor.is_some() {
                self.begin_tool_execution(tools, execution_mode);
                AgentStatus::ExecutingTools
            } else {
                let error = "No native tool executor is available for this Agent run.".to_string();
                self.last_error = Some(error.clone());
                self.status = AgentStatus::Error;
                self.emit(AgentEvent::StatusChanged(AgentStatus::Error, Some(error)));
                AgentStatus::Error
            }
        } else {
            self.upsert_assistant_message(text, None);
            self.set_last_reasoning(completed_reasoning);
            self.status = AgentStatus::Done;
            self.emit(AgentEvent::StatusChanged(AgentStatus::Done, None));
            AgentStatus::Done
        }
    }

    /// Spawn a background thread for the next API request.
    fn spawn_next_request(&mut self, tools: &[OpenAiTool], execution_mode: ToolExecutionMode) {
        self.turn_count += 1;
        if self.turn_count > self.max_turns {
            let error = "Step safety limit reached; this task is not complete. Continue to inspect the saved state and resume without replaying completed mutations.".to_string();
            self.last_error = Some(error.clone());
            self.push_message(ChatMessage::assistant(&error));
            self.status = AgentStatus::Error;
            self.emit(AgentEvent::StatusChanged(AgentStatus::Error, Some(error)));
            return;
        }

        let mut openai_messages: Vec<OpenAiMessage> = self.messages.iter().map(|message| {
            let mut outbound = message_to_openai(message);
            if message.reasoning.as_ref().is_some_and(|reasoning|
                reasoning.endpoint != self.client.config.base_url || reasoning.model != self.client.config.model
                    || reasoning.transport != self.client.config.reasoning_transport()
            ) {
                outbound.reasoning_content = None;
                outbound.reasoning = None;
                outbound.reasoning_details.clear();
            }
            outbound
        }).collect();
        openai_messages.extend(std::mem::take(&mut self.request_context));

        if self.client.is_deepseek() && !tools.is_empty() {
            for message in &mut openai_messages {
                if message.role == "assistant" && message.reasoning_content.is_none() {
                    message.reasoning_content = Some(String::new());
                }
            }
        }

        let client = self.client.clone();
        let media_root = self.media_root.clone();
        // Only images from the latest user turn are sent. A text-only turn
        // must not silently resend an older private image.
        let last_images = crate::chat_media::latest_user_images(&self.messages);
        // Tool-produced evidence is ephemeral and sent once. It never enters
        // persisted chat history or a later text-only turn.
        let evidence_images = if crate::model_capabilities::vision_support(
            &self.client.config.model,
        )
        .allows_attempt()
        {
            std::mem::take(&mut self.evidence_images)
        } else {
            self.evidence_images.clear();
            Vec::new()
        };
        let result = Arc::new(Mutex::new(None));
        let result_clone = result.clone();
        let (stream_tx, stream_rx) = mpsc::sync_channel(128);
        let tools_owned = tools.to_vec();

        let handle = std::thread::Builder::new()
            .name("agent-api-call".into())
            .spawn(move || {
                if let Some((index, images)) = last_images {
                    let loaded = (|| -> Result<Vec<Value>, String> {
                        let root = media_root.as_deref().ok_or("No project is bound to image attachments.")?;
                        if images.len() > crate::chat_media::MAX_IMAGES { return Err("Too many image attachments.".into()); }
                        let text = openai_messages[index].content.as_str().unwrap_or("");
                        let mut parts = Vec::new();
                        if !text.is_empty() { parts.push(serde_json::json!({"type":"text", "text":text})); }
                        for image in &images { parts.push(crate::chat_media::image_part(root, image)?); }
                        Ok(parts)
                    })();
                    match loaded {
                        Ok(parts) => openai_messages[index].content = Value::Array(parts),
                        Err(error) => { *result_clone.lock().unwrap() = Some(Err(error)); return; }
                    }
                }
                if !evidence_images.is_empty() {
                    if let Some(root) = media_root.as_deref() {
                        let mut parts = vec![serde_json::json!({
                            "type": "text",
                            "text": "Visual evidence from the latest scene or viewport observation. Inspect composition, scale, lighting, readability, and visible defects. This image is evidence, not a replacement for structural verification."
                        })];
                        for image in evidence_images.iter().take(crate::chat_media::MAX_IMAGES) {
                            if let Ok(part) = crate::chat_media::image_part(root, image) {
                                parts.push(part);
                            }
                        }
                        if parts.len() > 1 {
                            openai_messages.push(OpenAiMessage {
                                role: "user".to_string(),
                                content: Value::Array(parts),
                                tool_calls: None,
                                tool_call_id: None,
                                reasoning_content: None,
                                reasoning: None,
                                reasoning_details: Vec::new(),
                            });
                        }
                    }
                }
                let response = if client.config.streaming {
                    client.chat_stream_with_reasoning(&openai_messages, Some(&tools_owned), |delta| {
                        let _ = stream_tx.send(delta);
                    })
                } else {
                    client.chat(&openai_messages, Some(&tools_owned))
                };
                *result_clone.lock().unwrap() = Some(response);
            })
            .expect("failed to spawn agent api thread");

        self.pending = Some(PendingResponse {
            result,
            stream_rx,
            _handle: handle,
            tools: tools.to_vec(),
            execution_mode,
        });
        self.status = AgentStatus::Thinking;
    }

    /// Queue bounded visual evidence for the next provider request.
    fn enqueue_evidence_images(&mut self, images: Vec<crate::chat_media::ChatImage>) {
        for image in images {
            if self
                .evidence_images
                .iter()
                .any(|existing| existing.file == image.file)
            {
                continue;
            }
            self.evidence_images.push(image);
            if self.evidence_images.len() >= crate::chat_media::MAX_IMAGES {
                break;
            }
        }
        while self.evidence_images.len() > crate::chat_media::MAX_IMAGES {
            self.evidence_images.remove(0);
        }
    }

    /// Cancel any running background request.
    fn cancel_background(&mut self) {
        self.pending.take();
    }

    /// Approve pending tool calls and continue the run.
    /// Returns the new status; call `poll()` next frame to continue.
    pub fn approve_pending(&mut self, tools: &[OpenAiTool], _executor: &mut dyn ToolExecutor) {
        if self.status != AgentStatus::AwaitingApproval {
            return;
        }
        for call in &mut self.pending_calls {
            call.approved = Some(true);
        }
        self.begin_tool_execution(tools, ToolExecutionMode::Apply);
    }

    /// Deny pending tool calls and continue the run.
    pub fn deny_pending(
        &mut self,
        reason: impl Into<String>,
        tools: &[OpenAiTool],
        _executor: &mut dyn ToolExecutor,
    ) {
        if self.status != AgentStatus::AwaitingApproval {
            return;
        }
        let reason = reason.into();
        for call in &mut self.pending_calls {
            call.approved = Some(false);
            call.result = Some(reason.clone());
        }
        self.flush_pending_as_tool_messages();
        self.pending_calls.clear();
        self.spawn_next_request(tools, ToolExecutionMode::Inspect);
    }

    pub fn cancel(&mut self) {
        self.cancel_background();
        self.pending_calls.clear();
        self.pending_tool_execution = None;
        self.request_context.clear();
        self.status = AgentStatus::Done;
        self.turn_count = 0;
        self.tool_calls_executed = 0;
        self.last_tool_execution = None;
        self.run_started_at = None;
        self.streaming_message_index = None;
        self.cancel_active_task();
        self.sync_task_events();
    }

    pub fn set_review_mutations(&mut self, required: bool) {
        self.review_mutations = required;
        // Disabling review must also release an already suspended queue.
        // Discard only unapproved review snapshots; explicit decisions retain
        // their revision guard and denied operations remain denied.
        if !required && self.status == AgentStatus::AwaitingApproval {
            for call in &mut self.pending_calls {
                if call.result.is_none() && call.approved.is_none() {
                    call.expected_revision = None;
                    call.review_preview = None;
                }
            }
            self.status = AgentStatus::ExecutingTools;
            self.emit(AgentEvent::StatusChanged(AgentStatus::ExecutingTools, None));
            self.sync_task();
        }
    }

    /// Resume the retained operation queue after one scoped decision.
    pub fn decide_call(&mut self, id: &str, approved: bool) {
        if let Some(call) = self
            .pending_calls
            .iter_mut()
            .find(|call| call.id == id && call.result.is_none())
        {
            call.approved = Some(approved);
            if self.status == AgentStatus::AwaitingApproval {
                self.status = AgentStatus::ExecutingTools;
                self.emit(AgentEvent::StatusChanged(AgentStatus::ExecutingTools, None));
            }
        }
    }

    fn append_stream_delta(&mut self, delta: &str) {
        if delta.is_empty() {
            return;
        }
        let index = if let Some(index) = self.streaming_message_index {
            index
        } else {
            self.push_message(ChatMessage::assistant(""));
            let index = self.messages.len().saturating_sub(1);
            self.streaming_message_index = Some(index);
            index
        };
        let changed = if let Some(message) = self.messages.get_mut(index) {
            message.content.push_str(delta);
            true
        } else {
            false
        };
        if changed {
            self.bump_message_revision();
        }
    }

    fn append_reasoning_delta(&mut self, delta: &str) {
        if delta.is_empty() { return; }
        let index = if let Some(index) = self.streaming_message_index {
            index
        } else {
            self.push_message(ChatMessage::assistant(""));
            let index = self.messages.len().saturating_sub(1);
            self.streaming_message_index = Some(index);
            index
        };
        if let Some(message) = self.messages.get_mut(index) {
            let reasoning = message.reasoning.get_or_insert_with(|| ProviderReasoning {
                endpoint: self.client.config.base_url.clone(),
                model: self.client.config.model.clone(),
                transport: self.client.config.reasoning_transport(),
                text: String::new(),
                details: Vec::new(),
            });
            reasoning.text.push_str(delta);
            self.bump_message_revision();
        }
    }

    fn set_last_reasoning(&mut self, reasoning: Option<ProviderReasoning>) {
        if let Some(message) = self.messages.last_mut() {
            if message.role == MessageRole::Assistant && message.reasoning != reasoning {
                message.reasoning = reasoning;
                self.bump_message_revision();
            }
        }
    }

    fn upsert_assistant_message(&mut self, text: String, tool_calls: Option<Value>) {
        if let Some(index) = self.streaming_message_index.take() {
            if index < self.messages.len() {
                let changed = {
                    let message = &mut self.messages[index];
                    let changed = message.content != text || message.tool_calls != tool_calls;
                    message.content = text;
                    message.tool_calls = tool_calls;
                    changed
                };
                if changed {
                    self.bump_message_revision();
                }
                return;
            }
        }

        self.push_message(ChatMessage {
            id: Uuid::new_v4(),
            role: MessageRole::Assistant,
            content: text,
            images: Vec::new(),
            reasoning: None,
            timestamp: Utc::now(),
            tool_calls,
        });
    }

    fn begin_tool_execution(&mut self, tools: &[OpenAiTool], execution_mode: ToolExecutionMode) {
        self.pending_tool_execution = Some(PendingToolExecution {
            tools: tools.to_vec(),
            execution_mode,
        });
        self.last_tool_execution = None;
        self.status = AgentStatus::ExecutingTools;
        self.emit(AgentEvent::StatusChanged(AgentStatus::ExecutingTools, None));
    }

    /// Executes at most one tool call per scheduling interval so scene
    /// mutations cannot monopolize consecutive UI frames. Calls remain
    /// ordered because they share the editor executor.
    fn execute_next_tool(&mut self, executor: Option<&mut dyn ToolExecutor>) -> AgentStatus {
        let Some(executor) = executor else {
            let error = "No tool executor is available for queued agent work.".to_string();
            self.last_error = Some(error.clone());
            self.status = AgentStatus::Error;
            self.emit(AgentEvent::StatusChanged(AgentStatus::Error, Some(error)));
            return AgentStatus::Error;
        };

        if self
            .last_tool_execution
            .is_some_and(|last| last.elapsed() < self.tool_execution_interval)
        {
            return AgentStatus::ExecutingTools;
        }

        let max_tool_result_chars = self.max_tool_result_chars;
        let execution_mode = self
            .pending_tool_execution
            .as_ref()
            .map(|pending| pending.execution_mode)
            .unwrap_or(ToolExecutionMode::Inspect);
        if self.review_mutations && execution_mode == ToolExecutionMode::Apply {
            if self
                .pending_calls
                .iter()
                .find(|call| call.result.is_none())
                .is_some_and(|call| {
                    call.argument_error.is_none()
                        && call.approved.is_none()
                        && executor.kind(&call.name) == AgentToolKind::Mutation
                })
            {
                if let Some(call) = self
                    .pending_calls
                    .iter_mut()
                    .find(|call| call.result.is_none())
                {
                    call.expected_revision = executor.revision();
                    call.review_preview =
                        executor.preview_for_review(&call.id, &call.name, call.arguments.clone());
                }
                self.status = AgentStatus::AwaitingApproval;
                self.emit(AgentEvent::StatusChanged(
                    AgentStatus::AwaitingApproval,
                    None,
                ));
                return AgentStatus::AwaitingApproval;
            }
        }
        self.last_tool_execution = Some(Instant::now());
        let executed = self
            .pending_calls
            .iter_mut()
            .find(|call| call.result.is_none())
            .and_then(|call| {
                let (result, images) = if call.approved == Some(false) {
                    ("Denied by user.".to_string(), Vec::new())
                } else if let Some(error) = call.argument_error.as_deref() {
                    (
                        serde_json::json!({
                            "ok": false,
                            "summary": "The provider returned malformed tool arguments.",
                            "error": {
                                "code": "invalid_tool_json",
                                "message": error,
                                "suggestion": "Retry the tool call with one JSON object that matches the advertised schema."
                            }
                        })
                        .to_string(),
                        Vec::new(),
                    )
                } else {
                    if execution_mode == ToolExecutionMode::Inspect
                        && executor.kind(&call.name) == AgentToolKind::Mutation
                    {
                        (
                            format!(
                                "{{\"ok\":false,\"summary\":\"Tool '{}' is not available in Inspect mode.\"}}",
                                call.name
                            ),
                            Vec::new(),
                        )
                    } else {
                        let output = if executor.is_pending(&call.id) {
                            executor.poll_pending(&call.id)?
                        } else if call.expected_revision.is_some() && call.expected_revision != executor.revision() {
                            Ok(AgentToolResult { ok: false,
                                summary: "The document changed after review. Inspect the current revision before proposing another operation.".into(),
                                data: serde_json::json!({"error": {"code": "revision_conflict"}, "expected_revision": call.expected_revision, "current_revision": executor.revision()}),
                                references: Vec::new(), changed: false, revision: executor.revision(), diff: None,
                                verification: None, warnings: Vec::new(), details: Vec::new(), preview: false })
                        } else {
                            let output = executor.execute(&call.id, &call.name, call.arguments.clone(), execution_mode);
                            if executor.is_pending(&call.id) { return None; }
                            output
                        };
                        match output {
                            Ok(output) => (
                                output.model_content(max_tool_result_chars),
                                visual_evidence_from_result(&output),
                            ),
                            Err(error) => (format!("Error: {}", error), Vec::new()),
                        }
                    }
                };
                let result = truncate_tool_result(result, max_tool_result_chars);
                call.result = Some(result.clone());
                Some((call.id.clone(), result, images))
            });
        if let Some((id, result, images)) = executed {
            self.tool_calls_executed = self.tool_calls_executed.saturating_add(1);
            self.enqueue_evidence_images(images);
            self.emit(AgentEvent::ToolResult { id, result });
            return AgentStatus::ExecutingTools;
        }
        if self.pending_calls.iter().any(|call| call.result.is_none()) {
            return AgentStatus::ExecutingTools;
        }

        let Some(continuation) = self.pending_tool_execution.take() else {
            self.status = AgentStatus::Done;
            return AgentStatus::Done;
        };
        self.flush_pending_as_tool_messages();
        self.pending_calls.clear();
        self.spawn_next_request(&continuation.tools, continuation.execution_mode);
        self.status.clone()
    }

    fn flush_pending_as_tool_messages(&mut self) {
        let entries: Vec<(String, String, String)> = self
            .pending_calls
            .iter()
            .map(|call| {
                (
                    call.id.clone(),
                    call.name.clone(),
                    call.result
                        .clone()
                        .unwrap_or_else(|| "No result recorded.".to_string()),
                )
            })
            .collect();
        for (id, name, result) in entries {
            self.push_message(ChatMessage {
                id: Uuid::new_v4(),
                role: MessageRole::Tool,
                content: result,
                images: Vec::new(),
                reasoning: None,
                timestamp: Utc::now(),
                tool_calls: Some(serde_json::json!({
                    "tool_call_id": id,
                    "name": name,
                })),
            });
        }
    }

    /// Remove invalid tool-call exchanges retained by older runs.
    ///
    /// A provider requires every assistant tool call to be followed by one
    /// matching tool result. Older runtime versions could persist the
    /// assistant message before rejecting an oversized plan, leaving a
    /// conversation that fails again when the user presses Continue. Before a
    /// new request, preserve valid exchanges and neutralize incomplete ones.
    fn repair_orphaned_tool_call_history(&mut self) {
        let original = std::mem::take(&mut self.messages);
        let mut repaired = Vec::with_capacity(original.len());
        let mut index = 0;
        let mut changed = false;

        while index < original.len() {
            let message = original[index].clone();
            if message.role == MessageRole::Assistant {
                if let Some(calls) = message
                    .tool_calls
                    .as_ref()
                    .and_then(|value| serde_json::from_value::<Vec<ToolCall>>(value.clone()).ok())
                {
                    let mut following_tools = Vec::new();
                    while index + 1 + following_tools.len() < original.len()
                        && original[index + 1 + following_tools.len()].role == MessageRole::Tool
                    {
                        following_tools.push(original[index + 1 + following_tools.len()].clone());
                    }

                    let expected_ids = calls
                        .iter()
                        .map(|call| call.id.clone())
                        .collect::<HashSet<_>>();
                    let actual_ids = following_tools
                        .iter()
                        .filter_map(|tool| {
                            tool.tool_calls
                                .as_ref()
                                .and_then(|value| value.get("tool_call_id"))
                                .and_then(Value::as_str)
                                .map(str::to_string)
                        })
                        .collect::<HashSet<_>>();
                    let valid = following_tools.len() == calls.len()
                        && actual_ids.len() == calls.len()
                        && actual_ids == expected_ids;

                    if valid {
                        repaired.push(message);
                        repaired.extend(following_tools);
                        index += 1 + calls.len();
                        continue;
                    }

                    let mut sanitized = message;
                    sanitized.tool_calls = None;
                    repaired.push(sanitized);
                    index += 1 + following_tools.len();
                    changed = true;
                    continue;
                }
            }

            if message.role == MessageRole::Tool {
                changed = true;
                index += 1;
                continue;
            }

            repaired.push(message);
            index += 1;
        }

        if changed {
            self.messages = repaired;
            self.bump_message_revision();
        } else {
            self.messages = original;
        }
    }

    fn push_message(&mut self, message: ChatMessage) {
        self.messages.push(message.clone());
        self.bump_message_revision();
        self.emit(AgentEvent::MessageAdded(message));
    }

    pub fn message_revision(&self) -> u64 {
        self.message_revision
    }

    /// Returns the current bounded lifecycle state for the active run.
    ///
    /// The snapshot is transport-neutral and can be exposed by native UI,
    /// attached CLI, or MCP without exposing the provider thread internals.
    pub fn task_snapshot(&self) -> Option<AgentTaskSnapshot> {
        self.task_id.and_then(|id| self.task_manager.snapshot(id))
    }

    /// Returns a retained task by id, including a completed or cancelled
    /// run. External attached clients use this with `task.list` pagination
    /// without being limited to the currently active run.
    pub fn task_snapshot_by_id(&self, id: AgentTaskId) -> Option<AgentTaskSnapshot> {
        self.task_manager.snapshot(id)
    }

    /// Returns task lifecycle events retained after `sequence`.
    pub fn task_events_since(&self, sequence: u64) -> Vec<AgentTaskEvent> {
        self.task_manager.events_since(sequence)
    }

    pub fn task_snapshots(&self) -> Vec<AgentTaskSnapshot> {
        self.task_manager.list()
    }

    /// Current user-visible work state. Terminal runs return `None` so idle
    /// editor frames do not keep rebuilding the Agent surface.
    pub fn activity_snapshot(&self) -> Option<AgentActivitySnapshot> {
        if !matches!(
            self.status,
            AgentStatus::Thinking | AgentStatus::ExecutingTools | AgentStatus::AwaitingApproval
        ) {
            return None;
        }
        let pending_tools = self
            .pending_calls
            .iter()
            .filter(|call| call.result.is_none())
            .count();
        Some(AgentActivitySnapshot {
            status: self.status.clone(),
            elapsed_seconds: self
                .run_started_at
                .map(|started| started.elapsed().as_secs())
                .unwrap_or(0),
            current_tool: self
                .pending_calls
                .iter()
                .find(|call| call.result.is_none())
                .map(|call| call.name.clone()),
            completed_tools: self.tool_calls_executed,
            pending_tools,
            total_tools: self.tool_calls_executed.saturating_add(pending_tools),
            turn: self.turn_count,
        })
    }

    fn start_task(&mut self) {
        self.cancel_active_task();
        self.task_event_cursor = self
            .task_manager
            .events_since(0)
            .last()
            .map_or(0, |event| event.sequence);
        let handle = self
            .task_manager
            .start_running("agent.run", "Agent run", None, true);
        self.task_id = Some(handle.id());
        self.task_event_cursor = self
            .task_manager
            .snapshot(handle.id())
            .map_or(self.task_event_cursor, |snapshot| {
                snapshot.sequence.saturating_sub(1)
            });
    }

    fn cancel_active_task(&mut self) {
        if let Some(id) = self.task_id {
            let _ = self.task_manager.cancel(id);
        }
    }

    fn sync_task(&mut self) {
        let Some(id) = self.task_id else {
            return;
        };

        let (status, stage, completed, total, message) = match self.status {
            AgentStatus::Thinking => (
                AgentTaskStatus::Running,
                "thinking",
                self.turn_count.saturating_sub(1) as u32,
                None,
                "Waiting for the model response.".to_string(),
            ),
            AgentStatus::ExecutingTools => (
                AgentTaskStatus::Running,
                "executing_tools",
                self.tool_calls_executed as u32,
                Some(
                    self.tool_calls_executed.saturating_add(
                        self.pending_calls
                            .iter()
                            .filter(|call| call.result.is_none())
                            .count(),
                    ) as u32,
                ),
                self.pending_calls
                    .iter()
                    .find(|call| call.result.is_none())
                    .map(|call| {
                        format!(
                            "Using {}; {} tool call(s) completed, {} pending.",
                            call.name,
                            self.tool_calls_executed,
                            self.pending_calls
                                .iter()
                                .filter(|call| call.result.is_none())
                                .count()
                        )
                    })
                    .unwrap_or_else(|| {
                        format!("{} tool call(s) completed.", self.tool_calls_executed)
                    }),
            ),
            AgentStatus::AwaitingApproval => (
                AgentTaskStatus::WaitingApproval,
                "waiting_approval",
                0,
                Some(self.pending_calls.len() as u32),
                "Waiting for tool approval.".to_string(),
            ),
            AgentStatus::Done => (
                AgentTaskStatus::Completed,
                "completed",
                self.turn_count as u32,
                None,
                "Agent run completed.".to_string(),
            ),
            AgentStatus::Error => (
                AgentTaskStatus::Failed,
                "failed",
                self.tool_calls_executed as u32,
                None,
                self.last_error
                    .clone()
                    .unwrap_or_else(|| "Agent run failed.".to_string()),
            ),
        };

        let current = self.task_manager.snapshot(id);
        if current.as_ref().is_some_and(|snapshot| {
            snapshot.status == status
                && snapshot.progress.stage == stage
                && snapshot.progress.completed == completed
                && snapshot.progress.total == total
                && snapshot.progress.message == message
        }) {
            self.sync_task_events();
            return;
        }

        match status {
            AgentTaskStatus::Completed => {
                if current
                    .as_ref()
                    .is_some_and(|snapshot| !snapshot.status.is_terminal())
                {
                    let _ = self.task_manager.complete(
                        id,
                        Some(serde_json::json!({
                            "turns": self.turn_count,
                            "tool_calls": self.tool_calls_executed,
                            "messages": self.messages.len(),
                        })),
                    );
                }
            }
            AgentTaskStatus::Failed => {
                if current
                    .as_ref()
                    .is_some_and(|snapshot| !snapshot.status.is_terminal())
                {
                    let _ = self.task_manager.fail(id, message);
                }
            }
            AgentTaskStatus::WaitingApproval => {
                let _ = self.task_manager.set_status(id, status);
                let _ = self
                    .task_manager
                    .update_progress(id, stage, completed, total, message);
            }
            AgentTaskStatus::Running => {
                let _ = self.task_manager.set_status(id, status);
                let _ = self
                    .task_manager
                    .update_progress(id, stage, completed, total, message);
            }
            AgentTaskStatus::Queued | AgentTaskStatus::Cancelled => {}
        }
        self.sync_task_events();
    }

    fn sync_task_events(&mut self) {
        let events = self.task_manager.events_since(self.task_event_cursor);
        for event in events {
            self.task_event_cursor = self.task_event_cursor.max(event.sequence);
            self.emit(AgentEvent::TaskProgress(event.task));
        }
    }

    fn bump_message_revision(&mut self) {
        self.message_revision = self.message_revision.wrapping_add(1);
    }

    fn emit(&mut self, event: AgentEvent) {
        self.events.push(event);
    }
}

fn message_to_openai(message: &ChatMessage) -> OpenAiMessage {
    let role = match message.role {
        MessageRole::User => "user",
        MessageRole::Assistant => "assistant",
        MessageRole::System => "system",
        MessageRole::Tool => "tool",
    }
    .to_string();

    let content = if message.content.is_empty() && message.role == MessageRole::Assistant {
        serde_json::json!(" ")
    } else {
        serde_json::json!(&message.content)
    };

    let (tool_calls, tool_call_id) = match message.role {
        MessageRole::Assistant => {
            let calls = message
                .tool_calls
                .as_ref()
                .and_then(|value| serde_json::from_value::<Vec<ToolCall>>(value.clone()).ok());
            (calls, None)
        }
        MessageRole::Tool => {
            let id = message
                .tool_calls
                .as_ref()
                .and_then(|value| value.get("tool_call_id"))
                .and_then(|value| value.as_str())
                .map(String::from);
            (None, id)
        }
        _ => (None, None),
    };

    OpenAiMessage {
        role,
        content,
        tool_calls,
        tool_call_id,
        reasoning_content: message.reasoning.as_ref().filter(|r| (r.endpoint.trim_end_matches('/').starts_with("https://api.deepseek.com") || r.endpoint.starts_with("https://opencode.ai/inference/")) && r.details.is_empty()).map(|r| r.text.clone()),
        reasoning: message.reasoning.as_ref().filter(|r| r.endpoint.starts_with("https://openrouter.ai/") && r.details.is_empty()).map(|r| r.text.clone()),
        reasoning_details: message.reasoning.as_ref().map(|r| r.details.clone()).unwrap_or_default(),
    }
}

fn friendly_provider_error(error: String) -> String {
    if crate::model_capabilities::is_image_rejection(&error) {
        format!(
            "The selected model rejected image input. Structural tools and artifacts remain available; choose a vision-capable model to inspect captures. Provider detail: {error}"
        )
    } else {
        error
    }
}

fn visual_evidence_from_result(result: &AgentToolResult) -> Vec<crate::chat_media::ChatImage> {
    let Some(items) = result.data.get("visual_evidence").and_then(Value::as_array) else {
        return Vec::new();
    };
    let mut images = Vec::new();
    for item in items {
        let Some(file) = item.get("file").and_then(Value::as_str) else {
            continue;
        };
        if !file.ends_with(".png") {
            continue;
        }
        let width = item.get("width").and_then(Value::as_u64).unwrap_or(0) as u32;
        let height = item.get("height").and_then(Value::as_u64).unwrap_or(0) as u32;
        if images
            .iter()
            .any(|image: &crate::chat_media::ChatImage| image.file == file)
        {
            continue;
        }
        images.push(crate::chat_media::ChatImage {
            file: file.to_string(),
            width,
            height,
        });
        if images.len() >= crate::chat_media::MAX_IMAGES {
            break;
        }
    }
    images
}

fn truncate_tool_result(mut result: String, max_chars: usize) -> String {
    if result.chars().count() <= max_chars {
        return result;
    }

    let truncate_at = result
        .char_indices()
        .nth(max_chars)
        .map(|(index, _)| index)
        .unwrap_or(result.len());
    result.truncate(truncate_at);
    result.push_str("\n[Tool output truncated by the editor budget.]");
    result
}

fn truncate_chars(value: String, max_chars: usize) -> String {
    if value.chars().count() <= max_chars {
        return value;
    }
    let mut truncated = value.chars().take(max_chars).collect::<String>();
    truncated.push_str("...");
    truncated
}

fn compact_json_value(value: Value, depth: usize, max_items: usize, max_string: usize) -> Value {
    if depth == 0 {
        return match value {
            Value::Array(values) => serde_json::json!({"count": values.len(), "truncated": true}),
            Value::Object(values) => {
                serde_json::json!({"fields": values.len(), "truncated": true})
            }
            Value::String(value) => Value::String(truncate_chars(value, max_string)),
            other => other,
        };
    }
    match value {
        Value::String(value) => Value::String(truncate_chars(value, max_string)),
        Value::Array(values) => {
            let total = values.len();
            let mut values = values
                .into_iter()
                .take(max_items)
                .map(|value| compact_json_value(value, depth - 1, max_items, max_string))
                .collect::<Vec<_>>();
            if total > max_items {
                values.push(serde_json::json!({
                    "truncated": true,
                    "remaining": total - max_items
                }));
            }
            Value::Array(values)
        }
        Value::Object(values) => Value::Object(
            values
                .into_iter()
                .take(max_items)
                .map(|(key, value)| {
                    (
                        key,
                        compact_json_value(value, depth - 1, max_items, max_string),
                    )
                })
                .collect(),
        ),
        other => other,
    }
}

pub fn assistant_with_tool_calls(text: &str, calls: &[ToolCall]) -> OpenAiMessage {
    OpenAiMessage {
        role: "assistant".to_string(),
        content: serde_json::json!(text),
        tool_calls: Some(calls.to_vec()),
        tool_call_id: None,
        reasoning_content: None,
        reasoning: None,
        reasoning_details: Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn capture_visual_evidence_is_bounded_and_deduplicated() {
        let result = AgentToolResult::success(
            "captured",
            serde_json::json!({
                "visual_evidence": [
                    {"file": ".aura_rafi/agent_artifacts/front.png", "width": 960, "height": 540},
                    {"file": ".aura_rafi/agent_artifacts/front.png", "width": 960, "height": 540},
                    {"file": ".aura_rafi/agent_artifacts/side.png", "width": 960, "height": 540},
                    {"file": "notes.txt", "width": 960, "height": 540}
                ]
            }),
        );
        let images = visual_evidence_from_result(&result);
        assert_eq!(images.len(), 2);
        assert_eq!(images[0].file, ".aura_rafi/agent_artifacts/front.png");
        assert_eq!(images[1].file, ".aura_rafi/agent_artifacts/side.png");
    }

    #[derive(Default)]
    struct RecordingExecutor {
        calls: Vec<String>,
    }

    impl ToolExecutor for RecordingExecutor {
        fn execute(
            &mut self,
            _call_id: &str,
            name: &str,
            _arguments: Value,
            _mode: ToolExecutionMode,
        ) -> Result<AgentToolResult, String> {
            self.calls.push(name.to_string());
            Ok(AgentToolResult::success(
                format!("completed {name}"),
                Value::Null,
            ))
        }
    }

    #[test]
    fn active_permissions_apply_without_review_and_keep_inspect_plan_boundaries() {
        #[derive(Default)]
        struct PermissionExecutor {
            modes: Vec<ToolExecutionMode>,
            previews: usize,
        }
        impl ToolExecutor for PermissionExecutor {
            fn preview_for_review(
                &mut self,
                _: &str,
                _: &str,
                _: Value,
            ) -> Option<AgentToolResult> {
                self.previews += 1;
                None
            }
            fn execute(
                &mut self,
                _: &str,
                _: &str,
                _: Value,
                mode: ToolExecutionMode,
            ) -> Result<AgentToolResult, String> {
                self.modes.push(mode);
                Ok(AgentToolResult::success("executed", Value::Null))
            }
        }
        for mode in [
            ToolExecutionMode::Apply,
            ToolExecutionMode::Inspect,
            ToolExecutionMode::Preview,
        ] {
            let mut runtime = AgentRuntime::new(OpenAiConfig::default());
            runtime.set_tool_execution_interval(Duration::ZERO);
            runtime.set_review_mutations(false);
            runtime.pending_calls = vec![PendingToolCall::new(
                "op",
                "scene_create",
                serde_json::json!({}),
            )];
            runtime.begin_tool_execution(&[], mode);
            let mut executor = PermissionExecutor::default();
            assert_eq!(
                runtime.poll(Some(&mut executor)),
                AgentStatus::ExecutingTools
            );
            assert_eq!(executor.previews, 0);
            if mode == ToolExecutionMode::Inspect {
                assert!(executor.modes.is_empty());
                assert!(runtime.pending_calls[0]
                    .result
                    .as_deref()
                    .unwrap()
                    .contains("not available in Inspect"));
            } else {
                assert_eq!(executor.modes, vec![mode]);
                assert!(runtime.pending_calls[0].approved.is_none());
            }
            runtime.cancel();
        }
    }

    #[test]
    fn active_permissions_resume_pending_review_without_replaying_or_ignoring_denials() {
        let mut runtime = AgentRuntime::new(OpenAiConfig::default());
        runtime.set_tool_execution_interval(Duration::ZERO);
        runtime.set_review_mutations(true);
        runtime.pending_calls = vec![
            PendingToolCall::new("pending", "scene_create", serde_json::json!({})),
            PendingToolCall::new("denied", "scene_delete", serde_json::json!({})),
            PendingToolCall::new("completed", "scene_update", serde_json::json!({})),
        ];
        runtime.pending_calls[1].approved = Some(false);
        runtime.pending_calls[2].result = Some("already completed".into());
        runtime.begin_tool_execution(&[], ToolExecutionMode::Apply);
        let mut executor = RecordingExecutor::default();
        assert_eq!(
            runtime.poll(Some(&mut executor)),
            AgentStatus::AwaitingApproval
        );
        runtime.pending_calls[0].expected_revision = Some(4);
        runtime.pending_calls[0].review_preview =
            Some(AgentToolResult::success("old preview", Value::Null));
        runtime.set_review_mutations(false);
        assert_eq!(runtime.status, AgentStatus::ExecutingTools);
        assert!(runtime.pending_calls[0].expected_revision.is_none());
        assert!(runtime.pending_calls[0].review_preview.is_none());
        runtime.poll(Some(&mut executor));
        runtime.poll(Some(&mut executor));
        assert_eq!(executor.calls, vec!["scene_create"]);
        assert_eq!(
            runtime.pending_calls[1].result.as_deref(),
            Some("Denied by user.")
        );
        assert_eq!(
            runtime.pending_calls[2].result.as_deref(),
            Some("already completed")
        );
        runtime.cancel();
    }

    #[test]
    fn agent_workspace_scoped_review_rejects_a_stale_revision() {
        struct ReviewedExecutor {
            revision: u64,
            executions: usize,
        }
        impl ToolExecutor for ReviewedExecutor {
            fn revision(&self) -> Option<u64> {
                Some(self.revision)
            }
            fn execute(
                &mut self,
                _: &str,
                _: &str,
                _: Value,
                _: ToolExecutionMode,
            ) -> Result<AgentToolResult, String> {
                self.executions += 1;
                self.revision += 1;
                Ok(AgentToolResult::success("applied", Value::Null))
            }
        }
        let mut runtime = AgentRuntime::new(OpenAiConfig::default());
        runtime.set_tool_execution_interval(Duration::ZERO);
        runtime.set_review_mutations(true);
        runtime.pending_calls = vec![
            PendingToolCall::new("first", "game_add", serde_json::json!({})),
            PendingToolCall::new("second", "game_move", serde_json::json!({})),
        ];
        runtime.begin_tool_execution(&[], ToolExecutionMode::Apply);
        let mut executor = ReviewedExecutor {
            revision: 4,
            executions: 0,
        };
        assert_eq!(
            runtime.poll(Some(&mut executor)),
            AgentStatus::AwaitingApproval
        );
        assert_eq!(runtime.pending_calls[0].expected_revision, Some(4));
        assert_eq!(executor.executions, 0);
        runtime.decide_call("first", true);
        runtime.poll(Some(&mut executor));
        assert_eq!(executor.executions, 1);
        assert_eq!(
            runtime.poll(Some(&mut executor)),
            AgentStatus::AwaitingApproval
        );
        assert_eq!(runtime.pending_calls[1].expected_revision, Some(5));
        executor.revision = 6;
        runtime.decide_call("second", true);
        runtime.poll(Some(&mut executor));
        assert_eq!(executor.executions, 1);
        assert!(runtime.pending_calls[1]
            .result
            .as_deref()
            .unwrap()
            .contains("revision_conflict"));
    }

    #[test]
    fn queued_tools_execute_one_call_per_poll() {
        let mut runtime = AgentRuntime::new(OpenAiConfig::default());
        runtime.set_tool_execution_interval(Duration::ZERO);
        runtime.pending_calls = vec![
            PendingToolCall::new("first", "game_add", serde_json::json!({})),
            PendingToolCall::new("second", "game_move", serde_json::json!({})),
        ];
        runtime.pending_tool_execution = Some(PendingToolExecution {
            tools: Vec::new(),
            execution_mode: ToolExecutionMode::Apply,
        });
        runtime.status = AgentStatus::ExecutingTools;
        let mut executor = RecordingExecutor::default();

        assert_eq!(
            runtime.poll(Some(&mut executor)),
            AgentStatus::ExecutingTools
        );
        assert_eq!(executor.calls, vec!["game_add"]);
        assert!(runtime.pending_calls[0].result.is_some());
        assert!(runtime.pending_calls[1].result.is_none());

        assert_eq!(
            runtime.poll(Some(&mut executor)),
            AgentStatus::ExecutingTools
        );
        assert_eq!(executor.calls, vec!["game_add", "game_move"]);
        assert!(runtime.pending_calls[1].result.is_some());
    }

    #[test]
    fn asynchronous_tool_is_started_once_and_only_completed_results_are_recorded() {
        #[derive(Default)]
        struct AsyncExecutor {
            starts: usize,
            polls: usize,
            pending: bool,
        }
        impl ToolExecutor for AsyncExecutor {
            fn is_pending(&self, _: &str) -> bool {
                self.pending
            }
            fn execute(
                &mut self,
                _: &str,
                _: &str,
                _: Value,
                _: ToolExecutionMode,
            ) -> Result<AgentToolResult, String> {
                self.starts += 1;
                self.pending = true;
                Ok(AgentToolResult::success("queued", Value::Null))
            }
            fn poll_pending(&mut self, _: &str) -> Option<Result<AgentToolResult, String>> {
                self.polls += 1;
                if self.polls < 2 {
                    return None;
                }
                self.pending = false;
                Some(Ok(AgentToolResult::success(
                    "real image ready",
                    serde_json::json!({"image_path":"assets/generated/image.png"}),
                )))
            }
        }
        let mut runtime = AgentRuntime::new(OpenAiConfig::default());
        runtime.set_tool_execution_interval(Duration::ZERO);
        runtime.pending_calls = vec![PendingToolCall::new(
            "image",
            "asset_generate_image",
            serde_json::json!({}),
        )];
        runtime.pending_tool_execution = Some(PendingToolExecution {
            tools: vec![],
            execution_mode: ToolExecutionMode::Apply,
        });
        let mut executor = AsyncExecutor::default();
        runtime.execute_next_tool(Some(&mut executor));
        assert!(runtime.pending_calls[0].result.is_none());
        assert_eq!(runtime.tool_calls_executed, 0);
        runtime.execute_next_tool(Some(&mut executor));
        assert!(runtime.pending_calls[0].result.is_none());
        runtime.execute_next_tool(Some(&mut executor));
        assert_eq!(executor.starts, 1);
        assert_eq!(runtime.tool_calls_executed, 1);
        assert!(runtime.pending_calls[0]
            .result
            .as_ref()
            .unwrap()
            .contains("real image ready"));
    }

    #[test]
    fn executing_tools_blocks_input() {
        assert!(AgentStatus::ExecutingTools.blocks_input());
        assert!(!AgentStatus::Done.blocks_input());
    }

    #[test]
    fn activity_snapshot_exposes_current_tool_and_progress() {
        let mut runtime = AgentRuntime::new(OpenAiConfig::default());
        runtime.status = AgentStatus::ExecutingTools;
        runtime.run_started_at = Some(Instant::now() - Duration::from_secs(7));
        runtime.tool_calls_executed = 2;
        runtime.turn_count = 3;
        runtime.pending_calls = vec![
            PendingToolCall::new("current", "scene_build", serde_json::json!({})),
            PendingToolCall::new("next", "scene_verify", serde_json::json!({})),
        ];

        let activity = runtime.activity_snapshot().unwrap();

        assert_eq!(activity.current_tool.as_deref(), Some("scene_build"));
        assert_eq!(activity.completed_tools, 2);
        assert_eq!(activity.pending_tools, 2);
        assert_eq!(activity.total_tools, 4);
        assert!(activity.elapsed_seconds >= 7);
    }

    fn tool_call(id: &str, arguments: &str) -> ToolCall {
        ToolCall {
            id: id.to_string(),
            call_type: "function".to_string(),
            function: crate::openai_client::FunctionCall {
                name: "game_add".to_string(),
                arguments: arguments.to_string(),
            },
        }
    }

    #[test]
    fn malformed_tool_arguments_are_reported_without_executing() {
        let mut runtime = AgentRuntime::new(OpenAiConfig::default());
        let message = assistant_with_tool_calls("", &[tool_call("bad", "{not-json")]);

        let status =
            runtime.handle_assistant_message(message, None, &[], ToolExecutionMode::Inspect);

        assert_eq!(status, AgentStatus::Error);
        assert!(runtime.pending_calls[0].argument_error.is_some());
        assert_eq!(runtime.pending_calls[0].arguments, Value::Null);
    }

    #[test]
    fn default_tool_call_budget_is_unlimited() {
        let runtime = AgentRuntime::new(OpenAiConfig::default());

        assert_eq!(runtime.max_tool_calls, None);
        assert_eq!(runtime.max_turns, 128);
    }

    #[test]
    fn harness_step_guard_never_reports_a_completed_task() {
        let mut runtime = AgentRuntime::new(OpenAiConfig::default());
        runtime.set_run_limits(1, None, 512);
        runtime.turn_count = 1;
        runtime.spawn_next_request(&[], ToolExecutionMode::Inspect);
        assert_eq!(runtime.status, AgentStatus::Error);
        assert!(runtime.last_error.as_ref().unwrap().contains("not complete"));
        assert!(runtime.pending.is_none());
    }

    #[test]
    fn tool_call_budget_rejects_an_oversized_model_plan() {
        let mut runtime = AgentRuntime::new(OpenAiConfig::default());
        runtime.set_run_limits(4, Some(1), 512);
        let message =
            assistant_with_tool_calls("", &[tool_call("first", "{}"), tool_call("second", "{}")]);

        let status =
            runtime.handle_assistant_message(message, None, &[], ToolExecutionMode::Inspect);

        assert_eq!(status, AgentStatus::Error);
        assert!(runtime
            .last_error
            .as_deref()
            .is_some_and(|error| error.contains("remaining budget of 1")));
    }

    #[test]
    fn oversized_tool_plan_keeps_follow_up_history_provider_valid() {
        let mut runtime = AgentRuntime::new(OpenAiConfig::default());
        runtime.set_run_limits(4, Some(1), 512);
        let message =
            assistant_with_tool_calls("", &[tool_call("first", "{}"), tool_call("second", "{}")]);

        runtime.handle_assistant_message(message, None, &[], ToolExecutionMode::Inspect);

        let provider_messages = runtime
            .messages
            .iter()
            .map(message_to_openai)
            .collect::<Vec<_>>();
        assert_eq!(provider_messages.len(), 4);
        assert_eq!(provider_messages[0].role, "assistant");
        assert_eq!(provider_messages[0].tool_calls.as_ref().unwrap().len(), 2);
        assert_eq!(provider_messages[1].role, "tool");
        assert_eq!(provider_messages[2].role, "tool");
        assert_eq!(provider_messages[1].tool_call_id.as_deref(), Some("first"));
        assert_eq!(provider_messages[2].tool_call_id.as_deref(), Some("second"));
        assert_eq!(provider_messages[3].role, "assistant");
        assert!(provider_messages[1]
            .content
            .to_string()
            .contains("tool_call_budget_exceeded"));
    }

    #[test]
    fn persisted_orphaned_tool_calls_are_repaired_before_next_run() {
        let mut runtime = AgentRuntime::new(OpenAiConfig::default());
        let calls = vec![tool_call("first", "{}"), tool_call("second", "{}")];
        let mut assistant = ChatMessage::assistant("");
        assistant.tool_calls = Some(serde_json::to_value(calls).unwrap());
        runtime.messages = vec![assistant, ChatMessage::assistant("Budget exceeded")];

        runtime.repair_orphaned_tool_call_history();

        assert_eq!(runtime.messages.len(), 2);
        assert!(runtime.messages[0].tool_calls.is_none());
        assert_eq!(runtime.messages[1].content, "Budget exceeded");
        assert!(message_to_openai(&runtime.messages[0]).tool_calls.is_none());
    }

    #[test]
    fn tool_result_output_is_bounded_without_splitting_utf8() {
        let result = truncate_tool_result("abcdef".to_string(), 4);

        assert!(result.starts_with("abcd"));
        assert!(result.contains("truncated"));
    }

    #[test]
    fn streamed_text_reuses_one_assistant_message_until_completion() {
        let mut runtime = AgentRuntime::new(OpenAiConfig::default());

        runtime.append_stream_delta("Hola");
        runtime.append_stream_delta(" mundo");

        assert_eq!(runtime.messages.len(), 1);
        assert_eq!(runtime.messages[0].content, "Hola mundo");

        let status = runtime.handle_assistant_message(
            OpenAiMessage {
                role: "assistant".to_string(),
                content: serde_json::json!("Hola mundo"),
                tool_calls: None,
                tool_call_id: None,
                reasoning_content: None,
                reasoning: None,
                reasoning_details: Vec::new(),
            },
            None,
            &[],
            ToolExecutionMode::Inspect,
        );

        assert_eq!(status, AgentStatus::Done);
        assert_eq!(runtime.messages.len(), 1);
        assert!(runtime.streaming_message_index.is_none());
    }

    #[test]
    fn structured_tool_result_does_not_repeat_console_boilerplate() {
        let result = AgentToolResult::success("Created shelf", serde_json::json!({"id": 12}));
        let encoded = result.model_content(1_024);

        assert!(encoded.contains("Created shelf"));
        assert!(!encoded.contains("Command executed"));
    }

    #[test]
    fn initial_tool_context_is_ephemeral_and_provider_shaped() {
        let mut runtime = AgentRuntime::new(OpenAiConfig::default());
        runtime.set_initial_tool_context(
            "project_summary",
            serde_json::json!({}),
            AgentToolResult::success("Project loaded", serde_json::json!({"entities": 3})),
        );

        assert!(runtime.messages.is_empty());
        assert_eq!(runtime.request_context.len(), 2);
        assert_eq!(runtime.request_context[0].role, "assistant");
        assert_eq!(
            runtime.request_context[0]
                .tool_calls
                .as_ref()
                .and_then(|calls| calls.first())
                .map(|call| call.function.name.as_str()),
            Some("project_summary")
        );
        assert_eq!(runtime.request_context[1].role, "tool");
        assert!(runtime.request_context[1].tool_call_id.is_some());

        runtime.clear();
        assert!(runtime.request_context.is_empty());
    }
}
