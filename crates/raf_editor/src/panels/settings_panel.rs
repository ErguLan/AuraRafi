//! Settings panel - theme, language, performance, editor preferences,
//! simple mode, and target platform.

use crate::theme as app_theme;
use egui::Ui;
use raf_core::config::{EngineSettings, Language, RenderQuality, TargetPlatform, Theme, ViewportRenderMode};
use raf_core::i18n::t;

/// Draw the settings panel.
pub fn show_settings(ui: &mut Ui, settings: &mut EngineSettings) {
    let palette = app_theme::palette_for(settings.theme, settings.theme_experimental);

    egui::ScrollArea::vertical().show(ui, |ui| {
        // -- Simple Mode (top, prominent, clean) --
        let frame = egui::Frame::none()
            .fill(palette.widget)
            .rounding(6.0)
            .inner_margin(16.0)
            .stroke(egui::Stroke::new(1.0, palette.border));

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
                    .color(palette.text_dim),
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

                ui.vertical(|ui| {
                    ui.label(egui::RichText::new(t("settings.theme_experimental", settings.language)).size(12.0));
                    ui.add(
                        egui::Slider::new(&mut settings.theme_experimental, 0.0..=100.0)
                            .step_by(1.0)
                            .suffix("%"),
                    );
                    ui.label(
                        egui::RichText::new(t("settings.theme_experimental_desc", settings.language))
                            .size(11.0)
                            .color(palette.text_dim),
                    );
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
                            .color(palette.text_dim),
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
                ui.add_space(6.0);
                ui.horizontal(|ui| {
                    ui.label(egui::RichText::new(t("settings.viewport_render_mode", settings.language)).size(12.0));
                    egui::ComboBox::from_id_salt("viewport_render_mode_select")
                        .selected_text(match settings.viewport_render_mode {
                            ViewportRenderMode::Solid => t("settings.viewport_render_mode.solid", settings.language),
                            ViewportRenderMode::Wireframe => t("settings.viewport_render_mode.wireframe", settings.language),
                            ViewportRenderMode::Preview => t("settings.viewport_render_mode.preview", settings.language),
                        })
                        .show_ui(ui, |ui| {
                            ui.selectable_value(
                                &mut settings.viewport_render_mode,
                                ViewportRenderMode::Solid,
                                t("settings.viewport_render_mode.solid", settings.language),
                            );
                            ui.selectable_value(
                                &mut settings.viewport_render_mode,
                                ViewportRenderMode::Wireframe,
                                t("settings.viewport_render_mode.wireframe", settings.language),
                            );
                            ui.selectable_value(
                                &mut settings.viewport_render_mode,
                                ViewportRenderMode::Preview,
                                t("settings.viewport_render_mode.preview", settings.language),
                            );
                        });
                });
                ui.add_space(2.0);
                ui.checkbox(&mut settings.show_viewport_labels, t("settings.show_viewport_labels", settings.language));
                ui.add_space(2.0);
                ui.checkbox(
                    &mut settings.solid_show_surface_edges,
                    t("settings.solid_show_surface_edges", settings.language),
                );
                ui.add_space(2.0);
                ui.checkbox(
                    &mut settings.solid_xray_mode,
                    t("settings.solid_xray_mode", settings.language),
                );
                ui.add_space(2.0);
                ui.checkbox(
                    &mut settings.solid_face_tonality,
                    t("settings.solid_face_tonality", settings.language),
                );

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

                // -- Mouse --
                ui.separator();
                ui.add_space(4.0);
                ui.checkbox(&mut settings.invert_mouse_x, t("settings.invert_mouse_x", settings.language));
                ui.add_space(2.0);
                ui.checkbox(&mut settings.invert_mouse_y, t("settings.invert_mouse_y", settings.language));
                ui.add_space(10.0);

                ui.separator();
                ui.add_space(6.0);
                ui.label(
                    egui::RichText::new(t("settings.gizmo_controls", settings.language))
                        .size(12.0)
                        .strong(),
                );
                ui.add_space(6.0);

                ui.horizontal(|ui| {
                    ui.label(egui::RichText::new(t("settings.move_sensitivity", settings.language)).size(12.0));
                    ui.add(egui::Slider::new(&mut settings.move_gizmo_sensitivity, 0.25..=4.0).logarithmic(true));
                });
                ui.add_space(4.0);
                ui.horizontal(|ui| {
                    ui.label(egui::RichText::new(t("settings.rotate_sensitivity", settings.language)).size(12.0));
                    ui.add(egui::Slider::new(&mut settings.rotate_gizmo_sensitivity, 0.25..=4.0).logarithmic(true));
                });
                ui.add_space(4.0);
                ui.horizontal(|ui| {
                    ui.label(egui::RichText::new(t("settings.scale_sensitivity", settings.language)).size(12.0));
                    ui.add(egui::Slider::new(&mut settings.scale_gizmo_sensitivity, 0.25..=4.0).logarithmic(true));
                });
                ui.add_space(4.0);
                ui.checkbox(
                    &mut settings.uniform_scale_by_default,
                    t("settings.uniform_scale_by_default", settings.language),
                );
                ui.add_space(8.0);

                ui.separator();
                ui.add_space(6.0);
                ui.label(
                    egui::RichText::new(t("settings.shortcuts_experimental", settings.language))
                        .size(12.0)
                        .strong(),
                );
                ui.add_space(4.0);
                ui.label(
                    egui::RichText::new(t("settings.shortcuts_experimental_desc", settings.language))
                        .size(11.0)
                        .color(palette.text_dim),
                );
            });
    });
}
