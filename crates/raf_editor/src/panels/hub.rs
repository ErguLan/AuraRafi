use super::*;

use raf_core::project::RecentProjectEntry;
use std::path::{Path, PathBuf};
use uuid::Uuid;

// Hub layout constants tuned for an editorial, web-like workspace feel.
const HUB_SIDEBAR_WIDTH: f32 = 200.0;
const HUB_CONTENT_MAX_WIDTH: f32 = 1400.0;
const HUB_CONTENT_MIN_WIDTH: f32 = 520.0;
const HUB_CONTENT_HORIZONTAL_MARGIN: f32 = 48.0;
const HUB_SECTION_SPACING: f32 = 36.0;
const HUB_CARD_MIN_WIDTH: f32 = 200.0;
const HUB_CARD_MAX_WIDTH: f32 = 260.0;
const HUB_CARD_HEIGHT: f32 = 172.0;
const HUB_CARD_THUMB_HEIGHT: f32 = 96.0;
const HUB_CARD_SPACING: f32 = 16.0;

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

        let filtered_projects = filtered_recent_projects(
            &self.recent_projects.projects,
            self.hub_filter,
            &self.hub_search_query,
        );

        let mut pending_action = None;
        let mut pending_create = None;

        // ------------------------------------------------------------------
        // Left sidebar: navigation rail.
        // ------------------------------------------------------------------
        egui::SidePanel::left("hub_sidebar")
            .frame(egui::Frame::none().fill(palette.panel))
            .resizable(false)
            .exact_width(HUB_SIDEBAR_WIDTH)
            .show(ctx, |ui| {
                ui.set_width(HUB_SIDEBAR_WIDTH);
                ui.add_space(34.0);

                // Brand mark, centered and reduced.
                ui.vertical_centered(|ui| {
                    ui.image((logo_id, egui::vec2(40.0, 40.0)));
                    ui.add_space(8.0);
                    ui.label(
                        egui::RichText::new("AuraRafi")
                            .size(13.0)
                            .strong()
                            .color(palette.text),
                    );
                });

                ui.add_space(40.0);

                // Primary navigation.
                ui.horizontal(|ui| {
                    ui.add_space(16.0);
                    ui.vertical(|ui| {
                        ui.set_width(HUB_SIDEBAR_WIDTH - 32.0);

                        hub_nav_link(
                            ui,
                            &self.ui_icons,
                            "project_type_HUB.png",
                            t("app.hub_nav_projects", lang),
                            true,
                            &palette,
                        );

                        ui.add_space(6.0);

                        let settings_response = hub_nav_link(
                            ui,
                            &self.ui_icons,
                            "settings_HUB.png",
                            t("app.settings_menu", lang),
                            false,
                            &palette,
                        );
                        if settings_response.clicked() {
                            self.open_settings_screen(AppScreen::ProjectHub);
                        }
                    });
                });

                // Version anchored at the bottom-left.
                ui.with_layout(egui::Layout::bottom_up(egui::Align::Min), |ui| {
                    ui.horizontal(|ui| {
                        ui.add_space(18.0);
                        ui.label(
                            egui::RichText::new(format!("v{}", env!("CARGO_PKG_VERSION")))
                                .size(10.5)
                                .color(palette.text_dim),
                        );
                    });
                    ui.add_space(18.0);
                });
            });

        // ------------------------------------------------------------------
        // Central workspace.
        // ------------------------------------------------------------------
        egui::CentralPanel::default()
            .frame(egui::Frame::none().fill(palette.bg))
            .show(ctx, |ui| {
                egui::ScrollArea::vertical()
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        let available_width = ui.available_width();
                        let content_width = (available_width
                            - HUB_CONTENT_HORIZONTAL_MARGIN * 2.0)
                            .clamp(HUB_CONTENT_MIN_WIDTH, HUB_CONTENT_MAX_WIDTH);
                        let side_pad = ((available_width - content_width) / 2.0).max(24.0);

                        ui.horizontal(|ui| {
                            ui.add_space(side_pad);

                            ui.vertical(|ui| {
                                ui.set_width(content_width);
                                ui.set_max_width(content_width);

                                ui.add_space(40.0);

                                if let Some(project_type) = hub_header(
                                    ui,
                                    &self.ui_icons,
                                    &palette,
                                    lang,
                                    &filtered_projects,
                                ) {
                                    pending_create = Some(project_type);
                                }

                                ui.add_space(HUB_SECTION_SPACING);

                                if let Some(project_type) = hub_search_bar(
                                    ui,
                                    &self.ui_icons,
                                    &palette,
                                    lang,
                                    &mut self.hub_search_query,
                                    &mut self.hub_filter,
                                    filtered_projects.len(),
                                ) {
                                    pending_create = Some(project_type);
                                }

                                ui.add_space(28.0);

                                if filtered_projects.is_empty() {
                                    hub_empty_state(ui, &self.ui_icons, &palette, lang);
                                } else {
                                    hub_project_grid(
                                        ui,
                                        &self.ui_icons,
                                        &palette,
                                        lang,
                                        &filtered_projects,
                                        &mut pending_action,
                                    );
                                }

                                ui.add_space(40.0);
                            });
                        });
                    });
            });

        if let Some(project_type) = pending_create {
            self.screen = AppScreen::NewProject {
                name: String::new(),
                path: default_projects_dir(),
                project_type,
            };
        } else if let Some(action) = pending_action {
            match action {
                HubAction::Open(path) => self.open_project(&path),
                HubAction::Duplicate(path) => self.duplicate_project_from_hub(&path),
                HubAction::Forget(path) => {
                    self.recent_projects
                        .projects
                        .retain(|entry| entry.path != path);
                    let _ = self.recent_projects.save(&dirs_config_dir());
                }
            }
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
            self.console.log(
                LogLevel::Error,
                "Could not duplicate project: missing parent directory",
            );
            return;
        };

        let Some(file_name) = path.file_name().and_then(|name| name.to_str()) else {
            self.console.log(
                LogLevel::Error,
                "Could not duplicate project: invalid project folder name",
            );
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
                        &format!(
                            "Duplicated project created, but metadata update failed: {}",
                            error
                        ),
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
                    &format!(
                        "Duplicated files, but project metadata could not be loaded: {}",
                        error
                    ),
                );
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Filtering and sorting
// ---------------------------------------------------------------------------

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

// ---------------------------------------------------------------------------
// Sidebar navigation
// ---------------------------------------------------------------------------

fn hub_nav_link(
    ui: &mut egui::Ui,
    atlas: &UiIconAtlas,
    icon_name: &'static str,
    label: String,
    active: bool,
    palette: &app_theme::ThemePalette,
) -> egui::Response {
    let available_width = ui.available_width();
    let height = 36.0;
    let (rect, response) =
        ui.allocate_exact_size(egui::vec2(available_width, height), egui::Sense::click());

    let fill = if active {
        palette.widget_active
    } else if response.hovered() {
        palette.widget_hover
    } else {
        egui::Color32::TRANSPARENT
    };
    let stroke_color = if active {
        app_theme::ACCENT
    } else if response.hovered() {
        palette.border
    } else {
        egui::Color32::TRANSPARENT
    };

    ui.painter().rect_filled(rect, 6.0, fill);
    ui.painter()
        .rect_stroke(rect, 6.0, egui::Stroke::new(1.0, stroke_color));

    if active {
        let indicator = egui::Rect::from_min_size(
            rect.min,
            egui::vec2(2.0, rect.height()),
        );
        ui.painter().rect_filled(indicator, 1.0, app_theme::ACCENT);
    }

    let icon_rect = egui::Rect::from_center_size(
        egui::pos2(rect.left() + 22.0, rect.center().y),
        egui::vec2(18.0, 18.0),
    );
    atlas.paint(
        ui.painter(),
        icon_name,
        icon_rect,
        if active { app_theme::ACCENT } else { palette.text_dim },
    );

    ui.painter().text(
        egui::pos2(rect.left() + 44.0, rect.center().y),
        egui::Align2::LEFT_CENTER,
        label.clone(),
        egui::FontId::proportional(12.5),
        if active { palette.text } else { palette.text_dim },
    );

    response.on_hover_text(label)
}

// ---------------------------------------------------------------------------
// Header with title, stats and primary creation actions.
// ---------------------------------------------------------------------------

fn hub_header(
    ui: &mut egui::Ui,
    atlas: &UiIconAtlas,
    palette: &app_theme::ThemePalette,
    lang: raf_core::config::Language,
    projects: &[RecentProjectEntry],
) -> Option<ProjectType> {
    let mut create = None;

    ui.horizontal(|ui| {
        // Left: title, subtitle and compact stats.
        ui.vertical(|ui| {
            ui.label(
                egui::RichText::new(t("app.hub_projects", lang))
                    .size(24.0)
                    .strong()
                    .color(palette.text),
            );
            ui.add_space(5.0);
            ui.label(
                egui::RichText::new(t("app.hub_subtitle", lang))
                    .size(12.0)
                    .color(palette.text_dim),
            );
            ui.add_space(10.0);
            hub_stat_chips(ui, atlas, palette, lang, projects);
        });

        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if hub_primary_button(
                ui,
                atlas,
                palette,
                "project_electronics.png",
                t("app.hub_new_electronics_short", lang),
                false,
            )
            .clicked()
            {
                create = Some(ProjectType::Electronics);
            }

            ui.add_space(10.0);

            if hub_primary_button(
                ui,
                atlas,
                palette,
                "project_game.png",
                t("app.hub_new_game_short", lang),
                true,
            )
            .clicked()
            {
                create = Some(ProjectType::Game);
            }
        });
    });

    create
}

fn hub_stat_chips(
    ui: &mut egui::Ui,
    _atlas: &UiIconAtlas,
    palette: &app_theme::ThemePalette,
    lang: raf_core::config::Language,
    projects: &[RecentProjectEntry],
) {
    let total = projects.len();
    let game_count = projects
        .iter()
        .filter(|entry| entry.project_type == ProjectType::Game)
        .count();
    let electronics_count = total.saturating_sub(game_count);

    ui.horizontal(|ui| {
        let label = format!(
            "{} {}  ·  {} {}  ·  {} {}",
            total,
            t("app.hub_total_projects", lang),
            game_count,
            t("app.hub_game_kind", lang),
            electronics_count,
            t("app.hub_electronics_kind", lang)
        );

        ui.label(
            egui::RichText::new(label)
                .size(11.0)
                .color(palette.text_dim),
        );
    });
}

fn hub_primary_button(
    ui: &mut egui::Ui,
    atlas: &UiIconAtlas,
    palette: &app_theme::ThemePalette,
    icon_name: &'static str,
    label: String,
    primary: bool,
) -> egui::Response {
    let padding = egui::vec2(16.0, 10.0);
    let icon_size = egui::vec2(16.0, 16.0);
    let text_width = ui.fonts(|fonts| {
        fonts
            .layout_no_wrap(
                label.clone(),
                egui::FontId::proportional(12.0),
                egui::Color32::PLACEHOLDER,
            )
            .size()
            .x
    });
    let width = icon_size.x + 8.0 + text_width + padding.x * 2.0;
    let height = 34.0;

    let (rect, response) =
        ui.allocate_exact_size(egui::vec2(width, height), egui::Sense::click());

    let fill = if primary {
        if response.hovered() {
            app_theme::ACCENT_HOVER
        } else {
            app_theme::ACCENT
        }
    } else if response.hovered() {
        palette.widget_hover
    } else {
        palette.widget
    };
    let stroke = if primary {
        egui::Stroke::NONE
    } else {
        egui::Stroke::new(1.0, palette.border)
    };
    let text_color = if primary {
        egui::Color32::WHITE
    } else {
        palette.text
    };
    let icon_tint = if primary {
        egui::Color32::WHITE
    } else {
        app_theme::ACCENT
    };

    ui.painter().rect_filled(rect, 6.0, fill);
    ui.painter().rect_stroke(rect, 6.0, stroke);

    let icon_rect = egui::Rect::from_center_size(
        egui::pos2(rect.left() + padding.x + icon_size.x / 2.0, rect.center().y),
        icon_size,
    );
    atlas.paint(ui.painter(), icon_name, icon_rect, icon_tint);

    ui.painter().text(
        egui::pos2(rect.left() + padding.x + icon_size.x + 8.0, rect.center().y),
        egui::Align2::LEFT_CENTER,
        label,
        egui::FontId::proportional(12.0),
        text_color,
    );

    response
}

// ---------------------------------------------------------------------------
// Search and filter bar
// ---------------------------------------------------------------------------

fn hub_search_bar(
    ui: &mut egui::Ui,
    atlas: &UiIconAtlas,
    palette: &app_theme::ThemePalette,
    lang: raf_core::config::Language,
    search_query: &mut String,
    current_filter: &mut HubProjectFilter,
    visible_count: usize,
) -> Option<ProjectType> {
    egui::Frame::none()
        .fill(palette.panel)
        .rounding(8.0)
        .inner_margin(egui::Margin::symmetric(14.0, 10.0))
        .stroke(egui::Stroke::new(1.0, palette.border))
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                // Search input.
                let search_icon_rect = ui.allocate_exact_size(egui::vec2(16.0, 16.0), egui::Sense::hover()).0;
                atlas.paint(
                    ui.painter(),
                    "search_filter_HUB.png",
                    search_icon_rect,
                    palette.text_dim,
                );

                ui.add_space(8.0);

                let search_width = (ui.available_width() * 0.28).clamp(180.0, 320.0);
                ui.add_sized(
                    [search_width, 28.0],
                    egui::TextEdit::singleline(search_query)
                        .hint_text(t("app.hub_search_hint", lang)),
                );

                // Vertical divider.
                ui.add_space(14.0);
                let divider_y = ui.cursor().center().y;
                let divider_x = ui.cursor().left();
                ui.painter().line_segment(
                    [
                        egui::pos2(divider_x, divider_y - 10.0),
                        egui::pos2(divider_x, divider_y + 10.0),
                    ],
                    egui::Stroke::new(1.0, palette.border),
                );
                ui.add_space(8.0);

                // Filter pills.
                hub_filter_pill(
                    ui,
                    current_filter,
                    HubProjectFilter::All,
                    t("app.all", lang),
                    palette,
                );
                ui.add_space(6.0);
                hub_filter_pill(
                    ui,
                    current_filter,
                    HubProjectFilter::Game,
                    t("app.hub_game_kind", lang),
                    palette,
                );
                ui.add_space(6.0);
                hub_filter_pill(
                    ui,
                    current_filter,
                    HubProjectFilter::Electronics,
                    t("app.hub_electronics_kind", lang),
                    palette,
                );

                // Result count stays on the right, unobtrusive.
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.label(
                        egui::RichText::new(format!(
                            "{} {}",
                            visible_count,
                            t("app.hub_results_label", lang)
                        ))
                        .size(11.0)
                        .color(palette.text_dim),
                    );
                });
            });
        });

    None
}

fn hub_filter_pill(
    ui: &mut egui::Ui,
    current_filter: &mut HubProjectFilter,
    target_filter: HubProjectFilter,
    label: String,
    palette: &app_theme::ThemePalette,
) {
    let selected = *current_filter == target_filter;
    let padding = egui::vec2(12.0, 6.0);
    let text_width = ui.fonts(|fonts| {
        fonts
            .layout_no_wrap(
                label.clone(),
                egui::FontId::proportional(11.5),
                egui::Color32::PLACEHOLDER,
            )
            .size()
            .x
    });
    let width = text_width + padding.x * 2.0;
    let height = 26.0;

    let (rect, response) =
        ui.allocate_exact_size(egui::vec2(width, height), egui::Sense::click());

    let fill = if selected {
        app_theme::ACCENT
    } else if response.hovered() {
        palette.widget_hover
    } else {
        palette.widget
    };
    let stroke = if selected {
        app_theme::ACCENT
    } else {
        palette.border
    };
    let text_color = if selected {
        egui::Color32::WHITE
    } else {
        palette.text_dim
    };

    ui.painter().rect_filled(rect, 6.0, fill);
    ui.painter().rect_stroke(rect, 6.0, egui::Stroke::new(1.0, stroke));

    ui.painter().text(
        rect.center(),
        egui::Align2::CENTER_CENTER,
        label,
        egui::FontId::proportional(11.5),
        text_color,
    );

    if response.clicked() {
        *current_filter = target_filter;
    }
}

// ---------------------------------------------------------------------------
// Project grid
// ---------------------------------------------------------------------------

fn hub_project_grid(
    ui: &mut egui::Ui,
    atlas: &UiIconAtlas,
    palette: &app_theme::ThemePalette,
    lang: raf_core::config::Language,
    projects: &[RecentProjectEntry],
    pending_action: &mut Option<HubAction>,
) {
    let available_width = ui.available_width();
    let spacing = HUB_CARD_SPACING;

    let columns = ((available_width + spacing) / (HUB_CARD_MIN_WIDTH + spacing))
        .floor()
        .max(1.0) as usize;
    let card_width = ((available_width - (columns.saturating_sub(1)) as f32 * spacing)
        / columns as f32)
        .clamp(HUB_CARD_MIN_WIDTH, HUB_CARD_MAX_WIDTH);

    let mut column_index = 0;

    ui.horizontal_wrapped(|ui| {
        for (index, entry) in projects.iter().enumerate() {
            let action = hub_project_card(
                ui,
                atlas,
                palette,
                lang,
                entry,
                index == 0,
                card_width,
            );
            if pending_action.is_none() {
                *pending_action = action;
            }

            column_index += 1;
            if column_index < columns {
                ui.add_space(spacing);
            } else {
                column_index = 0;
            }
        }
    });
}

fn hub_project_card(
    ui: &mut egui::Ui,
    atlas: &UiIconAtlas,
    palette: &app_theme::ThemePalette,
    lang: raf_core::config::Language,
    entry: &RecentProjectEntry,
    featured: bool,
    card_width: f32,
) -> Option<HubAction> {
    let card_rect = ui
        .allocate_exact_size(
            egui::vec2(card_width, HUB_CARD_HEIGHT),
            egui::Sense::hover(),
        )
        .0;

    let card_id = ui.make_persistent_id(("hub_card", entry.path.to_string_lossy().as_ref()));
    let card_response = ui.interact(card_rect, card_id, egui::Sense::hover());
    let hovered = card_response.hovered();

    let fill = if hovered {
        palette.widget_hover
    } else {
        palette.panel
    };
    let stroke_color = if featured {
        app_theme::ACCENT
    } else if hovered {
        app_theme::ACCENT
    } else {
        palette.border
    };

    ui.painter().rect_filled(card_rect, 8.0, fill);
    ui.painter()
        .rect_stroke(card_rect, 8.0, egui::Stroke::new(1.0, stroke_color));

    // Thumbnail placeholder.
    let thumb_rect = egui::Rect::from_min_size(
        card_rect.min,
        egui::vec2(card_rect.width(), HUB_CARD_THUMB_HEIGHT),
    );
    hub_project_thumbnail(ui.painter(), atlas, palette, thumb_rect, entry.project_type);

    // Featured indicator: a small dot beside the name.
    let name_x = card_rect.left() + 14.0;
    let name_y = card_rect.top() + HUB_CARD_THUMB_HEIGHT + 22.0;
    let name_color = palette.text;

    if featured {
        let dot_rect = egui::Rect::from_center_size(
            egui::pos2(name_x + 4.0, name_y),
            egui::vec2(6.0, 6.0),
        );
        ui.painter().rect_filled(dot_rect, 3.0, app_theme::ACCENT);
    }

    let name_offset_x = if featured { 16.0 } else { 0.0 };
    let max_name_width = card_rect.width() - 28.0 - name_offset_x;
    let display_name = truncate_to_width(ui, &entry.name, max_name_width, 14.0);

    ui.painter().text(
        egui::pos2(name_x + name_offset_x, name_y),
        egui::Align2::LEFT_CENTER,
        display_name,
        egui::FontId::proportional(14.0),
        name_color,
    );

    // Type and date line.
    let meta_y = card_rect.top() + HUB_CARD_THUMB_HEIGHT + 46.0;
    let type_label = match entry.project_type {
        ProjectType::Game => t("app.hub_type_game_label", lang),
        ProjectType::Electronics => t("app.hub_type_electronics_label", lang),
    };
    let date_label = format_hub_date(entry.last_opened);
    ui.painter().text(
        egui::pos2(name_x, meta_y),
        egui::Align2::LEFT_CENTER,
        format!("{}  ·  {}", type_label, date_label),
        egui::FontId::proportional(11.0),
        palette.text_dim,
    );

    // Action buttons appear only on hover.
    if hovered {
        let button_size = egui::vec2(26.0, 26.0);
        let button_gap = 6.0;
        let right_x = card_rect.right() - 10.0;
        let top_y = card_rect.top() + 10.0;

        let forget_rect = egui::Rect::from_min_size(
            egui::pos2(right_x - button_size.x, top_y),
            button_size,
        );
        let duplicate_rect =
            forget_rect.translate(egui::vec2(-(button_size.x + button_gap), 0.0));
        let open_rect =
            duplicate_rect.translate(egui::vec2(-(button_size.x + button_gap), 0.0));

        let path_key = entry.path.to_string_lossy();

        if hub_card_action_button(
            ui,
            atlas,
            open_rect,
            ui.make_persistent_id(("hub_card_open", path_key.as_ref())),
            "open_HUB.png",
            t("app.hub_open", lang),
            palette,
            egui::Color32::WHITE,
        )
        .clicked()
        {
            return Some(HubAction::Open(entry.path.clone()));
        }

        if hub_card_action_button(
            ui,
            atlas,
            duplicate_rect,
            ui.make_persistent_id(("hub_card_duplicate", path_key.as_ref())),
            "duplicate_HUB.png",
            t("app.hub_duplicate", lang),
            palette,
            palette.text,
        )
        .clicked()
        {
            return Some(HubAction::Duplicate(entry.path.clone()));
        }

        if hub_card_action_button(
            ui,
            atlas,
            forget_rect,
            ui.make_persistent_id(("hub_card_forget", path_key.as_ref())),
            "delete_HUB.png",
            t("app.hub_delete", lang),
            palette,
            app_theme::STATUS_ERROR,
        )
        .clicked()
        {
            return Some(HubAction::Forget(entry.path.clone()));
        }
    }

    // Clicking the card body opens the project unless an action button consumed it.
    ui.input(|input| {
        if input.pointer.primary_clicked()
            && card_rect.contains(input.pointer.interact_pos().unwrap_or(egui::Pos2::ZERO))
        {
            return Some(HubAction::Open(entry.path.clone()));
        }
        None
    })
}

fn hub_project_thumbnail(
    painter: &egui::Painter,
    atlas: &UiIconAtlas,
    palette: &app_theme::ThemePalette,
    rect: egui::Rect,
    project_type: ProjectType,
) {
    let (base_color, glow_color, icon_name) = match project_type {
        ProjectType::Game => (
            egui::Color32::from_rgb(25, 42, 48),
            egui::Color32::from_rgb(39, 178, 211),
            "project_game.png",
        ),
        ProjectType::Electronics => (
            egui::Color32::from_rgb(42, 30, 18),
            app_theme::ACCENT,
            "project_electronics.png",
        ),
    };

    painter.rect_filled(rect, 8.0, base_color);

    // Subtle top glow line.
    let glow_height = 1.5;
    let glow_rect = egui::Rect::from_min_size(
        rect.min,
        egui::vec2(rect.width(), glow_height),
    );
    painter.rect_filled(glow_rect, 0.0, glow_color);

    // Icon centered in the thumbnail.
    let icon_rect = egui::Rect::from_center_size(rect.center(), egui::vec2(36.0, 36.0));
    atlas.paint(painter, icon_name, icon_rect, glow_color);

    // Very faint grid pattern to give texture.
    let grid_color = egui::Color32::from_rgba_unmultiplied(
        palette.text.r(),
        palette.text.g(),
        palette.text.b(),
        8,
    );
    let step = 16.0;
    let mut x = rect.left() + step;
    while x < rect.right() {
        painter.line_segment(
            [egui::pos2(x, rect.top()), egui::pos2(x, rect.bottom())],
            egui::Stroke::new(1.0, grid_color),
        );
        x += step;
    }
}

fn hub_card_action_button(
    ui: &mut egui::Ui,
    atlas: &UiIconAtlas,
    rect: egui::Rect,
    id: egui::Id,
    icon_name: &'static str,
    tooltip: String,
    palette: &app_theme::ThemePalette,
    tint: egui::Color32,
) -> egui::Response {
    let response = ui.interact(rect, id, egui::Sense::click());
    let fill = if response.hovered() {
        palette.widget
    } else {
        egui::Color32::from_rgba_unmultiplied(
            palette.widget.r(),
            palette.widget.g(),
            palette.widget.b(),
            220,
        )
    };

    ui.painter().rect_filled(rect, 5.0, fill);
    ui.painter()
        .rect_stroke(rect, 5.0, egui::Stroke::new(1.0, palette.border));

    let icon_rect = egui::Rect::from_center_size(rect.center(), egui::vec2(14.0, 14.0));
    atlas.paint(ui.painter(), icon_name, icon_rect, tint);

    response.on_hover_text(tooltip)
}

// ---------------------------------------------------------------------------
// Empty state
// ---------------------------------------------------------------------------

fn hub_empty_state(
    ui: &mut egui::Ui,
    atlas: &UiIconAtlas,
    palette: &app_theme::ThemePalette,
    lang: raf_core::config::Language,
) {
    let available = ui.available_rect_before_wrap();
    let center = available.center();
    let icon_size = egui::vec2(64.0, 64.0);
    let icon_rect = egui::Rect::from_center_size(center - egui::vec2(0.0, 32.0), icon_size);

    atlas.paint(ui.painter(), "empty_state.png", icon_rect, palette.text_dim);

    ui.painter().text(
        egui::pos2(center.x, center.y + 26.0),
        egui::Align2::CENTER_CENTER,
        t("app.hub_empty_title", lang),
        egui::FontId::proportional(16.0),
        palette.text,
    );

    ui.painter().text(
        egui::pos2(center.x, center.y + 50.0),
        egui::Align2::CENTER_CENTER,
        t("app.hub_empty_subtitle", lang),
        egui::FontId::proportional(12.0),
        palette.text_dim,
    );
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn truncate_to_width(ui: &egui::Ui, text: &str, max_width: f32, font_size: f32) -> String {
    let test_font = egui::FontId::proportional(font_size);
    let mut result = text.to_string();

    let measure = |s: &str| -> f32 {
        ui.fonts(|fonts| {
            fonts
                .layout_no_wrap(s.to_string(), test_font.clone(), egui::Color32::PLACEHOLDER)
                .size()
                .x
        })
    };

    if measure(&result) <= max_width {
        return result;
    }

    result.push_str("...");
    while measure(&result) > max_width && result.len() > 3 {
        let chars = result.chars().count();
        let remove_pos = chars.saturating_sub(4).max(1);
        result = result
            .chars()
            .enumerate()
            .filter(|(i, _)| *i != remove_pos)
            .map(|(_, c)| c)
            .collect();
    }

    result
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
