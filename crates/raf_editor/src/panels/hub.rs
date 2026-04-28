use super::*;

use raf_core::project::RecentProjectEntry;
use std::path::{Path, PathBuf};
use uuid::Uuid;

#[derive(Debug)]
enum HubAction {
    Open(PathBuf),
    Duplicate(PathBuf),
    Forget(PathBuf),
}

impl AuraRafiApp {
    pub fn show_project_hub(&mut self, ctx: &egui::Context) {
        let lang = self.settings.language;
        let is_dark = ctx.style().visuals.dark_mode;
        let palette = app_theme::palette_for_visuals(is_dark, self.settings.theme_experimental);
        let logo_id = self.hub_logo_texture(ctx).id();

        self.ui_icons.request_icons(HUB_UI_ICONS);
        self.ui_icons.process_load_budget(ctx, ui_icon_budget(ctx));

        let total_projects = self.recent_projects.projects.len();
        let game_projects = self
            .recent_projects
            .projects
            .iter()
            .filter(|entry| entry.project_type == ProjectType::Game)
            .count();
        let electronics_projects = total_projects.saturating_sub(game_projects);

        let filtered_projects = filtered_recent_projects(
            &self.recent_projects.projects,
            self.hub_filter,
            &self.hub_search_query,
        );

        egui::SidePanel::left("hub_sidebar")
            .frame(egui::Frame::none().fill(palette.bg))
            .resizable(false)
            .default_width(236.0)
            .show(ctx, |ui| {
                ui.add_space(18.0);
                ui.vertical_centered(|ui| {
                    ui.image((logo_id, egui::vec2(58.0, 58.0)));
                    ui.add_space(10.0);
                    ui.label(
                        egui::RichText::new("AuraRafi")
                            .size(20.0)
                            .strong()
                            .color(palette.text),
                    );
                    ui.label(
                        egui::RichText::new(format!("v{}", env!("CARGO_PKG_VERSION")))
                            .size(11.0)
                            .color(palette.text_dim),
                    );
                });

                ui.add_space(28.0);

                hub_nav_button(
                    ui,
                    &self.ui_icons,
                    "project_type_HUB.png",
                    t("app.my_projects", lang),
                    true,
                    &palette,
                );
                ui.add_space(8.0);

                if hub_nav_button(
                    ui,
                    &self.ui_icons,
                    "settings_HUB.png",
                    t("app.settings_menu", lang),
                    false,
                    &palette,
                )
                .clicked()
                {
                    self.previous_screen = Some(AppScreen::ProjectHub);
                    self.screen = AppScreen::Settings;
                }

                ui.add_space(20.0);

                egui::Frame::none()
                    .fill(palette.panel)
                    .rounding(10.0)
                    .inner_margin(egui::Margin::symmetric(14.0, 12.0))
                    .stroke(egui::Stroke::new(1.0, palette.border))
                    .show(ui, |ui| {
                        ui.label(
                            egui::RichText::new(t("app.hub_last_opened", lang))
                                .size(11.0)
                                .strong()
                                .color(palette.text_dim),
                        );
                        ui.add_space(10.0);

                        if let Some(entry) = filtered_projects.first() {
                            ui.label(
                                egui::RichText::new(&entry.name)
                                    .size(14.0)
                                    .strong()
                                    .color(palette.text),
                            );
                            ui.label(
                                egui::RichText::new(format_hub_date(entry.last_opened))
                                    .size(11.0)
                                    .color(app_theme::ACCENT),
                            );
                            ui.add_space(6.0);
                            ui.label(
                                egui::RichText::new(truncate_middle(&entry.path.to_string_lossy(), 28))
                                    .size(11.0)
                                    .color(palette.text_dim),
                            );
                        } else {
                            ui.label(
                                egui::RichText::new(t("app.no_recent_projects", lang))
                                    .size(12.0)
                                    .color(palette.text_dim),
                            );
                        }
                    });

                ui.with_layout(egui::Layout::bottom_up(egui::Align::Min), |ui| {
                    ui.label(
                        egui::RichText::new("Project Hub")
                            .size(11.0)
                            .color(palette.text_dim),
                    );
                });
            });

        let mut pending_action = None;

        egui::CentralPanel::default()
            .frame(egui::Frame::none().fill(palette.panel))
            .show(ctx, |ui| {
                egui::ScrollArea::vertical()
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        ui.add_space(24.0);

                        egui::Frame::none()
                            .fill(palette.widget)
                            .rounding(18.0)
                            .inner_margin(egui::Margin::symmetric(22.0, 20.0))
                            .stroke(egui::Stroke::new(1.0, palette.border))
                            .show(ui, |ui| {
                                ui.columns(2, |columns| {
                                    let header_ui = &mut columns[0];
                                    header_ui.vertical(|ui| {
                                        ui.label(
                                            egui::RichText::new(t("app.hub_title", lang))
                                                .size(26.0)
                                                .strong()
                                                .color(palette.text),
                                        );
                                        ui.add_space(4.0);
                                        ui.label(
                                            egui::RichText::new(t("app.hub_subtitle", lang))
                                                .size(13.0)
                                                .color(palette.text_dim),
                                        );
                                    });

                                    let stats_ui = &mut columns[1];
                                    stats_ui.vertical(|ui| {
                                        ui.add_space(2.0);
                                        ui.horizontal_wrapped(|ui| {
                                            ui.spacing_mut().item_spacing = egui::vec2(8.0, 8.0);
                                            stat_chip(
                                                ui,
                                                &self.ui_icons,
                                                "project_type_HUB.png",
                                                total_projects.to_string(),
                                                t("app.hub_total_projects", lang),
                                                &palette,
                                            );
                                            ui.add_space(8.0);
                                            stat_chip(
                                                ui,
                                                &self.ui_icons,
                                                "project_game.png",
                                                game_projects.to_string(),
                                                t("app.hub_game_kind", lang),
                                                &palette,
                                            );
                                            ui.add_space(8.0);
                                            stat_chip(
                                                ui,
                                                &self.ui_icons,
                                                "project_electronics.png",
                                                electronics_projects.to_string(),
                                                t("app.hub_electronics_kind", lang),
                                                &palette,
                                            );
                                        });
                                    });
                                });
                            });

                        ui.add_space(18.0);

                        egui::Frame::none()
                            .fill(palette.widget)
                            .rounding(16.0)
                            .inner_margin(egui::Margin::symmetric(18.0, 18.0))
                            .stroke(egui::Stroke::new(1.0, palette.border))
                            .show(ui, |ui| {
                                ui.horizontal(|ui| {
                                    ui.vertical(|ui| {
                                        ui.label(
                                            egui::RichText::new(t("app.hub_create_title", lang))
                                                .size(16.0)
                                                .strong()
                                                .color(palette.text),
                                        );
                                        ui.label(
                                            egui::RichText::new(t("app.hub_create_subtitle", lang))
                                                .size(12.0)
                                                .color(palette.text_dim),
                                        );
                                    });
                                });

                                ui.add_space(14.0);
                                ui.columns(2, |columns| {
                                    if new_project_card(
                                        &mut columns[0],
                                        &self.ui_icons,
                                        &palette,
                                        "project_game.png",
                                        t("app.hub_new_game_title", lang),
                                        t("app.hub_new_game_desc", lang),
                                        "game_card",
                                    )
                                    .clicked()
                                    {
                                        self.screen = AppScreen::NewProject {
                                            name: String::new(),
                                            path: default_projects_dir(),
                                            project_type: ProjectType::Game,
                                        };
                                    }

                                    if new_project_card(
                                        &mut columns[1],
                                        &self.ui_icons,
                                        &palette,
                                        "project_electronics.png",
                                        t("app.hub_new_electronics_title", lang),
                                        t("app.hub_new_electronics_desc", lang),
                                        "electronics_card",
                                    )
                                    .clicked()
                                    {
                                        self.screen = AppScreen::NewProject {
                                            name: String::new(),
                                            path: default_projects_dir(),
                                            project_type: ProjectType::Electronics,
                                        };
                                    }
                                });
                            });

                        ui.add_space(18.0);

                        egui::Frame::none()
                            .fill(palette.widget)
                            .rounding(16.0)
                            .inner_margin(egui::Margin::symmetric(18.0, 18.0))
                            .stroke(egui::Stroke::new(1.0, palette.border))
                            .show(ui, |ui| {
                                ui.horizontal(|ui| {
                                    ui.vertical(|ui| {
                                        ui.label(
                                            egui::RichText::new(t("app.recent_projects", lang))
                                                .size(16.0)
                                                .strong()
                                                .color(palette.text),
                                        );
                                        ui.label(
                                            egui::RichText::new(format!(
                                                "{} {}",
                                                filtered_projects.len(),
                                                t("app.hub_results_label", lang)
                                            ))
                                            .size(12.0)
                                            .color(palette.text_dim),
                                        );
                                    });
                                });

                                ui.add_space(12.0);

                                egui::Frame::none()
                                    .fill(palette.panel)
                                    .rounding(12.0)
                                    .inner_margin(egui::Margin::symmetric(12.0, 10.0))
                                    .stroke(egui::Stroke::new(1.0, palette.border))
                                    .show(ui, |ui| {
                                        ui.horizontal_wrapped(|ui| {
                                            draw_small_icon(
                                                ui,
                                                &self.ui_icons,
                                                "search_filter_HUB.png",
                                                egui::vec2(16.0, 16.0),
                                                palette.text_dim,
                                            );
                                            ui.add_sized(
                                                [280.0, 30.0],
                                                egui::TextEdit::singleline(&mut self.hub_search_query)
                                                    .hint_text(t("app.hub_search_hint", lang)),
                                            );
                                            ui.add_space(10.0);
                                            filter_button(
                                                ui,
                                                &mut self.hub_filter,
                                                HubProjectFilter::All,
                                                t("app.all", lang),
                                                &palette,
                                            );
                                            filter_button(
                                                ui,
                                                &mut self.hub_filter,
                                                HubProjectFilter::Game,
                                                t("app.hub_game_kind", lang),
                                                &palette,
                                            );
                                            filter_button(
                                                ui,
                                                &mut self.hub_filter,
                                                HubProjectFilter::Electronics,
                                                t("app.hub_electronics_kind", lang),
                                                &palette,
                                            );
                                        });
                                    });

                                ui.add_space(14.0);

                                if filtered_projects.is_empty() {
                                    empty_state_card(ui, &palette, t("app.hub_no_search_results", lang));
                                } else {
                                    for (index, entry) in filtered_projects.iter().enumerate() {
                                        let action = project_card(
                                            ui,
                                            &self.ui_icons,
                                            &palette,
                                            lang,
                                            entry,
                                            index == 0,
                                        );
                                        if pending_action.is_none() {
                                            pending_action = action;
                                        }
                                        ui.add_space(10.0);
                                    }
                                }
                            });
                    });
            });

        match pending_action {
            Some(HubAction::Open(path)) => self.open_project(&path),
            Some(HubAction::Duplicate(path)) => self.duplicate_project_from_hub(&path),
            Some(HubAction::Forget(path)) => {
                self.recent_projects.projects.retain(|entry| entry.path != path);
                let _ = self.recent_projects.save(&dirs_config_dir());
            }
            None => {}
        }
    }

    fn hub_logo_texture(&mut self, ctx: &egui::Context) -> &egui::TextureHandle {
        self.logo_texture.get_or_insert_with(|| {
            let icon_bytes = include_bytes!("../../../../editor/icon.png");
            let image = image::load_from_memory(icon_bytes).expect("Failed to load icon");
            let rgba = image.to_rgba8();
            let (width, height) = rgba.dimensions();
            ctx.load_texture(
                "hub_logo",
                egui::ColorImage::from_rgba_unmultiplied(
                    [width as usize, height as usize],
                    &rgba.into_vec(),
                ),
                egui::TextureOptions::default(),
            )
        })
    }

    fn duplicate_project_from_hub(&mut self, path: &Path) {
        let Some(parent_dir) = path.parent() else {
            self.console
                .log(LogLevel::Error, "Could not duplicate project: missing parent directory");
            return;
        };

        let Some(file_name) = path.file_name().and_then(|name| name.to_str()) else {
            self.console
                .log(LogLevel::Error, "Could not duplicate project: invalid project folder name");
            return;
        };

        let duplicated_dir = next_duplicate_dir(parent_dir, file_name);
        if let Err(error) = copy_dir_recursive(path, &duplicated_dir) {
            self.console.log(
                LogLevel::Error,
                &format!("Failed to duplicate project: {}", error),
            );
            return;
        }

        match Project::load(&duplicated_dir) {
            Ok(mut project) => {
                let folder_name = duplicated_dir
                    .file_name()
                    .and_then(|name| name.to_str())
                    .unwrap_or(file_name)
                    .to_string();
                project.id = Uuid::new_v4();
                project.name = folder_name;
                project.path = duplicated_dir.clone();

                if let Err(error) = project.save() {
                    self.console.log(
                        LogLevel::Error,
                        &format!("Duplicated project created, but metadata update failed: {}", error),
                    );
                    return;
                }

                self.recent_projects.add(&project);
                let _ = self.recent_projects.save(&dirs_config_dir());
                self.console.log(
                    LogLevel::Info,
                    &format!("Project duplicated as '{}'", project.name),
                );
            }
            Err(error) => {
                self.console.log(
                    LogLevel::Error,
                    &format!("Duplicated files, but project metadata could not be loaded: {}", error),
                );
            }
        }
    }
}

fn filtered_recent_projects(
    projects: &[RecentProjectEntry],
    filter: HubProjectFilter,
    query: &str,
) -> Vec<RecentProjectEntry> {
    let query = query.trim().to_ascii_lowercase();
    let mut filtered = projects.to_vec();
    filtered.sort_by(|left, right| right.last_opened.cmp(&left.last_opened));
    filtered.retain(|entry| matches_project_filter(entry, filter));

    if !query.is_empty() {
        filtered.retain(|entry| {
            entry.name.to_ascii_lowercase().contains(&query)
                || entry
                    .path
                    .to_string_lossy()
                    .to_ascii_lowercase()
                    .contains(&query)
        });
    }

    filtered
}

fn matches_project_filter(entry: &RecentProjectEntry, filter: HubProjectFilter) -> bool {
    match filter {
        HubProjectFilter::All => true,
        HubProjectFilter::Game => entry.project_type == ProjectType::Game,
        HubProjectFilter::Electronics => entry.project_type == ProjectType::Electronics,
    }
}

fn filter_button(
    ui: &mut egui::Ui,
    current_filter: &mut HubProjectFilter,
    target_filter: HubProjectFilter,
    label: String,
    palette: &app_theme::ThemePalette,
) {
    let selected = *current_filter == target_filter;
    let button = egui::Button::new(
        egui::RichText::new(label)
            .size(11.5)
            .color(if selected {
                egui::Color32::WHITE
            } else {
                palette.text_dim
            })
            .strong(),
    )
    .fill(if selected {
        app_theme::ACCENT
    } else {
        palette.widget
    })
    .stroke(egui::Stroke::new(1.0, if selected { app_theme::ACCENT } else { palette.border }))
    .rounding(999.0);

    if ui.add(button).clicked() {
        *current_filter = target_filter;
    }
}

fn hub_nav_button(
    ui: &mut egui::Ui,
    atlas: &UiIconAtlas,
    icon_name: &'static str,
    label: String,
    active: bool,
    palette: &app_theme::ThemePalette,
) -> egui::Response {
    let frame = egui::Frame::none()
        .fill(if active { palette.widget_active } else { egui::Color32::TRANSPARENT })
        .rounding(10.0)
        .inner_margin(egui::Margin::symmetric(12.0, 10.0))
        .stroke(egui::Stroke::new(
            1.0,
            if active { app_theme::ACCENT } else { palette.border },
        ));

    let inner = frame.show(ui, |ui| {
        ui.set_width(ui.available_width());
        ui.horizontal(|ui| {
            draw_small_icon(ui, atlas, icon_name, egui::vec2(16.0, 16.0), palette.text);
            ui.add_space(8.0);
            ui.label(
                egui::RichText::new(label.as_str())
                    .size(13.0)
                    .strong()
                    .color(if active { app_theme::ACCENT } else { palette.text_dim }),
            );
        });
    });

    ui.interact(inner.response.rect, ui.id().with(("hub_nav", label.as_str())), egui::Sense::click())
}

fn stat_chip(
    ui: &mut egui::Ui,
    atlas: &UiIconAtlas,
    icon_name: &'static str,
    value: String,
    label: String,
    palette: &app_theme::ThemePalette,
) {
    egui::Frame::none()
        .fill(palette.panel)
        .rounding(12.0)
        .inner_margin(egui::Margin::symmetric(10.0, 8.0))
        .stroke(egui::Stroke::new(1.0, palette.border))
        .show(ui, |ui| {
            ui.set_min_width(118.0);
            ui.horizontal(|ui| {
                draw_small_icon(ui, atlas, icon_name, egui::vec2(18.0, 18.0), palette.text);
                ui.add_space(8.0);
                ui.vertical(|ui| {
                    ui.add(
                        egui::Label::new(
                            egui::RichText::new(value)
                                .size(14.0)
                                .strong()
                                .color(palette.text),
                        )
                        .extend(),
                    );
                    ui.add(
                        egui::Label::new(
                            egui::RichText::new(label)
                                .size(10.5)
                                .color(palette.text_dim),
                        )
                        .extend(),
                    );
                });
            });
        });
}

fn new_project_card(
    ui: &mut egui::Ui,
    atlas: &UiIconAtlas,
    palette: &app_theme::ThemePalette,
    icon_name: &'static str,
    title: String,
    description: String,
    id_source: &'static str,
) -> egui::Response {
    let card = egui::Frame::none()
        .fill(palette.panel)
        .rounding(14.0)
        .inner_margin(egui::Margin::symmetric(16.0, 16.0))
        .stroke(egui::Stroke::new(1.0, palette.border))
        .show(ui, |ui| {
            ui.set_min_height(128.0);
            ui.vertical(|ui| {
                ui.horizontal(|ui| {
                    draw_small_icon(ui, atlas, icon_name, egui::vec2(26.0, 26.0), egui::Color32::WHITE);
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.label(
                            egui::RichText::new("+")
                                .size(20.0)
                                .strong()
                                .color(app_theme::ACCENT),
                        );
                    });
                });
                ui.add_space(16.0);
                ui.label(
                    egui::RichText::new(title)
                        .size(15.0)
                        .strong()
                        .color(palette.text),
                );
                ui.add_space(6.0);
                ui.label(
                    egui::RichText::new(description)
                        .size(12.0)
                        .color(palette.text_dim),
                );
            });
        });

    let response = ui.interact(
        card.response.rect,
        ui.id().with(("hub_create", id_source)),
        egui::Sense::click(),
    );
    if response.hovered() {
        ui.painter().rect_stroke(
            card.response.rect,
            14.0,
            egui::Stroke::new(1.0, app_theme::ACCENT),
        );
    }
    response
}

fn empty_state_card(ui: &mut egui::Ui, palette: &app_theme::ThemePalette, label: String) {
    egui::Frame::none()
        .fill(palette.panel)
        .rounding(14.0)
        .inner_margin(egui::Margin::symmetric(18.0, 22.0))
        .stroke(egui::Stroke::new(1.0, palette.border))
        .show(ui, |ui| {
            ui.vertical_centered(|ui| {
                ui.label(
                    egui::RichText::new(label)
                        .size(13.0)
                        .color(palette.text_dim),
                );
            });
        });
}

fn project_card(
    ui: &mut egui::Ui,
    atlas: &UiIconAtlas,
    palette: &app_theme::ThemePalette,
    lang: raf_core::config::Language,
    entry: &RecentProjectEntry,
    featured: bool,
) -> Option<HubAction> {
    let mut action = None;
    let fill_color = if featured { palette.widget_active } else { palette.panel };
    let type_icon = match entry.project_type {
        ProjectType::Game => "project_game.png",
        ProjectType::Electronics => "project_electronics.png",
    };
    let element_label = match entry.project_type {
        ProjectType::Game => t("app.hub_game_elements", lang),
        ProjectType::Electronics => t("app.hub_electronics_elements", lang),
    };

    egui::Frame::none()
        .fill(fill_color)
        .rounding(14.0)
        .inner_margin(egui::Margin::symmetric(16.0, 14.0))
        .stroke(egui::Stroke::new(
            1.0,
            if featured { app_theme::ACCENT } else { palette.border },
        ))
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                draw_small_icon(ui, atlas, type_icon, egui::vec2(28.0, 28.0), egui::Color32::WHITE);
                ui.add_space(10.0);

                ui.vertical(|ui| {
                    ui.horizontal_wrapped(|ui| {
                        ui.label(
                            egui::RichText::new(&entry.name)
                                .size(15.0)
                                .strong()
                                .color(palette.text),
                        );

                        if featured {
                            egui::Frame::none()
                                .fill(egui::Color32::from_rgba_premultiplied(255, 140, 0, 20))
                                .rounding(999.0)
                                .inner_margin(egui::Margin::symmetric(8.0, 4.0))
                                .show(ui, |ui| {
                                    ui.horizontal(|ui| {
                                        draw_small_icon(
                                            ui,
                                            atlas,
                                            "favorite_pin_HUB.png",
                                            egui::vec2(12.0, 12.0),
                                            app_theme::ACCENT,
                                        );
                                        ui.add_space(5.0);
                                        ui.add(
                                            egui::Label::new(
                                                egui::RichText::new(t("app.hub_last_opened", lang))
                                                    .size(10.0)
                                                    .strong()
                                                    .color(app_theme::ACCENT),
                                            )
                                            .extend(),
                                        );
                                    });
                                });
                        }
                    });

                    ui.add_space(5.0);
                    ui.horizontal_wrapped(|ui| {
                        metadata_pill(
                            ui,
                            palette,
                            format!(
                                "{}: {}",
                                t("app.hub_path", lang),
                                truncate_middle(&entry.path.to_string_lossy(), 54)
                            ),
                        );
                        metadata_pill(
                            ui,
                            palette,
                            format!(
                                "{}: {}",
                                t("app.hub_created", lang),
                                format_hub_date(entry.created_at)
                            ),
                        );
                        metadata_pill(
                            ui,
                            palette,
                            format!(
                                "{}: {}",
                                t("app.hub_modified", lang),
                                format_hub_date(entry.modified_at)
                            ),
                        );
                        metadata_pill(
                            ui,
                            palette,
                            format!(
                                "{}: {}",
                                t("app.hub_last_opened_label", lang),
                                format_hub_date(entry.last_opened)
                            ),
                        );
                        metadata_pill(
                            ui,
                            palette,
                            format!("{} {}", entry.n_elements, element_label),
                        );
                    });
                });

                ui.with_layout(egui::Layout::right_to_left(egui::Align::Min), |ui| {
                    if icon_button(
                        ui,
                        atlas,
                        "delete_HUB.png",
                        t("app.hub_delete", lang),
                        palette,
                        egui::Color32::from_rgb(216, 100, 100),
                    )
                    .clicked()
                    {
                        action = Some(HubAction::Forget(entry.path.clone()));
                    }

                    if icon_button(
                        ui,
                        atlas,
                        "duplicate_HUB.png",
                        t("app.hub_duplicate", lang),
                        palette,
                        palette.text,
                    )
                    .clicked()
                    {
                        action = Some(HubAction::Duplicate(entry.path.clone()));
                    }

                    if icon_button(
                        ui,
                        atlas,
                        "open_HUB.png",
                        t("app.hub_open", lang),
                        palette,
                        egui::Color32::WHITE,
                    )
                    .clicked()
                    {
                        action = Some(HubAction::Open(entry.path.clone()));
                    }
                });
            });
        });

    action
}

fn metadata_pill(ui: &mut egui::Ui, palette: &app_theme::ThemePalette, label: String) {
    egui::Frame::none()
        .fill(palette.widget)
        .rounding(999.0)
        .inner_margin(egui::Margin::symmetric(8.0, 5.0))
        .stroke(egui::Stroke::new(1.0, palette.border))
        .show(ui, |ui| {
            ui.add(
                egui::Label::new(
                    egui::RichText::new(label)
                        .size(11.0)
                        .color(palette.text_dim),
                )
                .extend(),
            );
        });
}

fn icon_button(
    ui: &mut egui::Ui,
    atlas: &UiIconAtlas,
    icon_name: &'static str,
    tooltip: String,
    palette: &app_theme::ThemePalette,
    tint: egui::Color32,
) -> egui::Response {
    let size = egui::vec2(32.0, 32.0);
    let (rect, response) = ui.allocate_exact_size(size, egui::Sense::click());
    let fill = if response.hovered() {
        palette.widget_active
    } else {
        palette.widget
    };

    ui.painter().rect_filled(rect, 8.0, fill);
    ui.painter()
        .rect_stroke(rect, 8.0, egui::Stroke::new(1.0, palette.border));

    let icon_rect = egui::Rect::from_center_size(rect.center(), egui::vec2(16.0, 16.0));
    atlas.paint(ui.painter(), icon_name, icon_rect, tint);

    response.on_hover_text(tooltip)
}

fn draw_small_icon(
    ui: &mut egui::Ui,
    atlas: &UiIconAtlas,
    icon_name: &'static str,
    size: egui::Vec2,
    tint: egui::Color32,
) {
    let (rect, _) = ui.allocate_exact_size(size, egui::Sense::hover());
    atlas.paint(ui.painter(), icon_name, rect, tint);
}

fn truncate_middle(text: &str, max_chars: usize) -> String {
    let char_count = text.chars().count();
    if char_count <= max_chars {
        return text.to_string();
    }

    let prefix_len = max_chars / 2;
    let suffix_len = max_chars.saturating_sub(prefix_len + 1);
    let prefix: String = text.chars().take(prefix_len).collect();
    let suffix: String = text
        .chars()
        .rev()
        .take(suffix_len)
        .collect::<String>()
        .chars()
        .rev()
        .collect();
    format!("{}...{}", prefix, suffix)
}

fn format_hub_date(date: chrono::DateTime<chrono::Utc>) -> String {
    date.format("%d/%m/%Y").to_string()
}

fn next_duplicate_dir(parent_dir: &Path, base_name: &str) -> PathBuf {
    let mut index = 1;
    loop {
        let candidate_name = if index == 1 {
            format!("{} Copy", base_name)
        } else {
            format!("{} Copy {}", base_name, index)
        };
        let candidate = parent_dir.join(candidate_name);
        if !candidate.exists() {
            return candidate;
        }
        index += 1;
    }
}

fn copy_dir_recursive(source: &Path, destination: &Path) -> std::io::Result<()> {
    std::fs::create_dir_all(destination)?;

    for entry in std::fs::read_dir(source)? {
        let entry = entry?;
        let entry_type = entry.file_type()?;
        let source_path = entry.path();
        let destination_path = destination.join(entry.file_name());

        if entry_type.is_dir() {
            copy_dir_recursive(&source_path, &destination_path)?;
        } else {
            std::fs::copy(&source_path, &destination_path)?;
        }
    }

    Ok(())
}
