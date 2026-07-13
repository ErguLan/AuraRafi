//! Compact project-session inspector for the temporary egui shell.
//!
//! It deliberately owns no documents. The app remains the transaction boundary
//! for saving and activating a session, which keeps this panel disposable when
//! the retained `raf_ui` inspector takes over.

use egui::Ui;
use raf_core::config::Language;
use raf_core::i18n::t;
use raf_core::session::{ProjectSessionKind, ProjectSessionRegistry, SessionId};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SessionPanelAction {
    Activate(SessionId),
    Create(ProjectSessionKind),
}

#[derive(Default)]
pub struct SessionsPanel {
    new_name: String,
    new_kind: Option<ProjectSessionKind>,
}

impl SessionsPanel {
    pub fn show(
        &mut self,
        ui: &mut Ui,
        registry: &ProjectSessionRegistry,
        lang: Language,
    ) -> Option<SessionPanelAction> {
        let mut action = None;
        ui.label(
            egui::RichText::new(t("app.sessions", lang))
                .size(11.0)
                .strong()
                .color(egui::Color32::from_rgb(130, 130, 140)),
        );
        ui.separator();

        egui::ScrollArea::vertical()
            .id_salt("project_sessions_list")
            .max_height(180.0)
            .show(ui, |ui| {
                for session in &registry.sessions {
                    let is_active = session.id == registry.active_session;
                    let label = format!(
                        "{}  {}",
                        session.name,
                        session_kind_label(session.kind, lang)
                    );
                    if ui.selectable_label(is_active, label).clicked() && !is_active {
                        action = Some(SessionPanelAction::Activate(session.id));
                    }
                }
            });

        ui.add_space(8.0);
        ui.separator();
        ui.add_space(6.0);
        ui.label(
            egui::RichText::new(t("app.session_create", lang))
                .size(11.0)
                .strong(),
        );
        ui.add(
            egui::TextEdit::singleline(&mut self.new_name)
                .hint_text(t("app.session_name", lang))
                .desired_width(ui.available_width()),
        );

        let selected_kind = self.new_kind.unwrap_or(ProjectSessionKind::World);
        egui::ComboBox::from_id_salt("project_session_kind")
            .selected_text(session_kind_label(selected_kind, lang))
            .show_ui(ui, |ui| {
                for kind in [
                    ProjectSessionKind::World,
                    ProjectSessionKind::Interface,
                    ProjectSessionKind::ElectronicsDesign,
                ] {
                    ui.selectable_value(
                        &mut self.new_kind,
                        Some(kind),
                        session_kind_label(kind, lang),
                    );
                }
            });

        let can_create = !self.new_name.trim().is_empty();
        if ui
            .add_enabled(can_create, egui::Button::new(t("app.session_create", lang)))
            .clicked()
        {
            action = Some(SessionPanelAction::Create(selected_kind));
        }

        action
    }

    pub fn take_new_name(&mut self) -> String {
        std::mem::take(&mut self.new_name).trim().to_string()
    }
}

fn session_kind_label(kind: ProjectSessionKind, lang: Language) -> String {
    let key = match kind {
        ProjectSessionKind::World => "app.session_kind_world",
        ProjectSessionKind::Interface => "app.session_kind_interface",
        ProjectSessionKind::ElectronicsDesign => "app.session_kind_electronics",
    };
    t(key, lang)
}
