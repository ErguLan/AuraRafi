//! Console panel - log output and messages.

use egui::Ui;
use raf_core::config::Language;
use raf_core::i18n::t;

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

    pub fn show(&mut self, ui: &mut Ui, lang: Language) {
        ui.horizontal(|ui| {
            // Subtle clear button
            let clear_btn = egui::Button::new(
                egui::RichText::new(t("app.clear", lang)).size(11.0).color(egui::Color32::from_rgb(180, 180, 190)),
            )
            .fill(egui::Color32::from_rgb(34, 34, 38))
            .rounding(4.0);

            if ui.add(clear_btn).clicked() {
                self.entries.clear();
            }

            ui.add_space(8.0);
            
            // Clean auto-scroll toggle
            ui.checkbox(&mut self.auto_scroll, egui::RichText::new(t("app.auto_scroll", lang)).size(11.0).color(egui::Color32::from_rgb(150, 150, 160)));
            
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
                
                let btn = egui::Button::new(
                    egui::RichText::new(label).size(11.0).color(text_color),
                )
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
