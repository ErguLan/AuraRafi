//! Backend state for the retained editor console.
//!
//! The model intentionally has no presentation dependency. A presentation
//! host mirrors the input into a transient control state, emits typed actions,
//! and the application boundary performs command execution.

use crate::commands::{CommandLevel, CommandOutput};
use raf_runtime::wire::RuntimeDiagnosticSeverity;
use std::collections::BTreeSet;

pub type ConsoleEntryId = u64;

pub const MAX_ENTRIES: usize = 512;

const MAX_HISTORY_ENTRIES: usize = 100;
const MAX_HISTORY_LINES: usize = 40;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LogLevel {
    Info,
    Success,
    Warning,
    Error,
    Verbose,
}

impl LogLevel {
    /// Structured runtime diagnostics use the same three severities.
    pub fn from_runtime_severity(severity: RuntimeDiagnosticSeverity) -> Self {
        match severity {
            RuntimeDiagnosticSeverity::Info => Self::Info,
            RuntimeDiagnosticSeverity::Warning => Self::Warning,
            RuntimeDiagnosticSeverity::Error => Self::Error,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Info => "info",
            Self::Success => "success",
            Self::Warning => "warning",
            Self::Error => "error",
            Self::Verbose => "verbose",
        }
    }
}

#[derive(Debug, Clone)]
pub struct LogEntry {
    pub id: ConsoleEntryId,
    pub level: LogLevel,
    pub message: String,
    pub timestamp: String,
    pub sender: Option<String>,
    /// Stable diagnostic code for machine filtering and localization fallback.
    pub code: Option<String>,
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
    pub filters_open: bool,
    pub filter_level: Option<LogLevel>,
    pub wrap_messages: bool,
    pub show_timestamps: bool,
    pub terminal_palette: bool,
    pub search_query: String,
    pub source_filter: Option<String>,
    pub column_widths: [f32; 2],
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
                code: None,
                block: None,
            }],
            auto_scroll: true,
            filters_open: false,
            filter_level: None,
            wrap_messages: false,
            show_timestamps: true,
            terminal_palette: false,
            search_query: String::new(),
            source_filter: None,
            column_widths: [76.0, 84.0],
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
            code: None,
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
            code: None,
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
            CommandLevel::Info if output.changed => LogLevel::Success,
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
            code: None,
            block: Some(ConsoleBlock {
                title: output.title,
                lines: output.lines,
                json,
            }),
        })
    }

    /// Appends an entry that already knows its severity, source and code.
    ///
    /// Runtime diagnostics use this path so the console keeps the stable code
    /// alongside the human message without changing [`CommandOutput`].
    pub fn log_structured(
        &mut self,
        level: LogLevel,
        source: impl Into<String>,
        message: impl Into<String>,
        code: Option<String>,
        block: Option<ConsoleBlock>,
    ) -> ConsoleEntryId {
        let id = self.allocate_entry_id();
        let source = source.into();
        self.append_entry(LogEntry {
            id,
            level,
            message: message.into(),
            timestamp: now_timestamp(),
            sender: (!source.is_empty()).then_some(source),
            code,
            block,
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
        self.entries
            .iter()
            .enumerate()
            .filter(|(_, entry)| self.entry_matches(entry))
    }

    /// Provides filtered entries keyed by their stable model IDs.
    pub fn filtered_entries_with_ids(&self) -> impl Iterator<Item = (ConsoleEntryId, &LogEntry)> {
        self.entries
            .iter()
            .filter(|entry| self.entry_matches(entry))
            .map(|entry| (entry.id, entry))
    }

    fn entry_matches(&self, entry: &LogEntry) -> bool {
        if let Some(filter) = self.filter_level {
            if entry.level != filter {
                return false;
            }
        }
        if let Some(source) = &self.source_filter {
            if entry.sender.as_deref() != Some(source.as_str()) {
                return false;
            }
        }
        let query = self.search_query.trim().to_ascii_lowercase();
        if !query.is_empty() {
            let message_matches = entry.message.to_ascii_lowercase().contains(&query);
            let source_matches = entry
                .sender
                .as_deref()
                .is_some_and(|sender| sender.to_ascii_lowercase().contains(&query));
            let code_matches = entry
                .code
                .as_deref()
                .is_some_and(|code| code.to_ascii_lowercase().contains(&query));
            if !message_matches && !source_matches && !code_matches {
                return false;
            }
        }
        true
    }

    /// Visible rows after all filters, collapsing consecutive plain duplicates
    /// into one line with a repeat count. Console spam becomes one `xN` row
    /// instead of dozens of identical entries.
    pub fn visible_groups(&self) -> Vec<(ConsoleEntryId, &LogEntry, usize)> {
        let mut groups: Vec<(ConsoleEntryId, &LogEntry, usize)> = Vec::new();
        for (id, entry) in self.filtered_entries_with_ids() {
            let repeats_previous = groups.last().is_some_and(|(_, last, _)| {
                last.block.is_none()
                    && entry.block.is_none()
                    && last.level == entry.level
                    && last.message == entry.message
                    && last.sender == entry.sender
                    && last.code == entry.code
            });
            if repeats_previous {
                if let Some((_, _, count)) = groups.last_mut() {
                    *count += 1;
                }
                continue;
            }
            groups.push((id, entry, 1));
        }
        groups
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

    pub fn set_wrap_messages(&mut self, enabled: bool) -> bool {
        if self.wrap_messages == enabled {
            return false;
        }
        self.wrap_messages = enabled;
        self.touch_content();
        true
    }

    pub fn set_show_timestamps(&mut self, enabled: bool) -> bool {
        if self.show_timestamps == enabled {
            return false;
        }
        self.show_timestamps = enabled;
        self.touch_content();
        true
    }

    pub fn set_search_query(&mut self, query: impl Into<String>) -> bool {
        let query = query.into();
        if self.search_query == query {
            return false;
        }
        self.search_query = query;
        self.touch_content();
        true
    }

    pub fn set_source_filter(&mut self, source: Option<String>) -> bool {
        if self.source_filter == source {
            return false;
        }
        self.source_filter = source;
        self.touch_content();
        true
    }

    pub fn toggle_source_filter(&mut self, source: &str) -> bool {
        if self.source_filter.as_deref() == Some(source) {
            self.set_source_filter(None)
        } else {
            self.set_source_filter(Some(source.to_string()))
        }
    }

    pub fn set_terminal_palette(&mut self, enabled: bool) -> bool {
        if self.terminal_palette == enabled {
            return false;
        }
        self.terminal_palette = enabled;
        self.touch_content();
        true
    }

    pub fn column_width(&self, index: usize) -> f32 {
        self.column_widths.get(index).copied().unwrap_or(76.0)
    }

    pub fn resize_column(&mut self, index: usize, delta: f32) -> bool {
        let Some(width) = self.column_widths.get_mut(index) else {
            return false;
        };
        let next = (*width + delta).clamp(44.0, 240.0);
        if (next - *width).abs() < f32::EPSILON {
            return false;
        }
        *width = next;
        true
    }

    pub fn reset_column_widths(&mut self) -> bool {
        if self.column_widths == [76.0, 84.0] {
            return false;
        }
        self.column_widths = [76.0, 84.0];
        true
    }

    pub fn error_count(&self) -> usize {
        self.entries
            .iter()
            .filter(|entry| entry.level == LogLevel::Error)
            .count()
    }

    /// Whether the active search matches this entry. Used by the surface to
    /// mark matched rows and tint the matched portion of the message.
    pub fn entry_has_search_match(&self, entry: &LogEntry) -> bool {
        let query = self.search_query.trim().to_ascii_lowercase();
        if query.is_empty() {
            return false;
        }
        entry.message.to_ascii_lowercase().contains(&query)
            || entry
                .sender
                .as_deref()
                .is_some_and(|sender| sender.to_ascii_lowercase().contains(&query))
            || entry
                .code
                .as_deref()
                .is_some_and(|code| code.to_ascii_lowercase().contains(&query))
    }

    /// Plain-text mirror of the visible log, suitable for files and clipboards.
    pub fn export_text(&self) -> String {
        let mut out = String::new();
        out.push_str("AuraRafi console log\n");
        for entry in self
            .entries
            .iter()
            .filter(|entry| self.entry_matches(entry))
        {
            out.push_str(&format!(
                "{} [{}]{} {}",
                entry.timestamp,
                entry.level.as_str().to_ascii_uppercase(),
                entry
                    .sender
                    .as_deref()
                    .map(|sender| format!(" {sender}:"))
                    .unwrap_or_default(),
                entry.message
            ));
            out.push('\n');
            if let Some(block) = &entry.block {
                for line in &block.lines {
                    out.push_str("  ");
                    out.push_str(line);
                    out.push('\n');
                }
                out.push_str(&block.json);
                out.push('\n');
            }
        }
        out
    }

    pub fn recent_command(&self, index: usize) -> Option<&String> {
        self.command_history.iter().rev().nth(index)
    }

    pub fn log_success(&mut self, message: impl Into<String>) -> ConsoleEntryId {
        self.log_with_id(LogLevel::Success, message)
    }

    pub fn log_verbose(&mut self, message: impl Into<String>) -> ConsoleEntryId {
        self.log_with_id(LogLevel::Verbose, message)
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
    chrono::Local::now().format("%H:%M:%S%.3f").to_string()
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

    #[test]
    fn runtime_severity_maps_to_console_levels() {
        assert_eq!(
            LogLevel::from_runtime_severity(RuntimeDiagnosticSeverity::Info),
            LogLevel::Info
        );
        assert_eq!(
            LogLevel::from_runtime_severity(RuntimeDiagnosticSeverity::Warning),
            LogLevel::Warning
        );
        assert_eq!(
            LogLevel::from_runtime_severity(RuntimeDiagnosticSeverity::Error),
            LogLevel::Error
        );
    }

    #[test]
    fn structured_entries_keep_source_and_code() {
        let mut console = ConsolePanel::default();
        let id = console.log_structured(
            LogLevel::Error,
            "runtime",
            "boom",
            Some("console.runtime_timeout".to_string()),
            None,
        );
        let entry = console.entry(id).expect("structured entry");
        assert_eq!(entry.sender.as_deref(), Some("runtime"));
        assert_eq!(entry.code.as_deref(), Some("console.runtime_timeout"));
        assert_eq!(entry.timestamp.len(), 12);
    }

    #[test]
    fn consecutive_duplicates_collapse_into_one_visible_group() {
        let mut console = ConsolePanel::default();
        console.log(LogLevel::Info, "Lighting unchanged.");
        console.log(LogLevel::Info, "Lighting unchanged.");
        console.log(LogLevel::Info, "Lighting unchanged.");
        console.log(LogLevel::Warning, "Lighting unchanged.");

        let groups = console.visible_groups();
        let repeated = groups
            .iter()
            .find(|(_, entry, _)| entry.message == "Lighting unchanged.");
        assert!(groups.iter().any(|(_, _, repeat)| *repeat == 3));
        assert!(groups
            .iter()
            .any(|(_, entry, repeat)| entry.level == LogLevel::Warning && *repeat == 1));
        assert!(repeated.is_some());
    }

    #[test]
    fn search_matches_message_source_and_code() {
        let mut console = ConsolePanel::default();
        console.log_structured(
            LogLevel::Info,
            "runtime",
            "Texture compiled",
            Some("asset.texture".to_string()),
            None,
        );
        assert!(console.set_search_query("texture"));
        assert_eq!(console.filtered_entries().count(), 1);
        assert!(console.set_search_query("runtime"));
        assert_eq!(console.filtered_entries().count(), 1);
        assert!(console.set_search_query("does-not-exist"));
        assert_eq!(console.filtered_entries().count(), 0);
        let entry = console
            .entries
            .iter()
            .find(|entry| entry.sender.as_deref() == Some("runtime"))
            .unwrap();
        assert!(!console.entry_has_search_match(entry));
        let entry_id = entry.id;
        console.set_search_query("texture");
        let entry = console.entry(entry_id).unwrap();
        assert!(console.entry_has_search_match(entry));
    }

    #[test]
    fn column_resize_is_bounded_and_resettable() {
        let mut console = ConsolePanel::default();
        assert!(console.resize_column(0, 40.0));
        assert_eq!(console.column_width(0), 116.0);
        assert!(console.resize_column(1, -500.0));
        assert_eq!(console.column_width(1), 44.0);
        assert!(console.reset_column_widths());
        assert_eq!(console.column_widths, [76.0, 84.0]);
    }

    #[test]
    fn export_text_keeps_timestamps_levels_sources_and_blocks() {
        let mut console = ConsolePanel::default();
        console.log_user("User", "/help");
        let text = console.export_text();
        assert!(text.contains("AuraRafi console log"));
        assert!(text.contains("[INFO] User: /help"));
    }

    #[test]
    fn console_ring_stays_bounded() {
        let mut console = ConsolePanel::default();
        for index in 0..MAX_ENTRIES + 16 {
            console.log(LogLevel::Info, format!("entry {index}"));
        }
        assert_eq!(console.entries.len(), MAX_ENTRIES);
        assert!(console.entries.first().is_some_and(|entry| entry.id > 0));
    }
}
