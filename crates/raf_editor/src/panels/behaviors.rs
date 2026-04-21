//! External script binding UI for the Properties panel.

use std::path::{Path, PathBuf};

use egui::{Color32, Stroke, Ui};
use raf_core::config::Language;
use raf_core::i18n::t;
use raf_core::scene::graph::SceneNodeId;
use raf_core::scene::SceneGraph;

use crate::script_support::{
    open_path_in_file_manager, open_script_in_external_editor, scan_script_catalog,
    script_file_name, validate_attached_script, ScriptCatalogEntry,
};
use crate::ui_icons::UiIconAtlas;

#[derive(Default)]
pub struct ScriptBindingsState {
    available_scripts: Vec<ScriptCatalogEntry>,
    search_filter: String,
    show_attach_window: bool,
    needs_rescan: bool,
    last_assets_root: Option<PathBuf>,
}

impl ScriptBindingsState {
    fn ensure_catalog(&mut self, assets_root: Option<&Path>) {
        let next_root = assets_root.map(Path::to_path_buf);
        if !self.needs_rescan && self.last_assets_root == next_root {
            return;
        }

        self.available_scripts = next_root
            .as_ref()
            .map(|root| scan_script_catalog(root))
            .unwrap_or_default();
        self.last_assets_root = next_root;
        self.needs_rescan = false;
    }
}

pub fn show_attached_behaviors(
    ui: &mut Ui,
    scene: &mut SceneGraph,
    id: SceneNodeId,
    lang: Language,
    assets_root: Option<&Path>,
    icons: &UiIconAtlas,
    state: &mut ScriptBindingsState,
) -> bool {
    state.ensure_catalog(assets_root);

    let node = match scene.get_mut(id) {
        Some(node) => node,
        None => return false,
    };

    let mut changed = false;
    let attached_scripts = node.scripts.clone();
    let mut remove_index = None;

    ui.add_space(8.0);
    ui.horizontal(|ui| {
        if let Some(icon) = icons.get("behaviors.png") {
            ui.add(egui::Image::new(icon).fit_to_exact_size(egui::vec2(16.0, 16.0)));
        }
        ui.label(
            egui::RichText::new(t("app.attached_behaviors", lang))
                .size(12.0)
                .color(egui::Color32::from_gray(120))
                .strong(),
        );
    });
    ui.add_space(4.0);

    egui::Frame::none()
        .fill(Color32::from_rgb(22, 22, 26))
        .rounding(8.0)
        .inner_margin(12.0)
        .stroke(Stroke::new(1.0, Color32::from_rgb(42, 42, 48)))
        .show(ui, |ui| {
            ui.label(
                egui::RichText::new(t("app.script_external_hint", lang))
                    .size(10.0)
                    .color(Color32::from_rgb(120, 120, 130)),
            );
            ui.add_space(8.0);

            if attached_scripts.is_empty() {
                ui.label(
                    egui::RichText::new(t("app.no_behaviors", lang))
                        .size(12.0)
                        .color(egui::Color32::from_gray(100)),
                );
            } else {
                for (index, script_path) in attached_scripts.iter().enumerate() {
                    let validation = validate_attached_script(assets_root, script_path);
                    let file_name = script_file_name(script_path);

                    egui::Frame::none()
                        .fill(Color32::from_rgb(28, 28, 33))
                        .rounding(6.0)
                        .inner_margin(8.0)
                        .stroke(Stroke::new(1.0, Color32::from_rgb(44, 44, 52)))
                        .show(ui, |ui| {
                            ui.horizontal(|ui| {
                                if let Some(icon) = icons.get("script.png") {
                                    ui.add(
                                        egui::Image::new(icon)
                                            .fit_to_exact_size(egui::vec2(16.0, 16.0)),
                                    );
                                }

                                ui.vertical(|ui| {
                                    ui.label(
                                        egui::RichText::new(file_name)
                                            .size(11.0)
                                            .color(Color32::from_rgb(225, 225, 230)),
                                    );
                                    ui.label(
                                        egui::RichText::new(script_path)
                                            .size(9.0)
                                            .color(Color32::from_rgb(110, 110, 120)),
                                    );
                                });

                                ui.add_space(6.0);
                                script_chip(ui, validation.language.label(), Color32::from_rgb(35, 35, 42), Color32::from_rgb(190, 190, 198));

                                let (status_label, status_fill, status_text) = if !validation.exists {
                                    (
                                        t("app.script_missing", lang),
                                        Color32::from_rgb(70, 32, 32),
                                        Color32::from_rgb(230, 140, 140),
                                    )
                                } else if !validation.supported {
                                    (
                                        t("app.script_unsupported", lang),
                                        Color32::from_rgb(66, 55, 24),
                                        Color32::from_rgb(225, 190, 110),
                                    )
                                } else if !validation.has_on_start && !validation.has_on_update {
                                    (
                                        t("app.script_no_entry_points", lang),
                                        Color32::from_rgb(62, 46, 24),
                                        Color32::from_rgb(225, 180, 120),
                                    )
                                } else {
                                    (
                                        t("app.script_ready", lang),
                                        Color32::from_rgb(24, 58, 38),
                                        Color32::from_rgb(132, 220, 170),
                                    )
                                };
                                script_chip(ui, &status_label, status_fill, status_text);

                                if validation.exists && (validation.has_on_start || validation.has_on_update) {
                                    let hooks_label = match (validation.has_on_start, validation.has_on_update) {
                                        (true, true) => "on_start + on_update",
                                        (true, false) => "on_start",
                                        (false, true) => "on_update",
                                        (false, false) => "-",
                                    };
                                    script_chip(ui, hooks_label, Color32::from_rgb(32, 42, 55), Color32::from_rgb(170, 200, 230));
                                }

                                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                    let remove_btn = egui::Button::new(
                                        egui::RichText::new("X")
                                            .size(11.0)
                                            .color(Color32::from_rgb(180, 110, 110)),
                                    )
                                    .frame(false);
                                    if ui.add(remove_btn).clicked() {
                                        remove_index = Some(index);
                                    }

                                    let reveal_btn = egui::Button::new(
                                        egui::RichText::new(t("app.script_reveal_folder", lang))
                                            .size(10.0),
                                    )
                                    .rounding(4.0);
                                    if ui
                                        .add_enabled(validation.absolute_path.is_some(), reveal_btn)
                                        .clicked()
                                    {
                                        if let Some(path) = &validation.absolute_path {
                                            let _ = open_path_in_file_manager(path);
                                        }
                                    }

                                    let open_btn = egui::Button::new(
                                        egui::RichText::new(t("app.script_open_external", lang))
                                            .size(10.0),
                                    )
                                    .rounding(4.0);
                                    if ui
                                        .add_enabled(validation.absolute_path.is_some(), open_btn)
                                        .clicked()
                                    {
                                        if let Some(path) = &validation.absolute_path {
                                            let _ = open_script_in_external_editor(path);
                                        }
                                    }
                                });
                            });
                        });
                    ui.add_space(4.0);
                }
            }

            if let Some(index) = remove_index {
                node.scripts.remove(index);
                changed = true;
            }

            ui.add_space(8.0);
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let attach_btn = egui::Button::new(t("app.attach_script", lang)).rounding(4.0);
                if ui.add_sized([120.0, 24.0], attach_btn).clicked() {
                    state.show_attach_window = true;
                }

                let rescan_btn = egui::Button::new(t("app.script_rescan", lang)).rounding(4.0);
                if ui.add_sized([84.0, 24.0], rescan_btn).clicked() {
                    state.needs_rescan = true;
                    state.ensure_catalog(assets_root);
                }

                let folder_btn = egui::Button::new(t("app.script_scripts_folder", lang)).rounding(4.0);
                if ui
                    .add_enabled(assets_root.is_some(), folder_btn)
                    .clicked()
                {
                    if let Some(root) = assets_root {
                        let _ = open_path_in_file_manager(&root.join("scripts"));
                    }
                }
            });
        });

    if state.show_attach_window {
        let catalog = state.available_scripts.clone();
        let assets_root_buf = assets_root.map(Path::to_path_buf);
        egui::Window::new(t("app.attach_script", lang))
            .collapsible(false)
            .resizable(true)
            .default_width(420.0)
            .default_height(320.0)
            .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
            .show(ui.ctx(), |ui| {
                ui.label(
                    egui::RichText::new(t("app.script_external_hint", lang))
                        .size(10.0)
                        .color(Color32::from_rgb(120, 120, 130)),
                );
                ui.add_space(8.0);

                ui.horizontal(|ui| {
                    ui.label(t("app.search", lang));
                    ui.add(egui::TextEdit::singleline(&mut state.search_filter).desired_width(180.0));
                    if ui.button(t("app.script_rescan", lang)).clicked() {
                        state.needs_rescan = true;
                        state.ensure_catalog(assets_root);
                    }
                });

                ui.add_space(8.0);
                let filter = state.search_filter.to_lowercase();
                let filtered: Vec<_> = catalog
                    .iter()
                    .filter(|entry| filter.is_empty() || entry.relative_path.to_lowercase().contains(&filter))
                    .collect();

                if filtered.is_empty() {
                    ui.label(
                        egui::RichText::new(t("app.script_registry_empty", lang))
                            .size(11.0)
                            .color(Color32::from_rgb(110, 110, 120)),
                    );
                } else {
                    egui::ScrollArea::vertical().show(ui, |ui| {
                        for entry in filtered {
                            let already_attached = node.scripts.iter().any(|script| script == &entry.relative_path);
                            egui::Frame::none()
                                .fill(Color32::from_rgb(27, 27, 32))
                                .rounding(6.0)
                                .inner_margin(8.0)
                                .stroke(Stroke::new(1.0, Color32::from_rgb(42, 42, 48)))
                                .show(ui, |ui| {
                                    ui.horizontal(|ui| {
                                        if let Some(icon) = icons.get("script.png") {
                                            ui.add(
                                                egui::Image::new(icon)
                                                    .fit_to_exact_size(egui::vec2(16.0, 16.0)),
                                            );
                                        }

                                        ui.vertical(|ui| {
                                            ui.label(
                                                egui::RichText::new(script_file_name(&entry.relative_path))
                                                    .size(11.0)
                                                    .color(Color32::from_rgb(225, 225, 230)),
                                            );
                                            ui.label(
                                                egui::RichText::new(&entry.relative_path)
                                                    .size(9.0)
                                                    .color(Color32::from_rgb(110, 110, 120)),
                                            );
                                        });

                                        script_chip(ui, entry.language.label(), Color32::from_rgb(35, 35, 42), Color32::from_rgb(190, 190, 198));
                                        if entry.has_on_start || entry.has_on_update {
                                            let hooks_label = match (entry.has_on_start, entry.has_on_update) {
                                                (true, true) => "on_start + on_update",
                                                (true, false) => "on_start",
                                                (false, true) => "on_update",
                                                (false, false) => "-",
                                            };
                                            script_chip(ui, hooks_label, Color32::from_rgb(32, 42, 55), Color32::from_rgb(170, 200, 230));
                                        }

                                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                            let label = if already_attached {
                                                t("app.script_attached", lang)
                                            } else {
                                                t("app.attach_script", lang)
                                            };
                                            let attach_btn = egui::Button::new(label).rounding(4.0);
                                            if ui.add_enabled(!already_attached, attach_btn).clicked() {
                                                node.scripts.push(entry.relative_path.clone());
                                                changed = true;
                                            }

                                            let open_btn = egui::Button::new(t("app.script_open_external", lang)).rounding(4.0);
                                            if ui.add(open_btn).clicked() {
                                                let _ = open_script_in_external_editor(&entry.absolute_path);
                                            }
                                        });
                                    });
                                });
                            ui.add_space(4.0);
                        }
                    });
                }

                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    if ui.button(t("app.cancel", lang)).clicked() {
                        state.show_attach_window = false;
                    }

                    if ui
                        .add_enabled(assets_root_buf.is_some(), egui::Button::new(t("app.script_scripts_folder", lang)))
                        .clicked()
                    {
                        if let Some(root) = &assets_root_buf {
                            let _ = open_path_in_file_manager(&root.join("scripts"));
                        }
                    }
                });
            });
    }

    changed
}

fn script_chip(ui: &mut Ui, label: &str, fill: Color32, text: Color32) {
    egui::Frame::none()
        .fill(fill)
        .rounding(10.0)
        .inner_margin(egui::Margin::symmetric(8.0, 3.0))
        .show(ui, |ui| {
            ui.label(
                egui::RichText::new(label)
                    .size(9.0)
                    .color(text),
            );
        });
}
