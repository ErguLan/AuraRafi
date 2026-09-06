//! Backend state for the retained editor console.
//!
//! The model intentionally has no presentation dependency. A presentation
//! host mirrors the input into a transient control state, emits typed actions,
//! and the application boundary performs command execution.

use crate::commands::{CommandLevel, CommandOutput};
use std::collections::BTreeSet;

pub type ConsoleEntryId = u64;

pub const MAX_ENTRIES: usize = 500;

const MAX_HISTORY_ENTRIES: usize = 100;
const MAX_HISTORY_LINES: usize = 40;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LogLevel {
    Info,
    Warning,
    Error,
}

#[derive(Debug, Clone)]
pub struct LogEntry {
    pub id: ConsoleEntryId,
    pub level: LogLevel,
    pub message: String,
    pub timestamp: String,
    pub sender: Option<String>,
    pub block: Option<ConsoleBlock>,
}

#[derive(Debug, Clone)]
pub struct ConsoleBlock {
    pub title: String,
    pub lines: Vec<String>,
    pub json: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConsoleSubmission {
    pub text: String,
}

#[derive(Debug, Clone)]
pub struct ConsolePanel {
    pub entries: Vec<LogEntry>,
    pub auto_scroll: bool,
    pub filter_level: Option<LogLevel>,
    input: String,
    pub command_history: Vec<String>,
    history_cursor: Option<usize>,
    next_entry_id: ConsoleEntryId,
    json_disclosures: BTreeSet<ConsoleEntryId>,
    content_revision: u64,
}

impl Default for ConsolePanel {
    fn default() -> Self {
        Self {
            entries: vec![LogEntry {
                id: 0,
                level: LogLevel::Info,
                message: "AuraRafi Engine initialized".to_string(),
                timestamp: now_timestamp(),
                sender: None,
                block: None,
            }],
            auto_scroll: true,
            filter_level: None,
            input: String::new(),
            command_history: Vec::new(),
            history_cursor: None,
            next_entry_id: 1,
            json_disclosures: BTreeSet::new(),
            content_revision: 0,
        }
    }
}

impl ConsolePanel {
    pub fn log(&mut self, level: LogLevel, message: impl Into<String>) {
        self.log_with_id(level, message);
    }

    pub fn log_with_id(&mut self, level: LogLevel, message: impl Into<String>) -> ConsoleEntryId {
        let id = self.allocate_entry_id();
        self.append_entry(LogEntry {
            id,
            level,
            message: message.into(),
            timestamp: now_timestamp(),
            sender: None,
            block: None,
        })
    }

    pub fn log_user(&mut self, sender: impl Into<String>, message: impl Into<String>) {
        self.log_user_with_id(sender, message);
    }

    pub fn log_user_with_id(
        &mut self,
        sender: impl Into<String>,
        message: impl Into<String>,
    ) -> ConsoleEntryId {
        let message = message.into();
        let id = self.allocate_entry_id();
        let entry_id = self.append_entry(LogEntry {
            id,
            level: LogLevel::Info,
            message: message.clone(),
            timestamp: now_timestamp(),
            sender: Some(sender.into()),
            block: None,
        });
        if message.trim_start().starts_with('/') {
            self.command_history.push(message.trim().to_string());
            if self.command_history.len() > MAX_HISTORY_ENTRIES {
                self.command_history.remove(0);
            }
        }
        self.history_cursor = None;
        entry_id
    }

    pub fn log_command_output(&mut self, output: CommandOutput) {
        self.log_command_output_with_id(output);
    }

    pub fn log_command_output_with_id(&mut self, output: CommandOutput) -> ConsoleEntryId {
        let level = match output.level {
            CommandLevel::Info => LogLevel::Info,
            CommandLevel::Warning => LogLevel::Warning,
            CommandLevel::Error => LogLevel::Error,
        };
        let json =
            serde_json::to_string_pretty(&output.json).unwrap_or_else(|_| output.json.to_string());
        let id = self.allocate_entry_id();
        self.append_entry(LogEntry {
            id,
            level,
            message: output.title.clone(),
            timestamp: now_timestamp(),
            sender: None,
            block: Some(ConsoleBlock {
                title: output.title,
                lines: output.lines,
                json,
            }),
        })
    }

    pub fn clear_entries(&mut self) {
        if !self.entries.is_empty() || !self.json_disclosures.is_empty() {
            self.entries.clear();
            self.json_disclosures.clear();
            self.touch_content();
        }
    }

    pub fn revision(&self) -> u64 {
        self.content_revision
    }

    pub fn filtered_entries(&self) -> impl Iterator<Item = (usize, &LogEntry)> {
        self.entries.iter().enumerate().filter(|(_, entry)| {
            self.filter_level
                .map(|filter| entry.level == filter)
                .unwrap_or(true)
        })
    }

    /// Provides filtered entries keyed by their stable model IDs.
    pub fn filtered_entries_with_ids(&self) -> impl Iterator<Item = (ConsoleEntryId, &LogEntry)> {
        self.entries
            .iter()
            .filter(|entry| {
                self.filter_level
                    .map(|filter| entry.level == filter)
                    .unwrap_or(true)
            })
            .map(|entry| (entry.id, entry))
    }

    /// Looks up an entry without depending on its current position in the log.
    pub fn entry(&self, id: ConsoleEntryId) -> Option<&LogEntry> {
        self.entries.iter().find(|entry| entry.id == id)
    }

    pub fn is_json_disclosed(&self, id: ConsoleEntryId) -> bool {
        self.json_disclosures.contains(&id)
    }

    pub fn set_json_disclosed(&mut self, id: ConsoleEntryId, disclosed: bool) -> bool {
        let Some(entry) = self.entry(id) else {
            return false;
        };
        if entry.block.is_none() {
            return false;
        }

        let changed = if disclosed {
            self.json_disclosures.insert(id)
        } else {
            self.json_disclosures.remove(&id)
        };
        if changed {
            self.touch_content();
        }
        true
    }

    pub fn toggle_json_disclosure(&mut self, id: ConsoleEntryId) -> bool {
        if !self.entry(id).is_some_and(|entry| entry.block.is_some()) {
            return false;
        }

        let disclosed = !self.json_disclosures.contains(&id);
        self.set_json_disclosed(id, disclosed);
        disclosed
    }

    pub fn input(&self) -> &str {
        &self.input
    }

    pub fn set_input(&mut self, value: String) {
        if self.input != value {
            self.input = value;
        }
    }

    pub fn set_auto_scroll(&mut self, enabled: bool) -> bool {
        if self.auto_scroll == enabled {
            return false;
        }
        self.auto_scroll = enabled;
        self.touch_content();
        true
    }

    pub fn set_filter_level(&mut self, level: Option<LogLevel>) -> bool {
        if self.filter_level == level {
            return false;
        }
        self.filter_level = level;
        self.touch_content();
        true
    }

    pub fn submit_input(&mut self) -> Option<ConsoleSubmission> {
        let text = self.input.trim().to_string();
        if text.is_empty() {
            return None;
        }
        self.input.clear();
        self.history_cursor = None;
        Some(ConsoleSubmission { text })
    }

    pub fn autocomplete_command(&mut self, command_names: &[String]) {
        let prefix = self.input.trim();
        if !prefix.starts_with('/') {
            return;
        }
        if let Some(completion) = command_names.iter().find(|name| name.starts_with(prefix)) {
            self.input = format!("{completion} ");
        }
    }

    pub fn select_previous_history(&mut self) {
        if self.command_history.is_empty() {
            return;
        }
        let next = self
            .history_cursor
            .map(|cursor| cursor.saturating_sub(1))
            .unwrap_or_else(|| self.command_history.len().saturating_sub(1));
        self.history_cursor = Some(next);
        if let Some(entry) = self.command_history.get(next) {
            self.input = entry.clone();
        }
    }

    pub fn select_next_history(&mut self) {
        let Some(cursor) = self.history_cursor else {
            return;
        };
        if cursor + 1 >= self.command_history.len() {
            self.history_cursor = None;
            self.input.clear();
        } else {
            let next = cursor + 1;
            self.history_cursor = Some(next);
            if let Some(entry) = self.command_history.get(next) {
                self.input = entry.clone();
            }
        }
    }

    pub fn history_lines(&self) -> impl Iterator<Item = &String> {
        self.command_history.iter().rev().take(MAX_HISTORY_LINES)
    }

    fn append_entry(&mut self, entry: LogEntry) -> ConsoleEntryId {
        let id = entry.id;
        self.entries.push(entry);
        self.trim_entries();
        self.touch_content();
        id
    }

    fn allocate_entry_id(&mut self) -> ConsoleEntryId {
        loop {
            let id = self.next_entry_id;
            self.next_entry_id = self.next_entry_id.wrapping_add(1);
            if !self.entries.iter().any(|entry| entry.id == id) {
                return id;
            }
        }
    }

    fn trim_entries(&mut self) {
        if self.entries.len() > MAX_ENTRIES {
            let remove_count = self.entries.len() - MAX_ENTRIES;
            for entry in self.entries.drain(0..remove_count) {
                self.json_disclosures.remove(&entry.id);
            }
        }
    }

    fn touch_content(&mut self) {
        self.content_revision = self.content_revision.wrapping_add(1);
    }
}

fn now_timestamp() -> String {
    chrono::Local::now().format("%H:%M:%S").to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slash_messages_are_kept_in_history_and_submit_clears_input() {
        let mut console = ConsolePanel::default();
        console.set_input("/help".to_string());
        let submission = console.submit_input().unwrap();
        console.log_user("User", &submission.text);

        assert_eq!(submission.text, "/help");
        assert_eq!(console.command_history, vec!["/help"]);
        assert!(console.input().is_empty());
    }

    #[test]
    fn filtering_keeps_original_entry_indices() {
        let mut console = ConsolePanel::default();
        console.log(LogLevel::Warning, "warning");
        console.filter_level = Some(LogLevel::Warning);
        let entries = console.filtered_entries().collect::<Vec<_>>();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].0, 1);
    }

    #[test]
    fn console_view_options_change_the_content_revision() {
        let mut console = ConsolePanel::default();
        let initial_revision = console.revision();

        assert!(console.set_auto_scroll(false));
        assert!(console.set_filter_level(Some(LogLevel::Info)));
        assert!(!console.set_filter_level(Some(LogLevel::Info)));
        assert!(console.revision() > initial_revision);
    }
}
