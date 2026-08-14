//! Action/presentation host for the RafUI application bar.

use eframe::{egui, egui_wgpu};
use raf_core::config::Language;
use raf_core::i18n::t;
use raf_render::api_graphic_basic::ui_surface::{
    StudioUiPalette, UiAction, UiControlState, UiDispatchedAction,
};
use raf_ui::{UiMotionSpec, UiTween};

use crate::application_bar_surface::{
    build_application_bar_surface, build_application_menu_popup_surface, AgentBarStatus,
    APPLICATION_MENU_POPUP_WIDTH,
};
use crate::application_menu::{build_application_menu, ApplicationMenuState};
use crate::panels::raf_ui_surface_bridge::RafUiSurfaceBridge;

pub struct ApplicationBarHost {
    surface: RafUiSurfaceBridge,
    menu_surface: RafUiSurfaceBridge,
    command_query: String,
    open_menu: Option<String>,
    menu_motion: UiTween,
    last_menu_time_seconds: f64,
}

impl Default for ApplicationBarHost {
    fn default() -> Self {
        let mut surface = RafUiSurfaceBridge::new("raf_ui_application_bar");
        for (key, bytes) in [
            (
                "editor.top.logo",
                // Reuse the same Rafi brand asset shown by the project Hub.
                include_bytes!("../../../editor/icon.png") as &[u8],
            ),
            (
                "editor.top.file",
                include_bytes!("../../../editor/assets/ui_icons/top/file.png") as &[u8],
            ),
            (
                "editor.top.edit",
                include_bytes!("../../../editor/assets/ui_icons/top/edit.png") as &[u8],
            ),
            (
                "editor.top.view",
                include_bytes!("../../../editor/assets/ui_icons/top/view.png") as &[u8],
            ),
            (
                "editor.top.project",
                include_bytes!("../../../editor/assets/ui_icons/top/project.png") as &[u8],
            ),
            (
                "editor.top.help",
                include_bytes!("../../../editor/assets/ui_icons/top/help.png") as &[u8],
            ),
            (
                "editor.top.save",
                include_bytes!("../../../editor/assets/ui_icons/top/save.png") as &[u8],
            ),
            (
                "editor.top.minimize",
                include_bytes!("../../../editor/assets/ui_icons/top/minimize.png") as &[u8],
            ),
            (
                "editor.top.maximize",
                include_bytes!("../../../editor/assets/ui_icons/top/maximize.png") as &[u8],
            ),
            (
                "editor.top.close",
                include_bytes!("../../../editor/assets/ui_icons/top/close.png") as &[u8],
            ),
        ] {
            let _ = surface.register_embedded_png(key, bytes);
        }
        Self {
            surface,
            menu_surface: RafUiSurfaceBridge::new("raf_ui_application_menu"),
            command_query: String::new(),
            open_menu: None,
            menu_motion: UiTween::new(0.0, UiMotionSpec::dock()),
            last_menu_time_seconds: 0.0,
        }
    }
}

impl ApplicationBarHost {
    pub fn application_menu(&self, state: ApplicationMenuState) -> raf_ui::UiApplicationMenu {
        build_application_menu(state)
    }

    pub fn command_query(&self) -> &str {
        &self.command_query
    }

    pub fn show(
        &mut self,
        ui: &mut egui::Ui,
        render_state: Option<&egui_wgpu::RenderState>,
        palette: StudioUiPalette,
        language: Language,
        project_name: &str,
        menu_state: ApplicationMenuState,
        agent_status: AgentBarStatus,
    ) -> Vec<String> {
        let surface = build_application_bar_surface(
            palette,
            project_name,
            menu_state.project_type,
            self.open_menu.as_deref(),
            agent_status,
        );
        let bar_rect = ui.available_rect_before_wrap();
        let actions = self.surface.show_with_control_state(
            ui,
            render_state,
            palette,
            surface,
            |controls: &mut UiControlState| {
                if !controls.has_text("application-bar.command-search.value") {
                    controls.set_text(
                        "application-bar.command-search.value",
                        self.command_query.clone(),
                        256,
                    );
                }
            },
            |key| t(key, language),
        );
        let mut commands = Vec::new();
        for action in actions {
            match action.action {
                UiAction::Command { name } if name.starts_with("application.menu.") => {
                    let menu_id = name.trim_start_matches("application.menu.").to_string();
                    if self.open_menu.as_deref() == Some(menu_id.as_str()) {
                        self.open_menu = None;
                    } else {
                        self.open_menu = Some(menu_id);
                    }
                }
                UiAction::Command { name } => commands.push(name),
                UiAction::SetText { key, value }
                    if key == "application-bar.command-search.value" =>
                {
                    self.command_query = value;
                }
                _ => {}
            }
        }

        let now_seconds = ui.ctx().input(|input| input.time);
        let delta_seconds = (now_seconds - self.last_menu_time_seconds)
            .max(0.0)
            .min(0.25) as f32;
        self.last_menu_time_seconds = now_seconds;
        self.menu_motion
            .set_target(if self.open_menu.is_some() { 1.0 } else { 0.0 });
        let progress = self.menu_motion.advance(delta_seconds, false);
        if let Some(menu_id) = self.open_menu.clone() {
            let menu = build_application_menu(menu_state)
                .menus
                .into_iter()
                .find(|menu| menu.id == menu_id);
            if let Some(menu) = menu {
                let x = application_menu_anchor_x(&menu.id);
                let y = bar_rect.bottom() + (1.0 - progress) * -6.0;
                let menu_surface = build_application_menu_popup_surface(palette, &menu);
                let popup_rect = egui::Rect::from_min_size(
                    egui::pos2(bar_rect.left() + x, y),
                    egui::vec2(APPLICATION_MENU_POPUP_WIDTH, application_menu_height(&menu)),
                );
                let popup_actions = egui::Area::new(egui::Id::new("rafui.application-menu"))
                    .order(egui::Order::Foreground)
                    .fixed_pos(popup_rect.min)
                    .show(ui.ctx(), |popup_ui| {
                        popup_ui
                            .allocate_ui_with_layout(
                                popup_rect.size(),
                                egui::Layout::top_down(egui::Align::Min),
                                |popup_ui| {
                                    self.menu_surface.show_transparent(
                                        popup_ui,
                                        render_state,
                                        palette,
                                        menu_surface,
                                        |key| t(key, language),
                                    )
                                },
                            )
                            .inner
                    })
                    .inner;
                for action in popup_actions {
                    if let UiAction::Command { name } = action.action {
                        commands.push(name);
                        self.open_menu = None;
                    }
                }
                let pointer = ui.ctx().input(|input| input.pointer.interact_pos());
                let outside_pressed = ui.ctx().input(|input| {
                    input.pointer.any_pressed()
                        && !pointer.is_some_and(|position| popup_rect.contains(position))
                });
                if outside_pressed || ui.ctx().input(|input| input.key_pressed(egui::Key::Escape)) {
                    self.open_menu = None;
                }
            } else {
                self.open_menu = None;
            }
        }
        if !self.menu_motion.is_settled() {
            ui.ctx().request_repaint();
        }
        commands
    }

    #[allow(dead_code)]
    fn command_ids(actions: Vec<UiDispatchedAction>) -> Vec<String> {
        actions
            .into_iter()
            .filter_map(|action| match action.action {
                UiAction::Command { name } => Some(name),
                _ => None,
            })
            .collect()
    }
}

fn application_menu_anchor_x(menu_id: &str) -> f32 {
    match menu_id {
        "file" => 239.0,
        "edit" => 299.0,
        "view" => 359.0,
        "project" => 423.0,
        "help" => 503.0,
        _ => 239.0,
    }
}

fn application_menu_height(menu: &raf_ui::UiMenu) -> f32 {
    8.0 + menu
        .items
        .iter()
        .map(|item| match item {
            raf_ui::UiMenuItem::Separator => 8.0,
            raf_ui::UiMenuItem::Command(_) | raf_ui::UiMenuItem::Submenu(_) => 30.0,
        })
        .sum::<f32>()
        + 8.0
}
