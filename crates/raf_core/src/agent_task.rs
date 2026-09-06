//! Shared lifecycle contract for long-running Agent and automation work.
//!
//! The manager is intentionally renderer- and transport-neutral. Hosts may
//! run work on a background thread, on the editor frame loop, or through an
//! external process, while CLI, MCP and RafUI receive the same bounded task
//! snapshots and progress events.

use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{HashMap, VecDeque};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use uuid::Uuid;

const DEFAULT_MAX_TASKS: usize = 64;
const MAX_EVENTS: usize = 256;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct AgentTaskId(pub Uuid);

impl AgentTaskId {
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }
}

impl Default for AgentTaskId {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Display for AgentTaskId {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(formatter)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AgentTaskStatus {
    Queued,
    Running,
    WaitingApproval,
    Completed,
    Failed,
    Cancelled,
}

impl AgentTaskStatus {
    pub fn is_terminal(self) -> bool {
        matches!(self, Self::Completed | Self::Failed | Self::Cancelled)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgentTaskProgress {
    #[serde(default)]
    pub stage: String,
    #[serde(default)]
    pub completed: u32,
    #[serde(default)]
    pub total: Option<u32>,
    #[serde(default)]
    pub message: String,
}

impl Default for AgentTaskProgress {
    fn default() -> Self {
        Self {
            stage: "queued".to_string(),
            completed: 0,
            total: None,
            message: String::new(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AgentTaskSnapshot {
    pub id: AgentTaskId,
    pub kind: String,
    pub title: String,
    pub status: AgentTaskStatus,
    pub progress: AgentTaskProgress,
    pub cancellable: bool,
    #[serde(default)]
    pub cancel_requested: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub result: Option<Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    /// Monotonic host-local sequence. It lets polling clients request only
    /// changes without depending on wall-clock timestamps.
    pub sequence: u64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AgentTaskEvent {
    pub sequence: u64,
    pub task: AgentTaskSnapshot,
}

#[derive(Clone)]
pub struct AgentTaskHandle {
    id: AgentTaskId,
    cancellation: Arc<AtomicBool>,
}

impl std::fmt::Debug for AgentTaskHandle {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("AgentTaskHandle")
            .field("id", &self.id)
            .field("cancelled", &self.is_cancelled())
            .finish()
    }
}

impl AgentTaskHandle {
    pub fn id(&self) -> AgentTaskId {
        self.id
    }

    pub fn is_cancelled(&self) -> bool {
        self.cancellation.load(Ordering::Acquire)
    }
}

#[derive(Debug)]
struct TaskRecord {
    snapshot: AgentTaskSnapshot,
    cancellation: Arc<AtomicBool>,
    created_order: u64,
}

/// Bounded in-memory task registry shared by native Agent, attached editor
/// hosts, CLI and MCP adapters. It owns lifecycle state, not the work itself.
#[derive(Debug)]
pub struct AgentTaskManager {
    tasks: HashMap<AgentTaskId, TaskRecord>,
    events: VecDeque<AgentTaskEvent>,
    next_order: u64,
    next_sequence: u64,
    max_tasks: usize,
}

impl Default for AgentTaskManager {
    fn default() -> Self {
        Self::new()
    }
}

impl AgentTaskManager {
    pub fn new() -> Self {
        Self {
            tasks: HashMap::new(),
            events: VecDeque::new(),
            next_order: 0,
            next_sequence: 0,
            max_tasks: DEFAULT_MAX_TASKS,
        }
    }

    pub fn with_max_tasks(max_tasks: usize) -> Self {
        let mut manager = Self::new();
        manager.max_tasks = max_tasks.max(1);
        manager
    }

    pub fn start(
        &mut self,
        kind: impl Into<String>,
        title: impl Into<String>,
        total: Option<u32>,
        cancellable: bool,
    ) -> AgentTaskHandle {
        self.prune_terminal_tasks();
        let id = AgentTaskId::new();
        let cancellation = Arc::new(AtomicBool::new(false));
        self.next_order = self.next_order.saturating_add(1);
        let snapshot = AgentTaskSnapshot {
            id,
            kind: kind.into(),
            title: title.into(),
            status: AgentTaskStatus::Queued,
            progress: AgentTaskProgress {
                total,
                ..AgentTaskProgress::default()
            },
            cancellable,
            cancel_requested: false,
            result: None,
            error: None,
            sequence: 0,
        };
        self.tasks.insert(
            id,
            TaskRecord {
                snapshot,
                cancellation: Arc::clone(&cancellation),
                created_order: self.next_order,
            },
        );
        self.emit(id);
        AgentTaskHandle { id, cancellation }
    }

    pub fn start_running(
        &mut self,
        kind: impl Into<String>,
        title: impl Into<String>,
        total: Option<u32>,
        cancellable: bool,
    ) -> AgentTaskHandle {
        let handle = self.start(kind, title, total, cancellable);
        let _ = self.set_status(handle.id, AgentTaskStatus::Running);
        handle
    }

    pub fn set_status(&mut self, id: AgentTaskId, status: AgentTaskStatus) -> bool {
        let Some(record) = self.tasks.get_mut(&id) else {
            return false;
        };
        if record.snapshot.status.is_terminal() && record.snapshot.status != status {
            return false;
        }
        if record.snapshot.status == status {
            return true;
        }
        record.snapshot.status = status;
        record.snapshot.progress.stage = match status {
            AgentTaskStatus::Queued => "queued",
            AgentTaskStatus::Running => "running",
            AgentTaskStatus::WaitingApproval => "waiting_approval",
            AgentTaskStatus::Completed => "completed",
            AgentTaskStatus::Failed => "failed",
            AgentTaskStatus::Cancelled => "cancelled",
        }
        .to_string();
        self.emit(id);
        true
    }

    pub fn update_progress(
        &mut self,
        id: AgentTaskId,
        stage: impl Into<String>,
        completed: u32,
        total: Option<u32>,
        message: impl Into<String>,
    ) -> bool {
        let Some(record) = self.tasks.get_mut(&id) else {
            return false;
        };
        if record.snapshot.status.is_terminal() {
            return false;
        }
        if record.snapshot.status == AgentTaskStatus::Queued {
            record.snapshot.status = AgentTaskStatus::Running;
        }
        record.snapshot.progress = AgentTaskProgress {
            stage: stage.into(),
            completed: total.map_or(completed, |total| completed.min(total)),
            total,
            message: message.into(),
        };
        self.emit(id);
        true
    }

    pub fn complete(&mut self, id: AgentTaskId, result: Option<Value>) -> bool {
        let Some(record) = self.tasks.get_mut(&id) else {
            return false;
        };
        if record.snapshot.status.is_terminal() {
            return false;
        }
        if let Some(total) = record.snapshot.progress.total {
            record.snapshot.progress.completed = total;
        }
        record.snapshot.status = AgentTaskStatus::Completed;
        record.snapshot.progress.stage = "completed".to_string();
        record.snapshot.progress.message = "Task completed.".to_string();
        record.snapshot.result = result;
        self.emit(id);
        true
    }

    pub fn fail(&mut self, id: AgentTaskId, error: impl Into<String>) -> bool {
        let Some(record) = self.tasks.get_mut(&id) else {
            return false;
        };
        if record.snapshot.status.is_terminal() {
            return false;
        }
        let error = error.into();
        record.snapshot.status = AgentTaskStatus::Failed;
        record.snapshot.progress.stage = "failed".to_string();
        record.snapshot.progress.message = error.clone();
        record.snapshot.error = Some(error);
        self.emit(id);
        true
    }

    /// Requests cancellation and publishes the cancelled state immediately.
    /// The worker must still check its handle and stop at a safe boundary.
    pub fn cancel(&mut self, id: AgentTaskId) -> bool {
        let Some(record) = self.tasks.get_mut(&id) else {
            return false;
        };
        if record.snapshot.status.is_terminal() || !record.snapshot.cancellable {
            return false;
        }
        record.cancellation.store(true, Ordering::Release);
        record.snapshot.cancel_requested = true;
        record.snapshot.status = AgentTaskStatus::Cancelled;
        record.snapshot.progress.stage = "cancelled".to_string();
        record.snapshot.progress.message = "Cancellation requested.".to_string();
        self.emit(id);
        true
    }

    pub fn snapshot(&self, id: AgentTaskId) -> Option<AgentTaskSnapshot> {
        self.tasks.get(&id).map(|record| record.snapshot.clone())
    }

    pub fn list(&self) -> Vec<AgentTaskSnapshot> {
        let mut records = self
            .tasks
            .values()
            .map(|record| (record.created_order, record.snapshot.clone()))
            .collect::<Vec<_>>();
        records.sort_by_key(|(order, _)| *order);
        records.into_iter().map(|(_, snapshot)| snapshot).collect()
    }

    pub fn drain_events(&mut self) -> Vec<AgentTaskEvent> {
        self.events.drain(..).collect()
    }

    pub fn events_since(&self, sequence: u64) -> Vec<AgentTaskEvent> {
        self.events
            .iter()
            .filter(|event| event.sequence > sequence)
            .cloned()
            .collect()
    }

    fn emit(&mut self, id: AgentTaskId) {
        self.next_sequence = self.next_sequence.saturating_add(1);
        let Some(record) = self.tasks.get_mut(&id) else {
            return;
        };
        record.snapshot.sequence = self.next_sequence;
        self.events.push_back(AgentTaskEvent {
            sequence: self.next_sequence,
            task: record.snapshot.clone(),
        });
        while self.events.len() > MAX_EVENTS {
            self.events.pop_front();
        }
    }

    fn prune_terminal_tasks(&mut self) {
        if self.tasks.len() < self.max_tasks {
            return;
        }
        let mut terminal = self
            .tasks
            .iter()
            .filter(|(_, record)| record.snapshot.status.is_terminal())
            .map(|(id, record)| (*id, record.created_order))
            .collect::<Vec<_>>();
        terminal.sort_by_key(|(_, order)| *order);
        if let Some((id, _)) = terminal.first().copied() {
            self.tasks.remove(&id);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn task_progress_and_completion_publish_bounded_snapshots() {
        let mut manager = AgentTaskManager::new();
        let handle = manager.start_running("scene.reconcile", "Build scene", Some(3), true);
        assert_eq!(
            manager.snapshot(handle.id()).unwrap().status,
            AgentTaskStatus::Running
        );
        assert!(manager.update_progress(
            handle.id(),
            "creating",
            2,
            Some(3),
            "Two groups created."
        ));
        assert!(manager.complete(handle.id(), Some(serde_json::json!({"created": 3}))));

        let snapshot = manager.snapshot(handle.id()).unwrap();
        assert_eq!(snapshot.status, AgentTaskStatus::Completed);
        assert_eq!(snapshot.progress.completed, 3);
        assert_eq!(snapshot.result, Some(serde_json::json!({"created": 3})));
        assert!(manager.drain_events().len() >= 3);
    }

    #[test]
    fn cancellation_is_cooperative_and_handle_observes_it() {
        let mut manager = AgentTaskManager::new();
        let handle = manager.start_running("asset.import", "Import asset", None, true);
        assert!(manager.cancel(handle.id()));
        assert!(handle.is_cancelled());
        assert_eq!(
            manager.snapshot(handle.id()).unwrap().status,
            AgentTaskStatus::Cancelled
        );
        assert!(!manager.update_progress(handle.id(), "working", 1, None, "late update"));
        assert!(!manager.complete(handle.id(), None));
    }

    #[test]
    fn non_cancellable_tasks_reject_cancel_without_mutating_state() {
        let mut manager = AgentTaskManager::new();
        let handle = manager.start_running("project.save", "Save project", None, false);
        assert!(!manager.cancel(handle.id()));
        let snapshot = manager.snapshot(handle.id()).unwrap();
        assert_eq!(snapshot.status, AgentTaskStatus::Running);
        assert!(!snapshot.cancel_requested);
    }
}
