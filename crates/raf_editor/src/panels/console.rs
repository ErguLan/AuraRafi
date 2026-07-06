//! Console panel - log output and messages.

use egui::Ui;
use raf_core::config::Language;
use raf_core::i18n::t;

use crate::commands::{CommandLevel, CommandOutput};

/// Log severity.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LogLevel {
    Info,
    Warning,
    Error,
}

/// A console log entry.
#[derive(Debug, Clone)]
pub struct LogEntry {
    pub level: LogLevel,
    pub message: String,
    pub timestamp: String,
    pub sender: Option<String>,
    pub block: Option<ConsoleBlock>,
}

/// Structured command output rendered as a compact console card.
#[derive(Debug, Clone)]
pub struct ConsoleBlock {
    pub title: String,
    pub lines: Vec<String>,
    pub json: String,
}

/// A submitted console message.
#[derive(Debug, Clone)]
pub struct ConsoleSubmission {
    pub text: String,
}

/// Console panel state.
pub struct ConsolePanel {
    pub entries: Vec<LogEntry>,
    pub auto_scroll: bool,
    pub filter_level: Option<LogLevel>,
    input: String,
    pub command_history: Vec<String>,
    history_cursor: Option<usize>,
}

impl Default for ConsolePanel {
    fn default() -> Self {
        Self {
            entries: vec![LogEntry {
                level: LogLevel::Info,
                message: "AuraRafi Engine initialized".to_string(),
                timestamp: "00:00:00".to_string(),
                sender: None,
                block: None,
            }],
            auto_scroll: true,
            filter_level: None,
            input: String::new(),
            command_history: Vec::new(),
            history_cursor: None,
        }
    }
}

impl ConsolePanel {
    /// Add a log entry.
    pub fn log(&mut self, level: LogLevel, message: &str) {
        self.entries.push(LogEntry {
            level,
            message: message.to_string(),
            timestamp: chrono::Local::now().format("%H:%M:%S").to_string(),
            sender: None,
            block: None,
        });
        self.trim_entries();
    }

    pub fn log_user(&mut self, sender: &str, message: &str) {
        self.entries.push(LogEntry {
            level: LogLevel::Info,
            message: message.to_string(),
            timestamp: chrono::Local::now().format("%H:%M:%S").to_string(),
            sender: Some(sender.to_string()),
            block: None,
        });
        if message.trim_start().starts_with('/') {
            self.command_history.push(message.trim().to_string());
            if self.command_history.len() > 100 {
                self.command_history.remove(0);
            }
        }
        self.history_cursor = None;
        self.trim_entries();
    }

    pub fn log_command_output(&mut self, output: CommandOutput) {
        let level = match output.level {
            CommandLevel::Info => LogLevel::Info,
            CommandLevel::Warning => LogLevel::Warning,
            CommandLevel::Error => LogLevel::Error,
        };
        let json = serde_json::to_string_pretty(&output.json).unwrap_or_else(|_| "{}".to_string());
        self.entries.push(LogEntry {
            level,
            message: output.title.clone(),
            timestamp: chrono::Local::now().format("%H:%M:%S").to_string(),
            sender: None,
            block: Some(ConsoleBlock {
                title: output.title,
                lines: output.lines,
                json,
            }),
        });
        self.trim_entries();
    }

    pub fn clear_entries(&mut self) {
        self.entries.clear();
    }

    pub fn history_lines(&self) -> Vec<String> {
        self.command_history
            .iter()
            .rev()
            .take(40)
            .cloned()
            .collect()
    }

    pub fn show(
        &mut self,
        ui: &mut Ui,
        lang: Language,
        input_enabled: bool,
        command_names: &[String],
    ) -> Vec<ConsoleSubmission> {
        let mut submissions = Vec::new();

        ui.horizontal(|ui| {
            // Subtle clear button
            let clear_btn = egui::Button::new(
                egui::RichText::new(t("app.clear", lang))
                    .size(11.0)
                    .color(egui::Color32::from_rgb(180, 180, 190)),
            )
            .fill(egui::Color32::from_rgb(34, 34, 38))
            .rounding(4.0);

            if ui.add(clear_btn).clicked() {
                self.entries.clear();
            }

            ui.add_space(8.0);

            // Clean auto-scroll toggle
            ui.checkbox(
                &mut self.auto_scroll,
                egui::RichText::new(t("app.auto_scroll", lang))
                    .size(11.0)
                    .color(egui::Color32::from_rgb(150, 150, 160)),
            );

            ui.add_space(8.0);
            ui.separator();
            ui.add_space(8.0);

            // Professional segmented toggle look for filters
            let filters = [
                (None, t("app.all", lang)),
                (Some(LogLevel::Info), t("app.info", lang)),
                (Some(LogLevel::Warning), t("app.warn", lang)),
                (Some(LogLevel::Error), t("app.error", lang)),
            ];

            for (level, label) in filters {
                let is_active = self.filter_level == level;

                // Muted pill design for filters
                let text_color = if is_active {
                    egui::Color32::WHITE
                } else {
                    egui::Color32::from_rgb(130, 130, 140)
                };

                let bg_color = if is_active {
                    egui::Color32::from_rgb(65, 65, 75) // Neutral active state, no bright orange
                } else {
                    egui::Color32::TRANSPARENT
                };

                let btn =
                    egui::Button::new(egui::RichText::new(label).size(11.0).color(text_color))
                        .fill(bg_color)
                        .frame(is_active)
                        .rounding(4.0)
                        .min_size(egui::Vec2::new(32.0, 18.0));

                if ui.add(btn).clicked() {
                    self.filter_level = level;
                }
            }
        });

        ui.separator();

        if input_enabled {
            self.show_input_row(ui, lang, command_names, &mut submissions);
            ui.add_space(6.0);
        } else {
            ui.label(
                egui::RichText::new(t("console.commands_disabled", lang))
                    .size(11.0)
                    .color(egui::Color32::from_rgb(140, 140, 150)),
            );
            ui.add_space(6.0);
        }

        let scroll = egui::ScrollArea::vertical().auto_shrink([false, false]);
        let scroll = if self.auto_scroll {
            scroll.stick_to_bottom(true)
        } else {
            scroll
        };

        scroll.show(ui, |ui| {
            for (entry_index, entry) in self.entries.iter().enumerate() {
                if let Some(filter) = &self.filter_level {
                    if entry.level != *filter {
                        continue;
                    }
                }

                ui.push_id(("console_entry", entry_index), |ui| {
                    let color = match entry.level {
                        LogLevel::Info => egui::Color32::from_rgb(180, 180, 190),
                        LogLevel::Warning => egui::Color32::from_rgb(230, 180, 60),
                        LogLevel::Error => egui::Color32::from_rgb(230, 80, 80),
                    };

                    if let Some(block) = &entry.block {
                        draw_block(ui, entry, block, color, lang);
                    } else {
                        ui.horizontal(|ui| {
                            ui.colored_label(
                                egui::Color32::from_rgb(100, 100, 110),
                                &entry.timestamp,
                            );
                            if let Some(sender) = &entry.sender {
                                ui.colored_label(
                                    egui::Color32::from_rgb(245, 165, 65),
                                    format!("{sender}:"),
                                );
                            }
                            ui.colored_label(color, &entry.message);
                        });
                    }
                });
            }
        });

        submissions
    }

    fn show_input_row(
        &mut self,
        ui: &mut Ui,
        lang: Language,
        command_names: &[String],
        submissions: &mut Vec<ConsoleSubmission>,
    ) {
        ui.horizontal(|ui| {
            ui.label(
                egui::RichText::new("User1")
                    .size(11.0)
                    .strong()
                    .color(egui::Color32::from_rgb(245, 165, 65)),
            );
            let response = ui.add(
                egui::TextEdit::singleline(&mut self.input)
                    .hint_text(t("console.input_hint", lang))
                    .desired_width(f32::INFINITY),
            );

            if response.has_focus() {
                if ui.input(|input| input.key_pressed(egui::Key::Tab)) {
                    self.autocomplete(command_names);
                }
                if ui.input(|input| input.key_pressed(egui::Key::ArrowUp)) {
                    self.history_previous();
                }
                if ui.input(|input| input.key_pressed(egui::Key::ArrowDown)) {
                    self.history_next();
                }
            }

            let enter_pressed =
                response.lost_focus() && ui.input(|input| input.key_pressed(egui::Key::Enter));
            let send_clicked = ui
                .add(
                    egui::Button::new(
                        egui::RichText::new(t("console.send", lang))
                            .size(11.0)
                            .color(egui::Color32::WHITE),
                    )
                    .fill(egui::Color32::from_rgb(190, 115, 30))
                    .rounding(4.0),
                )
                .clicked();

            if enter_pressed || send_clicked {
                let text = self.input.trim().to_string();
                if !text.is_empty() {
                    submissions.push(ConsoleSubmission { text });
                    self.input.clear();
                }
            }
        });

        if self.input.trim_start().starts_with('/') {
            ui.label(
                egui::RichText::new(t("console.tab_hint", lang))
                    .size(10.0)
                    .color(egui::Color32::from_rgb(120, 120, 130)),
            );
        }
    }

    fn autocomplete(&mut self, command_names: &[String]) {
        let prefix = self.input.trim();
        if !prefix.starts_with('/') {
            return;
        }
        let matches = command_names
            .iter()
            .filter(|name| name.starts_with(prefix))
            .cloned()
            .collect::<Vec<_>>();
        if let Some(completion) = matches.first() {
            self.input = format!("{completion} ");
        }
    }

    fn history_previous(&mut self) {
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

    fn history_next(&mut self) {
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

    fn trim_entries(&mut self) {
        const MAX_ENTRIES: usize = 500;
        if self.entries.len() > MAX_ENTRIES {
            let remove_count = self.entries.len() - MAX_ENTRIES;
            self.entries.drain(0..remove_count);
        }
    }
}

fn draw_block(
    ui: &mut Ui,
    entry: &LogEntry,
    block: &ConsoleBlock,
    color: egui::Color32,
    lang: Language,
) {
    let frame = egui::Frame::none()
        .fill(egui::Color32::from_rgb(18, 18, 22))
        .rounding(6.0)
        .inner_margin(8.0)
        .stroke(egui::Stroke::new(1.0, egui::Color32::from_rgb(45, 45, 52)));

    frame.show(ui, |ui| {
        ui.horizontal(|ui| {
            ui.colored_label(egui::Color32::from_rgb(100, 100, 110), &entry.timestamp);
            ui.colored_label(color, egui::RichText::new(&block.title).strong());
        });
        ui.add_space(4.0);
        for line in &block.lines {
            ui.label(
                egui::RichText::new(line)
                    .monospace()
                    .size(10.0)
                    .color(egui::Color32::from_rgb(190, 190, 200)),
            );
        }
        ui.collapsing(t("console.json", lang), |ui| {
            ui.label(
                egui::RichText::new(&block.json)
                    .monospace()
                    .size(10.0)
                    .color(egui::Color32::from_rgb(155, 165, 180)),
            );
        });
    });
}
