impl AuraRafiApp {
    // Project Hub
    // -----------------------------------------------------------------------

    /// Show the project hub (recent projects + create new).
    fn show_project_hub(&mut self, ctx: &egui::Context) {
        let _lang = self.settings.language;
        let is_dark = ctx.style().visuals.dark_mode;
        
        let bg_color = if is_dark { app_theme::DARK_BG } else { app_theme::LIGHT_BG };
        let panel_color = if is_dark { app_theme::DARK_PANEL } else { app_theme::LIGHT_PANEL };
        let text_color = if is_dark { app_theme::DARK_TEXT } else { app_theme::LIGHT_TEXT };
        let text_dim_color = if is_dark { app_theme::DARK_TEXT_DIM } else { app_theme::LIGHT_TEXT_DIM };
        let _border_color = if is_dark { app_theme::DARK_BORDER } else { app_theme::LIGHT_BORDER };

        // --- Load Logo Texture ---
        let logo_res = self.logo_texture.get_or_insert_with(|| {
            let icon_bytes = include_bytes!("../../../../editor/icon.png");
            let image = image::load_from_memory(icon_bytes).expect("Failed to load icon");
            let rgba = image.to_rgba8();
            let (w, h) = rgba.dimensions();
            ctx.load_texture(
                "hub_logo",
                egui::ColorImage::from_rgba_unmultiplied(
                    [w as usize, h as usize],
                    &rgba.to_vec(),
                ),
                egui::TextureOptions::default()
            )
        });
        let logo_id = logo_res.id();

        egui::SidePanel::left("hub_sidebar")
            .frame(egui::Frame::none().fill(bg_color))
            .resizable(false)
            .default_width(200.0)
            .show(ctx, |ui| {
                // --- Professional Logo ---
                ui.vertical_centered(|ui| {
                    ui.add_space(20.0);
                    ui.image((logo_id, egui::vec2(52.0, 52.0)));
                    ui.add_space(8.0);
                    ui.label(egui::RichText::new("AuraRafi").strong().color(text_color));
                });
                
                ui.add_space(32.0);

                ui.with_layout(egui::Layout::top_down_justified(egui::Align::Min), |ui| {
                    ui.spacing_mut().item_spacing.y = 4.0;
                    
                    // Sidebar active stylings
                    let active_bg = egui::Color32::from_rgb(45, 45, 52);
                    let inactive_bg = egui::Color32::TRANSPARENT;
                    
                    // My Projects - Active
                    let btn_projects = egui::Button::new(
                        egui::RichText::new("My Projects").color(app_theme::ACCENT).size(13.0)
                    ).fill(active_bg).rounding(4.0).frame(true);
                    
                    if ui.add_sized([ui.available_width() - 16.0, 30.0], btn_projects).clicked() {
                        // Already here
                    }

                    ui.add_space(4.0);

                    // Settings - Inactive
                    let btn_settings = egui::Button::new(
                        egui::RichText::new("Settings").color(text_dim_color).size(13.0)
                    ).fill(inactive_bg).rounding(4.0).frame(false);

                    if ui.add_sized([ui.available_width() - 16.0, 30.0], btn_settings).clicked() {
                        self.previous_screen = Some(AppScreen::ProjectHub);
                        self.screen = AppScreen::Settings;
                    }
                });
            });

        // --- Main Content ---
        egui::CentralPanel::default()
            .frame(egui::Frame::none().fill(panel_color))
            .show(ctx, |ui| {
            egui::ScrollArea::vertical().show(ui, |ui| {
                ui.add_space(30.0);
                
                // Header (Welcome)
                ui.label(
                    egui::RichText::new("AuraRafi Project Hub")
                        .size(22.0)
                        .color(text_color),
                );
                ui.label(
                    egui::RichText::new("Develop your own game or electronic project")
                        .size(13.0)
                        .color(text_dim_color),
                );
                
                ui.add_space(36.0);

                // --- TOP: Create New Cards ---
                ui.columns(2, |columns| {
                    // Game Project
                    let ui0 = &mut columns[0];
                    let game_frame = egui::Frame::none()
                        .fill(egui::Color32::from_rgb(32, 32, 36))
                        .rounding(6.0)
                        .inner_margin(20.0)
                        .stroke(egui::Stroke::new(1.0, egui::Color32::from_rgb(50, 50, 55)));
                        
                    let game_card = game_frame.show(ui0, |ui| {
                        ui.horizontal(|ui| {
                            ui.vertical(|ui| {
                                ui.label(egui::RichText::new("NEW GAME").strong().size(13.0).color(text_color));
                                ui.add_space(4.0);
                                ui.label(egui::RichText::new("Start a blank 3D project.").size(12.0).color(app_theme::DARK_TEXT_DIM));
                            });
                            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                ui.label(egui::RichText::new("+").size(20.0).color(app_theme::ACCENT));
                            });
                        });
                    });
                    
                    let resp = ui0.interact(game_card.response.rect, ui0.id().with("game_card_click"), egui::Sense::click());
                    if resp.clicked() {
                        self.screen = AppScreen::NewProject {
                            name: String::new(),
                            path: default_projects_dir(),
                            project_type: ProjectType::Game,
                        };
                    }
                    if resp.hovered() {
                        ui0.painter().rect_stroke(game_card.response.rect, 6.0, egui::Stroke::new(1.0, app_theme::ACCENT));
                    }

                    // Electronics Project
                    let ui1 = &mut columns[1];
                    let elec_frame = egui::Frame::none()
                        .fill(egui::Color32::from_rgb(32, 32, 36))
                        .rounding(6.0)
                        .inner_margin(20.0)
                        .stroke(egui::Stroke::new(1.0, egui::Color32::from_rgb(50, 50, 55)));
                        
                    let elec_card = elec_frame.show(ui1, |ui| {
                        ui.horizontal(|ui| {
                            ui.vertical(|ui| {
                                ui.label(egui::RichText::new("NEW ELECTRONICS").strong().size(13.0).color(text_color));
                                ui.add_space(4.0);
                                ui.label(egui::RichText::new("PCB design and simulation.").size(12.0).color(app_theme::DARK_TEXT_DIM));
                            });
                            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                ui.label(egui::RichText::new("+").size(20.0).color(app_theme::ACCENT));
                            });
                        });
                    });

                    let resp = ui1.interact(elec_card.response.rect, ui1.id().with("elec_card_click"), egui::Sense::click());
                    if resp.clicked() {
                        self.screen = AppScreen::NewProject {
                            name: String::new(),
                            path: default_projects_dir(),
                            project_type: ProjectType::Electronics,
                        };
                    }
                    if resp.hovered() {
                        ui1.painter().rect_stroke(elec_card.response.rect, 6.0, egui::Stroke::new(1.0, app_theme::ACCENT));
                    }
                });

                ui.add_space(40.0);
                ui.separator();
                ui.add_space(24.0);

                // --- BOTTOM: My Projects (List) ---
                ui.label(
                    egui::RichText::new("RECENT PROJECTS")
                        .size(12.0)
                        .color(text_dim_color),
                );
                ui.add_space(12.0);

                if self.recent_projects.projects.is_empty() {
                    ui.add_space(20.0);
                    ui.label(
                        egui::RichText::new("No projects yet.")
                            .color(app_theme::DARK_TEXT_DIM),
                    );
                } else {
                    let mut open_path: Option<std::path::PathBuf> = None;
                    let mut duplicate_path: Option<std::path::PathBuf> = None;
                    let mut delete_path: Option<std::path::PathBuf> = None;

                    for entry in &self.recent_projects.projects {
                        let frame = egui::Frame::none()
                            .fill(egui::Color32::from_rgb(26, 26, 28))
                            .rounding(6.0)
                            .inner_margin(16.0)
                            .stroke(egui::Stroke::new(1.0, egui::Color32::from_rgb(40, 40, 45)));

                        frame.show(ui, |ui| {
                            ui.horizontal(|ui| {
                                // Left: Name and Basic Info
                                ui.vertical(|ui| {
                                    ui.horizontal(|ui| {
                                        ui.label(egui::RichText::new(&entry.name).size(14.0).strong().color(text_color));
                                        ui.add_space(8.0);
                                        
                                        // Elegant Pill Badge
                                        let type_label = match entry.project_type {
                                            ProjectType::Game => "GAME",
                                            ProjectType::Electronics => "ELECTRONICS",
                                        };
                                        egui::Frame::none()
                                            .fill(egui::Color32::from_rgba_unmultiplied(255, 140, 0, 30))
                                            .rounding(4.0)
                                            .inner_margin(egui::Margin::symmetric(6.0, 2.0))
                                            .show(ui, |ui| {
                                                ui.label(egui::RichText::new(type_label).size(10.0).strong().color(app_theme::ACCENT));
                                            });
                                    });
                                    
                                    ui.add_space(6.0);
                                    
                                    ui.horizontal(|ui| {
                                        ui.label(
                                            egui::RichText::new(format!("Created: {}", entry.created_at.format("%d/%m/%Y")))
                                                .size(11.0)
                                                .color(app_theme::DARK_TEXT_DIM)
                                        );
                                        ui.add_space(12.0);
                                        ui.label(
                                            egui::RichText::new(format!("Modified: {}", entry.modified_at.format("%d/%m/%Y")))
                                                .size(11.0)
                                                .color(app_theme::DARK_TEXT_DIM)
                                        );
                                        ui.add_space(12.0);
                                        
                                        let elem_label = match entry.project_type {
                                            ProjectType::Game => "nodes",
                                            ProjectType::Electronics => "components",
                                        };
                                        ui.label(
                                            egui::RichText::new(format!("{} {}", entry.n_elements, elem_label))
                                                .size(11.0)
                                                .color(app_theme::DARK_TEXT_DIM)
                                        );
                                    });
                                });
                                
                                // Right: Actions
                                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                    // Primary action
                                    let open_btn = egui::Button::new(
                                        egui::RichText::new("Open").size(12.0).color(egui::Color32::WHITE)
                                    ).fill(egui::Color32::from_rgb(50, 50, 55)).rounding(4.0);
                                    
                                    if ui.add_sized([60.0, 24.0], open_btn).clicked() {
                                        open_path = Some(entry.path.clone());
                                    }
                                    
                                    ui.add_space(8.0);
                                    
                                    // Secondary Actions
                                    let dup_btn = egui::Button::new(
                                        egui::RichText::new("Clone").size(11.0).color(text_dim_color)
                                    ).fill(egui::Color32::TRANSPARENT).frame(false);
                                    
                                    if ui.add(dup_btn).clicked() {
                                        duplicate_path = Some(entry.path.clone());
                                    }
                                    
                                    let del_btn = egui::Button::new(
                                        egui::RichText::new("Delete").size(11.0).color(egui::Color32::from_rgb(180, 80, 80))
                                    ).fill(egui::Color32::TRANSPARENT).frame(false);
                                    
                                    if ui.add(del_btn).clicked() {
                                        delete_path = Some(entry.path.clone());
                                    }
                                });
                            });
                        });
                        ui.add_space(8.0);
                    }
                    
                    if let Some(path) = open_path {
                        self.open_project(&path);
                    }
                    if let Some(_path) = duplicate_path {
                        // TODO: Implement clone logic
                    }
                    if let Some(path) = delete_path {
                        self.recent_projects.projects.retain(|p| p.path != path);
                        let _ = self.recent_projects.save(&dirs_config_dir());
                    }
                }
            });
        });
    }

    // -----------------------------------------------------------------------
    // New Project Form
    // -----------------------------------------------------------------------

    fn show_new_project(
        &mut self,
        ctx: &egui::Context,
        mut name: String,
        mut path: String,
        project_type: ProjectType,
    ) {
        let _lang = self.settings.language;
        egui::CentralPanel::default().show(ctx, |ui| {
            ui.add_space(50.0);
            ui.vertical_centered(|ui| {
                
                let title = match project_type {
                    ProjectType::Game => {
                        t("app.new_game_project", self.settings.language)
                    }
                    ProjectType::Electronics => {
                        t("app.new_electronics_project", self.settings.language)
                    }
                };
                ui.heading(
                    egui::RichText::new(title)
                        .size(28.0)
                        .color(app_theme::ACCENT),
                );
                ui.add_space(24.0);
            });

            // Centered form.
            ui.vertical_centered(|ui| {
                ui.set_max_width(500.0);

                ui.group(|ui| {
                    ui.set_min_width(460.0);
                    ui.add_space(8.0);

                    ui.horizontal(|ui| {
                        ui.label(
                            egui::RichText::new(t("app.project_name", _lang)).strong(),
                        );
                        ui.add_sized(
                            [300.0, 24.0],
                            egui::TextEdit::singleline(&mut name)
                                .hint_text("My Awesome Project"),
                        );
                    });

                    ui.add_space(6.0);

                    ui.horizontal(|ui| {
                        ui.label(
                            egui::RichText::new(t("app.location", _lang)).strong(),
                        );
                        ui.add_sized(
                            [300.0, 24.0],
                            egui::TextEdit::singleline(&mut path),
                        );
                    });

                    ui.add_space(6.0);

                    ui.horizontal(|ui| {
                        ui.label(
                            egui::RichText::new(t("app.type", _lang)).strong(),
                        );
                        ui.label(
                            egui::RichText::new(project_type.display_name())
                                .color(app_theme::ACCENT),
                        );
                    });

                    ui.add_space(8.0);
                });

                ui.add_space(20.0);

                ui.horizontal(|ui| {
                    if ui
                        .add_sized([120.0, 30.0], egui::Button::new(t("app.cancel", _lang)))
                        .clicked()
                    {
                        self.screen = AppScreen::ProjectHub;
                        return;
                    }

                    ui.add_space(12.0);

                    let can_create = !name.is_empty() && !path.is_empty();
                    let _create_btn = egui::Button::new(
                        egui::RichText::new(t("app.create_project", _lang))
                            .color(egui::Color32::WHITE),
                    )
                    .fill(if can_create {
                        app_theme::ACCENT
                    } else {
                        app_theme::DARK_WIDGET
                    });

                    if ui
                        .add_enabled(can_create, egui::Button::new(
                            egui::RichText::new(t("app.create_project", _lang))
                                .color(egui::Color32::WHITE)
                                .strong(),
                        ))
                        .clicked()
                    {
                        let project_path = std::path::PathBuf::from(&path);
                        match Project::create(&name, project_type, &project_path) {
                            Ok(project) => {
                                self.console.log(
                                    LogLevel::Info,
                                    &format!("Project '{}' created", project.name),
                                );
                                let config_dir = dirs_config_dir();
                                self.recent_projects.add(&project);
                                let _ = self.recent_projects.save(&config_dir);
                                self.current_project = Some(project);
                                self.init_scene_for_type(project_type);
                                self.screen = AppScreen::Editor;
                            }
                            Err(e) => {
                                self.console.log(
                                    LogLevel::Error,
                                    &format!("Failed to create project: {}", e),
                                );
                            }
                        }
                    }
                });
            });

            // Update the screen state with edited fields.
            if self.screen != AppScreen::ProjectHub && self.screen != AppScreen::Editor {
                self.screen = AppScreen::NewProject {
                    name,
                    path,
                    project_type,
                };
            }
        });
    }
}
