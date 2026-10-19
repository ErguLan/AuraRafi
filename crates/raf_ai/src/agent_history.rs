//! Per-project Agent history persistence.
//!
//! Conversations are saved inside the active project as `.ai/agent_history.ron`.
//! The parent `.ai` folder keeps history collocated with the project without
//! polluting the asset tree.

use crate::chat::ChatMessage;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Sender};
use std::thread::{self, JoinHandle};

struct HistorySaveRequest {
    project_root: PathBuf,
    history: AgentHistory,
}

/// Serializes and writes Agent history away from the editor frame thread.
///
/// The editor still owns the in-memory history and sends immutable snapshots to
/// this worker. This keeps filesystem latency and RON serialization out of the
/// render/input loop without sharing mutable editor state across threads.
pub struct AgentHistoryWriter {
    sender: Option<Sender<HistorySaveRequest>>,
    handle: Option<JoinHandle<Option<String>>>,
    finish_error: Option<String>,
}

impl Default for AgentHistoryWriter {
    fn default() -> Self {
        Self::new()
    }
}

impl AgentHistoryWriter {
    pub fn new() -> Self {
        let (sender, receiver) = mpsc::channel::<HistorySaveRequest>();
        let handle = thread::Builder::new()
            .name("agent-history-writer".to_string())
            .spawn(move || {
                let mut failures = HashMap::new();
                while let Ok(request) = receiver.recv() {
                    let mut latest_by_project = HashMap::new();
                    latest_by_project.insert(request.project_root, request.history);
                    for pending in receiver.try_iter() {
                        latest_by_project.insert(pending.project_root, pending.history);
                    }
                    for (project_root, history) in latest_by_project {
                        if let Err(error) = history.save(&project_root) {
                            failures.insert(project_root.clone(), error.to_string());
                            tracing::warn!(
                                project = %project_root.display(),
                                %error,
                                "failed to persist Agent history"
                            );
                        } else {
                            failures.remove(&project_root);
                        }
                    }
                }
                (!failures.is_empty())
                    .then(|| failures.into_values().collect::<Vec<_>>().join("\n"))
            })
            .expect("failed to spawn Agent history writer");

        Self {
            sender: Some(sender),
            handle: Some(handle),
            finish_error: None,
        }
    }

    pub fn enqueue(&self, project_root: &Path, history: &AgentHistory) {
        let Some(sender) = self.sender.as_ref() else {
            return;
        };
        let _ = sender.send(HistorySaveRequest {
            project_root: project_root.to_path_buf(),
            history: history.clone(),
        });
    }

    /// Stop accepting snapshots and let the worker drain without blocking UI.
    pub fn finish_if_idle(&mut self) -> Result<bool, String> {
        self.sender.take();
        if self
            .handle
            .as_ref()
            .is_some_and(|handle| !handle.is_finished())
        {
            return Ok(false);
        }
        if let Some(handle) = self.handle.take() {
            self.finish_error = handle
                .join()
                .unwrap_or_else(|_| Some("agent_workspace.history_save_failed".into()));
        }
        self.finish_error.clone().map_or(Ok(true), Err)
    }
}

impl Drop for AgentHistoryWriter {
    fn drop(&mut self) {
        self.sender.take();
        if let Some(handle) = self.handle.take() {
            let _ = handle.join();
        }
    }
}

/// A saved Agent conversation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentSession {
    /// Stable session id (UUID v4 as string).
    pub id: String,
    /// Human-readable title, usually the first user message.
    pub title: String,
    /// Messages in this session.
    pub messages: Vec<ChatMessage>,
    /// UTC timestamp of the last message.
    pub updated_at: String,
    /// Immutable world/design target, independent of the selected editor tab.
    #[serde(default)]
    pub document_id: Option<uuid::Uuid>,
    #[serde(default)]
    pub last_task: Option<raf_core::AgentTaskSnapshot>,
    #[serde(default)]
    pub task_events: Vec<raf_core::AgentTaskEvent>,
    #[serde(default)]
    pub saved_revision: Option<u64>,
    #[serde(default)]
    pub draft: String,
}

impl AgentSession {
    pub fn new(id: impl Into<String>, title: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            title: title.into(),
            messages: Vec::new(),
            updated_at: chrono::Utc::now().to_rfc3339(),
            document_id: None,
            last_task: None,
            task_events: Vec::new(),
            saved_revision: None,
            draft: String::new(),
        }
    }

    /// Update the timestamp to now.
    pub fn touch(&mut self) {
        self.updated_at = chrono::Utc::now().to_rfc3339();
    }
}

/// In-memory and on-disk history for the Agent panel.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct AgentHistory {
    pub sessions: Vec<AgentSession>,
    /// Index of the active session in `sessions`. None means no session selected.
    pub active_index: Option<usize>,
}

impl AgentHistory {
    /// Directory name inside the project where history lives.
    pub const FOLDER: &'static str = ".ai";
    /// File name for persisted history.
    pub const FILE_NAME: &'static str = "agent_history.ron";

    /// Path to the history file for a given project root.
    pub fn path_for(project_root: &Path) -> PathBuf {
        project_root.join(Self::FOLDER).join(Self::FILE_NAME)
    }

    /// Load history from disk, returning an empty one if the file does not exist.
    pub fn load(project_root: &Path) -> Self {
        Self::load_checked(project_root).unwrap_or_default()
    }

    pub fn load_checked(project_root: &Path) -> Result<Self, String> {
        let path = raf_core::project_store::confined_path(
            project_root,
            &PathBuf::from(Self::FOLDER).join(Self::FILE_NAME),
        )?;
        let Some(bytes) = raf_core::project_store::read_recoverable(&path)? else {
            return Ok(Self::default());
        };
        ron::de::from_bytes(&bytes).map_err(|e| e.to_string())
    }

    /// Save history to disk. Creates the `.ai` folder if needed.
    pub fn save(&self, project_root: &Path) -> Result<(), Box<dyn std::error::Error>> {
        let path = raf_core::project_store::confined_path(
            project_root,
            &PathBuf::from(Self::FOLDER).join(Self::FILE_NAME),
        )
        .map_err(std::io::Error::other)?;
        let pretty = ron::ser::PrettyConfig::default();
        let text = ron::ser::to_string_pretty(self, pretty)?;
        raf_core::project_store::write_atomic(&path, text.as_bytes())
            .map_err(std::io::Error::other)?;
        Ok(())
    }

    /// Start a new session and make it active.
    pub fn start_session(&mut self, title: impl Into<String>) -> usize {
        let id = uuid::Uuid::new_v4().to_string();
        let session = AgentSession::new(id, title);
        self.sessions.push(session);
        let index = self.sessions.len() - 1;
        self.active_index = Some(index);
        index
    }

    /// Restores the saved active session when possible, otherwise selects the
    /// most recently updated session or creates the first one.
    pub fn ensure_active_session(&mut self, title: impl Into<String>) -> usize {
        if let Some(index) = self
            .active_index
            .filter(|index| *index < self.sessions.len())
        {
            return index;
        }

        if let Some((index, _)) = self
            .sessions
            .iter()
            .enumerate()
            .max_by(|(_, left), (_, right)| left.updated_at.cmp(&right.updated_at))
        {
            self.active_index = Some(index);
            return index;
        }

        self.start_session(title)
    }

    /// Get the active session, if any.
    pub fn active_session(&self) -> Option<&AgentSession> {
        self.active_index.and_then(|index| self.sessions.get(index))
    }

    /// Get the active session mutably.
    pub fn active_session_mut(&mut self) -> Option<&mut AgentSession> {
        self.active_index
            .and_then(|index| self.sessions.get_mut(index))
    }

    /// Append a message to the active session and touch its timestamp.
    pub fn push_message(&mut self, message: ChatMessage) {
        if let Some(session) = self.active_session_mut() {
            session.messages.push(message);
            session.touch();
        }
    }

    /// Clear the active session messages but keep the session entry.
    pub fn clear_active(&mut self) {
        if let Some(session) = self.active_session_mut() {
            session.messages.clear();
            session.touch();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip_ron() {
        let mut history = AgentHistory::default();
        history.start_session("Test session");
        history.push_message(ChatMessage::user("hello"));
        let serialized =
            ron::ser::to_string_pretty(&history, ron::ser::PrettyConfig::default()).unwrap();
        let deserialized: AgentHistory = ron::from_str(&serialized).unwrap();
        assert_eq!(deserialized.sessions.len(), 1);
        assert_eq!(deserialized.sessions[0].messages.len(), 1);
    }

    #[test]
    fn ensure_active_session_preserves_saved_selection() {
        let mut history = AgentHistory::default();
        let first = history.start_session("First");
        let second = history.start_session("Second");
        history.active_index = Some(first);

        assert_eq!(history.ensure_active_session("New"), first);
        assert_eq!(history.active_index, Some(first));

        history.active_index = None;
        history.sessions[second].updated_at = "2099-01-01T00:00:00Z".to_string();
        assert_eq!(history.ensure_active_session("New"), second);
        assert_eq!(history.sessions.len(), 2);
    }
}
