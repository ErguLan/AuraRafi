use egui::Ui;
use raf_core::config::{Language, RenderPreset};
use raf_core::i18n::t;
use raf_core::project::Project;

pub fn show_project_settings(ui: &mut Ui, project: &mut Project, lang: Language) -> bool {
    let mut changed = false;

    egui::ScrollArea::vertical().show(ui, |ui| {
        ui.label(
            egui::RichText::new(t("app.project_settings", lang))
                .size(11.0)
                .strong()
                .color(egui::Color32::from_rgb(130, 130, 140)),
        );
        ui.separator();

        card(ui, t("app.project_overview", lang), |ui| {
            info_row(ui, t("app.project_name", lang), &project.name);
            info_row(ui, t("app.project_type", lang), project.project_type.display_name());
            info_row(ui, t("app.project_version", lang), &project.engine_version);
            info_row(ui, t("app.project_path", lang), &project.path.display().to_string());
        });

        ui.add_space(8.0);

        card(ui, t("app.project_layout", lang), |ui| {
            changed |= ui
                .checkbox(
                    &mut project.settings.show_hierarchy_panel,
                    t("app.show_hierarchy_panel", lang),
                )
                .changed();
            changed |= ui
                .checkbox(
                    &mut project.settings.show_properties_panel,
                    t("app.show_properties_panel", lang),
                )
                .changed();
        });

        ui.add_space(8.0);

        card(ui, t("app.project_runtime", lang), |ui| {
            changed |= ui
                .checkbox(
                    &mut project.settings.enable_audio,
                    t("app.enable_audio", lang),
                )
                .changed();
            changed |= ui
                .checkbox(
                    &mut project.settings.enable_physics,
                    t("app.enable_physics", lang),
                )
                .changed();
            changed |= ui
                .checkbox(
                    &mut project.settings.pause_when_unfocused,
                    t("app.pause_when_unfocused", lang),
                )
                .changed();
            changed |= ui
                .checkbox(
                    &mut project.settings.enable_complements,
                    t("app.enable_complements", lang),
                )
                .changed();

            ui.add_space(6.0);
            ui.horizontal(|ui| {
                ui.label(t("app.default_scene_name", lang));
                changed |= ui
                    .text_edit_singleline(&mut project.settings.default_scene_name)
                    .changed();
            });
        });

        ui.add_space(8.0);

        card(ui, t("app.graphics_policy", lang), |ui| {
            changed |= ui
                .checkbox(
                    &mut project.settings.allow_gpu_features,
                    t("app.allow_gpu_features", lang),
                )
                .changed();

            ui.add_space(6.0);
            changed |= ui
                .checkbox(
                    &mut project.settings.depth_accurate,
                    t("app.depth_accurate", lang),
                )
                .changed();

            ui.label(
                egui::RichText::new(t("app.depth_accurate_desc", lang))
                    .size(10.0)
                    .color(egui::Color32::from_rgb(150, 150, 158)),
            );

            ui.add_space(6.0);
            ui.horizontal(|ui| {
                ui.label(t("app.depth_resolution_scale", lang));
                changed |= ui
                    .add_enabled(
                        project.settings.depth_accurate,
                        egui::Slider::new(&mut project.settings.depth_resolution_scale, 0.35..=1.0)
                            .step_by(0.05)
                            .suffix("x"),
                    )
                    .changed();
            });

            ui.add_space(6.0);
            ui.horizontal(|ui| {
                ui.label(t("app.render_preset", lang));
                egui::ComboBox::from_id_salt("project_render_preset")
                    .selected_text(render_preset_label(project.settings.runtime_render_preset))
                    .show_ui(ui, |ui| {
                        for preset in [
                            RenderPreset::Potato,
                            RenderPreset::Low,
                            RenderPreset::Medium,
                            RenderPreset::High,
                        ] {
                            changed |= ui
                                .selectable_value(
                                    &mut project.settings.runtime_render_preset,
                                    preset,
                                    render_preset_label(preset),
                                )
                                .changed();
                        }
                    });
            });
        });
    });

    changed
}

fn card(ui: &mut Ui, title: String, add_contents: impl FnOnce(&mut Ui)) {
    let frame = egui::Frame::none()
        .fill(egui::Color32::from_rgb(22, 22, 26))
        .rounding(8.0)
        .inner_margin(12.0)
        .stroke(egui::Stroke::new(1.0, egui::Color32::from_rgb(42, 42, 48)));

    frame.show(ui, |ui| {
        ui.label(
            egui::RichText::new(title)
                .size(12.0)
                .strong()
                .color(egui::Color32::from_rgb(205, 205, 210)),
        );
        ui.add_space(8.0);
        add_contents(ui);
    });
}

fn info_row(ui: &mut Ui, label: String, value: &str) {
    ui.horizontal_wrapped(|ui| {
        ui.label(
            egui::RichText::new(label)
                .size(11.0)
                .color(egui::Color32::from_rgb(125, 125, 135)),
        );
        ui.label(
            egui::RichText::new(value)
                .size(11.0)
                .color(egui::Color32::from_rgb(210, 210, 215)),
        );
    });
}

fn render_preset_label(preset: RenderPreset) -> &'static str {
    match preset {
        RenderPreset::Potato => "Potato",
        RenderPreset::Low => "Low",
        RenderPreset::Medium => "Medium",
        RenderPreset::High => "High",
    }
}