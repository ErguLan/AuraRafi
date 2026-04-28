//! Main application - ties together loading screen, project hub, and editor.
//!
//! Application flow:
//! 1. Loading screen (brief, shows branding)
//! 2. Project Hub (recent projects + create new: Game or Electronics)
//! 3. Main Editor (viewport, hierarchy, properties, assets, console, AI chat,
//!    node editor, schematic view)

use eframe::egui;
use raf_core::i18n::t;
use raf_core::config::{EngineSettings, RenderPreset, Theme};
use raf_core::project::{Project, ProjectType, RecentProjects};
use raf_core::scene::graph::Primitive;
use raf_core::scene::SceneGraph;
use raf_render::render_config::RenderConfig;

#[path = "panels/hub.rs"]
mod hub;

use crate::ui_icons::UiIconAtlas;
use crate::game_runtime::{GameRuntimeState, RuntimeInputState};
use crate::panels::ai_chat::AiChatPanel;
use crate::panels::asset_browser::AssetBrowserPanel;
use crate::panels::console::{ConsolePanel, LogLevel};
use crate::panels::hierarchy::HierarchyPanel;
use crate::panels::node_editor::NodeEditorDocument;
use crate::panels::node_editor::NodeEditorPanel;
use crate::panels::pcb_panels;
use crate::panels::pcb_view::PcbViewPanel;
use crate::panels::properties::PropertiesPanel;
use crate::panels::project_settings;
use crate::panels::schematic_panels;
use crate::panels::schematic_view::SchematicViewPanel;
use crate::panels::settings_panel;
use crate::panels::viewport::ViewportPanel;
use crate::pcb_document::{load_pcb_document, save_pcb_document};
use crate::schematic_document::{load_schematic_document, save_schematic_document};
use crate::theme as app_theme;

// ---------------------------------------------------------------------------
// Application state machine
// ---------------------------------------------------------------------------

/// Current screen of the application.
#[derive(Debug, Clone, PartialEq)]
enum AppScreen {
    /// Loading screen with progress.
    Loading { progress: f32, start_time: f64 },
    /// Project hub: choose recent or create new.
    ProjectHub,
    /// Create new project form.
    NewProject {
        name: String,
        path: String,
        project_type: ProjectType,
    },
    /// Main editor.
    Editor,
    /// Settings screen (overlay).
    Settings,
}

/// Bottom panel tab selection in the editor.
#[derive(Debug, Clone, PartialEq, Eq)]
enum BottomTab {
    Assets,
    Console,
    AiChat,
    NodeEditor,
    ProjectSettings,
    Complement(String),
}

/// Central viewport mode for the editor body.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ViewportMode {
    Scene,
    Schematic,
    Pcb,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum HubProjectFilter {
    All,
    Game,
    Electronics,
}

#[derive(Debug, Clone)]
enum EditorHistorySnapshot {
    Scene(String),
    Schematic(String),
    Pcb(String),
}

const EDITOR_UI_ICONS: &[&str] = &[
    "3-dots vertical.png",
    "ai_chat.png",
    "assets.png",
    "audio.png",
    "console.png",
    "cube.png",
    "cylinder.png",
    "empty.png",
    "folder.png",
    "focus.png",
    "hidden.png",
    "image.png",
    "material.png",
    "model.png",
    "move.png",
    "node_editor.png",
    "object_mode.png",
    "opacity.png",
    "plane.png",
    "project_settings.png",
    "rotate.png",
    "scene.png",
    "script.png",
    "select.png",
    "shape.png",
    "sphere.png",
    "sprite.png",
    "transform.png",
    "variables.png",
    "vertex_mode.png",
    "visible.png",
];

const HUB_UI_ICONS: &[&str] = &[
    "delete_HUB.png",
    "duplicate_HUB.png",
    "favorite_pin_HUB.png",
    "open_HUB.png",
    "project_type_HUB.png",
    "search_filter_HUB.png",
    "settings_HUB.png",
    "project_game.png",
    "project_electronics.png",
];

// ---------------------------------------------------------------------------
// Main app
// ---------------------------------------------------------------------------

/// The AuraRafi editor application.
pub struct AuraRafiApp {
    // State
    screen: AppScreen,
    previous_screen: Option<AppScreen>,
    settings: EngineSettings,
    recent_projects: RecentProjects,

    // Active project
    current_project: Option<Project>,
    scene: SceneGraph,
    runtime: Option<GameRuntimeState>,

    // Editor panels
    viewport: ViewportPanel,
    hierarchy: HierarchyPanel,
    properties: PropertiesPanel,
    asset_browser: AssetBrowserPanel,
    console: ConsolePanel,
    ai_chat: AiChatPanel,
    node_editor: NodeEditorPanel,
    schematic_view: SchematicViewPanel,
    pcb_view: PcbViewPanel,

    // Editor state
    bottom_tab: BottomTab,
    bottom_panel_snap_height: Option<f32>,
    viewport_mode: ViewportMode,
    _show_settings: bool,
    
    // Extensions
    complement_registry: raf_core::complement::ComplementRegistry,
    command_bus: raf_core::command::CommandBus,
    frame_count: u64,

    // v0.3.0: UX state
    /// Whether scene has unsaved changes.
    scene_modified: bool,
    /// Last status message for the status bar.
    last_action: String,
    /// Elapsed seconds since last auto-save.
    auto_save_elapsed: f32,
    /// Snapshots for undo in the active editor document.
    undo_stack: Vec<EditorHistorySnapshot>,
    /// Snapshots for redo.
    redo_stack: Vec<EditorHistorySnapshot>,
    /// Project logo texture.
    logo_texture: Option<egui::TextureHandle>,
    ui_icons: UiIconAtlas,
    hub_search_query: String,
    hub_filter: HubProjectFilter,
}

impl AuraRafiApp {
    /// Create the application.
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        // Load settings from app data directory.
        let config_dir = dirs_config_dir();
        let settings = EngineSettings::load(&config_dir);
        let recent_projects = RecentProjects::load(&config_dir);

        // Apply initial theme.
        app_theme::apply_theme(&cc.egui_ctx, settings.theme, settings.theme_experimental);

        // Set font size.
        let mut style = (*cc.egui_ctx.style()).clone();
        style.text_styles.iter_mut().for_each(|(_, font_id)| {
            font_id.size = settings.font_size;
        });
        cc.egui_ctx.set_style(style);

        Self {
            screen: AppScreen::Loading {
                progress: 0.0,
                start_time: 0.0,
            },
            previous_screen: None,
            settings,
            recent_projects,
            current_project: None,
            scene: SceneGraph::new(),
            runtime: None,
            viewport: ViewportPanel::default(),
            hierarchy: HierarchyPanel::default(),
            properties: PropertiesPanel::default(),
            asset_browser: AssetBrowserPanel::default(),
            console: ConsolePanel::default(),
            ai_chat: AiChatPanel::default(),
            node_editor: NodeEditorPanel::default(),
            schematic_view: SchematicViewPanel::default(),
            pcb_view: PcbViewPanel::default(),

            complement_registry: raf_core::complement::ComplementRegistry::new(),
            command_bus: raf_core::command::CommandBus::new(),

            bottom_tab: BottomTab::Console,
            bottom_panel_snap_height: None,
            viewport_mode: ViewportMode::Scene,
            _show_settings: false,
            frame_count: 0,
            scene_modified: false,
            last_action: String::new(),
            auto_save_elapsed: 0.0,
            undo_stack: Vec::new(),
            redo_stack: Vec::new(),
            logo_texture: None,
            ui_icons: UiIconAtlas::default(),
            hub_search_query: String::new(),
            hub_filter: HubProjectFilter::All,
        }
    }
}

impl eframe::App for AuraRafiApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.frame_count += 1;

        // Re-apply theme every frame (cheap, ensures consistency).
        app_theme::apply_theme(ctx, self.settings.theme, self.settings.theme_experimental);

        match self.screen.clone() {
            AppScreen::Loading {
                progress,
                start_time,
            } => {
                self.show_loading(ctx, progress, start_time);
            }
            AppScreen::ProjectHub => {
                self.show_project_hub(ctx);
            }
            AppScreen::NewProject {
                name,
                path,
                project_type,
            } => {
                self.show_new_project(ctx, name, path, project_type);
            }
            AppScreen::Editor => {
                self.show_editor(ctx);
            }
            AppScreen::Settings => {
                self.show_settings_screen(ctx);
            }
        }
    }
}

impl AuraRafiApp {
    // -----------------------------------------------------------------------
    // Loading Screen
    // -----------------------------------------------------------------------

    fn show_loading(&mut self, ctx: &egui::Context, _progress: f32, start_time: f64) {
        let time = ctx.input(|i| i.time);
        let start = if start_time == 0.0 { time } else { start_time };
        let palette = app_theme::palette_for(self.settings.theme, self.settings.theme_experimental);

        egui::CentralPanel::default()
            .frame(egui::Frame::default().fill(palette.bg))
            .show(ctx, |ui| {
                let available = ui.available_rect_before_wrap();
                let center = available.center();

                // Brand name with subtle glow effect.
                ui.painter().text(
                    egui::pos2(center.x, center.y - 50.0),
                    egui::Align2::CENTER_CENTER,
                    "Proyecto Rafi",
                    egui::FontId::proportional(52.0),
                    app_theme::ACCENT,
                );

                // Tagline.
                let tagline = t("app.develop_your_own_project", self.settings.language);
                ui.painter().text(
                    egui::pos2(center.x, center.y),
                    egui::Align2::CENTER_CENTER,
                    tagline,
                    egui::FontId::proportional(16.0),
                    palette.text_dim,
                );

                // Progress bar.
                let bar_width = 320.0;
                let bar_height = 3.0;
                let bar_rect = egui::Rect::from_center_size(
                    egui::pos2(center.x, center.y + 50.0),
                    egui::vec2(bar_width, bar_height),
                );

                // Background track.
                ui.painter().rect_filled(
                    bar_rect,
                    bar_height / 2.0,
                    palette.widget,
                );

                // Fill.
                let new_progress = ((time - start) / 1.5).min(1.0) as f32;
                let fill_rect = egui::Rect::from_min_size(
                    bar_rect.min,
                    egui::vec2(bar_width * new_progress, bar_height),
                );
                ui.painter().rect_filled(
                    fill_rect,
                    bar_height / 2.0,
                    app_theme::ACCENT,
                );

                // Loading text.
                let loading_text =
                    t("app.loading", self.settings.language);
                ui.painter().text(
                    egui::pos2(center.x, center.y + 68.0),
                    egui::Align2::CENTER_CENTER,
                    loading_text,
                    egui::FontId::proportional(12.0),
                    palette.text_dim,
                );

                // --- Subtle Yoll credit at the bottom ---
                ui.painter().text(
                    egui::pos2(center.x, available.bottom() - 36.0),
                    egui::Align2::CENTER_CENTER,
                    "A project by Yoll",
                    egui::FontId::proportional(11.0),
                    palette.text_dim,
                );
                ui.painter().text(
                    egui::pos2(center.x, available.bottom() - 20.0),
                    egui::Align2::CENTER_CENTER,
                    "yoll.site",
                    egui::FontId::proportional(10.0),
                    palette.border,
                );

                // Version (small, corner).
                ui.painter().text(
                    egui::pos2(available.right() - 10.0, available.bottom() - 10.0),
                    egui::Align2::RIGHT_BOTTOM,
                    format!("v{}", env!("CARGO_PKG_VERSION")),
                    egui::FontId::proportional(9.0),
                    palette.separator,
                );

                // Transition after loading completes.
                if new_progress >= 1.0 {
                    self.screen = AppScreen::ProjectHub;
                } else {
                    self.screen = AppScreen::Loading {
                        progress: new_progress,
                        start_time: start,
                    };
                    ctx.request_repaint();
                }
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
        let palette = app_theme::palette_for_visuals(ctx.style().visuals.dark_mode, self.settings.theme_experimental);
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
                        palette.widget
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
                                self.current_project = Some(project.clone());
                                // Wire assets path to browser.
                                let assets_dir = std::path::PathBuf::from(&project.path).join("assets");
                                self.asset_browser.project_assets_path = Some(assets_dir);
                                self.asset_browser.scan_project_folder();
                                self.init_scene_for_type(project_type);
                                self.runtime = None;
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

    // -----------------------------------------------------------------------
    // Main Editor
    // -----------------------------------------------------------------------

    fn show_editor(&mut self, ctx: &egui::Context) {
        let _lang = self.settings.language;
        let palette = app_theme::palette_for_visuals(ctx.style().visuals.dark_mode, self.settings.theme_experimental);
        self.ui_icons.request_icons(EDITOR_UI_ICONS);
        self.ui_icons.process_load_budget(ctx, ui_icon_budget(ctx));

        // --- Global keyboard shortcuts ---
        self.handle_global_shortcuts(ctx);

        // --- Auto-save ---
        self.handle_auto_save(ctx);

        if let Some(runtime) = self.runtime.as_mut() {
            let report = runtime.update(
                ctx.input(|input| input.predicted_dt.max(1.0 / 240.0)),
                RuntimeInputState::from_egui(ctx),
            );
            for log in report.logs {
                self.console.log(LogLevel::Info, &log);
            }
            for error in report.errors {
                self.console.log(LogLevel::Error, &error);
            }
        }

        // Top menu bar.
        egui::TopBottomPanel::top("menu_bar").show(ctx, |ui| {
            egui::menu::bar(ui, |ui| {
                // -- File --
                ui.menu_button(t("app.file", _lang), |ui| {
                    if ui.button(t("app.new_project", _lang)).clicked() {
                        self.screen = AppScreen::ProjectHub;
                        ui.close_menu();
                    }
                    if ui.button(t("app.save_menu", _lang)).clicked() {
                        self.do_save();
                        ui.close_menu();
                    }
                    ui.separator();
                    if ui.button(t("app.settings_menu", _lang)).clicked() {
                        self.previous_screen = Some(AppScreen::Editor);
                        self.screen = AppScreen::Settings;
                        ui.close_menu();
                    }
                    ui.separator();
                    if ui.button(t("app.exit_to_hub", _lang)).clicked() {
                        self.screen = AppScreen::ProjectHub;
                        ui.close_menu();
                    }
                });

                // -- Edit --
                ui.menu_button(t("app.edit_menu", _lang), |ui| {
                    let undo_label = format!("{}  [{}]", t("app.undo_menu", _lang), self.undo_stack.len());
                    if ui.add_enabled(!self.undo_stack.is_empty(), egui::Button::new(undo_label)).clicked() {
                        self.do_undo();
                        ui.close_menu();
                    }
                    let redo_label = format!("{}  [{}]", t("app.redo_menu", _lang), self.redo_stack.len());
                    if ui.add_enabled(!self.redo_stack.is_empty(), egui::Button::new(redo_label)).clicked() {
                        self.do_redo();
                        ui.close_menu();
                    }
                    ui.separator();
                    if ui.button(t("app.duplicate_menu", _lang)).clicked() {
                        self.do_duplicate();
                        ui.close_menu();
                    }
                    if ui.button(t("app.delete_menu", _lang)).clicked() {
                        self.do_delete();
                        ui.close_menu();
                    }
                    ui.separator();
                    if ui.button(t("app.select_all_menu", _lang)).clicked() {
                        self.do_select_all();
                        ui.close_menu();
                    }
                });

                // -- View --
                ui.menu_button(t("app.view_menu", _lang), |ui| {
                    ui.checkbox(&mut self.settings.grid_visible, t("app.grid_menu", _lang));
                    ui.separator();
                    match self.current_project.as_ref().map(|project| project.project_type) {
                        Some(ProjectType::Electronics) => {
                            if ui.selectable_label(self.viewport_mode == ViewportMode::Schematic, t("app.schematic_view", _lang)).clicked() {
                                self.viewport_mode = ViewportMode::Schematic;
                                ui.close_menu();
                            }
                            if ui.selectable_label(self.viewport_mode == ViewportMode::Pcb, t("app.pcb_view", _lang)).clicked() {
                                self.sync_pcb_from_schematic();
                                self.viewport_mode = ViewportMode::Pcb;
                                ui.close_menu();
                            }
                        }
                        _ => {
                            if ui.selectable_label(self.viewport_mode == ViewportMode::Scene, t("app.scene_view", _lang)).clicked() {
                                self.viewport_mode = ViewportMode::Scene;
                                ui.close_menu();
                            }
                        }
                    }
                });

                // -- Project --
                ui.menu_button(t("app.project_menu", _lang), |ui| {
                    if let Some(project) = &self.current_project {
                        ui.label(
                            egui::RichText::new(&project.name)
                                .color(app_theme::ACCENT)
                                .strong(),
                        );
                        ui.label(project.project_type.display_name());
                        ui.separator();
                        ui.label(
                            egui::RichText::new(project.path.display().to_string())
                                .small()
                                .color(palette.text_dim),
                        );
                        ui.separator();
                        if ui.button(t("app.open_folder", _lang)).clicked() {
                            #[cfg(target_os = "windows")]
                            { let _ = std::process::Command::new("explorer").arg(project.path.as_os_str()).spawn(); }
                            ui.close_menu();
                        }
                    }
                    if ui.button(t("app.close_project", _lang)).clicked() {
                        self.screen = AppScreen::ProjectHub;
                        ui.close_menu();
                    }
                });

                // -- Help --
                ui.menu_button(t("app.help_menu", _lang), |ui| {
                    ui.label(egui::RichText::new(t("app.keyboard_shortcuts", _lang)).strong());
                    ui.label(t("app.shortcut_undo_redo_context", _lang));
                    ui.label(t("app.save_menu", _lang));
                    ui.label(t("app.undo_menu", _lang));
                    ui.label(t("app.redo_menu", _lang));
                    ui.label(t("app.duplicate_menu", _lang));
                    ui.label(t("app.select_all_menu", _lang));
                    ui.label(t("app.delete_menu", _lang));
                    ui.label(t("app.shortcut_multi_select", _lang));
                    ui.label(t("app.shortcut_tools", _lang));
                    ui.label(t("app.shortcut_orbit_camera", _lang));
                    ui.label(t("app.shortcut_alt_orbit_camera", _lang));
                    ui.label(t("app.shortcut_pan_camera", _lang));
                    ui.label(t("app.shortcut_zoom_camera", _lang));
                    ui.label(t("app.shortcut_fly_camera", _lang));
                    ui.label(t("app.shortcut_focus_selected", _lang));
                    ui.label(t("app.shortcut_toggle_edit_mode", _lang));
                    ui.label(t("app.shortcut_reset_view", _lang));
                    ui.label(t("app.shortcut_customization_experimental", _lang));
                    ui.separator();
                    ui.label(format!("Proyecto Rafi v{}", env!("CARGO_PKG_VERSION")));
                    // No unconditional ui.close_menu() -- keeps menu open.
                });

                // Right side: mode indicator | FPS | Build/Run.
                ui.with_layout(
                    egui::Layout::right_to_left(egui::Align::Center),
                    |ui| {
                        // Build/Run button - integrated into toolbar, not a floating badge.
                        let build_text = if let Some(project) = &self.current_project {
                            match project.project_type {
                                ProjectType::Game => {
                                    if self.runtime.is_some() {
                                        t("app.stop_game", _lang)
                                    } else {
                                        t("app.run_game", _lang)
                                    }
                                }
                                ProjectType::Electronics => t("app.electrical_test_btn", _lang),
                            }
                        } else {
                            t("app.build_btn", _lang).to_string()
                        };

                        let build_btn = egui::Button::new(
                            egui::RichText::new(build_text)
                                .color(egui::Color32::WHITE)
                                .size(12.0),
                        )
                        .fill(app_theme::ACCENT)
                        .rounding(4.0);

                        if ui.add(build_btn).clicked() {
                            self.handle_build();
                        }

                        ui.separator();

                        // FPS counter.
                        let fps = ctx.input(|i| {
                            if i.predicted_dt > 0.0 {
                                (1.0 / i.predicted_dt) as u32
                            } else {
                                0
                            }
                        });
                        ui.label(
                            egui::RichText::new(format!("{} FPS", fps))
                                .size(11.0)
                                .color(palette.text_dim),
                        );

                        // Viewport mode indicator.
                        ui.separator();
                        let mode_text = match self.viewport_mode {
                            ViewportMode::Scene => t("app.scene_view", _lang),
                            ViewportMode::Schematic => t("app.schematic_view", _lang),
                            ViewportMode::Pcb => t("app.pcb_view", _lang),
                        };
                        ui.label(
                            egui::RichText::new(mode_text)
                                .size(11.0)
                                .color(app_theme::ACCENT),
                        );
                    },
                );
            });
        });

        // Status bar at bottom.
        egui::TopBottomPanel::bottom("status_bar")
            .max_height(22.0)
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    if let Some(project) = &self.current_project {
                        let modified = if self.scene_modified { " *" } else { "" };
                        ui.label(
                            egui::RichText::new(format!("{}{}", project.name, modified))
                                .size(11.0)
                                .color(app_theme::ACCENT),
                        );
                        ui.separator();
                        ui.label(
                            egui::RichText::new(project.project_type.display_name())
                                .size(11.0)
                                .color(palette.text_dim),
                        );
                    }
                    ui.separator();
                    let status_counts = match self.viewport_mode {
                        ViewportMode::Scene => {
                            format!("{} {}", t("app.entities_count", _lang), self.scene.all_valid_ids().len())
                        }
                        ViewportMode::Schematic => format!(
                            "{} {} | {} {}",
                            t("app.schematic_components", _lang),
                            self.schematic_view.schematic.components.len(),
                            t("app.schematic_wires", _lang),
                            self.schematic_view.schematic.wires.len()
                        ),
                        ViewportMode::Pcb => format!(
                            "{} {} | {} {} | {} {}",
                            t("app.pcb_components", _lang),
                            self.pcb_view.layout.components.len(),
                            t("app.pcb_traces", _lang),
                            self.pcb_view.layout.traces.len(),
                            t("app.pcb_airwires", _lang),
                            self.pcb_view.layout.airwires.len()
                        ),
                    };
                    ui.label(
                        egui::RichText::new(status_counts)
                            .size(11.0)
                            .color(palette.text_dim),
                    );
                    ui.separator();
                    ui.label(
                        egui::RichText::new(format!(
                            "U:{} R:{}",
                            self.undo_stack.len(),
                            self.redo_stack.len()
                        ))
                        .size(11.0)
                        .color(palette.text_dim),
                    );
                    if !self.last_action.is_empty() {
                        ui.separator();
                        ui.label(
                            egui::RichText::new(&self.last_action)
                                .size(11.0)
                                .color(palette.text_dim),
                        );
                    }
                    ui.with_layout(
                        egui::Layout::right_to_left(egui::Align::Center),
                        |ui| {
                            let theme_name = match self.settings.theme {
                                Theme::Dark => "Dark",
                                Theme::Light => "Light",
                                Theme::System => "System",
                            };
                            ui.label(
                                egui::RichText::new(format!(
                                    "{} | {}",
                                    self.settings.language.display_name(),
                                    theme_name
                                ))
                                .size(11.0)
                                .color(palette.text_dim),
                            );
                        },
                    );
                });
            });

        let (show_hierarchy_panel, show_properties_panel, complements_enabled) = self
            .current_project
            .as_ref()
            .map(|project| {
                (
                    project.settings.show_hierarchy_panel,
                    project.settings.show_properties_panel,
                    project.settings.enable_complements,
                )
            })
            .unwrap_or((true, true, true));

        if !complements_enabled && matches!(self.bottom_tab, BottomTab::Complement(_)) {
            self.bottom_tab = BottomTab::ProjectSettings;
        }

        let mut bottom_panel = egui::TopBottomPanel::bottom("bottom_panel");
        bottom_panel = if let Some(height) = self.bottom_panel_snap_height {
            bottom_panel
                .resizable(false)
                .min_height(height)
                .max_height(height)
        } else {
            bottom_panel
                .resizable(true)
                .min_height(90.0)
                .default_height(200.0)
        };

        bottom_panel.show(ctx, |ui| {
                ui.horizontal(|ui| {
                    let mut tabs = vec![
                        (BottomTab::Console, "Console".to_string()),
                        (BottomTab::Assets, "Assets".to_string()),
                        (BottomTab::ProjectSettings, t("app.project_settings_tab", _lang)),
                        (BottomTab::NodeEditor, "Node Editor".to_string()),
                        (BottomTab::AiChat, "AI Chat".to_string()),
                    ];
                    if complements_enabled {
                        for comp in &self.complement_registry.complements {
                        tabs.push((BottomTab::Complement(comp.id().to_string()), comp.name().to_string()));
                        }
                    }
                    
                    let mut tab_changed = None;
                    for (tab, label) in tabs {
                        let is_active = self.bottom_tab == tab;
                        let response = draw_bottom_tab_button(
                            ui,
                            &self.ui_icons,
                            bottom_tab_icon(&tab),
                            &label,
                            is_active,
                        );

                        if response.clicked() {
                            tab_changed = Some(tab);
                        }
                        
                        ui.add_space(16.0);
                    }

                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        for (height, label) in [
                            (None, "F"),
                            (Some(340.0), "L"),
                            (Some(220.0), "M"),
                            (Some(110.0), "S"),
                        ] {
                            let is_active = self.bottom_panel_snap_height == height;
                            let button = egui::Button::new(
                                egui::RichText::new(label)
                                    .size(10.0)
                                    .color(if is_active {
                                        egui::Color32::WHITE
                                    } else {
                                        egui::Color32::from_rgb(150, 150, 160)
                                    }),
                            )
                            .fill(if is_active {
                                egui::Color32::from_rgb(70, 70, 76)
                            } else {
                                egui::Color32::TRANSPARENT
                            })
                            .rounding(4.0)
                            .min_size(egui::Vec2::new(22.0, 18.0));

                            if ui.add(button).clicked() {
                                self.bottom_panel_snap_height = height;
                            }
                        }
                    });

                    if let Some(t) = tab_changed {
                        self.bottom_tab = t;
                    }
                });
                ui.separator();

                match &self.bottom_tab {
                    BottomTab::Assets => {
                        self.asset_browser.process_scan_budget(96);
                        self.asset_browser.show(ui, self.settings.language, &self.ui_icons);
                        // Handle "Add Entity" from asset browser.
                        if self.asset_browser.add_entity_clicked {
                            self.asset_browser.add_entity_clicked = false;
                            ui.memory_mut(|m| m.toggle_popup(egui::Id::new("add_entity_popup")));
                        }
                        let add_popup_id = egui::Id::new("add_entity_popup");
                        let fake_resp = ui.make_persistent_id("add_entity_anchor");
                        let anchor = ui.interact(ui.max_rect(), fake_resp, egui::Sense::hover());
                        egui::popup_above_or_below_widget(
                            ui,
                            add_popup_id,
                            &anchor,
                            egui::AboveOrBelow::Above,
                            egui::PopupCloseBehavior::CloseOnClickOutside,
                            |ui| {
                                ui.label(
                                    egui::RichText::new(t("app.add_entity", self.settings.language))
                                        .size(11.0).strong()
                                );
                                ui.separator();
                                let primitives = [
                                    Primitive::Cube,
                                    Primitive::Sphere,
                                    Primitive::Plane,
                                    Primitive::Cylinder,
                                ];
                                for prim in primitives {
                                    if ui.button(prim.label()).clicked() {
                                        self.push_undo_snapshot();
                                        let name = format!("{} {}", prim.label(), self.scene.len() + 1);
                                        let id = self.scene.add_root_with_primitive(&name, prim);
                                        self.hierarchy.selected_node = Some(id);
                                        self.hierarchy.selected_nodes = vec![id];
                                        self.viewport.selected = vec![id];
                                        let add_msg = format!("Added: {}", name);
                                        self.last_action = add_msg.clone();
                                        self.console.log(LogLevel::Info, &add_msg);
                                    }
                                }
                            },
                        );
                    }
                    BottomTab::Console => self.console.show(ui, self.settings.language),
                    BottomTab::ProjectSettings => {
                        let changed = if let Some(project) = self.current_project.as_mut() {
                            project_settings::show_project_settings(ui, project, self.settings.language)
                        } else {
                            ui.label(
                                egui::RichText::new(t("app.no_entity_selected", self.settings.language))
                                    .size(11.0)
                                    .color(palette.text_dim),
                            );
                            false
                        };

                        if changed {
                            if let Some(project) = &self.current_project {
                                let _ = project.save();
                            }
                            let msg = t("app.project_settings_saved", self.settings.language);
                            self.last_action = msg.clone();
                            self.console.log(LogLevel::Info, &msg);
                        }
                    }
                    BottomTab::AiChat => {
                        self.ai_chat.lang = self.settings.language;
                        self.ai_chat.show(ui);
                    }
                    BottomTab::NodeEditor => self.node_editor.show(ui, self.settings.language),
                    BottomTab::Complement(id) => {
                        let mut context = raf_core::complement::ComplementContext {
                            lang: self.settings.language,
                            command_bus: &mut self.command_bus,
                        };
                        if let Some(comp) = self.complement_registry.complements.iter_mut().find(|c| c.id() == id) {
                            comp.draw_ui(&mut context); // No egui UI context in EngineComplement trait for now.
                        }
                    }
                }
            });

        // Left panel: Hierarchy.
        if show_hierarchy_panel {
            egui::SidePanel::left("hierarchy_panel")
                .resizable(true)
                .default_width(200.0)
                .min_width(150.0)
                .show(ctx, |ui| {
                    match self.viewport_mode {
                        ViewportMode::Scene => {
                            if self.runtime.is_some() {
                                ui.label(
                                    egui::RichText::new(t("app.runtime_scene_locked", self.settings.language))
                                        .size(11.0)
                                        .color(palette.text_dim),
                                );
                                return;
                            }

                            let prev_hier_vec = self.hierarchy.selected_nodes.clone();
                            self.hierarchy.show(ui, &mut self.scene, self.settings.language, &self.ui_icons);

                            let actions = self.hierarchy.take_actions();
                            if let Some(del_id) = actions.delete {
                                self.push_undo_snapshot();
                                let name = self.scene.get(del_id).map(|n| n.name.clone()).unwrap_or_default();
                                if self.scene.remove_node(del_id) {
                                    self.hierarchy.selected_node = None;
                                    self.hierarchy.selected_nodes.clear();
                                    self.viewport.selected.clear();
                                    let msg = format!("{} {}", t("app.deleted_msg", self.settings.language), name);
                                    self.last_action = msg.clone();
                                    self.console.log(LogLevel::Info, &msg);
                                }
                            }
                            if let Some(dup_id) = actions.duplicate {
                                self.push_undo_snapshot();
                                if let Some(new_id) = self.scene.duplicate_node(dup_id) {
                                    self.hierarchy.selected_node = Some(new_id);
                                    self.hierarchy.selected_nodes = vec![new_id];
                                    self.viewport.selected = vec![new_id];
                                    let name = self.scene.get(new_id).map(|n| n.name.clone()).unwrap_or_default();
                                    let msg = format!("{} {}", t("app.duplicated_msg", self.settings.language), name);
                                    self.last_action = msg.clone();
                                    self.console.log(LogLevel::Info, &msg);
                                }
                            }
                            if let Some(folder_id) = actions.ungroup {
                                self.push_undo_snapshot();
                                let name = self.scene.get(folder_id).map(|n| n.name.clone()).unwrap_or_default();
                                if self.scene.ungroup_node(folder_id) {
                                    self.hierarchy.selected_node = None;
                                    self.hierarchy.selected_nodes.clear();
                                    self.viewport.selected.clear();
                                    let msg = format!("{} {}", t("app.ungrouped_msg", self.settings.language), name);
                                    self.last_action = msg.clone();
                                    self.console.log(LogLevel::Info, &msg);
                                }
                            }
                            if let Some(toggle_id) = actions.toggle_visibility {
                                self.push_undo_snapshot();
                                if let Some(node) = self.scene.get_mut(toggle_id) {
                                    node.visible = !node.visible;
                                    let msg = format!("{} {}", t("app.visibility_toggled", self.settings.language), node.name);
                                    self.last_action = msg.clone();
                                    self.console.log(LogLevel::Info, &msg);
                                }
                            }
                            if let Some(parent) = actions.create_folder_parent {
                                self.push_undo_snapshot();
                                let new_id = if let Some(parent_id) = parent {
                                    self.scene.add_child_folder(parent_id, "Folder")
                                } else {
                                    self.scene.add_root_folder("Folder")
                                };
                                self.hierarchy.selected_node = Some(new_id);
                                self.hierarchy.selected_nodes = vec![new_id];
                                self.viewport.selected = vec![new_id];
                                let msg = t("app.folder_created", self.settings.language);
                                self.last_action = msg.clone();
                                self.console.log(LogLevel::Info, &msg);
                            }
                            if let Some((dragged, new_parent)) = actions.reparent {
                                self.push_undo_snapshot();
                                if self.scene.reparent_node(dragged, new_parent) {
                                    self.hierarchy.selected_node = Some(dragged);
                                    self.hierarchy.selected_nodes = vec![dragged];
                                    self.viewport.selected = vec![dragged];
                                    let msg = t("app.node_reparented", self.settings.language);
                                    self.last_action = msg.clone();
                                    self.console.log(LogLevel::Info, &msg);
                                }
                            }
                            if actions.edited {
                                self.mark_scene_modified();
                            }

                            if self.hierarchy.selected_nodes != prev_hier_vec {
                                self.viewport.selected = self.hierarchy.selected_nodes.clone();
                            }

                            if self.viewport.selected != self.hierarchy.selected_nodes {
                                self.hierarchy.selected_nodes = self.viewport.selected.clone();
                                self.hierarchy.selected_node = self.viewport.selected.first().copied();
                            }
                        }
                        ViewportMode::Schematic => {
                            let _ = schematic_panels::show_schematic_hierarchy(
                                ui,
                                &mut self.schematic_view,
                                self.settings.language,
                            );
                        }
                        ViewportMode::Pcb => {
                            let _ = pcb_panels::show_pcb_hierarchy(
                                ui,
                                &mut self.pcb_view,
                                self.settings.language,
                            );
                        }
                    }
            });
        }

        // Right panel: Properties.
        if show_properties_panel {
            egui::SidePanel::right("properties_panel")
                .resizable(true)
                .default_width(300.0)
                .min_width(220.0)
                .show(ctx, |ui| {
                    match self.viewport_mode {
                        ViewportMode::Scene => {
                            if self.runtime.is_some() {
                                ui.label(
                                    egui::RichText::new(t("app.runtime_scene_locked", self.settings.language))
                                        .size(11.0)
                                        .color(palette.text_dim),
                                );
                                return;
                            }

                            let before_snapshot = self.current_history_snapshot();
                            let properties_changed = self.properties.show(
                                ui,
                                &mut self.scene,
                                self.hierarchy.selected_node,
                                self.settings.language,
                                &self.ui_icons,
                                self.asset_browser.project_assets_path.as_deref(),
                            );
                            if properties_changed {
                                self.record_document_change(before_snapshot);
                            }
                        }
                        ViewportMode::Schematic => {
                            let before_snapshot = self.current_history_snapshot();
                            if schematic_panels::show_schematic_properties(
                                ui,
                                &mut self.schematic_view,
                                self.settings.language,
                            ) {
                                self.record_document_change(before_snapshot);
                            }
                        }
                        ViewportMode::Pcb => {
                            let before_snapshot = self.current_history_snapshot();
                            if pcb_panels::show_pcb_properties(
                                ui,
                                &mut self.pcb_view,
                                self.settings.language,
                            ) {
                                self.record_document_change(before_snapshot);
                            }
                        }
                    }
            });
        }

        // Central panel: Viewport or Schematic.
        egui::CentralPanel::default().show(ctx, |ui| {
            if self
                .current_project
                .as_ref()
                .map(|project| project.project_type == ProjectType::Electronics)
                .unwrap_or(false)
            {
                ui.horizontal(|ui| {
                    if ui
                        .selectable_label(self.viewport_mode == ViewportMode::Schematic, t("app.schematic_view", self.settings.language))
                        .clicked()
                    {
                        self.viewport_mode = ViewportMode::Schematic;
                    }
                    if ui
                        .selectable_label(self.viewport_mode == ViewportMode::Pcb, t("app.pcb_view", self.settings.language))
                        .clicked()
                    {
                        self.sync_pcb_from_schematic();
                        self.viewport_mode = ViewportMode::Pcb;
                    }
                });
                ui.add_space(8.0);
            }

            match self.viewport_mode {
                ViewportMode::Scene => {
                    self.viewport.frame_time_hint = ctx.input(|i| i.predicted_dt);
                    self.viewport.render_cfg = self
                        .current_project
                        .as_ref()
                        .map(|project| {
                            let mut config = match project.settings.runtime_render_preset {
                                RenderPreset::Potato => RenderConfig::potato(),
                                RenderPreset::Low => RenderConfig::low(),
                                RenderPreset::Medium => RenderConfig::medium(),
                                RenderPreset::High => RenderConfig::high(),
                            };
                            config.depth_accurate = project.settings.depth_accurate;
                            config.depth_resolution_scale = project
                                .settings
                                .depth_resolution_scale
                                .clamp(0.35, 1.0);
                            config
                        })
                        .unwrap_or_else(RenderConfig::potato);
                    self.viewport.grid_visible = self.settings.grid_visible;
                    self.viewport.grid_spacing = self.settings.grid_size.max(0.1);
                    self.viewport.invert_mouse_x = self.settings.invert_mouse_x;
                    self.viewport.invert_mouse_y = self.settings.invert_mouse_y;
                    self.viewport.move_sensitivity = self.settings.move_gizmo_sensitivity.max(0.1);
                    self.viewport.rotate_sensitivity = self.settings.rotate_gizmo_sensitivity.max(0.1);
                    self.viewport.scale_sensitivity = self.settings.scale_gizmo_sensitivity.max(0.1);
                    self.viewport.uniform_scale_by_default = self.settings.uniform_scale_by_default;
                    self.viewport.solid_show_surface_edges = self.settings.solid_show_surface_edges;
                    self.viewport.solid_xray_mode = self.settings.solid_xray_mode;
                    self.viewport.solid_face_tonality = self.settings.solid_face_tonality;
                    self.viewport.render_style = match self.settings.viewport_render_mode {
                        raf_core::config::ViewportRenderMode::Solid => crate::panels::viewport::RenderStyle::Solid,
                        raf_core::config::ViewportRenderMode::Wireframe => crate::panels::viewport::RenderStyle::Wireframe,
                        raf_core::config::ViewportRenderMode::Preview => crate::panels::viewport::RenderStyle::Preview,
                    };
                    self.viewport.show_labels = self.settings.show_viewport_labels;
                    if let Some(runtime) = self.runtime.as_mut() {
                        let _ = self.viewport.show(
                            ctx,
                            ui,
                            &mut runtime.scene,
                            self.settings.theme != Theme::Light,
                            self.settings.language,
                            &self.ui_icons,
                        );
                    } else {
                        let before_snapshot = self.current_history_snapshot();
                        let viewport_changed = self.viewport.show(
                            ctx,
                            ui,
                            &mut self.scene,
                            self.settings.theme != Theme::Light,
                            self.settings.language,
                            &self.ui_icons,
                        );
                        if viewport_changed {
                            self.record_document_change(before_snapshot);
                        }
                    }
                }
                ViewportMode::Schematic => {
                    let before_snapshot = self.current_history_snapshot();
                    self.schematic_view.lang = self.settings.language;
                    if self.schematic_view.show(ui) {
                        self.record_document_change(before_snapshot);
                    }
                }
                ViewportMode::Pcb => {
                    let before_snapshot = self.current_history_snapshot();
                    self.pcb_view.lang = self.settings.language;
                    if self.pcb_view.show(ui) {
                        self.record_document_change(before_snapshot);
                    }
                }
            }
        });

        if self.viewport_mode == ViewportMode::Scene && self.viewport.selected != self.hierarchy.selected_nodes {
            self.hierarchy.selected_nodes = self.viewport.selected.clone();
            self.hierarchy.selected_node = self.viewport.selected.first().copied();
        }
    }

    // -----------------------------------------------------------------------
    // Settings Screen
    // -----------------------------------------------------------------------

    fn show_settings_screen(&mut self, ctx: &egui::Context) {
        let _lang = self.settings.language;
        let mut close = false;
        let mut save = false;
        let palette = app_theme::palette_for_visuals(ctx.style().visuals.dark_mode, self.settings.theme_experimental);

        egui::CentralPanel::default().show(ctx, |ui| {
            ui.vertical_centered(|ui| {
                ui.set_max_width(540.0); // Clean, constrained center column
                
                ui.add_space(32.0);
                
                // Professional Title (Extralight-like, subtle)
                ui.label(
                    egui::RichText::new(t("app.engine_settings_title", _lang))
                        .size(16.0)
                        .color(palette.text_dim),
                );
                
                ui.add_space(20.0);
                ui.separator();
                ui.add_space(16.0);

                // Need a frame for the settings panel content
                let frame = egui::Frame::none()
                    .fill(palette.panel)
                    .rounding(8.0)
                    .inner_margin(24.0)
                    .stroke(egui::Stroke::new(1.0, palette.border));
                
                frame.show(ui, |ui| {
                    settings_panel::show_settings(ui, &mut self.settings);
                });

                ui.add_space(24.0);

                // Action buttons below
                ui.horizontal(|ui| {
                    // Right aligned buttons
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        // Styled save button using accent but thinner or less aggressive
                        let save_btn = egui::Button::new(
                            egui::RichText::new(t("app.save_and_close", _lang))
                                .size(13.0)
                                .color(egui::Color32::WHITE),
                        )
                        .fill(app_theme::ACCENT)
                        .rounding(4.0);

                        if ui.add_sized([120.0, 32.0], save_btn).clicked() {
                            save = true;
                            close = true;
                        }

                        ui.add_space(12.0);

                        let cancel_btn = egui::Button::new(
                            egui::RichText::new(t("app.cancel", _lang))
                                .size(13.0),
                        )
                        .rounding(4.0);

                        if ui.add_sized([90.0, 32.0], cancel_btn).clicked() {
                            close = true;
                        }
                    });
                });
            });
        });

        if close {
            if save {
                let config_dir = dirs_config_dir();
                let _ = self.settings.save(&config_dir);
                self.console.log(LogLevel::Info, "Settings saved.");
            }
            self.screen = self
                .previous_screen
                .take()
                .unwrap_or(AppScreen::ProjectHub);
        }
    }

    // -----------------------------------------------------------------------
    // v0.3.0: Undo/Redo (scene snapshot based, max 50)
    // -----------------------------------------------------------------------

    /// Push current document state to undo stack.
    fn push_undo_snapshot(&mut self) {
        if let Some(snapshot) = self.current_history_snapshot() {
            self.push_history_snapshot(snapshot);
        }
    }

    fn current_history_snapshot(&self) -> Option<EditorHistorySnapshot> {
        match self.viewport_mode {
            ViewportMode::Scene => ron::ser::to_string(&self.scene)
                .ok()
                .map(EditorHistorySnapshot::Scene),
            ViewportMode::Schematic => ron::ser::to_string(&self.schematic_view.schematic)
                .ok()
                .map(EditorHistorySnapshot::Schematic),
            ViewportMode::Pcb => ron::ser::to_string(&self.pcb_view.layout)
                .ok()
                .map(EditorHistorySnapshot::Pcb),
        }
    }

    fn push_history_snapshot(&mut self, snapshot: EditorHistorySnapshot) {
        self.undo_stack.push(snapshot);
        if self.undo_stack.len() > 50 {
            self.undo_stack.remove(0);
        }
        self.redo_stack.clear();
        self.mark_scene_modified();
    }

    fn record_document_change(&mut self, before_snapshot: Option<EditorHistorySnapshot>) {
        if let Some(snapshot) = before_snapshot {
            self.push_history_snapshot(snapshot);
        } else {
            self.mark_scene_modified();
        }
    }

    fn apply_history_snapshot(&mut self, snapshot: EditorHistorySnapshot) -> bool {
        match snapshot {
            EditorHistorySnapshot::Scene(data) => {
                if let Ok(restored) = ron::from_str::<SceneGraph>(&data) {
                    self.scene = restored;
                    self.hierarchy.selected_node = None;
                    self.hierarchy.selected_nodes.clear();
                    self.viewport.selected.clear();
                    return true;
                }
            }
            EditorHistorySnapshot::Schematic(data) => {
                if let Ok(restored) = ron::from_str::<raf_electronics::schematic::Schematic>(&data) {
                    self.schematic_view.schematic = restored;
                    self.schematic_view.clear_selection();
                    self.hierarchy.selected_node = None;
                    self.hierarchy.selected_nodes.clear();
                    self.viewport.selected.clear();
                    return true;
                }
            }
            EditorHistorySnapshot::Pcb(data) => {
                if let Ok(restored) = ron::from_str::<raf_electronics::PcbLayout>(&data) {
                    self.pcb_view.layout = restored;
                    self.pcb_view.clear_selection();
                    self.hierarchy.selected_node = None;
                    self.hierarchy.selected_nodes.clear();
                    self.viewport.selected.clear();
                    return true;
                }
            }
        }

        false
    }

    fn do_undo(&mut self) {
        if let Some(snapshot) = self.undo_stack.pop() {
            if let Some(current) = self.current_history_snapshot() {
                self.redo_stack.push(current);
            }
            if self.apply_history_snapshot(snapshot) {
                let msg = t("app.undo", self.settings.language);
                self.last_action = msg.to_string();
                self.console.log(LogLevel::Info, &msg);
                self.mark_scene_modified();
            }
        }
    }

    fn do_redo(&mut self) {
        if let Some(snapshot) = self.redo_stack.pop() {
            if let Some(current) = self.current_history_snapshot() {
                self.undo_stack.push(current);
            }
            if self.apply_history_snapshot(snapshot) {
                let msg = t("app.redo", self.settings.language);
                self.last_action = msg.to_string();
                self.console.log(LogLevel::Info, &msg);
                self.mark_scene_modified();
            }
        }
    }

    // -----------------------------------------------------------------------
    // v0.3.0: Scene actions
    // -----------------------------------------------------------------------

    fn do_delete(&mut self) {
        if self.runtime.is_some() && self.viewport_mode == ViewportMode::Scene {
            let msg = t("app.runtime_scene_locked", self.settings.language);
            self.last_action = msg.clone();
            self.console.log(LogLevel::Info, &msg);
            return;
        }

        let _lang = self.settings.language;
        if self.viewport_mode == ViewportMode::Schematic {
            let undo_len = self.undo_stack.len();
            self.push_undo_snapshot();
            if self.schematic_view.delete_selection() {
                self.mark_scene_modified();
                let msg = t("app.delete_del", _lang);
                self.last_action = msg.to_string();
                self.console.log(LogLevel::Info, &msg);
            } else if self.undo_stack.len() > undo_len {
                self.undo_stack.pop();
            }
            return;
        }
        if self.viewport_mode == ViewportMode::Pcb {
            let undo_len = self.undo_stack.len();
            self.push_undo_snapshot();
            if self.pcb_view.delete_selection() {
                self.mark_scene_modified();
                let msg = t("app.delete_del", _lang);
                self.last_action = msg.to_string();
                self.console.log(LogLevel::Info, &msg);
            } else if self.undo_stack.len() > undo_len {
                self.undo_stack.pop();
            }
            return;
        }
        if let Some(id) = self.hierarchy.selected_node {
            self.push_undo_snapshot();
            let name = self.scene.get(id).map(|n| n.name.clone()).unwrap_or_default();
            if self.scene.remove_node(id) {
                self.hierarchy.selected_node = None;
                self.hierarchy.selected_nodes.clear();
                self.viewport.selected.clear();
                let msg = format!("{} {}", t("app.deleted_msg", _lang), name);
                self.last_action = msg.clone();
                self.console.log(LogLevel::Info, &msg);
            }
        }
    }

    fn do_duplicate(&mut self) {
        if self.runtime.is_some() && self.viewport_mode == ViewportMode::Scene {
            let msg = t("app.runtime_scene_locked", self.settings.language);
            self.last_action = msg.clone();
            self.console.log(LogLevel::Info, &msg);
            return;
        }

        let _lang = self.settings.language;
        if self.viewport_mode == ViewportMode::Schematic {
            let undo_len = self.undo_stack.len();
            self.push_undo_snapshot();
            if self.schematic_view.duplicate_selection() {
                self.mark_scene_modified();
                let msg = t("app.duplicate_menu", _lang);
                self.last_action = msg.to_string();
                self.console.log(LogLevel::Info, &msg);
            } else if self.undo_stack.len() > undo_len {
                self.undo_stack.pop();
            }
            return;
        }
        if self.viewport_mode == ViewportMode::Pcb {
            let msg = t("app.pcb_duplicate_disabled", _lang);
            self.last_action = msg.to_string();
            self.console.log(LogLevel::Info, &msg);
            return;
        }
        if let Some(id) = self.hierarchy.selected_node {
            self.push_undo_snapshot();
            if let Some(new_id) = self.scene.duplicate_node(id) {
                self.hierarchy.selected_node = Some(new_id);
                self.hierarchy.selected_nodes = vec![new_id];
                self.viewport.selected = vec![new_id];
                let name = self.scene.get(new_id).map(|n| n.name.clone()).unwrap_or_default();
                let msg = format!("{} {}", t("app.duplicated_msg", _lang), name);
                self.last_action = msg.clone();
                self.console.log(LogLevel::Info, &msg);
            }
        }
    }

    fn do_select_all(&mut self) {
        if self.runtime.is_some() && self.viewport_mode == ViewportMode::Scene {
            let msg = t("app.runtime_scene_locked", self.settings.language);
            self.last_action = msg.clone();
            self.console.log(LogLevel::Info, &msg);
            return;
        }

        if self.viewport_mode == ViewportMode::Schematic {
            self.schematic_view.clear_selection();
            let _lang = self.settings.language;
            let msg = format!(
                "{} {} | {} {}",
                t("app.schematic_components", _lang),
                self.schematic_view.schematic.components.len(),
                t("app.schematic_wires", _lang),
                self.schematic_view.schematic.wires.len()
            );
            self.last_action = msg.clone();
            self.console.log(LogLevel::Info, &msg);
            return;
        }
        if self.viewport_mode == ViewportMode::Pcb {
            self.pcb_view.clear_selection();
            let _lang = self.settings.language;
            let msg = format!(
                "{} {} | {} {} | {} {}",
                t("app.pcb_components", _lang),
                self.pcb_view.layout.components.len(),
                t("app.pcb_traces", _lang),
                self.pcb_view.layout.traces.len(),
                t("app.pcb_airwires", _lang),
                self.pcb_view.layout.airwires.len()
            );
            self.last_action = msg.clone();
            self.console.log(LogLevel::Info, &msg);
            return;
        }
        let ids = self.scene.all_valid_ids();
        self.hierarchy.selected_nodes = ids.clone();
        self.hierarchy.selected_node = ids.first().copied();
        self.viewport.selected = ids.clone();
        let _lang = self.settings.language;
        let msg = format!("{} {}", ids.len(), t("app.entities_found_msg", _lang));
        self.last_action = msg.clone();
        self.console.log(LogLevel::Info, &msg);
    }

    fn do_save(&mut self) {
        let _lang = self.settings.language;
        if let Some(project) = self.current_project.clone() {
            let _ = project.save();
            match project.project_type {
                ProjectType::Game => {
                    let scene_path = project.path.join("scene.ron");
                    let nodes_path = project.path.join("nodes.ron");
                    let _ = self.scene.save_ron(&scene_path);
                    if let Ok(data) = ron::ser::to_string_pretty(
                        &self.node_editor.document(),
                        ron::ser::PrettyConfig::default(),
                    ) {
                        let _ = std::fs::write(nodes_path, data);
                    }
                }
                ProjectType::Electronics => {
                    self.sync_pcb_from_schematic();
                    let schematic_path = project.path.join("schematic.ron");
                    let pcb_path = project.path.join("pcb_layout.ron");
                    let _ = save_schematic_document(&schematic_path, &self.schematic_view.schematic);
                    let _ = save_pcb_document(&pcb_path, &self.pcb_view.layout);
                }
            }
            self.scene_modified = false;
            self.auto_save_elapsed = 0.0;
            let msg = t("app.project_saved", self.settings.language);
            self.last_action = msg.to_string();
            self.console.log(LogLevel::Info, &msg);
        }
    }

    // -----------------------------------------------------------------------
    // v0.3.0: Global shortcuts
    // -----------------------------------------------------------------------

    fn handle_global_shortcuts(&mut self, ctx: &egui::Context) {
        let text_input_active = ctx.wants_keyboard_input();
        let node_editor_owns_history = self.bottom_tab == BottomTab::NodeEditor;
        let action: Option<u8> = ctx.input(|i| {
            let ctrl = i.modifiers.ctrl || i.modifiers.mac_cmd;
            if ctrl && i.modifiers.shift && i.key_pressed(egui::Key::Z) && !node_editor_owns_history {
                return Some(2);
            }
            if ctrl && i.key_pressed(egui::Key::S) { return Some(4); }
            if text_input_active {
                return None;
            }
            if ctrl && i.key_pressed(egui::Key::Z) && !node_editor_owns_history { return Some(1); }
            if ctrl && i.key_pressed(egui::Key::Y) && !node_editor_owns_history { return Some(2); }
            if ctrl && i.key_pressed(egui::Key::D) { return Some(3); }
            if ctrl && i.key_pressed(egui::Key::A) { return Some(5); }
            if i.key_pressed(egui::Key::Delete) { return Some(6); }
            None
        });
        match action {
            Some(1) => self.do_undo(),
            Some(2) => self.do_redo(),
            Some(3) => self.do_duplicate(),
            Some(4) => self.do_save(),
            Some(5) => self.do_select_all(),
            Some(6) => self.do_delete(),
            _ => {}
        }
    }

    // -----------------------------------------------------------------------
    // v0.3.0: Auto-save
    // -----------------------------------------------------------------------

    fn handle_auto_save(&mut self, ctx: &egui::Context) {
        if !self.scene_modified {
            return;
        }
        let dt = ctx.input(|i| i.predicted_dt);
        self.auto_save_elapsed += dt;
        let interval = self.settings.auto_save_interval_seconds as f32;
        if interval > 0.0 && self.auto_save_elapsed >= interval {
            self.do_save();
            let _lang = self.settings.language;
            let msg = t("app.auto_saved", self.settings.language);
            self.last_action = msg.to_string();
            self.console.log(LogLevel::Info, &msg);
        }
    }

    fn mark_scene_modified(&mut self) {
        self.scene_modified = true;
        self.auto_save_elapsed = 0.0;
    }

    // -----------------------------------------------------------------------
    // Build / Run
    // -----------------------------------------------------------------------

    fn handle_build(&mut self) {
        if let Some(project) = self.current_project.clone() {
            match project.project_type {
                ProjectType::Game => {
                    if self.runtime.is_some() {
                        self.runtime = None;
                        let msg = t("app.runtime_stopped", self.settings.language);
                        self.last_action = msg.clone();
                        self.console.log(LogLevel::Info, &msg);
                        return;
                    }

                    let (runtime, report) = GameRuntimeState::start(
                        &self.scene,
                        &self.node_editor.document(),
                        Some(project.path.join("assets")),
                        &project.settings,
                    );
                    for log in report.logs {
                        self.console.log(LogLevel::Info, &log);
                    }
                    for error in report.errors {
                        self.console.log(LogLevel::Error, &error);
                    }

                    self.runtime = Some(runtime);
                    let msg = t("app.runtime_started", self.settings.language);
                    self.last_action = msg.clone();
                    self.console.log(LogLevel::Info, &msg);
                }
                ProjectType::Electronics => {
                    self.console.log(
                        LogLevel::Info,
                        "Running DC Simulation...",
                    );
                    
                    // 1. Run design checks first
                    let results = self.schematic_view.schematic.electrical_test();
                    for result in &results {
                        if result.contains("passed") {
                            self.console.log(LogLevel::Info, &result);
                        } else {
                            self.console.log(LogLevel::Warning, &result);
                        }
                    }

                    // 2. Run actual math simulation
                    let sim_results = raf_electronics::simulation::simulate_dc(&self.schematic_view.schematic);
                    if sim_results.converged {
                        self.console.log(LogLevel::Info, "Simulation converged successfully.");
                        for (ci, current) in sim_results.component_currents {
                            let comp = &self.schematic_view.schematic.components[ci];
                            let msg = format!("Component [{}]: Current = {:.5} A", comp.id, current);
                            self.console.log(LogLevel::Info, &msg);
                        }
                        for (net_id, voltage) in sim_results.node_voltages {
                            let msg = format!("Net [N{:03}]: Voltage = {:.2} V", net_id, voltage);
                            self.console.log(LogLevel::Info, &msg);
                        }
                    } else {
                        self.console.log(LogLevel::Error, "Simulation failed to converge.");
                        for err in sim_results.messages {
                            self.console.log(LogLevel::Error, &err);
                        }
                    }
                }
            }
        }
    }

    fn sync_pcb_from_schematic(&mut self) {
        let summary = self.pcb_view.sync_from_schematic(&self.schematic_view.schematic);
        let msg = format!(
            "PCB sync: +{} / ~{} / -{} / {} nets",
            summary.added_components,
            summary.updated_components,
            summary.removed_components,
            summary.nets,
        );
        self.last_action = msg.clone();
        self.console.log(LogLevel::Info, &msg);
    }

    // -----------------------------------------------------------------------
    // Helpers
    // -----------------------------------------------------------------------

    fn open_project(&mut self, path: &std::path::Path) {
        match Project::load(path) {
            Ok(project) => {
                let config_dir = dirs_config_dir();
                self.recent_projects.add(&project);
                let _ = self.recent_projects.save(&config_dir);
                let ptype = project.project_type;

                self.init_scene_for_type(ptype);
                self.runtime = None;

                match ptype {
                    ProjectType::Game => {
                        let scene_path = project.path.join("scene.ron");
                        let nodes_path = project.path.join("nodes.ron");
                        if scene_path.exists() {
                            self.scene = SceneGraph::load_ron(&scene_path);
                            let msg = t("app.scene_loaded_from_file", self.settings.language);
                            self.console.log(LogLevel::Info, &msg);
                        }
                        if nodes_path.exists() {
                            if let Ok(data) = std::fs::read_to_string(&nodes_path) {
                                if let Ok(document) = ron::from_str::<NodeEditorDocument>(&data) {
                                    self.node_editor.load_document(document);
                                }
                            }
                        } else {
                            self.node_editor.load_document(NodeEditorDocument::default());
                        }
                    }
                    ProjectType::Electronics => {
                        self.node_editor.load_document(NodeEditorDocument::default());
                        let schematic_path = project.path.join("schematic.ron");
                        let pcb_path = project.path.join("pcb_layout.ron");
                        self.schematic_view.schematic = if schematic_path.exists() {
                            if let Some(schematic) = load_schematic_document(&schematic_path) {
                                let msg = t("app.schematic_loaded_from_file", self.settings.language);
                                self.console.log(LogLevel::Info, &msg);
                                schematic
                            } else {
                                raf_electronics::schematic::Schematic::new(&project.name)
                            }
                        } else {
                            raf_electronics::schematic::Schematic::new(&project.name)
                        };
                        self.pcb_view.layout = if pcb_path.exists() {
                            if let Some(layout) = load_pcb_document(&pcb_path) {
                                let msg = t("app.pcb_loaded_from_file", self.settings.language);
                                self.console.log(LogLevel::Info, &msg);
                                layout
                            } else {
                                raf_electronics::PcbLayout::new(&project.name)
                            }
                        } else {
                            raf_electronics::PcbLayout::new(&project.name)
                        };
                        self.sync_pcb_from_schematic();
                        self.schematic_view.clear_selection();
                        self.pcb_view.clear_selection();
                    }
                }

                // Switch viewport mode based on project type.
                self.viewport_mode = match ptype {
                    ProjectType::Game => ViewportMode::Scene,
                    ProjectType::Electronics => ViewportMode::Schematic,
                };

                // Clear undo/redo for new session.
                self.undo_stack.clear();
                self.redo_stack.clear();
                self.scene_modified = false;
                self.last_action.clear();

                self.current_project = Some(project.clone());
                // Wire assets path to browser.
                let assets_dir = std::path::PathBuf::from(&project.path).join("assets");
                self.asset_browser.project_assets_path = Some(assets_dir);
                self.asset_browser.scan_project_folder();
                self.screen = AppScreen::Editor;
                let _lang = self.settings.language;
                let msg = t("app.project_loaded", self.settings.language);
                self.console.log(LogLevel::Info, &msg);
            }
            Err(e) => {
                self.console.log(
                    LogLevel::Error,
                    &format!("Failed to open project: {}", e),
                );
            }
        }
    }

    fn init_scene_for_type(&mut self, project_type: ProjectType) {
        self.scene = SceneGraph::new();
        match project_type {
            ProjectType::Game => {
                let root = self.scene.add_root("Scene Root");
                let camera = self.scene.add_child(root, "Main Camera");
                if let Some(cam_node) = self.scene.get_mut(camera) {
                    cam_node.position = glam::Vec3::new(0.0, 5.0, 10.0);
                }
                self.scene.add_child(root, "Directional Light");
            }
            ProjectType::Electronics => {
                self.scene.add_root("Schematic Root");
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Utility functions
// ---------------------------------------------------------------------------

/// Get the config directory for storing settings. Creates it if needed.
fn dirs_config_dir() -> std::path::PathBuf {
    let dir = dirs_next::config_dir()
        .unwrap_or_else(|| std::path::PathBuf::from("."))
        .join("AuraRafi");
    let _ = std::fs::create_dir_all(&dir);
    dir
}

/// Default directory for new projects.
fn default_projects_dir() -> String {
    dirs_next::document_dir()
        .unwrap_or_else(|| std::path::PathBuf::from("."))
        .join("AuraRafi Projects")
        .display()
        .to_string()
}

fn bottom_tab_icon(tab: &BottomTab) -> Option<&'static str> {
    match tab {
        BottomTab::Assets => Some("assets.png"),
        BottomTab::Console => Some("console.png"),
        BottomTab::AiChat => Some("ai_chat.png"),
        BottomTab::NodeEditor => Some("node_editor.png"),
        BottomTab::ProjectSettings => Some("project_settings.png"),
        BottomTab::Complement(_) => None,
    }
}

fn draw_bottom_tab_button(
    ui: &mut egui::Ui,
    icons: &UiIconAtlas,
    icon_name: Option<&'static str>,
    label: &str,
    is_active: bool,
) -> egui::Response {
    let width = (52.0 + (label.chars().count() as f32 * 7.0)).max(92.0);
    let (rect, response) = ui.allocate_exact_size(egui::vec2(width, 24.0), egui::Sense::click());
    let painter = ui.painter();

    if is_active {
        painter.rect_filled(
            rect.expand2(egui::vec2(0.0, 1.0)),
            6.0,
            egui::Color32::from_rgba_premultiplied(212, 119, 26, 18),
        );
    } else if response.hovered() {
        painter.rect_filled(
            rect.expand2(egui::vec2(0.0, 1.0)),
            6.0,
            egui::Color32::from_rgba_premultiplied(255, 255, 255, 10),
        );
    }

    if let Some(icon_name) = icon_name {
        let icon_rect = egui::Rect::from_center_size(
            egui::pos2(rect.left() + 14.0, rect.center().y),
            egui::vec2(16.0, 16.0),
        );
        let _ = icons.paint(ui.painter(), icon_name, icon_rect, egui::Color32::WHITE);
    }

    let text_color = if is_active {
        app_theme::ACCENT
    } else {
        egui::Color32::from_rgb(150, 150, 160)
    };
    painter.text(
        egui::pos2(rect.left() + 28.0, rect.center().y),
        egui::Align2::LEFT_CENTER,
        label,
        egui::FontId::proportional(13.0),
        text_color,
    );

    if is_active {
        painter.line_segment(
            [
                egui::Pos2::new(rect.left() + 2.0, rect.bottom() + 3.0),
                egui::Pos2::new(rect.right() - 2.0, rect.bottom() + 3.0),
            ],
            egui::Stroke::new(2.0, app_theme::ACCENT),
        );
    }

    response
}

fn ui_icon_budget(ctx: &egui::Context) -> usize {
    let dt = ctx.input(|i| i.predicted_dt.max(1.0 / 240.0));
    if dt > (1.0 / 45.0) {
        1
    } else {
        2
    }
}
include!("panels/complements.rs");
