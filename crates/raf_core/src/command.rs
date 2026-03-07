//! Command bus - every operation is a serializable command.
//!
//! This is the backbone for:
//! - **Undo/Redo**: commands record their inverse automatically.
//! - **AI tool-calling**: AI agents emit commands through the same bus.
//! - **Replay**: sessions can be recorded and replayed.
//!
//! Every public operation in the engine should go through the command bus.

use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Unique identifier for a command instance.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct CommandId(pub Uuid);

impl CommandId {
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }
}

impl Default for CommandId {
    fn default() -> Self {
        Self::new()
    }
}

/// A single engine command. All operations that modify state flow through here.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Command {
    /// Unique id for this command instance.
    pub id: CommandId,
    /// Machine-readable command name (e.g. "create_entity", "set_property").
    pub name: String,
    /// Category for grouping ("scene", "asset", "electronics", "editor").
    pub category: String,
    /// Human-readable description for the UI/log.
    pub description: String,
    /// JSON-encoded parameters.
    pub params: serde_json::Value,
    /// Whether this command has been executed.
    pub executed: bool,
}

impl Command {
    /// Create a new command with the given name and parameters.
    pub fn new(name: &str, category: &str, description: &str, params: serde_json::Value) -> Self {
        Self {
            id: CommandId::new(),
            name: name.to_string(),
            category: category.to_string(),
            description: description.to_string(),
            params,
            executed: false,
        }
    }
}

/// Central command bus. Commands are queued, executed, and recorded.
pub struct CommandBus {
    /// Commands waiting to be executed this frame.
    pending: Vec<Command>,
    /// History of executed commands (for undo).
    history: Vec<Command>,
    /// Commands that were undone (for redo).
    redo_stack: Vec<Command>,
    /// Maximum history size to prevent unbounded memory growth.
    max_history: usize,
}

impl CommandBus {
    /// Create a new command bus with a default history limit.
    pub fn new() -> Self {
        Self {
            pending: Vec::new(),
            history: Vec::new(),
            redo_stack: Vec::new(),
            max_history: 1000,
        }
    }

    /// Submit a command to the bus. It will be processed on the next flush.
    pub fn submit(&mut self, command: Command) {
        tracing::debug!("Command submitted: {} ({})", command.name, command.category);
        self.pending.push(command);
    }

    /// Take all pending commands for processing. Returns them and clears the
    /// pending queue.
    pub fn flush(&mut self) -> Vec<Command> {
        let commands = std::mem::take(&mut self.pending);
        commands
    }

    /// Record a command as executed (call after the handler processes it).
    pub fn record_executed(&mut self, mut command: Command) {
        command.executed = true;
        self.redo_stack.clear(); // New command invalidates redo stack.
        self.history.push(command);

        // Trim history if it exceeds the limit.
        if self.history.len() > self.max_history {
            self.history.remove(0);
        }
    }

    /// Get the last executed command for undo. Returns `None` if history is
    /// empty.
    pub fn undo(&mut self) -> Option<Command> {
        let cmd = self.history.pop()?;
        self.redo_stack.push(cmd.clone());
        Some(cmd)
    }

    /// Get the last undone command for redo. Returns `None` if redo stack is
    /// empty.
    pub fn redo(&mut self) -> Option<Command> {
        let cmd = self.redo_stack.pop()?;
        self.history.push(cmd.clone());
        Some(cmd)
    }

    /// Number of pending commands.
    pub fn pending_count(&self) -> usize {
        self.pending.len()
    }

    /// Number of commands in history.
    pub fn history_count(&self) -> usize {
        self.history.len()
    }

    /// Read-only view of command history (for display in UI).
    pub fn history(&self) -> &[Command] {
        &self.history
    }
}

impl Default for CommandBus {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn submit_and_flush() {
        let mut bus = CommandBus::new();
        bus.submit(Command::new(
            "create_entity",
            "scene",
            "Create a cube",
            serde_json::json!({"name": "Cube"}),
        ));
        assert_eq!(bus.pending_count(), 1);
        let cmds = bus.flush();
        assert_eq!(cmds.len(), 1);
        assert_eq!(bus.pending_count(), 0);
    }

    #[test]
    fn undo_redo() {
        let mut bus = CommandBus::new();
        let cmd = Command::new(
            "test",
            "test",
            "test command",
            serde_json::json!({}),
        );
        bus.record_executed(cmd);
        assert_eq!(bus.history_count(), 1);

        let undone = bus.undo().unwrap();
        assert_eq!(undone.name, "test");
        assert_eq!(bus.history_count(), 0);

        let redone = bus.redo().unwrap();
        assert_eq!(redone.name, "test");
        assert_eq!(bus.history_count(), 1);
    }
}
