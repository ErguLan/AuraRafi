//! Agent runtime - planning, tool-calling loop, and event generation.
//!
//! The runtime is non-blocking: HTTP requests run on a background thread
//! so the UI stays responsive. The caller calls `poll()` each frame to
//! advance the state machine.

use std::sync::{mpsc, Arc, Mutex};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use crate::chat::{ChatMessage, MessageRole};
use crate::openai_client::{OpenAiClient, OpenAiConfig, OpenAiMessage, OpenAiTool, ToolCall};
use chrono::Utc;
use serde_json::Value;
use uuid::Uuid;

/// Executor for tools invoked by the agent.
pub trait ToolExecutor {
    fn execute(&mut self, name: &str, arguments: Value) -> Result<String, String>;
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

impl AgentStatus {
    /// Whether the agent currently owns the input flow.
    pub fn blocks_input(&self) -> bool {
        matches!(
            self,
            Self::Thinking | Self::ExecutingTools | Self::AwaitingApproval
        )
    }
}

/// Events emitted for UI observation.
#[derive(Debug, Clone)]
pub enum AgentEvent {
    MessageAdded(ChatMessage),
    ToolCallsRequested(Vec<PendingToolCall>),
    ToolResult { id: String, result: String },
    StatusChanged(AgentStatus, Option<String>),
}

/// Shared cell for passing background-thread results back to the runtime.
struct PendingResponse {
    result: Arc<Mutex<Option<Result<OpenAiMessage, String>>>>,
    stream_rx: mpsc::Receiver<String>,
    _handle: JoinHandle<()>,
    tools: Vec<OpenAiTool>,
    active_mode: bool,
}

/// A queued sequence of tool calls that must preserve editor mutation order.
struct PendingToolExecution {
    tools: Vec<OpenAiTool>,
    active_mode: bool,
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
    max_tool_calls: usize,
    tool_calls_executed: usize,
    max_tool_result_chars: usize,
    tool_execution_interval: Duration,
    last_tool_execution: Option<Instant>,
    pending: Option<PendingResponse>,
    pending_tool_execution: Option<PendingToolExecution>,
    streaming_message_index: Option<usize>,
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
            max_turns: 16,
            turn_count: 0,
            max_tool_calls: 24,
            tool_calls_executed: 0,
            max_tool_result_chars: 16 * 1024,
            // Give the editor a presentation frame between consecutive
            // scene/asset mutations requested by the model.
            tool_execution_interval: Duration::from_millis(40),
            last_tool_execution: None,
            pending: None,
            pending_tool_execution: None,
            streaming_message_index: None,
        }
    }

    pub fn clear(&mut self) {
        self.cancel_background();
        self.messages.clear();
        self.pending_calls.clear();
        self.status = AgentStatus::Done;
        self.last_error = None;
        self.events.clear();
        self.turn_count = 0;
        self.tool_calls_executed = 0;
        self.last_tool_execution = None;
        self.pending_tool_execution = None;
        self.streaming_message_index = None;
    }

    pub fn set_system_prompt(&mut self, prompt: impl Into<String>) {
        let prompt = prompt.into();
        if let Some(first) = self.messages.first_mut() {
            if first.role == MessageRole::System {
                if first.content == prompt {
                    return;
                }
                first.content = prompt;
                return;
            }
        }
        self.messages.insert(0, ChatMessage::system(&prompt));
    }

    /// Start a new agent run. The user message is added immediately and a
    /// background thread is spawned for the first API call. Returns immediately.
    /// Call `poll()` each frame to advance the state machine.
    pub fn start_run(
        &mut self,
        user_message: impl Into<String>,
        tools: &[OpenAiTool],
        active_mode: bool,
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
        self.streaming_message_index = None;
        self.push_message(ChatMessage::user(&content));
        self.spawn_next_request(tools, active_mode);
    }

    /// Sets bounded limits for one editor request. These limits make a model's
    /// plan cooperative with the UI thread and the project's resource budget.
    pub fn set_run_limits(
        &mut self,
        max_turns: usize,
        max_tool_calls: usize,
        max_tool_result_chars: usize,
    ) {
        self.max_turns = max_turns.max(1);
        self.max_tool_calls = max_tool_calls.max(1);
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
        match self.status {
            AgentStatus::Thinking => self.check_pending(executor),
            AgentStatus::ExecutingTools => self.execute_next_tool(executor),
            AgentStatus::AwaitingApproval | AgentStatus::Done | AgentStatus::Error => {
                self.status.clone()
            }
        }
    }

    /// Check if the background thread is done and process the response.
    fn check_pending(&mut self, executor: Option<&mut dyn ToolExecutor>) -> AgentStatus {
        let deltas = self
            .pending
            .as_ref()
            .map(|pending| pending.stream_rx.try_iter().collect::<Vec<_>>())
            .unwrap_or_default();
        for delta in deltas {
            self.append_stream_delta(&delta);
        }

        let Some(pending) = self.pending.as_ref() else {
            self.status = AgentStatus::Done;
            return AgentStatus::Done;
        };

        let result_lock = pending.result.lock().unwrap();
        let result = match result_lock.as_ref() {
            Some(r) => r.clone(),
            None => return AgentStatus::Thinking, // still running
        };
        drop(result_lock);

        // Thread is done; take ownership of the pending data.
        let PendingResponse {
            tools, active_mode, ..
        } = self.pending.take().unwrap();

        match result {
            Ok(message) => self.handle_assistant_message(message, executor, &tools, active_mode),
            Err(error) => {
                self.last_error = Some(error.clone());
                self.status = AgentStatus::Error;
                let error_message = format!("Error: {}", error);
                if let Some(index) = self.streaming_message_index.take() {
                    if let Some(message) = self.messages.get_mut(index) {
                        message.content = error_message;
                        message.tool_calls = None;
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
        active_mode: bool,
    ) -> AgentStatus {
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

            let remaining_calls = self.max_tool_calls.saturating_sub(self.tool_calls_executed);
            if tool_calls.len() > remaining_calls {
                let error = format!(
                    "The agent requested {} tool calls, but this run has a remaining budget of {}.",
                    tool_calls.len(),
                    remaining_calls
                );
                self.last_error = Some(error.clone());
                self.status = AgentStatus::Error;
                self.push_message(ChatMessage::assistant(&error));
                self.emit(AgentEvent::StatusChanged(AgentStatus::Error, Some(error)));
                return AgentStatus::Error;
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

            if active_mode {
                if executor.is_some() {
                    self.begin_tool_execution(tools, true);
                    AgentStatus::ExecutingTools
                } else {
                    let error = "No tool executor is available for active agent mode.".to_string();
                    self.last_error = Some(error.clone());
                    self.status = AgentStatus::Error;
                    self.emit(AgentEvent::StatusChanged(AgentStatus::Error, Some(error)));
                    AgentStatus::Error
                }
            } else {
                self.status = AgentStatus::AwaitingApproval;
                self.emit(AgentEvent::StatusChanged(
                    AgentStatus::AwaitingApproval,
                    None,
                ));
                AgentStatus::AwaitingApproval
            }
        } else {
            self.upsert_assistant_message(text, None);
            self.status = AgentStatus::Done;
            self.turn_count = 0;
            self.emit(AgentEvent::StatusChanged(AgentStatus::Done, None));
            AgentStatus::Done
        }
    }

    /// Spawn a background thread for the next API request.
    fn spawn_next_request(&mut self, tools: &[OpenAiTool], active_mode: bool) {
        self.turn_count += 1;
        if self.turn_count > self.max_turns {
            let error = "I reached the maximum number of steps for this request.".to_string();
            self.last_error = Some(error.clone());
            self.push_message(ChatMessage::assistant(&error));
            self.status = AgentStatus::Done;
            self.emit(AgentEvent::StatusChanged(AgentStatus::Done, None));
            return;
        }

        let openai_messages: Vec<OpenAiMessage> =
            self.messages.iter().map(message_to_openai).collect();

        let client = self.client.clone();
        let result = Arc::new(Mutex::new(None));
        let result_clone = result.clone();
        let (stream_tx, stream_rx) = mpsc::channel();
        let tools_owned = tools.to_vec();

        let handle = std::thread::Builder::new()
            .name("agent-api-call".into())
            .spawn(move || {
                let response = if client.config.streaming {
                    client.chat_stream(&openai_messages, Some(&tools_owned), |delta| {
                        let _ = stream_tx.send(delta.to_string());
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
            active_mode,
        });
        self.status = AgentStatus::Thinking;
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
        self.begin_tool_execution(tools, false);
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
        self.spawn_next_request(tools, false);
    }

    pub fn cancel(&mut self) {
        self.cancel_background();
        self.pending_calls.clear();
        self.pending_tool_execution = None;
        self.status = AgentStatus::Done;
        self.turn_count = 0;
        self.tool_calls_executed = 0;
        self.last_tool_execution = None;
        self.streaming_message_index = None;
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
        if let Some(message) = self.messages.get_mut(index) {
            message.content.push_str(delta);
        }
    }

    fn upsert_assistant_message(&mut self, text: String, tool_calls: Option<Value>) {
        if let Some(index) = self.streaming_message_index.take() {
            if let Some(message) = self.messages.get_mut(index) {
                message.content = text;
                message.tool_calls = tool_calls;
                return;
            }
        }

        self.push_message(ChatMessage {
            id: Uuid::new_v4(),
            role: MessageRole::Assistant,
            content: text,
            timestamp: Utc::now(),
            tool_calls,
        });
    }

    fn begin_tool_execution(&mut self, tools: &[OpenAiTool], active_mode: bool) {
        self.pending_tool_execution = Some(PendingToolExecution {
            tools: tools.to_vec(),
            active_mode,
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
        self.last_tool_execution = Some(Instant::now());
        let executed = self
            .pending_calls
            .iter_mut()
            .find(|call| call.result.is_none())
            .map(|call| {
                let result = if call.approved == Some(false) {
                    "Denied by user.".to_string()
                } else if let Some(error) = call.argument_error.as_deref() {
                    format!("Error: {error}")
                } else {
                    match executor.execute(&call.name, call.arguments.clone()) {
                        Ok(output) => format!("Command executed:\n{}", output),
                        Err(error) => format!("Error: {}", error),
                    }
                };
                let result = truncate_tool_result(result, max_tool_result_chars);
                call.result = Some(result.clone());
                (call.id.clone(), result)
            });
        if let Some((id, result)) = executed {
            self.tool_calls_executed = self.tool_calls_executed.saturating_add(1);
            self.emit(AgentEvent::ToolResult { id, result });
            return AgentStatus::ExecutingTools;
        }

        let Some(continuation) = self.pending_tool_execution.take() else {
            self.status = AgentStatus::Done;
            return AgentStatus::Done;
        };
        self.flush_pending_as_tool_messages();
        self.pending_calls.clear();
        self.spawn_next_request(&continuation.tools, continuation.active_mode);
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
                timestamp: Utc::now(),
                tool_calls: Some(serde_json::json!({
                    "tool_call_id": id,
                    "name": name,
                })),
            });
        }
    }

    fn push_message(&mut self, message: ChatMessage) {
        self.messages.push(message.clone());
        self.emit(AgentEvent::MessageAdded(message));
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
    }
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

pub fn assistant_with_tool_calls(text: &str, calls: &[ToolCall]) -> OpenAiMessage {
    OpenAiMessage {
        role: "assistant".to_string(),
        content: serde_json::json!(text),
        tool_calls: Some(calls.to_vec()),
        tool_call_id: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[derive(Default)]
    struct RecordingExecutor {
        calls: Vec<String>,
    }

    impl ToolExecutor for RecordingExecutor {
        fn execute(&mut self, name: &str, _arguments: Value) -> Result<String, String> {
            self.calls.push(name.to_string());
            Ok(format!("completed {name}"))
        }
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
            active_mode: true,
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
    fn executing_tools_blocks_input() {
        assert!(AgentStatus::ExecutingTools.blocks_input());
        assert!(!AgentStatus::Done.blocks_input());
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

        let status = runtime.handle_assistant_message(message, None, &[], false);

        assert_eq!(status, AgentStatus::AwaitingApproval);
        assert!(runtime.pending_calls[0].argument_error.is_some());
        assert_eq!(runtime.pending_calls[0].arguments, Value::Null);
    }

    #[test]
    fn tool_call_budget_rejects_an_oversized_model_plan() {
        let mut runtime = AgentRuntime::new(OpenAiConfig::default());
        runtime.set_run_limits(4, 1, 512);
        let message =
            assistant_with_tool_calls("", &[tool_call("first", "{}"), tool_call("second", "{}")]);

        let status = runtime.handle_assistant_message(message, None, &[], false);

        assert_eq!(status, AgentStatus::Error);
        assert!(runtime
            .last_error
            .as_deref()
            .is_some_and(|error| error.contains("remaining budget of 1")));
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
            },
            None,
            &[],
            false,
        );

        assert_eq!(status, AgentStatus::Done);
        assert_eq!(runtime.messages.len(), 1);
        assert!(runtime.streaming_message_index.is_none());
    }
}
