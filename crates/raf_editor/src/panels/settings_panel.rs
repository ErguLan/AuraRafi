//! Settings panel - theme, language, performance, editor preferences.

use egui::Ui;
use raf_core::config::{EngineSettings, Language, RenderQuality, Theme};

/// Draw the settings panel.
pub fn show_settings(ui: &mut Ui, settings: &mut EngineSettings) {
    egui::ScrollArea::vertical().show(ui, |ui| {
        // -- Appearance --
        egui::CollapsingHeader::new("Appearance")
            .default_open(true)
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.label("Theme:");
                    ui.selectable_value(&mut settings.theme, Theme::Dark, "Dark");
                    ui.selectable_value(&mut settings.theme, Theme::Light, "Light");
                    ui.selectable_value(&mut settings.theme, Theme::System, "System");
                });

                ui.horizontal(|ui| {
                    ui.label("Font Size:");
                    ui.add(
                        egui::Slider::new(&mut settings.font_size, 10.0..=24.0).step_by(1.0),
                    );
                });

                ui.horizontal(|ui| {
                    ui.label("UI Scale:");
                    ui.add(
                        egui::Slider::new(&mut settings.ui_scale, 0.5..=2.0).step_by(0.1),
                    );
                });
            });

        ui.add_space(4.0);

        // -- Language --
        egui::CollapsingHeader::new("Language")
            .default_open(true)
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.selectable_value(
                        &mut settings.language,
                        Language::English,
                        Language::English.display_name(),
                    );
                    ui.selectable_value(
                        &mut settings.language,
                        Language::Spanish,
                        Language::Spanish.display_name(),
                    );
                });
            });

        ui.add_space(4.0);

        // -- Performance --
        egui::CollapsingHeader::new("Performance")
            .default_open(false)
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.label("Quality:");
                    ui.selectable_value(
                        &mut settings.render_quality,
                        RenderQuality::Potato,
                        "Potato (0)",
                    );
                    ui.selectable_value(
                        &mut settings.render_quality,
                        RenderQuality::Low,
                        "Low (1)",
                    );
                    ui.selectable_value(
                        &mut settings.render_quality,
                        RenderQuality::Medium,
                        "Medium (2)",
                    );
                    ui.selectable_value(
                        &mut settings.render_quality,
                        RenderQuality::High,
                        "High (3)",
                    );
                });

                ui.horizontal(|ui| {
                    ui.label("FPS Limit:");
                    ui.add(egui::DragValue::new(&mut settings.fps_limit).range(15..=240));
                });

                ui.checkbox(&mut settings.vsync, "VSync");
                ui.checkbox(&mut settings.multithreading, "Multithreading");
            });

        ui.add_space(4.0);

        // -- Editor --
        egui::CollapsingHeader::new("Editor")
            .default_open(false)
            .show(ui, |ui| {
                ui.checkbox(&mut settings.grid_visible, "Show Grid");
                ui.checkbox(&mut settings.snap_to_grid, "Snap to Grid");

                ui.horizontal(|ui| {
                    ui.label("Grid Size:");
                    ui.add(
                        egui::DragValue::new(&mut settings.grid_size)
                            .speed(0.1)
                            .range(0.1..=10.0),
                    );
                });

                ui.horizontal(|ui| {
                    ui.label("Auto-save (seconds):");
                    ui.add(
                        egui::DragValue::new(&mut settings.auto_save_interval_seconds)
                            .range(30..=600),
                    );
                });

                ui.horizontal(|ui| {
                    ui.label("Units:");
                    if ui
                        .selectable_label(settings.units_metric, "Metric")
                        .clicked()
                    {
                        settings.units_metric = true;
                    }
                    if ui
                        .selectable_label(!settings.units_metric, "Imperial")
                        .clicked()
                    {
                        settings.units_metric = false;
                    }
                });
            });
    });
}
