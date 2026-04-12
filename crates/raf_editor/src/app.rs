//! Main application - ties together loading screen, project hub, and editor.
//!
//! Application flow:
//! 1. Loading screen (brief, shows branding)
//! 2. Project Hub (recent projects + create new: Game or Electronics)
//! 3. Main Editor (viewport, hierarchy, properties, assets, console, AI chat,
//!    node editor, schematic view)

use eframe::egui;
use raf_core::i18n::t;
use raf_core::config::{EngineSettings, Theme};
use raf_core::project::{Project, ProjectType, RecentProjects};
use raf_core::scene::graph::Primitive;
use raf_core::scene::SceneGraph;

use crate::panels::ai_chat::AiChatPanel;
use image;
use crate::panels::asset_browser::AssetBrowserPanel;
use crate::panels::console::{ConsolePanel, LogLevel};
use crate::panels::hierarchy::HierarchyPanel;
use crate::panels::node_editor::NodeEditorPanel;
use crate::panels::properties::PropertiesPanel;
use crate::panels::schematic_view::SchematicViewPanel;
use crate::panels::settings_panel;
use crate::panels::viewport::ViewportPanel;
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
    Complement(String),
}

/// Central viewport mode for the editor body.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ViewportMode {
    Scene,
    Schematic,
}

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

    // Editor panels
    viewport: ViewportPanel,
    hierarchy: HierarchyPanel,
    properties: PropertiesPanel,
    asset_browser: AssetBrowserPanel,
    console: ConsolePanel,
    ai_chat: AiChatPanel,
    node_editor: NodeEditorPanel,
    schematic_view: SchematicViewPanel,

    // Editor state
    bottom_tab: BottomTab,
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
    /// Snapshots for undo (lightweight RON of scene).
    undo_stack: Vec<String>,
    /// Snapshots for redo.
    redo_stack: Vec<String>,
    /// Project logo texture.
    logo_texture: Option<egui::TextureHandle>,
}

impl AuraRafiApp {
    /// Create the application.
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        // Load settings from app data directory.
        let config_dir = dirs_config_dir();
        let settings = EngineSettings::load(&config_dir);
        let recent_projects = RecentProjects::load(&config_dir);

        // Apply initial theme.
        app_theme::apply_theme(&cc.egui_ctx, settings.theme);

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
            viewport: ViewportPanel::default(),
            hierarchy: HierarchyPanel::default(),
            properties: PropertiesPanel::default(),
            asset_browser: AssetBrowserPanel::default(),
            console: ConsolePanel::default(),
            ai_chat: AiChatPanel::default(),
            node_editor: NodeEditorPanel::default(),
            schematic_view: SchematicViewPanel::default(),

            complement_registry: raf_core::complement::ComplementRegistry::new(),
            command_bus: raf_core::command::CommandBus::new(),

            bottom_tab: BottomTab::Console,
            viewport_mode: ViewportMode::Scene,
            _show_settings: false,
            frame_count: 0,
            scene_modified: false,
            last_action: String::new(),
            auto_save_elapsed: 0.0,
            undo_stack: Vec::new(),
            redo_stack: Vec::new(),
            logo_texture: None,
        }
    }
}

impl eframe::App for AuraRafiApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.frame_count += 1;

        // Re-apply theme every frame (cheap, ensures consistency).
        app_theme::apply_theme(ctx, self.settings.theme);

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

        egui::CentralPanel::default()
            .frame(egui::Frame::default().fill(app_theme::DARK_BG))
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
                    app_theme::DARK_TEXT_DIM,
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
                    app_theme::DARK_WIDGET,
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
                    app_theme::DARK_TEXT_DIM,
                );

                // --- Subtle Yoll credit at the bottom ---
                ui.painter().text(
                    egui::pos2(center.x, available.bottom() - 36.0),
                    egui::Align2::CENTER_CENTER,
                    "A project by Yoll",
                    egui::FontId::proportional(11.0),
                    egui::Color32::from_rgb(90, 90, 100),
                );
                ui.painter().text(
                    egui::pos2(center.x, available.bottom() - 20.0),
                    egui::Align2::CENTER_CENTER,
                    "yoll.site",
                    egui::FontId::proportional(10.0),
                    egui::Color32::from_rgb(70, 70, 80),
                );

                // Version (small, corner).
                ui.painter().text(
                    egui::pos2(available.right() - 10.0, available.bottom() - 10.0),
                    egui::Align2::RIGHT_BOTTOM,
                    format!("v{}", env!("CARGO_PKG_VERSION")),
                    egui::FontId::proportional(9.0),
                    egui::Color32::from_rgb(55, 55, 65),
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
            let icon_bytes = include_bytes!("../../../editor/icon.png");
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

    // -----------------------------------------------------------------------
    // Main Editor
    // -----------------------------------------------------------------------

    fn show_editor(&mut self, ctx: &egui::Context) {
        let _lang = self.settings.language;

        // --- Global keyboard shortcuts ---
        self.handle_global_shortcuts(ctx);

        // --- Auto-save ---
        self.handle_auto_save(ctx);

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
                    if ui.selectable_label(self.viewport_mode == ViewportMode::Scene, t("app.scene_view", _lang)).clicked() {
                        self.viewport_mode = ViewportMode::Scene;
                        ui.close_menu();
                    }
                    if ui.selectable_label(self.viewport_mode == ViewportMode::Schematic, t("app.schematic_view", _lang)).clicked() {
                        self.viewport_mode = ViewportMode::Schematic;
                        ui.close_menu();
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
                                .color(app_theme::DARK_TEXT_DIM),
                        );
                    }
                    ui.close_menu();
                });

                // -- Help --
                ui.menu_button(t("app.help_menu", _lang), |ui| {
                    ui.label(egui::RichText::new(t("app.keyboard_shortcuts", _lang)).strong());
                    ui.label("Ctrl+S  -  Save");
                    ui.label("Ctrl+Z  -  Undo");
                    ui.label("Ctrl+Y  -  Redo");
                    ui.label("Ctrl+D  -  Duplicate");
                    ui.label("Ctrl+A  -  Select All");
                    ui.label("Del     -  Delete");
                    ui.label("Q/W/E/R -  Tool Select");
                    ui.separator();
                    ui.label(format!("Proyecto Rafi v{}", env!("CARGO_PKG_VERSION")));
                    ui.close_menu();
                });

                // Right side: mode indicator | FPS | Build/Run.
                ui.with_layout(
                    egui::Layout::right_to_left(egui::Align::Center),
                    |ui| {
                        // Build/Run button - integrated into toolbar, not a floating badge.
                        let build_text = if let Some(project) = &self.current_project {
                            match project.project_type {
                                ProjectType::Game => t("app.run_game", _lang),
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
                                .color(app_theme::DARK_TEXT_DIM),
                        );

                        // Viewport mode indicator.
                        ui.separator();
                        let mode_text = match self.viewport_mode {
                            ViewportMode::Scene => t("app.scene_view", _lang),
                            ViewportMode::Schematic => t("app.schematic_view", _lang),
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
                                .color(app_theme::DARK_TEXT_DIM),
                        );
                    }
                    ui.separator();
                    ui.label(
                        egui::RichText::new(format!("{} {}", t("app.entities_count", _lang), self.scene.all_valid_ids().len()))
                            .size(11.0)
                            .color(app_theme::DARK_TEXT_DIM),
                    );
                    ui.separator();
                    ui.label(
                        egui::RichText::new(format!(
                            "U:{} R:{}",
                            self.undo_stack.len(),
                            self.redo_stack.len()
                        ))
                        .size(11.0)
                        .color(app_theme::DARK_TEXT_DIM),
                    );
                    if !self.last_action.is_empty() {
                        ui.separator();
                        ui.label(
                            egui::RichText::new(&self.last_action)
                                .size(11.0)
                                .color(app_theme::DARK_TEXT_DIM),
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
                                .color(app_theme::DARK_TEXT_DIM),
                            );
                        },
                    );
                });
            });

        // Bottom panel (tabbed: Assets / Console / AI Chat / Node Editor).
        egui::TopBottomPanel::bottom("bottom_panel")
            .resizable(true)
            .min_height(120.0)
            .default_height(200.0)
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    let mut tabs = vec![
                        (BottomTab::Console, "Console".to_string()),
                        (BottomTab::Assets, "Assets".to_string()),
                        (BottomTab::NodeEditor, "Node Editor".to_string()),
                        (BottomTab::AiChat, "AI Chat".to_string()),
                    ];
                    for comp in &self.complement_registry.complements {
                        tabs.push((BottomTab::Complement(comp.id().to_string()), comp.name().to_string()));
                    }
                    
                    let mut tab_changed = None;
                    for (tab, label) in tabs {
                        let is_active = self.bottom_tab == tab;
                        let text_color = if is_active {
                            app_theme::ACCENT
                        } else {
                            egui::Color32::from_rgb(150, 150, 160)
                        };
                        
                        let btn = egui::Button::new(
                            egui::RichText::new(label).size(13.0).color(text_color),
                        )
                        .fill(egui::Color32::TRANSPARENT)
                        .frame(false);

                        let response = ui.add(btn);
                        
                        // Draw underline for the active tab
                        if is_active {
                            let rect = response.rect;
                            ui.painter().line_segment(
                                [
                                    egui::Pos2::new(rect.left(), rect.bottom() + 3.0),
                                    egui::Pos2::new(rect.right(), rect.bottom() + 3.0),
                                ],
                                egui::Stroke::new(2.0, app_theme::ACCENT),
                            );
                        }

                        if response.clicked() {
                            tab_changed = Some(tab);
                        }
                        
                        ui.add_space(16.0);
                    }
                    if let Some(t) = tab_changed {
                        self.bottom_tab = t;
                    }
                });
                ui.separator();

                match &self.bottom_tab {
                    BottomTab::Assets => self.asset_browser.show(ui, self.settings.language),
                    BottomTab::Console => self.console.show(ui, self.settings.language),
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
        egui::SidePanel::left("hierarchy_panel")
            .resizable(true)
            .default_width(200.0)
            .min_width(150.0)
            .show(ctx, |ui| {
                self.hierarchy.show(ui, &self.scene, self.settings.language);

                ui.add_space(8.0);
                ui.separator();
                ui.add_space(4.0);

                // Subtle + Add button (not a giant orange CTA).
                let add_btn = egui::Button::new(
                    egui::RichText::new("+ Add").size(12.0),
                )
                .rounding(4.0);

                let response = ui.add_sized([ui.available_width(), 26.0], add_btn);
                let popup_id = egui::Id::new("add_entity_popup");

                if response.clicked() {
                    ui.memory_mut(|m| m.toggle_popup(popup_id));
                }

                egui::popup_below_widget(
                    ui,
                    popup_id,
                    &response,
                    egui::PopupCloseBehavior::CloseOnClickOutside,
                    |ui| {
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
                                self.viewport.selected = Some(id);
                                let add_msg = format!("Added: {}", name);
                                self.last_action = add_msg.clone();
                                self.console.log(LogLevel::Info, &add_msg);
                            }
                        }
                    },
                );
            });

        // Right panel: Properties.
        egui::SidePanel::right("properties_panel")
            .resizable(true)
            .default_width(280.0)
            .min_width(200.0)
            .show(ctx, |ui| {
                self.properties
                    .show(ui, &mut self.scene, self.hierarchy.selected_node, self.settings.language);
            });

        // Central panel: Viewport or Schematic.
        egui::CentralPanel::default().show(ctx, |ui| {
            match self.viewport_mode {
                ViewportMode::Scene => {
                    self.viewport.grid_visible = self.settings.grid_visible;
                    self.viewport.show(
                        ui,
                        &self.scene,
                        self.settings.theme != Theme::Light,
                        self.settings.language,
                    );
                }
                ViewportMode::Schematic => {
                    self.schematic_view.lang = self.settings.language;
                    self.schematic_view.show(ui);
                }
            }
        });
    }

    // -----------------------------------------------------------------------
    // Settings Screen
    // -----------------------------------------------------------------------

    fn show_settings_screen(&mut self, ctx: &egui::Context) {
        let _lang = self.settings.language;
        let mut close = false;
        let mut save = false;

        egui::CentralPanel::default().show(ctx, |ui| {
            ui.vertical_centered(|ui| {
                ui.set_max_width(540.0); // Clean, constrained center column
                
                ui.add_space(32.0);
                
                // Professional Title (Extralight-like, subtle)
                ui.label(
                    egui::RichText::new(t("app.engine_settings_title", _lang))
                        .size(16.0)
                        .color(egui::Color32::from_rgb(140, 140, 150)),
                );
                
                ui.add_space(20.0);
                ui.separator();
                ui.add_space(16.0);

                // Need a frame for the settings panel content
                let frame = egui::Frame::none()
                    .fill(if ctx.style().visuals.dark_mode { app_theme::DARK_PANEL } else { app_theme::LIGHT_PANEL })
                    .rounding(8.0)
                    .inner_margin(24.0)
                    .stroke(egui::Stroke::new(1.0, if ctx.style().visuals.dark_mode { app_theme::DARK_BORDER } else { app_theme::LIGHT_BORDER }));
                
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

    /// Push current scene state to undo stack.
    fn push_undo_snapshot(&mut self) {
        // Serialize scene to RON string (lightweight).
        if let Ok(data) = ron::ser::to_string(&self.scene) {
            self.undo_stack.push(data);
            // Cap at 50 to keep memory low.
            if self.undo_stack.len() > 50 {
                self.undo_stack.remove(0);
            }
            self.redo_stack.clear();
            self.scene_modified = true;
        }
    }

    fn do_undo(&mut self) {
        if let Some(snapshot) = self.undo_stack.pop() {
            // Save current to redo.
            if let Ok(current) = ron::ser::to_string(&self.scene) {
                self.redo_stack.push(current);
            }
            // Restore.
            if let Ok(restored) = ron::from_str::<SceneGraph>(&snapshot) {
                self.scene = restored;
                self.hierarchy.selected_node = None;
                self.viewport.selected = None;
                let _lang = self.settings.language;
                let msg = t("app.undo", self.settings.language);
                self.last_action = msg.to_string();
                self.console.log(LogLevel::Info, &msg);
            }
        }
    }

    fn do_redo(&mut self) {
        if let Some(snapshot) = self.redo_stack.pop() {
            // Save current to undo.
            if let Ok(current) = ron::ser::to_string(&self.scene) {
                self.undo_stack.push(current);
            }
            // Restore.
            if let Ok(restored) = ron::from_str::<SceneGraph>(&snapshot) {
                self.scene = restored;
                self.hierarchy.selected_node = None;
                self.viewport.selected = None;
                let _lang = self.settings.language;
                let msg = t("app.redo", self.settings.language);
                self.last_action = msg.to_string();
                self.console.log(LogLevel::Info, &msg);
            }
        }
    }

    // -----------------------------------------------------------------------
    // v0.3.0: Scene actions
    // -----------------------------------------------------------------------

    fn do_delete(&mut self) {
        let _lang = self.settings.language;
        if let Some(id) = self.hierarchy.selected_node {
            self.push_undo_snapshot();
            let name = self.scene.get(id).map(|n| n.name.clone()).unwrap_or_default();
            if self.scene.remove_node(id) {
                self.hierarchy.selected_node = None;
                self.viewport.selected = None;
                let msg = format!("{} {}", t("app.deleted_msg", _lang), name);
                self.last_action = msg.clone();
                self.console.log(LogLevel::Info, &msg);
            }
        }
    }

    fn do_duplicate(&mut self) {
        let _lang = self.settings.language;
        if let Some(id) = self.hierarchy.selected_node {
            self.push_undo_snapshot();
            if let Some(new_id) = self.scene.duplicate_node(id) {
                self.hierarchy.selected_node = Some(new_id);
                self.viewport.selected = Some(new_id);
                let name = self.scene.get(new_id).map(|n| n.name.clone()).unwrap_or_default();
                let msg = format!("{} {}", t("app.duplicated_msg", _lang), name);
                self.last_action = msg.clone();
                self.console.log(LogLevel::Info, &msg);
            }
        }
    }

    fn do_select_all(&mut self) {
        let ids = self.scene.all_valid_ids();
        if let Some(first) = ids.first() {
            self.hierarchy.selected_node = Some(*first);
            self.viewport.selected = Some(*first);
        }
        let _lang = self.settings.language;
        let msg = format!("{} {}", ids.len(), t("app.entities_found_msg", _lang));
        self.last_action = msg.clone();
        self.console.log(LogLevel::Info, &msg);
    }

    fn do_save(&mut self) {
        let _lang = self.settings.language;
        if let Some(project) = &self.current_project {
            let _ = project.save();
            // Save scene alongside project.
            let scene_path = project.path.join("scene.ron");
            let _ = self.scene.save_ron(&scene_path);
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
        let action: Option<u8> = ctx.input(|i| {
            let ctrl = i.modifiers.ctrl || i.modifiers.mac_cmd;
            if ctrl && i.key_pressed(egui::Key::Z) { return Some(1); }
            if ctrl && i.key_pressed(egui::Key::Y) { return Some(2); }
            if ctrl && i.key_pressed(egui::Key::D) { return Some(3); }
            if ctrl && i.key_pressed(egui::Key::S) { return Some(4); }
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

    // -----------------------------------------------------------------------
    // Build / Run
    // -----------------------------------------------------------------------

    fn handle_build(&mut self) {
        if let Some(project) = &self.current_project {
            match project.project_type {
                ProjectType::Game => {
                    self.console.log(
                        LogLevel::Info,
                        "Building game project...",
                    );
                    self.console.log(
                        LogLevel::Info,
                        &format!(
                            "Scene contains {} entities",
                            self.scene.len()
                        ),
                    );
                    self.console.log(
                        LogLevel::Warning,
                        "Game runtime not implemented yet - scene validated successfully",
                    );
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

                // Try to load saved scene, fall back to default.
                let scene_path = project.path.join("scene.ron");
                if scene_path.exists() {
                    self.scene = SceneGraph::load_ron(&scene_path);
                    let _lang = self.settings.language;
                    let msg = t("app.scene_loaded_from_file", self.settings.language);
                    self.console.log(LogLevel::Info, &msg);
                } else {
                    self.init_scene_for_type(ptype);
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

                self.current_project = Some(project);
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
include!("panels/complements.rs");
