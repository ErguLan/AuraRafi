//! AI Chat panel - multi-provider AI with OpenClaw as first functional backend.
//!
//! The provider dropdown lets users choose between OpenClaw (local, works now),
//! OpenRouter, OpenAI, GenAI, Claude (pending API key support).

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
    pub is_es: bool,
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
            is_es: false,
        }
    }
}

impl AiChatPanel {
    /// Draw the AI chat panel.
    pub fn show(&mut self, ui: &mut Ui) {
        let is_es = self.is_es;

        // Top bar: provider selector + connection status.
        ui.horizontal(|ui| {
            ui.label(if is_es { "Proveedor:" } else { "Provider:" });
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
                let status_text = if is_es {
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

                let connect_label = if is_es { "Conectar" } else { "Connect" };
                if ui.button(connect_label).clicked() {
                    let connected = self.openclaw.ping();
                    if connected {
                        self.chat.messages.push(ChatMessage::system(
                            if is_es {
                                "Conectado a OpenClaw en localhost:18789"
                            } else {
                                "Connected to OpenClaw at localhost:18789"
                            },
                        ));
                    } else {
                        self.chat.messages.push(ChatMessage::system(
                            if is_es {
                                "No se pudo conectar. Asegurate de que OpenClaw esta corriendo \
                                 (openclaw gateway start)"
                            } else {
                                "Could not connect. Make sure OpenClaw is running \
                                 (openclaw gateway start)"
                            },
                        ));
                    }
                }
            } else {
                // Other providers: show "pending" message.
                ui.colored_label(
                    theme::DARK_TEXT_DIM,
                    if is_es {
                        "Pendiente - configura tu API key"
                    } else {
                        "Pending - configure your API key"
                    },
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
                        .hint_text(if is_es { "opcional" } else { "optional" }),
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
                        if is_es { "Tu" } else { "You" },
                        theme::ACCENT,
                    ),
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
            let hint = if is_es { "Escribe un mensaje..." } else { "Type a message..." };
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
            let send_label = if is_es { "Enviar" } else { "Send" };
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
                    self.sending = true;
                    match self.openclaw.send_message(&input) {
                        Ok(response_text) => {
                            self.chat.messages.push(ChatMessage::assistant(&response_text));
                        }
                        Err(err) => {
                            self.chat.messages.push(ChatMessage::system(
                                &format!(
                                    "{}: {}",
                                    if is_es { "Error" } else { "Error" },
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
                            ui.label(if is_es {
                                "Este proveedor aun no esta implementado. Usa OpenClaw."
                            } else {
                                "This provider is not implemented yet. Use OpenClaw."
                            });
                        },
                    );
                }
            }
        });
    }
}
