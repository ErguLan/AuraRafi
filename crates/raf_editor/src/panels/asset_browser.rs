//! Asset browser panel - view and manage project assets.
//! Supports: file listing from project folder, drag-and-drop importing,
//! "Go to Folder" OS explorer, "Create Script" with IDE link dialog.
//! All text translated ES/EN.

use egui::Ui;
use raf_assets::importer::AssetType;
use raf_core::config::Language;
use raf_core::i18n::t;
use std::fs::ReadDir;
use std::path::PathBuf;

use crate::script_support::{asset_relative_path, is_script_file, open_script_in_external_editor};
use crate::theme;
use crate::ui_icons::UiIconAtlas;

#[derive(Clone, Copy, PartialEq, Eq)]
enum ScriptTemplateKind {
    Rust,
    Cpp,
    Rhai,
}

/// State for the asset browser panel.
pub struct AssetBrowserPanel {
    pub search_query: String,
    pub selected_filter: Option<AssetType>,
    /// Live asset entries scanned from project folder.
    pub entries: Vec<AssetEntry>,
    /// Root path of the project assets folder.
    pub project_assets_path: Option<PathBuf>,
    /// If true, show the IDE recommendation dialog.
    show_ide_dialog: bool,
    /// Script file that triggered the IDE dialog.
    ide_dialog_file: String,
    /// Absolute script file path for opening in an external editor.
    ide_dialog_path: Option<PathBuf>,
    /// Status message to show temporarily.
    status_message: Option<String>,
    /// Signals that "Add Entity" was clicked (consumed by app.rs).
    pub add_entity_clicked: bool,
    /// If true, show the script language picker.
    show_script_menu: bool,
    /// Pending directories for deferred asset scanning.
    pending_scan_dirs: Vec<PathBuf>,
    /// Directory iterator currently being consumed within the per-frame budget.
    active_scan_dir: Option<ReadDir>,
    /// Whether a scan is currently in progress.
    pub scan_in_progress: bool,
}

/// A display entry in the asset browser.
pub struct AssetEntry {
    pub name: String,
    pub asset_type: AssetType,
    pub size_display: String,
    pub full_path: Option<PathBuf>,
}

impl Default for AssetBrowserPanel {
    fn default() -> Self {
        Self {
            search_query: String::new(),
            selected_filter: None,
            entries: Vec::new(),
            project_assets_path: None,
            show_ide_dialog: false,
            ide_dialog_file: String::new(),
            ide_dialog_path: None,
            status_message: None,
            add_entity_clicked: false,
            show_script_menu: false,
            pending_scan_dirs: Vec::new(),
            active_scan_dir: None,
            scan_in_progress: false,
        }
    }
}

impl AssetBrowserPanel {
    /// Draw the asset browser panel.
    pub fn show(&mut self, ui: &mut Ui, lang: Language, icons: &UiIconAtlas) {
        let palette = theme::palette_for_visuals(ui.visuals().dark_mode, 0.0);
        // --- Top bar: search + filter + action buttons ---
        egui::Frame::none()
            .fill(palette.panel)
            .stroke(egui::Stroke::new(1.0, palette.border))
            .inner_margin(egui::Margin::same(8.0))
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.label(
                        egui::RichText::new(t("app.search", lang))
                            .size(11.0)
                            .color(palette.text_dim),
                    );
                    ui.add(egui::TextEdit::singleline(&mut self.search_query).desired_width(220.0));

                    ui.separator();

                    // Filter buttons.
                    let all_active = self.selected_filter.is_none();
                    if ui
                        .selectable_label(all_active, t("app.all", lang))
                        .clicked()
                    {
                        self.selected_filter = None;
                    }
                    let img_active = self.selected_filter == Some(AssetType::Image);
                    if ui
                        .selectable_label(img_active, t("app.images", lang))
                        .clicked()
                    {
                        self.selected_filter = Some(AssetType::Image);
                    }
                    let mdl_active = self.selected_filter == Some(AssetType::Model3D);
                    if ui
                        .selectable_label(mdl_active, t("app.models", lang))
                        .clicked()
                    {
                        self.selected_filter = Some(AssetType::Model3D);
                    }
                    let aud_active = self.selected_filter == Some(AssetType::Audio);
                    if ui
                        .selectable_label(aud_active, t("app.audio", lang))
                        .clicked()
                    {
                        self.selected_filter = Some(AssetType::Audio);
                    }
                    let scr_active = self.selected_filter == Some(AssetType::Scene);
                    if ui
                        .selectable_label(scr_active, t("app.scripts_filter", lang))
                        .clicked()
                    {
                        self.selected_filter = Some(AssetType::Scene);
                    }

                    ui.separator();

                    // Action buttons.
                    if ui.button(t("app.add_entity", lang)).clicked() {
                        self.add_entity_clicked = true;
                    }
                    ui.separator();
                    if ui.button(t("app.open_folder", lang)).clicked() {
                        self.open_assets_folder();
                    }
                    if ui.button(t("app.create_script", lang)).clicked() {
                        self.show_script_menu = true;
                    }
                    if ui.button(t("app.refresh_assets", lang)).clicked() {
                        self.scan_project_folder();
                    }
                });
            });

        ui.add_space(6.0);

        if self.show_script_menu {
            self.draw_script_template_menu(ui, lang);
        }

        ui.separator();

        // --- Status message ---
        if self.scan_in_progress {
            ui.label(
                egui::RichText::new(t("app.scanning_assets", lang))
                    .size(10.0)
                    .color(egui::Color32::from_rgb(130, 130, 145)),
            );
        }
        if let Some(msg) = &self.status_message.clone() {
            ui.label(
                egui::RichText::new(msg)
                    .size(10.0)
                    .color(egui::Color32::from_rgb(140, 200, 140)),
            );
        }

        // --- Drag-and-drop zone ---
        let dropped = ui.input(|i| i.raw.dropped_files.clone());
        if !dropped.is_empty() {
            self.handle_dropped_files(&dropped, lang);
        }

        // --- Asset grid ---
        egui::ScrollArea::vertical()
            .auto_shrink([false, false])
            .show(ui, |ui| {
                ui.horizontal_wrapped(|ui| {
                    ui.spacing_mut().item_spacing = egui::vec2(10.0, 10.0);
                    let filtered: Vec<&AssetEntry> = self
                        .entries
                        .iter()
                        .filter(|e| {
                            if let Some(filter) = &self.selected_filter {
                                if e.asset_type != *filter {
                                    return false;
                                }
                            }
                            if !self.search_query.is_empty() {
                                return e
                                    .name
                                    .to_lowercase()
                                    .contains(&self.search_query.to_lowercase());
                            }
                            true
                        })
                        .collect();

                    if filtered.is_empty() {
                        ui.vertical_centered(|ui| {
                            ui.add_space(12.0);
                            ui.label(
                                egui::RichText::new(t("app.no_assets", lang))
                                    .size(11.0)
                                    .color(egui::Color32::from_rgb(100, 100, 110)),
                            );
                            ui.add_space(4.0);
                            ui.label(
                                egui::RichText::new(t("app.drag_drop_hint", lang))
                                    .size(10.0)
                                    .color(egui::Color32::from_rgb(80, 80, 90)),
                            );
                        });
                    } else {
                        for entry in filtered {
                            let type_icon = icon_for_asset(icons, &entry.asset_type);
                            let response = ui.group(|ui| {
                                ui.set_min_width(132.0);
                                ui.set_max_width(160.0);
                                ui.set_min_height(92.0);
                                ui.vertical_centered(|ui| {
                                    if let Some(icon) = type_icon {
                                        ui.add(
                                            egui::Image::new(icon)
                                                .fit_to_exact_size(egui::Vec2::new(32.0, 32.0)),
                                        );
                                    } else {
                                        let (fallback_icon, fallback_color) =
                                            asset_type_visual(&entry.asset_type);
                                        ui.label(
                                            egui::RichText::new(fallback_icon)
                                                .size(20.0)
                                                .color(fallback_color),
                                        );
                                    }
                                    ui.label(
                                        egui::RichText::new(&entry.name)
                                            .size(11.0)
                                            .color(palette.text),
                                    );
                                    ui.label(
                                        egui::RichText::new(&entry.size_display)
                                            .size(10.0)
                                            .color(palette.text_dim),
                                    );
                                });
                            });
                            // Double-click: open script IDE dialog or do nothing for other types.
                            if response.response.double_clicked() {
                                if is_script_file(&entry.name) {
                                    self.show_ide_dialog = true;
                                    self.ide_dialog_path = entry.full_path.clone();
                                    self.ide_dialog_file = entry
                                        .full_path
                                        .as_ref()
                                        .and_then(|full_path| {
                                            self.project_assets_path
                                                .as_ref()
                                                .map(|root| asset_relative_path(root, full_path))
                                        })
                                        .unwrap_or_else(|| entry.name.clone());
                                }
                            }
                        }
                    }
                });
            });

        // --- IDE recommendation dialog ---
        if self.show_ide_dialog {
            self.draw_ide_dialog(ui, lang);
        }
    }

    /// Open the assets folder in the OS file explorer.
    pub fn open_assets_folder(&self) {
        if let Some(path) = &self.project_assets_path {
            let _ = std::fs::create_dir_all(path);
            #[cfg(target_os = "windows")]
            {
                let _ = std::process::Command::new("explorer")
                    .arg(path.as_os_str())
                    .spawn();
            }
            #[cfg(target_os = "macos")]
            {
                let _ = std::process::Command::new("open")
                    .arg(path.as_os_str())
                    .spawn();
            }
            #[cfg(target_os = "linux")]
            {
                let _ = std::process::Command::new("xdg-open")
                    .arg(path.as_os_str())
                    .spawn();
            }
        }
    }

    /// Create a new empty script file in the assets/scripts folder.
    fn create_new_script(&mut self, lang: Language, kind: ScriptTemplateKind) {
        if let Some(base) = &self.project_assets_path {
            let scripts_dir = base.join("scripts");
            let _ = std::fs::create_dir_all(&scripts_dir);

            // Find the next available script name.
            let mut idx = 1;
            loop {
                let (extension, header) = script_template(kind);
                let name = format!("new_script_{}.{}", idx, extension);
                let target = scripts_dir.join(&name);
                if !target.exists() {
                    if std::fs::write(&target, header).is_ok() {
                        self.status_message =
                            Some(format!("{}: {}", t("app.script_created", lang), name));
                        // Show IDE dialog immediately.
                        self.show_ide_dialog = true;
                        self.ide_dialog_file = format!("scripts/{}", name);
                        self.ide_dialog_path = Some(target.clone());
                    }
                    break;
                }
                idx += 1;
            }

            // Refresh listing.
            self.scan_project_folder();
        }
    }

    /// Create a script from a retained UI command.
    pub fn create_script_from_kind(&mut self, lang: Language, kind: &str) {
        let template = match kind {
            "rust" => ScriptTemplateKind::Rust,
            "cpp" => ScriptTemplateKind::Cpp,
            "rhai" => ScriptTemplateKind::Rhai,
            _ => return,
        };
        self.create_new_script(lang, template);
    }

    fn draw_script_template_menu(&mut self, ui: &mut Ui, lang: Language) {
        egui::Window::new(t("app.create_script", lang))
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
            .show(ui.ctx(), |ui| {
                ui.label(t("app.script_language", lang));
                ui.add_space(8.0);

                if ui.button(t("app.script_language_rust", lang)).clicked() {
                    self.create_new_script(lang, ScriptTemplateKind::Rust);
                    self.show_script_menu = false;
                }
                if ui.button(t("app.script_language_cpp", lang)).clicked() {
                    self.create_new_script(lang, ScriptTemplateKind::Cpp);
                    self.show_script_menu = false;
                }
                if ui.button(t("app.script_language_rhai", lang)).clicked() {
                    self.create_new_script(lang, ScriptTemplateKind::Rhai);
                    self.show_script_menu = false;
                }

                ui.add_space(8.0);
                if ui.button(t("app.cancel", lang)).clicked() {
                    self.show_script_menu = false;
                }
            });
    }

    /// Handle files dropped onto the asset browser.
    pub fn handle_dropped_files(&mut self, files: &[egui::DroppedFile], lang: Language) {
        if let Some(base) = &self.project_assets_path.clone() {
            let _ = std::fs::create_dir_all(base);
            let mut count = 0;
            for file in files {
                if let Some(path) = &file.path {
                    if let Some(fname) = path.file_name() {
                        let dest = base.join(fname);
                        if std::fs::copy(path, &dest).is_ok() {
                            count += 1;
                        }
                    }
                }
            }
            if count > 0 {
                self.status_message = Some(format!(
                    "{}: {} {}",
                    t("app.imported_assets", lang),
                    count,
                    if count == 1 { "file" } else { "files" }
                ));
                self.scan_project_folder();
            }
        }
    }

    /// Scan the project assets folder and populate entries.
    pub fn scan_project_folder(&mut self) {
        self.entries.clear();
        self.active_scan_dir = None;
        if let Some(base) = &self.project_assets_path {
            if base.exists() {
                self.pending_scan_dirs.clear();
                self.pending_scan_dirs.push(base.clone());
                self.scan_in_progress = true;
            }
        }
    }

    /// Process a small batch of pending asset scan work.
    pub fn process_scan_budget(&mut self, max_entries: usize) {
        if !self.scan_in_progress {
            return;
        }

        let mut processed = 0usize;

        while processed < max_entries {
            if self.active_scan_dir.is_none() {
                let Some(dir) = self.pending_scan_dirs.pop() else {
                    self.scan_in_progress = false;
                    return;
                };
                self.active_scan_dir = std::fs::read_dir(dir).ok();
                continue;
            }

            let next_entry = self
                .active_scan_dir
                .as_mut()
                .and_then(|directory| directory.next());
            let Some(entry) = next_entry else {
                self.active_scan_dir = None;
                continue;
            };
            let Ok(entry) = entry else {
                continue;
            };

            let path = entry.path();
            if path.is_dir() {
                self.pending_scan_dirs.push(path);
                continue;
            }

            if let Some(fname) = path.file_name().and_then(|name| name.to_str()) {
                let size = std::fs::metadata(&path)
                    .map(|metadata| metadata.len())
                    .unwrap_or(0);
                self.entries.push(AssetEntry {
                    name: fname.to_string(),
                    asset_type: classify_file(fname),
                    size_display: format_size(size),
                    full_path: Some(path),
                });
                processed += 1;
            }
        }
    }

    pub fn open_script_entry(&mut self, index: usize) {
        let Some(entry) = self.entries.get(index) else {
            return;
        };
        if !is_script_file(&entry.name) {
            return;
        }
        self.show_ide_dialog = true;
        self.ide_dialog_path = entry.full_path.clone();
        self.ide_dialog_file = entry
            .full_path
            .as_ref()
            .and_then(|full_path| {
                self.project_assets_path
                    .as_ref()
                    .map(|root| asset_relative_path(root, full_path))
            })
            .unwrap_or_else(|| entry.name.clone());
    }

    pub fn ide_dialog_open(&self) -> bool {
        self.show_ide_dialog
    }

    pub fn ide_dialog_file(&self) -> &str {
        &self.ide_dialog_file
    }

    pub fn status_message(&self) -> Option<&str> {
        self.status_message.as_deref()
    }

    pub fn close_ide_dialog(&mut self) {
        self.show_ide_dialog = false;
    }

    pub fn open_ide_yoll(&mut self) {
        let _ = open_url("https://www.yoll.site/#documentation/IDEYoll");
        self.close_ide_dialog();
    }

    pub fn open_ide_vscode(&mut self) {
        if let Some(path) = &self.ide_dialog_path {
            let _ = open_script_in_external_editor(path);
        } else if let Some(base) = &self.project_assets_path {
            let _ = std::process::Command::new("code")
                .arg(base.as_os_str())
                .spawn();
        }
        self.close_ide_dialog();
    }

    /// Draw the IDE recommendation dialog.
    fn draw_ide_dialog(&mut self, ui: &mut Ui, lang: Language) {
        egui::Window::new(t("app.ide_dialog_title", lang))
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
            .show(ui.ctx(), |ui| {
                ui.label(
                    egui::RichText::new(format!(
                        "{}: {}",
                        t("app.ide_dialog_file", lang),
                        &self.ide_dialog_file
                    ))
                    .size(11.0),
                );
                ui.add_space(6.0);
                ui.label(t("app.ide_dialog_message", lang));
                ui.add_space(8.0);

                ui.horizontal(|ui| {
                    // Yoll IDE link.
                    if ui.button(t("app.ide_open_yoll", lang)).clicked() {
                        let _ = open_url("https://www.yoll.site/#documentation/IDEYoll");
                        self.show_ide_dialog = false;
                    }

                    // VS Code.
                    if ui.button(t("app.ide_open_vscode", lang)).clicked() {
                        if let Some(path) = &self.ide_dialog_path {
                            let _ = open_script_in_external_editor(path);
                        } else if let Some(base) = &self.project_assets_path {
                            let _ = std::process::Command::new("code")
                                .arg(base.as_os_str())
                                .spawn();
                        }
                        self.show_ide_dialog = false;
                    }

                    ui.separator();

                    // Close.
                    if ui.button(t("app.cancel", lang)).clicked() {
                        self.show_ide_dialog = false;
                    }
                });
            });
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Return icon and color for each asset type.
fn asset_type_visual(at: &AssetType) -> (&'static str, egui::Color32) {
    match at {
        AssetType::Image => ("IMG", egui::Color32::from_rgb(150, 150, 150)),
        AssetType::Model3D => ("3D", egui::Color32::from_rgb(212, 119, 26)),
        AssetType::Audio => ("AUD", egui::Color32::from_rgb(110, 170, 120)),
        AssetType::Scene => ("SRC", egui::Color32::from_rgb(170, 170, 170)),
        AssetType::Unknown => ("?", egui::Color32::from_rgb(130, 130, 140)),
    }
}

fn icon_for_asset<'a>(
    icons: &'a UiIconAtlas,
    asset_type: &AssetType,
) -> Option<&'a egui::TextureHandle> {
    let key = match asset_type {
        AssetType::Image => "image.png",
        AssetType::Model3D => "model.png",
        AssetType::Audio => "audio.png",
        AssetType::Scene => "script.png",
        AssetType::Unknown => return None,
    };

    icons.get(key)
}

fn script_template(kind: ScriptTemplateKind) -> (&'static str, &'static str) {
    match kind {
        ScriptTemplateKind::Rust => (
            "rs",
            "// AuraRafi Rust Script\n// Created automatically\n\npub fn on_start() {\n    // Initialize your entity or system here\n}\n\npub fn on_update(dt: f32) {\n    let _ = dt;\n    // Called every frame\n}\n",
        ),
        ScriptTemplateKind::Cpp => (
            "cpp",
            "// AuraRafi C++ Script\n// Created automatically\n\nextern \"C\" void on_start() {\n    // Initialize your entity or system here\n}\n\nextern \"C\" void on_update(float dt) {\n    (void)dt;\n    // Called every frame\n}\n",
        ),
        ScriptTemplateKind::Rhai => (
            "rhai",
            "// AuraRafi Rhai Script\n// Created automatically\n\nfn on_start(ctx) {\n    print(\"Started: \" + ctx.name);\n}\n\nfn on_update(ctx) {\n    if ctx.key_down(\"SPACE\") {\n        ctx.play_audio();\n    }\n}\n\nfn on_trigger_enter(ctx, other_path) {\n    print(\"Trigger: \" + ctx.path + \" -> \" + other_path);\n}\n",
        ),
    }
}

/// Classify file extension into AssetType.
fn classify_file(name: &str) -> AssetType {
    let lower = name.to_lowercase();
    if lower.ends_with(".png")
        || lower.ends_with(".jpg")
        || lower.ends_with(".jpeg")
        || lower.ends_with(".bmp")
        || lower.ends_with(".tga")
        || lower.ends_with(".webp")
    {
        AssetType::Image
    } else if lower.ends_with(".obj")
        || lower.ends_with(".gltf")
        || lower.ends_with(".glb")
        || lower.ends_with(".fbx")
        || lower.ends_with(".stl")
    {
        AssetType::Model3D
    } else if lower.ends_with(".wav")
        || lower.ends_with(".ogg")
        || lower.ends_with(".mp3")
        || lower.ends_with(".flac")
    {
        AssetType::Audio
    } else if is_script_file(name) || lower.ends_with(".ron") || lower.ends_with(".json") {
        AssetType::Scene
    } else {
        AssetType::Unknown
    }
}

/// Format byte count into human-readable string.
fn format_size(bytes: u64) -> String {
    if bytes < 1024 {
        format!("{} B", bytes)
    } else if bytes < 1024 * 1024 {
        format!("{:.1} KB", bytes as f64 / 1024.0)
    } else {
        format!("{:.1} MB", bytes as f64 / (1024.0 * 1024.0))
    }
}

/// Open a URL in the default browser.
fn open_url(url: &str) {
    #[cfg(target_os = "windows")]
    {
        let _ = std::process::Command::new("cmd")
            .args(["/C", "start", url])
            .spawn();
    }
    #[cfg(target_os = "macos")]
    {
        let _ = std::process::Command::new("open").arg(url).spawn();
    }
    #[cfg(target_os = "linux")]
    {
        let _ = std::process::Command::new("xdg-open").arg(url).spawn();
    }
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::time::{SystemTime, UNIX_EPOCH};

    use super::*;

    #[test]
    fn budgeted_scan_keeps_remaining_directory_entries() {
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system clock")
            .as_nanos();
        let root = std::env::temp_dir().join(format!("raf_asset_browser_scan_{stamp}"));
        let nested = root.join("nested");
        fs::create_dir_all(&nested).expect("create test directories");
        for name in ["a.png", "b.png", "c.gltf", "d.rhai", "nested/e.wav"] {
            let path = root.join(name);
            if let Some(parent) = path.parent() {
                fs::create_dir_all(parent).expect("create nested parent");
            }
            fs::write(path, "asset").expect("write test asset");
        }

        let mut panel = AssetBrowserPanel {
            project_assets_path: Some(root.clone()),
            ..Default::default()
        };
        panel.scan_project_folder();
        let mut passes = 0;
        while panel.scan_in_progress {
            panel.process_scan_budget(1);
            passes += 1;
            assert!(
                passes < 32,
                "asset scan should finish within a small budget"
            );
        }

        let mut names: Vec<_> = panel
            .entries
            .iter()
            .map(|entry| entry.name.as_str())
            .collect();
        names.sort_unstable();
        assert_eq!(names, vec!["a.png", "b.png", "c.gltf", "d.rhai", "e.wav"]);

        let _ = fs::remove_dir_all(root);
    }
}
