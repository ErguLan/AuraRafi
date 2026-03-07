//! Console panel - log output and messages.

use egui::Ui;

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
}

/// Console panel state.
pub struct ConsolePanel {
    pub entries: Vec<LogEntry>,
    pub auto_scroll: bool,
    pub filter_level: Option<LogLevel>,
}

impl Default for ConsolePanel {
    fn default() -> Self {
        Self {
            entries: vec![LogEntry {
                level: LogLevel::Info,
                message: "AuraRafi Engine initialized".to_string(),
                timestamp: "00:00:00".to_string(),
            }],
            auto_scroll: true,
            filter_level: None,
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
        });
    }

    /// Draw the console panel.
    pub fn show(&mut self, ui: &mut Ui) {
        ui.horizontal(|ui| {
            if ui.button("Clear").clicked() {
                self.entries.clear();
            }
            ui.checkbox(&mut self.auto_scroll, "Auto-scroll");
            ui.separator();
            if ui.selectable_label(self.filter_level.is_none(), "All").clicked() {
                self.filter_level = None;
            }
            if ui
                .selectable_label(self.filter_level == Some(LogLevel::Info), "Info")
                .clicked()
            {
                self.filter_level = Some(LogLevel::Info);
            }
            if ui
                .selectable_label(self.filter_level == Some(LogLevel::Warning), "Warn")
                .clicked()
            {
                self.filter_level = Some(LogLevel::Warning);
            }
            if ui
                .selectable_label(self.filter_level == Some(LogLevel::Error), "Error")
                .clicked()
            {
                self.filter_level = Some(LogLevel::Error);
            }
        });

        ui.separator();

        let scroll = egui::ScrollArea::vertical().auto_shrink([false, false]);
        let scroll = if self.auto_scroll {
            scroll.stick_to_bottom(true)
        } else {
            scroll
        };

        scroll.show(ui, |ui| {
            for entry in &self.entries {
                if let Some(filter) = &self.filter_level {
                    if entry.level != *filter {
                        continue;
                    }
                }

                let color = match entry.level {
                    LogLevel::Info => egui::Color32::from_rgb(180, 180, 190),
                    LogLevel::Warning => egui::Color32::from_rgb(230, 180, 60),
                    LogLevel::Error => egui::Color32::from_rgb(230, 80, 80),
                };

                ui.horizontal(|ui| {
                    ui.colored_label(
                        egui::Color32::from_rgb(100, 100, 110),
                        &entry.timestamp,
                    );
                    ui.colored_label(color, &entry.message);
                });
            }
        });
    }
}
