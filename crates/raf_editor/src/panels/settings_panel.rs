//! Settings panel - theme, language, performance, editor preferences,
//! simple mode, and target platform.

use egui::Ui;
use raf_core::config::{EngineSettings, Language, RenderQuality, TargetPlatform, Theme};
use raf_core::i18n::t;

/// Draw the settings panel.
pub fn show_settings(ui: &mut Ui, settings: &mut EngineSettings) {
    egui::ScrollArea::vertical().show(ui, |ui| {
        // -- Simple Mode (top, prominent, clean) --
        let frame = egui::Frame::none()
            .fill(egui::Color32::from_rgb(32, 32, 36))
            .rounding(6.0)
            .inner_margin(16.0)
            .stroke(egui::Stroke::new(1.0, egui::Color32::from_rgb(45, 45, 50)));

        frame.show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label(egui::RichText::new(t("settings.simple_mode", settings.language)).size(14.0).strong());
                ui.add_space(8.0);
                ui.checkbox(&mut settings.simple_mode, "");
            });
            ui.add_space(4.0);
            ui.label(
                egui::RichText::new(t("settings.simple_mode_desc", settings.language))
                    .size(11.0)
                    .color(egui::Color32::from_rgb(140, 140, 150)),
            );
        });

        ui.add_space(16.0);

        // -- Appearance --
        egui::CollapsingHeader::new(egui::RichText::new(t("settings.appearance", settings.language)).strong())
            .default_open(true)
            .show(ui, |ui| {
                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    ui.label(egui::RichText::new(t("settings.theme", settings.language)).size(12.0));
                    ui.selectable_value(&mut settings.theme, Theme::Dark, "Dark");
                    ui.selectable_value(&mut settings.theme, Theme::Light, "Light");
                    ui.selectable_value(&mut settings.theme, Theme::System, "System");
                });
                
                ui.add_space(6.0);

                ui.horizontal(|ui| {
                    ui.label(egui::RichText::new(t("settings.font_size", settings.language)).size(12.0));
                    ui.add(
                        egui::Slider::new(&mut settings.font_size, 10.0..=24.0).step_by(1.0).text("px"),
                    );
                });

                ui.add_space(6.0);

                ui.horizontal(|ui| {
                    ui.label(egui::RichText::new(t("settings.ui_scale", settings.language)).size(12.0));
                    ui.add(
                        egui::Slider::new(&mut settings.ui_scale, 0.5..=2.0).step_by(0.1),
                    );
                });
                ui.add_space(8.0);
            });

        ui.add_space(4.0);

        // -- Language --
        egui::CollapsingHeader::new(egui::RichText::new(t("settings.language", settings.language)).strong())
            .default_open(true)
            .show(ui, |ui| {
                ui.add_space(8.0);
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
                ui.add_space(8.0);
            });

        ui.add_space(4.0);

        // -- Target Platform --
        egui::CollapsingHeader::new(egui::RichText::new(t("settings.target_platform", settings.language)).strong())
            .default_open(false)
            .show(ui, |ui| {
                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    ui.label(egui::RichText::new(t("settings.platform", settings.language)).size(12.0));
                    egui::ComboBox::from_id_salt("target_platform_select")
                        .selected_text(settings.target_platform.display_name())
                        .show_ui(ui, |ui| {
                            for platform in TargetPlatform::all() {
                                ui.selectable_value(
                                    &mut settings.target_platform,
                                    *platform,
                                    platform.display_name(),
                                );
                            }
                        });
                });
                
                ui.add_space(6.0);

                ui.checkbox(&mut settings.responsive_layout, t("settings.responsive_layout", settings.language));
                ui.add_space(2.0);
                ui.checkbox(&mut settings.headless, t("settings.headless", settings.language));

                // Info about current platform.
                if !settings.simple_mode {
                    ui.add_space(10.0);
                    let info = match settings.target_platform {
                        TargetPlatform::Desktop => t("settings.platform.desktop", settings.language),
                        TargetPlatform::Mobile => t("settings.platform.mobile", settings.language),
                        TargetPlatform::Web => t("settings.platform.web", settings.language),
                        TargetPlatform::Cloud => t("settings.platform.cloud", settings.language),
                        TargetPlatform::Console => t("settings.platform.console", settings.language),
                    };
                    ui.label(
                        egui::RichText::new(info)
                            .size(11.0)
                            .color(egui::Color32::from_rgb(120, 120, 130)),
                    );
                }
                ui.add_space(8.0);
            });

        ui.add_space(4.0);

        // -- Performance --
        egui::CollapsingHeader::new(egui::RichText::new(t("settings.performance", settings.language)).strong())
            .default_open(false)
            .show(ui, |ui| {
                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    ui.label(egui::RichText::new(t("settings.quality", settings.language)).size(12.0));
                    ui.selectable_value(&mut settings.render_quality, RenderQuality::Potato, "Potato (0)");
                    ui.selectable_value(&mut settings.render_quality, RenderQuality::Low, "Low (1)");
                    ui.selectable_value(&mut settings.render_quality, RenderQuality::Medium, "Medium (2)");
                    ui.selectable_value(&mut settings.render_quality, RenderQuality::High, "High (3)");
                });
                
                ui.add_space(6.0);

                ui.horizontal(|ui| {
                    ui.label(egui::RichText::new(t("settings.fps_limit", settings.language)).size(12.0));
                    ui.add(egui::DragValue::new(&mut settings.fps_limit).range(15..=240).suffix(" fps"));
                });

                ui.add_space(4.0);

                ui.checkbox(&mut settings.vsync, t("settings.vsync", settings.language));

                if !settings.simple_mode {
                    ui.add_space(2.0);
                    ui.checkbox(&mut settings.multithreading, t("settings.multithreading", settings.language));
                }
                ui.add_space(8.0);
            });

        ui.add_space(4.0);

        // -- Editor --
        egui::CollapsingHeader::new(egui::RichText::new(t("settings.editor", settings.language)).strong())
            .default_open(false)
            .show(ui, |ui| {
                ui.add_space(8.0);
                ui.checkbox(&mut settings.grid_visible, t("settings.show_grid", settings.language));
                ui.add_space(2.0);
                ui.checkbox(&mut settings.snap_to_grid, t("settings.snap_to_grid", settings.language));

                if !settings.simple_mode {
                    ui.add_space(6.0);
                    ui.horizontal(|ui| {
                        ui.label(egui::RichText::new(t("settings.grid_size", settings.language)).size(12.0));
                        ui.add(
                            egui::DragValue::new(&mut settings.grid_size)
                                .speed(0.1)
                                .range(0.1..=10.0),
                        );
                    });

                    ui.add_space(6.0);
                    ui.horizontal(|ui| {
                        ui.label(egui::RichText::new(t("settings.auto_save", settings.language)).size(12.0));
                        ui.add(
                            egui::DragValue::new(&mut settings.auto_save_interval_seconds)
                                .range(30..=600)
                                .suffix(" s"),
                        );
                    });

                    ui.add_space(6.0);
                    ui.horizontal(|ui| {
                        ui.label(egui::RichText::new(t("settings.units", settings.language)).size(12.0));
                        if ui.selectable_label(settings.units_metric, t("settings.metric", settings.language)).clicked() {
                            settings.units_metric = true;
                        }
                        if ui.selectable_label(!settings.units_metric, t("settings.imperial", settings.language)).clicked() {
                            settings.units_metric = false;
                        }
                    });
                }
                ui.add_space(8.0);
            });
    });
}
