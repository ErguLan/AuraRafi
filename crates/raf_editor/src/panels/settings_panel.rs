//! Settings panel - theme, language, performance, editor preferences,
//! simple mode, and target platform.

use egui::Ui;
use raf_core::config::{EngineSettings, Language, RenderQuality, TargetPlatform, Theme};

/// Draw the settings panel.
pub fn show_settings(ui: &mut Ui, settings: &mut EngineSettings) {
    let is_es = settings.language == Language::Spanish;

    egui::ScrollArea::vertical().show(ui, |ui| {
        // -- Simple Mode (top, prominent) --
        ui.group(|ui| {
            ui.horizontal(|ui| {
                let label = if is_es { "Modo Simple" } else { "Simple Mode" };
                ui.heading(label);
                ui.add_space(8.0);
                ui.checkbox(&mut settings.simple_mode, "");
            });
            let desc = if is_es {
                "Oculta parametros avanzados. Ideal para principiantes."
            } else {
                "Hides advanced parameters. Ideal for beginners."
            };
            ui.label(
                egui::RichText::new(desc)
                    .small()
                    .color(egui::Color32::from_rgb(120, 120, 130)),
            );
        });

        ui.add_space(8.0);

        // -- Appearance --
        let appearance_label = if is_es { "Apariencia" } else { "Appearance" };
        egui::CollapsingHeader::new(appearance_label)
            .default_open(true)
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    let label = if is_es { "Tema:" } else { "Theme:" };
                    ui.label(label);
                    ui.selectable_value(&mut settings.theme, Theme::Dark, if is_es { "Oscuro" } else { "Dark" });
                    ui.selectable_value(&mut settings.theme, Theme::Light, if is_es { "Claro" } else { "Light" });
                    ui.selectable_value(&mut settings.theme, Theme::System, "System");
                });

                ui.horizontal(|ui| {
                    let label = if is_es { "Tamano de fuente:" } else { "Font Size:" };
                    ui.label(label);
                    ui.add(
                        egui::Slider::new(&mut settings.font_size, 10.0..=24.0).step_by(1.0),
                    );
                });

                ui.horizontal(|ui| {
                    let label = if is_es { "Escala de UI:" } else { "UI Scale:" };
                    ui.label(label);
                    ui.add(
                        egui::Slider::new(&mut settings.ui_scale, 0.5..=2.0).step_by(0.1),
                    );
                });
            });

        ui.add_space(4.0);

        // -- Language --
        let lang_label = if is_es { "Idioma" } else { "Language" };
        egui::CollapsingHeader::new(lang_label)
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

        // -- Target Platform --
        let platform_label = if is_es { "Plataforma Objetivo" } else { "Target Platform" };
        egui::CollapsingHeader::new(platform_label)
            .default_open(false)
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    let label = if is_es { "Plataforma:" } else { "Platform:" };
                    ui.label(label);
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

                let responsive_label = if is_es {
                    "Interfaz responsiva (pantallas pequenas)"
                } else {
                    "Responsive layout (small screens)"
                };
                ui.checkbox(&mut settings.responsive_layout, responsive_label);

                let headless_label = if is_es {
                    "Modo sin ventana (servidor/nube)"
                } else {
                    "Headless mode (server/cloud)"
                };
                ui.checkbox(&mut settings.headless, headless_label);

                // Info about current platform.
                if !settings.simple_mode {
                    let info = match settings.target_platform {
                        TargetPlatform::Desktop => {
                            if is_es { "Windows, macOS, Linux. Rendering completo." }
                            else { "Windows, macOS, Linux. Full rendering." }
                        }
                        TargetPlatform::Mobile => {
                            if is_es { "Android/iOS. Interfaz adaptativa, input tactil." }
                            else { "Android/iOS. Adaptive layout, touch input." }
                        }
                        TargetPlatform::Web => {
                            if is_es { "WebAssembly. Se ejecuta en cualquier navegador." }
                            else { "WebAssembly. Runs in any browser." }
                        }
                        TargetPlatform::Cloud => {
                            if is_es { "Streaming en nube. Compatible con GamePass, GeForce NOW." }
                            else { "Cloud streaming. Compatible with GamePass, GeForce NOW." }
                        }
                        TargetPlatform::Console => {
                            if is_es { "Consolas. Requiere SDK del fabricante (futuro)." }
                            else { "Consoles. Requires manufacturer SDK (future)." }
                        }
                    };
                    ui.label(
                        egui::RichText::new(info)
                            .small()
                            .color(egui::Color32::from_rgb(100, 100, 110)),
                    );
                }
            });

        ui.add_space(4.0);

        // -- Performance --
        let perf_label = if is_es { "Rendimiento" } else { "Performance" };
        egui::CollapsingHeader::new(perf_label)
            .default_open(false)
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    let label = if is_es { "Calidad:" } else { "Quality:" };
                    ui.label(label);
                    ui.selectable_value(
                        &mut settings.render_quality,
                        RenderQuality::Potato,
                        "Potato (0)",
                    );
                    ui.selectable_value(
                        &mut settings.render_quality,
                        RenderQuality::Low,
                        if is_es { "Baja (1)" } else { "Low (1)" },
                    );
                    ui.selectable_value(
                        &mut settings.render_quality,
                        RenderQuality::Medium,
                        if is_es { "Media (2)" } else { "Medium (2)" },
                    );
                    ui.selectable_value(
                        &mut settings.render_quality,
                        RenderQuality::High,
                        if is_es { "Alta (3)" } else { "High (3)" },
                    );
                });

                ui.horizontal(|ui| {
                    let label = if is_es { "Limite de FPS:" } else { "FPS Limit:" };
                    ui.label(label);
                    ui.add(egui::DragValue::new(&mut settings.fps_limit).range(15..=240));
                });

                ui.checkbox(&mut settings.vsync, "VSync");

                if !settings.simple_mode {
                    let mt_label = if is_es { "Multihilo" } else { "Multithreading" };
                    ui.checkbox(&mut settings.multithreading, mt_label);
                }
            });

        ui.add_space(4.0);

        // -- Editor --
        let editor_label = if is_es { "Editor" } else { "Editor" };
        egui::CollapsingHeader::new(editor_label)
            .default_open(false)
            .show(ui, |ui| {
                let grid_label = if is_es { "Mostrar Cuadricula" } else { "Show Grid" };
                ui.checkbox(&mut settings.grid_visible, grid_label);

                let snap_label = if is_es { "Ajustar a Cuadricula" } else { "Snap to Grid" };
                ui.checkbox(&mut settings.snap_to_grid, snap_label);

                if !settings.simple_mode {
                    ui.horizontal(|ui| {
                        let label = if is_es { "Tamano de cuadricula:" } else { "Grid Size:" };
                        ui.label(label);
                        ui.add(
                            egui::DragValue::new(&mut settings.grid_size)
                                .speed(0.1)
                                .range(0.1..=10.0),
                        );
                    });

                    ui.horizontal(|ui| {
                        let label = if is_es {
                            "Auto-guardado (segundos):"
                        } else {
                            "Auto-save (seconds):"
                        };
                        ui.label(label);
                        ui.add(
                            egui::DragValue::new(&mut settings.auto_save_interval_seconds)
                                .range(30..=600),
                        );
                    });

                    ui.horizontal(|ui| {
                        let label = if is_es { "Unidades:" } else { "Units:" };
                        ui.label(label);
                        let metric = if is_es { "Metrico" } else { "Metric" };
                        if ui
                            .selectable_label(settings.units_metric, metric)
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
                }
            });
    });
}
