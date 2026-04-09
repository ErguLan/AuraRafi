//! AI Chat panel - multi-provider AI with OpenClaw as first functional backend.
//!
//! The provider dropdown lets users choose between OpenClaw (local, works now),
//! OpenRouter, OpenAI, GenAI, Claude (pending API key support).

use raf_core::Language;
use raf_core::i18n::t;
use egui::Ui;
use raf_ai::chat::{ChatMessage, ChatPanel, MessageRole};
use raf_ai::openclaw::{ConnectionStatus, OpenClawClient};
use raf_ai::provider::AiProvider;

use crate::theme;

/// State for the AI chat panel in the editor.
pub struct AiChatPanel {
    pub chat: ChatPanel,
    pub selected_provider: AiProvider,
    pub openclaw: OpenClawClient,
    /// Whether a send is currently in progress (for UI feedback).
    sending: bool,
    /// Language flag for translations.
    pub lang: Language,
}

impl Default for AiChatPanel {
    fn default() -> Self {
        let mut chat = ChatPanel::default();
        // Replace default system message with a useful one.
        chat.messages = vec![ChatMessage::system(
            "AI panel ready. Select OpenClaw to connect to your local AI, \
             or another provider when configured.",
        )];
        chat.is_available = true;

        Self {
            chat,
            selected_provider: AiProvider::OpenClaw,
            openclaw: OpenClawClient::default(),
            sending: false,
            lang: Language::English,
        }
    }
}

impl AiChatPanel {
    /// Draw the AI chat panel.
    pub fn show(&mut self, ui: &mut Ui) {
        let lang = self.lang;

        // Top bar: provider selector + connection status.
        ui.horizontal(|ui| {
            ui.label(t("app.provider", self.lang));
            egui::ComboBox::from_id_salt("ai_provider_select")
                .selected_text(self.selected_provider.display_name())
                .show_ui(ui, |ui| {
                    for provider in AiProvider::all() {
                        let label = format!(
                            "{}  {}",
                            provider.display_name(),
                            if *provider == AiProvider::OpenClaw {
                                ""
                            } else {
                                "(pending)"
                            }
                        );
                        ui.selectable_value(
                            &mut self.selected_provider,
                            *provider,
                            label,
                        );
                    }
                });

            ui.separator();

            // Connection status for OpenClaw.
            if self.selected_provider == AiProvider::OpenClaw {
                let status_text = if lang == raf_core::config::Language::Spanish {
                    self.openclaw.status_text_es()
                } else {
                    self.openclaw.status_text()
                };
                let status_color = match self.openclaw.status {
                    ConnectionStatus::Connected => egui::Color32::from_rgb(80, 200, 120),
                    ConnectionStatus::Error => egui::Color32::from_rgb(220, 80, 80),
                    ConnectionStatus::Sending => theme::ACCENT,
                    ConnectionStatus::Disconnected => theme::DARK_TEXT_DIM,
                };
                ui.colored_label(status_color, status_text);

                let connect_label = t("app.connect", self.lang);
                if ui.button(connect_label).clicked() {
                    let connected = self.openclaw.ping();
                    if connected {
                        self.chat.messages.push(ChatMessage::system(
                            &t("app.connected_to_openclaw_at_localhost_18789", self.lang),
                        ));
                    } else {
                        self.chat.messages.push(ChatMessage::system(
                            &t("app.could_not_connect_make_sure_openclaw_is_running_openclaw_gateway_start", self.lang),
                        ));
                    }
                }
            } else {
                // Other providers: show "pending" message.
                ui.colored_label(
                    theme::DARK_TEXT_DIM,
                    t("app.pending_configure_your_api_key", self.lang),
                );
            }
        });

        // OpenClaw URL config (only shown when OpenClaw selected).
        if self.selected_provider == AiProvider::OpenClaw {
            ui.horizontal(|ui| {
                ui.label("URL:");
                ui.add(
                    egui::TextEdit::singleline(&mut self.openclaw.config.gateway_url)
                        .desired_width(200.0)
                        .hint_text("http://localhost:18789"),
                );
                ui.label("Token:");
                ui.add(
                    egui::TextEdit::singleline(&mut self.openclaw.config.token)
                        .desired_width(120.0)
                        .password(true)
                        .hint_text(t("app.optional", self.lang)),
                );
            });
        }

        ui.separator();

        // Message area.
        let scroll = egui::ScrollArea::vertical()
            .auto_shrink([false, false])
            .stick_to_bottom(true);

        scroll.show(ui, |ui| {
            for msg in &self.chat.messages {
                let (prefix, color) = match msg.role {
                    MessageRole::User => (
                        t("app.you", self.lang),
                        theme::ACCENT,
                    ),
                    MessageRole::Assistant => ("AI".to_string(), egui::Color32::from_rgb(100, 200, 140)),
                    MessageRole::System => ("System".to_string(), theme::DARK_TEXT_DIM),
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
            let hint = t("app.type_a_message", self.lang);
            let response = ui.add_sized(
                [available_width, 28.0],
                egui::TextEdit::singleline(&mut self.chat.input_text)
                    .hint_text(hint),
            );

            // Send enabled only for OpenClaw (connected) or future providers (configured).
            let can_send = match self.selected_provider {
                AiProvider::OpenClaw => {
                    self.openclaw.status == ConnectionStatus::Connected && !self.sending
                }
                _ => false, // Other providers not yet implemented.
            };

            let send_enabled = can_send && !self.chat.input_text.is_empty();
            let send_label = t("app.send", self.lang);
            let send_button = ui.add_enabled(
                send_enabled,
                egui::Button::new(send_label),
            );

            // Send on Enter key or button click.
            let enter_pressed = response.lost_focus()
                && ui.input(|i| i.key_pressed(egui::Key::Enter));

            if (send_button.clicked() || enter_pressed) && send_enabled {
                let input = self.chat.input_text.clone();
                self.chat.input_text.clear();

                // Add user message to chat.
                self.chat.messages.push(ChatMessage::user(&input));

                // Send to OpenClaw.
                if self.selected_provider == AiProvider::OpenClaw {
                    // self.sending = true;
                    match self.openclaw.send_message(&input) {
                        Ok(response_text) => {
                            self.chat.messages.push(ChatMessage::assistant(&response_text));
                        }
                        Err(err) => {
                            self.chat.messages.push(ChatMessage::system(
                                &format!(
                                    "{}: {}",
                                    t("app.error", self.lang),
                                    err
                                ),
                            ));
                        }
                    }
                    self.sending = false;
                }
            }

            // Tooltip when provider not available.
            if !can_send && self.selected_provider != AiProvider::OpenClaw {
                if send_button.hovered() || response.hovered() {
                    egui::show_tooltip_at_pointer(
                        ui.ctx(),
                        ui.layer_id(),
                        egui::Id::new("ai_tooltip"),
                        |ui| {
                            ui.label(t("app.this_provider_is_not_implemented_yet_use_openclaw", self.lang));
                        },
                    );
                }
            }
        });
    }
}
