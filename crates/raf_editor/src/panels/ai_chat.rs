//! AI Chat panel - structure for AI interaction (not functional yet).

use egui::Ui;
use raf_ai::chat::{ChatPanel, MessageRole};
use raf_ai::provider::AiProvider;

use crate::theme;

/// State for the AI chat panel in the editor.
pub struct AiChatPanel {
    pub chat: ChatPanel,
    pub selected_provider: AiProvider,
}

impl Default for AiChatPanel {
    fn default() -> Self {
        Self {
            chat: ChatPanel::default(),
            selected_provider: AiProvider::OpenRouter,
        }
    }
}

impl AiChatPanel {
    /// Draw the AI chat panel.
    pub fn show(&mut self, ui: &mut Ui) {
        // Provider selector.
        ui.horizontal(|ui| {
            ui.label("Provider:");
            egui::ComboBox::from_id_salt("ai_provider_select")
                .selected_text(self.selected_provider.display_name())
                .show_ui(ui, |ui| {
                    for provider in AiProvider::all() {
                        ui.selectable_value(
                            &mut self.selected_provider,
                            *provider,
                            provider.display_name(),
                        );
                    }
                });
        });

        ui.separator();

        // Message area.
        let scroll = egui::ScrollArea::vertical()
            .auto_shrink([false, false])
            .stick_to_bottom(true);

        scroll.show(ui, |ui| {
            for msg in &self.chat.messages {
                let (prefix, color) = match msg.role {
                    MessageRole::User => ("You", theme::ACCENT),
                    MessageRole::Assistant => ("AI", egui::Color32::from_rgb(100, 200, 140)),
                    MessageRole::System => ("System", theme::DARK_TEXT_DIM),
                };

                ui.horizontal_wrapped(|ui| {
                    ui.colored_label(color, format!("[{}]", prefix));
                    ui.label(&msg.content);
                });
                ui.add_space(4.0);
            }
        });

        ui.separator();

        // Input area.
        ui.horizontal(|ui| {
            let available_width = ui.available_width() - 80.0;
            let response = ui.add_sized(
                [available_width, 28.0],
                egui::TextEdit::singleline(&mut self.chat.input_text)
                    .hint_text("Type a message..."),
            );

            let send_enabled = self.chat.is_available && !self.chat.input_text.is_empty();
            let send_button = ui.add_enabled(send_enabled, egui::Button::new("Send"));

            if !self.chat.is_available {
                // Show tooltip explaining AI is not yet available.
                if send_button.hovered() || response.hovered() {
                    egui::show_tooltip_at_pointer(ui.ctx(), ui.layer_id(), egui::Id::new("ai_tooltip"), |ui| {
                        ui.label("AI Integration: Haven't been developed yet");
                    });
                }
            }
        });
    }
}
