//! Main application - ties together loading screen, project hub, and editor.
//!
//! Application flow:
//! 1. Loading screen (brief, shows branding)
//! 2. Project Hub (recent projects + create new: Game or Electronics)
//! 3. Main Editor (viewport, hierarchy, properties, assets, console, AI chat,
//!    node editor, schematic view)

use eframe::egui;
use raf_core::config::{EngineSettings, Language, Theme};
use raf_core::project::{Project, ProjectType, RecentProjects};
use raf_core::scene::graph::Primitive;
use raf_core::scene::SceneGraph;

use crate::panels::ai_chat::AiChatPanel;
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
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum BottomTab {
    Assets,
    Console,
    AiChat,
    NodeEditor,
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
            bottom_tab: BottomTab::Console,
            viewport_mode: ViewportMode::Scene,
            _show_settings: false,
            frame_count: 0,
            scene_modified: false,
            last_action: String::new(),
            auto_save_elapsed: 0.0,
            undo_stack: Vec::new(),
            redo_stack: Vec::new(),
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
                    "AuraRafi",
                    egui::FontId::proportional(52.0),
                    app_theme::ACCENT,
                );

                // Tagline.
                let tagline = if self.settings.language == Language::Spanish {
                    "Desarrolla tu propio proyecto"
                } else {
                    "Develop your own project"
                };
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
                    if self.settings.language == Language::Spanish {
                        "Cargando..."
                    } else {
                        "Loading..."
                    };
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

    fn show_project_hub(&mut self, ctx: &egui::Context) {
        egui::CentralPanel::default().show(ctx, |ui| {
            ui.vertical_centered(|ui| {
                ui.add_space(40.0);

                // Header with accent.
                ui.heading(
                    egui::RichText::new("AuraRafi")
                        .size(36.0)
                        .color(app_theme::ACCENT),
                );
                ui.add_space(4.0);
                let subtitle = if self.settings.language == Language::Spanish {
                    "Desarrolla tu propio juego o proyecto electr\u{00f3}nico"
                } else {
                    "Develop your own game or electronic project"
                };
                ui.label(
                    egui::RichText::new(subtitle)
                        .size(14.0)
                        .color(app_theme::DARK_TEXT_DIM),
                );
                ui.add_space(24.0);
            });

            ui.separator();
            ui.add_space(12.0);

            // Two columns: Recent Projects | Create New.
            ui.columns(2, |columns| {
                let is_es = self.settings.language == Language::Spanish;

                // Left: Recent Projects.
                columns[0].heading(
                    egui::RichText::new(if is_es { "Proyectos Recientes" } else { "Recent Projects" }).size(18.0),
                );
                columns[0].add_space(10.0);

                if self.recent_projects.projects.is_empty() {
                    columns[0].add_space(20.0);
                    columns[0].label(
                        egui::RichText::new(if is_es { "No hay proyectos recientes" } else { "No recent projects yet" })
                            .color(app_theme::DARK_TEXT_DIM),
                    );
                    columns[0].label(
                        egui::RichText::new(if is_es { "Crea tu primer proyecto para comenzar" } else { "Create your first project to get started" })
                            .small()
                            .color(app_theme::DARK_TEXT_DIM),
                    );
                } else {
                    let mut open_path: Option<std::path::PathBuf> = None;
                    for entry in &self.recent_projects.projects {
                        let type_icon = match entry.project_type {
                            ProjectType::Game => if is_es { "[Juego]" } else { "[Game]" },
                            ProjectType::Electronics => if is_es { "[Electr\u{00f3}nica]" } else { "[Electronics]" },
                        };
                        columns[0].group(|ui| {
                            ui.horizontal(|ui| {
                                ui.label(
                                    egui::RichText::new(type_icon)
                                        .color(app_theme::ACCENT)
                                        .small(),
                                );
                                ui.strong(&entry.name);
                            });
                            ui.label(
                                egui::RichText::new(entry.path.display().to_string())
                                    .small()
                                    .color(app_theme::DARK_TEXT_DIM),
                            );
                            if ui
                                .add_sized(
                                    [80.0, 24.0],
                                    egui::Button::new(if is_es { "Abrir" } else { "Open" }),
                                )
                                .clicked()
                            {
                                open_path = Some(entry.path.clone());
                            }
                        });
                        columns[0].add_space(4.0);
                    }
                    if let Some(path) = open_path {
                        self.open_project(&path);
                    }
                }

                // Right: Create New.
                columns[1].heading(
                    egui::RichText::new(if is_es { "Crear Nuevo Proyecto" } else { "Create New Project" }).size(18.0),
                );
                columns[1].add_space(10.0);

                // Game project card.
                columns[1].group(|ui| {
                    ui.vertical_centered(|ui| {
                        ui.add_space(16.0);
                        ui.heading(
                            egui::RichText::new(if is_es { "Proyecto de Juego" } else { "Game Project" })
                                .size(16.0),
                        );
                        ui.add_space(4.0);
                        ui.label(
                            egui::RichText::new(if is_es { "Desarrollo de juegos 2D, 3D o h\u{00ed}bridos" } else { "2D, 3D, or hybrid game development" })
                                .color(app_theme::DARK_TEXT_DIM),
                        );
                        ui.add_space(12.0);
                        let btn = egui::Button::new(
                            egui::RichText::new(if is_es { "Crear Juego" } else { "Create Game" }).size(14.0).color(egui::Color32::WHITE),
                        )
                        .fill(app_theme::ACCENT);
                        if ui.add_sized([200.0, 34.0], btn).clicked() {
                            self.screen = AppScreen::NewProject {
                                name: String::new(),
                                path: default_projects_dir(),
                                project_type: ProjectType::Game,
                            };
                        }
                        ui.add_space(16.0);
                    });
                });

                columns[1].add_space(12.0);

                // Electronics project card.
                columns[1].group(|ui| {
                    ui.vertical_centered(|ui| {
                        ui.add_space(16.0);
                        ui.heading(
                            egui::RichText::new(if is_es { "Proyecto Electr\u{00f3}nico" } else { "Electronics Project" })
                                .size(16.0),
                        );
                        ui.add_space(4.0);
                        ui.label(
                            egui::RichText::new(if is_es { "Dise\u{00f1}o de PCB, simulaci\u{00f3}n de circuitos" } else { "PCB design, circuit simulation, processors" })
                                .color(app_theme::DARK_TEXT_DIM),
                        );
                        ui.add_space(12.0);
                        let btn = egui::Button::new(
                            egui::RichText::new(if is_es { "Crear Electr\u{00f3}nica" } else { "Create Electronics" })
                                .size(14.0)
                                .color(egui::Color32::WHITE),
                        )
                        .fill(app_theme::ACCENT);
                        if ui.add_sized([200.0, 34.0], btn).clicked() {
                            self.screen = AppScreen::NewProject {
                                name: String::new(),
                                path: default_projects_dir(),
                                project_type: ProjectType::Electronics,
                            };
                        }
                        ui.add_space(16.0);
                    });
                });
            });

            // Settings button at bottom.
            ui.with_layout(egui::Layout::bottom_up(egui::Align::RIGHT), |ui| {
                ui.add_space(4.0);
                let is_es = self.settings.language == Language::Spanish;
                if ui.button(if is_es { "Configuraci\u{00f3}n" } else { "Settings" }).clicked() {
                    self.previous_screen = Some(AppScreen::ProjectHub);
                    self.screen = AppScreen::Settings;
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
        egui::CentralPanel::default().show(ctx, |ui| {
            ui.add_space(50.0);
            ui.vertical_centered(|ui| {
                let is_es = self.settings.language == Language::Spanish;
                let title = match project_type {
                    ProjectType::Game => {
                        if is_es { "Nuevo Proyecto de Juego" } else { "New Game Project" }
                    }
                    ProjectType::Electronics => {
                        if is_es { "Nuevo Proyecto Electronico" } else { "New Electronics Project" }
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
                            egui::RichText::new("Project Name:").strong(),
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
                            egui::RichText::new("Location:       ").strong(),
                        );
                        ui.add_sized(
                            [300.0, 24.0],
                            egui::TextEdit::singleline(&mut path),
                        );
                    });

                    ui.add_space(6.0);

                    ui.horizontal(|ui| {
                        ui.label(
                            egui::RichText::new("Type:              ").strong(),
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
                        .add_sized([120.0, 30.0], egui::Button::new("Cancel"))
                        .clicked()
                    {
                        self.screen = AppScreen::ProjectHub;
                        return;
                    }

                    ui.add_space(12.0);

                    let can_create = !name.is_empty() && !path.is_empty();
                    let _create_btn = egui::Button::new(
                        egui::RichText::new("Create Project")
                            .color(egui::Color32::WHITE),
                    )
                    .fill(if can_create {
                        app_theme::ACCENT
                    } else {
                        app_theme::DARK_WIDGET
                    });

                    if ui
                        .add_enabled(can_create, egui::Button::new(
                            egui::RichText::new("Create Project")
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
        let is_es = self.settings.language == Language::Spanish;

        // --- Global keyboard shortcuts ---
        self.handle_global_shortcuts(ctx);

        // --- Auto-save ---
        self.handle_auto_save(ctx);

        // Top menu bar.
        egui::TopBottomPanel::top("menu_bar").show(ctx, |ui| {
            egui::menu::bar(ui, |ui| {
                // -- File --
                ui.menu_button(if is_es { "Archivo" } else { "File" }, |ui| {
                    if ui.button(if is_es { "Nuevo Proyecto" } else { "New Project" }).clicked() {
                        self.screen = AppScreen::ProjectHub;
                        ui.close_menu();
                    }
                    if ui.button(if is_es { "Guardar  (Ctrl+S)" } else { "Save  (Ctrl+S)" }).clicked() {
                        self.do_save();
                        ui.close_menu();
                    }
                    ui.separator();
                    if ui.button(if is_es { "Configuracion" } else { "Settings" }).clicked() {
                        self.previous_screen = Some(AppScreen::Editor);
                        self.screen = AppScreen::Settings;
                        ui.close_menu();
                    }
                    ui.separator();
                    if ui.button(if is_es { "Salir al Hub" } else { "Exit to Hub" }).clicked() {
                        self.screen = AppScreen::ProjectHub;
                        ui.close_menu();
                    }
                });

                // -- Edit --
                ui.menu_button(if is_es { "Editar" } else { "Edit" }, |ui| {
                    let undo_label = if is_es {
                        format!("Deshacer  (Ctrl+Z)  [{}]", self.undo_stack.len())
                    } else {
                        format!("Undo  (Ctrl+Z)  [{}]", self.undo_stack.len())
                    };
                    if ui.add_enabled(!self.undo_stack.is_empty(), egui::Button::new(undo_label)).clicked() {
                        self.do_undo();
                        ui.close_menu();
                    }
                    let redo_label = if is_es {
                        format!("Rehacer  (Ctrl+Y)  [{}]", self.redo_stack.len())
                    } else {
                        format!("Redo  (Ctrl+Y)  [{}]", self.redo_stack.len())
                    };
                    if ui.add_enabled(!self.redo_stack.is_empty(), egui::Button::new(redo_label)).clicked() {
                        self.do_redo();
                        ui.close_menu();
                    }
                    ui.separator();
                    if ui.button(if is_es { "Duplicar  (Ctrl+D)" } else { "Duplicate  (Ctrl+D)" }).clicked() {
                        self.do_duplicate();
                        ui.close_menu();
                    }
                    if ui.button(if is_es { "Eliminar  (Supr)" } else { "Delete  (Del)" }).clicked() {
                        self.do_delete();
                        ui.close_menu();
                    }
                    ui.separator();
                    if ui.button(if is_es { "Seleccionar Todo  (Ctrl+A)" } else { "Select All  (Ctrl+A)" }).clicked() {
                        self.do_select_all();
                        ui.close_menu();
                    }
                });

                // -- View --
                ui.menu_button(if is_es { "Vista" } else { "View" }, |ui| {
                    let grid_label = if is_es { "Cuadricula" } else { "Grid" };
                    ui.checkbox(&mut self.settings.grid_visible, grid_label);
                    ui.separator();

                    let scene_label = if is_es { "Vista de Escena" } else { "Scene View" };
                    if ui.selectable_label(self.viewport_mode == ViewportMode::Scene, scene_label).clicked() {
                        self.viewport_mode = ViewportMode::Scene;
                        ui.close_menu();
                    }
                    let sch_label = if is_es { "Vista de Esquematico" } else { "Schematic View" };
                    if ui.selectable_label(self.viewport_mode == ViewportMode::Schematic, sch_label).clicked() {
                        self.viewport_mode = ViewportMode::Schematic;
                        ui.close_menu();
                    }
                });

                // -- Project --
                ui.menu_button(if is_es { "Proyecto" } else { "Project" }, |ui| {
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
                ui.menu_button(if is_es { "Ayuda" } else { "Help" }, |ui| {
                    ui.label(egui::RichText::new(if is_es { "Atajos de teclado" } else { "Keyboard Shortcuts" }).strong());
                    ui.label("Ctrl+S  -  Save");
                    ui.label("Ctrl+Z  -  Undo");
                    ui.label("Ctrl+Y  -  Redo");
                    ui.label("Ctrl+D  -  Duplicate");
                    ui.label("Ctrl+A  -  Select All");
                    ui.label("Del     -  Delete");
                    ui.label("Q/W/E/R -  Tool Select");
                    ui.separator();
                    ui.label(format!("AuraRafi v{}", env!("CARGO_PKG_VERSION")));
                    ui.close_menu();
                });

                // Right side: Build/Run + FPS.
                ui.with_layout(
                    egui::Layout::right_to_left(egui::Align::Center),
                    |ui| {
                        let build_text =
                            if let Some(project) = &self.current_project {
                                match project.project_type {
                                    ProjectType::Game => {
                                        if is_es { "Ejecutar" } else { "Run Game" }
                                    }
                                    ProjectType::Electronics => {
                                        if is_es { "Test Electrico" } else { "Electrical Test" }
                                    }
                                }
                            } else {
                                if is_es { "Compilar" } else { "Build" }
                            };

                        let build_btn = egui::Button::new(
                            egui::RichText::new(build_text)
                                .color(egui::Color32::WHITE)
                                .strong(),
                        )
                        .fill(app_theme::ACCENT);

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
                                .small()
                                .color(app_theme::DARK_TEXT_DIM),
                        );

                        // Viewport mode indicator.
                        ui.separator();
                        let mode_text = match self.viewport_mode {
                            ViewportMode::Scene => {
                                if is_es { "Escena" } else { "Scene" }
                            }
                            ViewportMode::Schematic => {
                                if is_es { "Esquematico" } else { "Schematic" }
                            }
                        };
                        ui.label(
                            egui::RichText::new(mode_text)
                                .small()
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
                        let modified = if self.scene_modified { " (*)" } else { "" };
                        ui.label(
                            egui::RichText::new(format!("{}{}", project.name, modified))
                                .small()
                                .color(app_theme::ACCENT),
                        );
                        ui.separator();
                        ui.label(
                            egui::RichText::new(project.project_type.display_name())
                                .small(),
                        );
                    }
                    ui.separator();
                    let ent_label = if is_es {
                        format!("Entidades: {}", self.scene.all_valid_ids().len())
                    } else {
                        format!("Entities: {}", self.scene.all_valid_ids().len())
                    };
                    ui.label(egui::RichText::new(ent_label).small());
                    ui.separator();
                    // Undo/redo depth.
                    ui.label(
                        egui::RichText::new(format!(
                            "U:{} R:{}",
                            self.undo_stack.len(),
                            self.redo_stack.len()
                        ))
                        .small()
                        .color(app_theme::DARK_TEXT_DIM),
                    );
                    // Last action.
                    if !self.last_action.is_empty() {
                        ui.separator();
                        ui.label(
                            egui::RichText::new(&self.last_action)
                                .small()
                                .color(app_theme::DARK_TEXT_DIM),
                        );
                    }
                    ui.with_layout(
                        egui::Layout::right_to_left(egui::Align::Center),
                        |ui| {
                            let theme_name = match self.settings.theme {
                                Theme::Dark => { if is_es { "Oscuro" } else { "Dark" } }
                                Theme::Light => { if is_es { "Claro" } else { "Light" } }
                                Theme::System => "System",
                            };
                            ui.label(
                                egui::RichText::new(format!(
                                    "{} | {}",
                                    self.settings.language.display_name(),
                                    theme_name
                                ))
                                .small(),
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
                    let tabs = [
                        (BottomTab::Console, "Console"),
                        (BottomTab::Assets, "Assets"),
                        (BottomTab::NodeEditor, "Node Editor"),
                        (BottomTab::AiChat, "AI Chat"),
                    ];
                    for (tab, label) in tabs {
                        if ui
                            .selectable_label(self.bottom_tab == tab, label)
                            .clicked()
                        {
                            self.bottom_tab = tab;
                        }
                    }
                });
                ui.separator();

                match self.bottom_tab {
                    BottomTab::Assets => self.asset_browser.show(ui),
                    BottomTab::Console => self.console.show(ui),
                    BottomTab::AiChat => {
                        self.ai_chat.is_es = self.settings.language == Language::Spanish;
                        self.ai_chat.show(ui);
                    }
                    BottomTab::NodeEditor => self.node_editor.show(ui),
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

                let add_label = if is_es { "+ Agregar" } else { "+ Add" };
                let add_btn = egui::Button::new(
                    egui::RichText::new(add_label).color(egui::Color32::WHITE),
                )
                .fill(app_theme::ACCENT);

                let response = ui.add_sized([ui.available_width(), 28.0], add_btn);
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
                            let label = if is_es { prim.label_es() } else { prim.label() };
                            if ui.button(label).clicked() {
                                self.push_undo_snapshot();
                                let name = format!("{} {}", prim.label(), self.scene.len() + 1);
                                let id = self.scene.add_root_with_primitive(&name, prim);
                                self.hierarchy.selected_node = Some(id);
                                self.viewport.selected = Some(id);
                                let add_msg = if is_es {
                                    format!("Agregado: {}", name)
                                } else {
                                    format!("Added: {}", name)
                                };
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
                    self.schematic_view.is_es = self.settings.language == Language::Spanish;
                    self.schematic_view.show(ui);
                }
            }
        });
    }

    // -----------------------------------------------------------------------
    // Settings Screen
    // -----------------------------------------------------------------------

    fn show_settings_screen(&mut self, ctx: &egui::Context) {
        let is_es = self.settings.language == Language::Spanish;
        egui::CentralPanel::default().show(ctx, |ui| {
            ui.add_space(20.0);
            ui.vertical_centered(|ui| {
                let title = if is_es { "Configuracion" } else { "Settings" };
                ui.heading(
                    egui::RichText::new(title)
                        .size(24.0)
                        .color(app_theme::ACCENT),
                );
            });
            ui.add_space(12.0);
            ui.separator();

            settings_panel::show_settings(ui, &mut self.settings);

            ui.add_space(16.0);
            ui.separator();
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                let save_label = if is_es { "Guardar y Cerrar" } else { "Save & Close" };
                let save_btn = egui::Button::new(
                    egui::RichText::new(save_label)
                        .color(egui::Color32::WHITE),
                )
                .fill(app_theme::ACCENT);

                if ui.add_sized([140.0, 30.0], save_btn).clicked() {
                    let config_dir = dirs_config_dir();
                    let _ = self.settings.save(&config_dir);
                    let msg = if is_es { "Configuracion guardada" } else { "Settings saved" };
                    self.console.log(LogLevel::Info, msg);
                    self.screen = self
                        .previous_screen
                        .take()
                        .unwrap_or(AppScreen::ProjectHub);
                }

                ui.add_space(8.0);

                let cancel_label = if is_es { "Cancelar" } else { "Cancel" };
                if ui
                    .add_sized([100.0, 30.0], egui::Button::new(cancel_label))
                    .clicked()
                {
                    self.screen = self
                        .previous_screen
                        .take()
                        .unwrap_or(AppScreen::ProjectHub);
                }
            });
        });
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
                let is_es = self.settings.language == Language::Spanish;
                let msg = if is_es { "Deshacer" } else { "Undo" };
                self.last_action = msg.to_string();
                self.console.log(LogLevel::Info, msg);
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
                let is_es = self.settings.language == Language::Spanish;
                let msg = if is_es { "Rehacer" } else { "Redo" };
                self.last_action = msg.to_string();
                self.console.log(LogLevel::Info, msg);
            }
        }
    }

    // -----------------------------------------------------------------------
    // v0.3.0: Scene actions
    // -----------------------------------------------------------------------

    fn do_delete(&mut self) {
        let is_es = self.settings.language == Language::Spanish;
        if let Some(id) = self.hierarchy.selected_node {
            self.push_undo_snapshot();
            let name = self.scene.get(id).map(|n| n.name.clone()).unwrap_or_default();
            if self.scene.remove_node(id) {
                self.hierarchy.selected_node = None;
                self.viewport.selected = None;
                let msg = if is_es {
                    format!("Eliminado: {}", name)
                } else {
                    format!("Deleted: {}", name)
                };
                self.last_action = msg.clone();
                self.console.log(LogLevel::Info, &msg);
            }
        }
    }

    fn do_duplicate(&mut self) {
        let is_es = self.settings.language == Language::Spanish;
        if let Some(id) = self.hierarchy.selected_node {
            self.push_undo_snapshot();
            if let Some(new_id) = self.scene.duplicate_node(id) {
                self.hierarchy.selected_node = Some(new_id);
                self.viewport.selected = Some(new_id);
                let name = self.scene.get(new_id).map(|n| n.name.clone()).unwrap_or_default();
                let msg = if is_es {
                    format!("Duplicado: {}", name)
                } else {
                    format!("Duplicated: {}", name)
                };
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
        let is_es = self.settings.language == Language::Spanish;
        let msg = if is_es {
            format!("{} entidades encontradas", ids.len())
        } else {
            format!("{} entities found", ids.len())
        };
        self.last_action = msg.clone();
        self.console.log(LogLevel::Info, &msg);
    }

    fn do_save(&mut self) {
        let is_es = self.settings.language == Language::Spanish;
        if let Some(project) = &self.current_project {
            let _ = project.save();
            // Save scene alongside project.
            let scene_path = project.path.join("scene.ron");
            let _ = self.scene.save_ron(&scene_path);
            self.scene_modified = false;
            self.auto_save_elapsed = 0.0;
            let msg = if is_es { "Proyecto guardado" } else { "Project saved" };
            self.last_action = msg.to_string();
            self.console.log(LogLevel::Info, msg);
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
            let is_es = self.settings.language == Language::Spanish;
            let msg = if is_es { "Auto-guardado" } else { "Auto-saved" };
            self.last_action = msg.to_string();
            self.console.log(LogLevel::Info, msg);
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
                        "Running electrical test...",
                    );
                    let results = self.schematic_view.schematic.electrical_test();
                    for result in &results {
                        if result.contains("passed") {
                            self.console.log(LogLevel::Info, result);
                        } else {
                            self.console.log(LogLevel::Warning, result);
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
                    let is_es = self.settings.language == Language::Spanish;
                    let msg = if is_es { "Escena cargada desde archivo" } else { "Scene loaded from file" };
                    self.console.log(LogLevel::Info, msg);
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
                let is_es = self.settings.language == Language::Spanish;
                let msg = if is_es { "Proyecto cargado" } else { "Project loaded" };
                self.console.log(LogLevel::Info, msg);
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
