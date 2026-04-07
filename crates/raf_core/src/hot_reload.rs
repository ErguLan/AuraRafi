//! Hot reload system - file change detection without restart.
//!
//! Polling-based file watcher that detects when project files change
//! on disk (by another tool, collaborator, or mod). No external crate
//! needed - just checks file modification timestamps periodically.
//!
//! Zero cost when disabled. When enabled, runs a stat() check every
//! N seconds (configurable). No threads, no async, no dependencies.
//!
//! Use cases:
//! - Hot reload: change a value in scene.ron externally, engine picks it up
//! - Mod support: scripts in mods/ folder get detected and loaded
//! - Collaborative dev: another dev saves to shared folder, your engine sees it
//! - Live editing: change a config file and see results instantly

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::SystemTime;
use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// Watch categories
// ---------------------------------------------------------------------------

/// Category of a watched file. Tells the engine what kind of reload to do.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum WatchCategory {
    /// Scene file (.ron) - reloads scene graph.
    Scene,
    /// Schematic file (.ron) - reloads electronics schematic.
    Schematic,
    /// Engine config file (.ron) - reloads settings.
    Config,
    /// Script file (.lua, .rhai, .wasm, etc.) - reloads game logic / mods.
    Script,
    /// Asset file (image, model, sound) - reloads asset data.
    Asset,
    /// Project file (project.ron) - reloads project metadata.
    Project,
    /// Any other file type.
    Other,
}

impl WatchCategory {
    /// Label for UI.
    pub fn label(&self) -> &'static str {
        match self {
            Self::Scene => "Scene",
            Self::Schematic => "Schematic",
            Self::Config => "Config",
            Self::Script => "Script",
            Self::Asset => "Asset",
            Self::Project => "Project",
            Self::Other => "Other",
        }
    }

    /// Label in Spanish.
    pub fn label_es(&self) -> &'static str {
        match self {
            Self::Scene => "Escena",
            Self::Schematic => "Esquematico",
            Self::Config => "Configuracion",
            Self::Script => "Script",
            Self::Asset => "Recurso",
            Self::Project => "Proyecto",
            Self::Other => "Otro",
        }
    }

    /// Auto-detect category from file extension.
    pub fn from_extension(ext: &str) -> Self {
        match ext.to_lowercase().as_str() {
            "ron" => Self::Scene, // Could be scene or schematic - caller refines
            "lua" | "rhai" | "wasm" | "js" => Self::Script,
            "png" | "jpg" | "jpeg" | "bmp" | "svg" | "webp" => Self::Asset,
            "wav" | "ogg" | "mp3" => Self::Asset,
            "obj" | "gltf" | "glb" | "fbx" => Self::Asset,
            "toml" | "json" | "yaml" | "yml" => Self::Config,
            _ => Self::Other,
        }
    }
}

// ---------------------------------------------------------------------------
// Watched file entry
// ---------------------------------------------------------------------------

/// A single file being watched.
#[derive(Debug, Clone)]
struct WatchedFile {
    /// Full path to the file.
    path: PathBuf,
    /// Category for this file.
    category: WatchCategory,
    /// Last known modification time.
    last_modified: Option<SystemTime>,
    /// Whether this file has been detected as changed (pending reload).
    changed: bool,
}

// ---------------------------------------------------------------------------
// File change event
// ---------------------------------------------------------------------------

/// A reported file change.
#[derive(Debug, Clone)]
pub struct FileChange {
    /// Path of the changed file.
    pub path: PathBuf,
    /// What category of file changed.
    pub category: WatchCategory,
    /// Whether the file was created (didn't exist before).
    pub is_new: bool,
    /// Whether the file was deleted.
    pub is_deleted: bool,
}

// ---------------------------------------------------------------------------
// Hot reload config
// ---------------------------------------------------------------------------

/// Configuration for the hot reload system.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HotReloadConfig {
    /// Whether hot reload is enabled.
    pub enabled: bool,
    /// Poll interval in seconds (how often to check for changes).
    /// Higher = less CPU. Lower = faster detection.
    pub poll_interval_secs: f32,
    /// Whether to watch the entire project directory recursively.
    pub watch_recursive: bool,
    /// Maximum number of files to watch (prevents scanning huge folders).
    pub max_watched_files: usize,
    /// Whether to watch script/mod files.
    pub watch_scripts: bool,
    /// Whether to watch asset files.
    pub watch_assets: bool,
    /// Whether to auto-reload on change or just notify (user confirms).
    pub auto_reload: bool,
    /// Root directory to watch (project directory).
    #[serde(skip)]
    pub watch_root: PathBuf,
}

impl Default for HotReloadConfig {
    fn default() -> Self {
        Self {
            enabled: true, // On by default - this is expected behavior
            poll_interval_secs: 2.0, // Check every 2 seconds
            watch_recursive: true,
            max_watched_files: 500,
            watch_scripts: true,
            watch_assets: true,
            auto_reload: false, // Notify first, don't auto-reload by default
            watch_root: PathBuf::new(),
        }
    }
}

// ---------------------------------------------------------------------------
// Hot reload state (the watcher)
// ---------------------------------------------------------------------------

/// Runtime state for hot reload file watching.
/// Call `tick()` each frame with delta time. When `poll_interval` elapses,
/// it checks all watched files for changes.
#[derive(Debug, Clone)]
pub struct HotReloadState {
    /// Files being watched.
    watched: HashMap<PathBuf, WatchedFile>,
    /// Time since last poll.
    time_since_poll: f32,
    /// Changes detected since last drain.
    pending_changes: Vec<FileChange>,
    /// Total files scanned (for stats).
    pub total_scanned: usize,
    /// Total changes detected (lifetime).
    pub total_changes: usize,
    /// Whether a scan is needed (after adding/removing files).
    needs_scan: bool,
}

impl Default for HotReloadState {
    fn default() -> Self {
        Self {
            watched: HashMap::new(),
            time_since_poll: 0.0,
            pending_changes: Vec::new(),
            total_scanned: 0,
            total_changes: 0,
            needs_scan: true,
        }
    }
}

impl HotReloadState {
    /// Create a new empty watcher.
    pub fn new() -> Self {
        Self::default()
    }

    /// Register a specific file to watch.
    pub fn watch_file(&mut self, path: &Path, category: WatchCategory) {
        let last_modified = std::fs::metadata(path)
            .and_then(|m| m.modified())
            .ok();

        self.watched.insert(path.to_path_buf(), WatchedFile {
            path: path.to_path_buf(),
            category,
            last_modified,
            changed: false,
        });
    }

    /// Stop watching a file.
    pub fn unwatch_file(&mut self, path: &Path) {
        self.watched.remove(path);
    }

    /// Scan a directory and register all relevant files.
    pub fn scan_directory(&mut self, dir: &Path, config: &HotReloadConfig) {
        if !dir.exists() || !dir.is_dir() {
            return;
        }

        let entries = match std::fs::read_dir(dir) {
            Ok(e) => e,
            Err(_) => return,
        };

        for entry in entries.flatten() {
            if self.watched.len() >= config.max_watched_files {
                break;
            }

            let path = entry.path();

            if path.is_dir() && config.watch_recursive {
                // Skip hidden directories and target directories.
                let name = path.file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or("");
                if name.starts_with('.') || name == "target" || name == "target_gnu" {
                    continue;
                }
                self.scan_directory(&path, config);
            } else if path.is_file() {
                let ext = path.extension()
                    .and_then(|e| e.to_str())
                    .unwrap_or("");

                let category = WatchCategory::from_extension(ext);

                // Skip categories the user doesn't want to watch.
                match category {
                    WatchCategory::Script if !config.watch_scripts => continue,
                    WatchCategory::Asset if !config.watch_assets => continue,
                    WatchCategory::Other => continue, // Don't watch unknown files
                    _ => {}
                }

                self.watch_file(&path, category);
            }
        }

        self.needs_scan = false;
    }

    /// Advance the timer and poll for changes when interval elapses.
    /// Call this every frame with delta time (seconds).
    /// Returns the number of new changes detected.
    pub fn tick(&mut self, dt: f32, config: &HotReloadConfig) -> usize {
        if !config.enabled {
            return 0;
        }

        self.time_since_poll += dt;

        if self.time_since_poll < config.poll_interval_secs {
            return 0;
        }

        self.time_since_poll = 0.0;
        self.poll_changes()
    }

    /// Check all watched files for modifications.
    fn poll_changes(&mut self) -> usize {
        let mut new_changes = 0;
        let mut changes_to_add: Vec<FileChange> = Vec::new();

        for (_path, entry) in self.watched.iter_mut() {
            self.total_scanned += 1;

            let current_modified = std::fs::metadata(&entry.path)
                .and_then(|m| m.modified())
                .ok();

            match (entry.last_modified, current_modified) {
                // File exists and has been modified.
                (Some(old), Some(new)) if new > old => {
                    entry.last_modified = Some(new);
                    entry.changed = true;
                    changes_to_add.push(FileChange {
                        path: entry.path.clone(),
                        category: entry.category,
                        is_new: false,
                        is_deleted: false,
                    });
                    new_changes += 1;
                }
                // File appeared (was None, now exists).
                (None, Some(new)) => {
                    entry.last_modified = Some(new);
                    entry.changed = true;
                    changes_to_add.push(FileChange {
                        path: entry.path.clone(),
                        category: entry.category,
                        is_new: true,
                        is_deleted: false,
                    });
                    new_changes += 1;
                }
                // File disappeared (existed, now doesn't).
                (Some(_), None) => {
                    entry.last_modified = None;
                    entry.changed = true;
                    changes_to_add.push(FileChange {
                        path: entry.path.clone(),
                        category: entry.category,
                        is_new: false,
                        is_deleted: true,
                    });
                    new_changes += 1;
                }
                // No change.
                _ => {}
            }
        }

        self.total_changes += new_changes;
        self.pending_changes.extend(changes_to_add);
        new_changes
    }

    /// Get pending changes without consuming them (for UI preview).
    pub fn peek_changes(&self) -> &[FileChange] {
        &self.pending_changes
    }

    /// Consume and return all pending changes.
    /// The engine processes these and reloads the affected data.
    pub fn drain_changes(&mut self) -> Vec<FileChange> {
        // Reset changed flags.
        for entry in self.watched.values_mut() {
            entry.changed = false;
        }
        std::mem::take(&mut self.pending_changes)
    }

    /// Check if there are pending changes.
    pub fn has_changes(&self) -> bool {
        !self.pending_changes.is_empty()
    }

    /// Number of changes pending.
    pub fn pending_count(&self) -> usize {
        self.pending_changes.len()
    }

    /// Number of files being watched.
    pub fn watched_count(&self) -> usize {
        self.watched.len()
    }

    /// Clear all watched files.
    pub fn clear(&mut self) {
        self.watched.clear();
        self.pending_changes.clear();
        self.needs_scan = true;
    }

    /// Force a rescan of the watch directory on next tick.
    pub fn request_rescan(&mut self) {
        self.needs_scan = true;
    }

    /// Whether a directory scan is needed.
    pub fn needs_scan(&self) -> bool {
        self.needs_scan
    }

    /// Get change summary for status bar display.
    pub fn status_summary(&self) -> String {
        if self.pending_changes.is_empty() {
            return String::new();
        }
        let count = self.pending_changes.len();
        if count == 1 {
            let c = &self.pending_changes[0];
            let name = c.path.file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("?");
            format!("{} changed", name)
        } else {
            format!("{} files changed", count)
        }
    }

    /// Spanish version.
    pub fn status_summary_es(&self) -> String {
        if self.pending_changes.is_empty() {
            return String::new();
        }
        let count = self.pending_changes.len();
        if count == 1 {
            let c = &self.pending_changes[0];
            let name = c.path.file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("?");
            format!("{} modificado", name)
        } else {
            format!("{} archivos modificados", count)
        }
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn category_from_extension() {
        assert_eq!(WatchCategory::from_extension("lua"), WatchCategory::Script);
        assert_eq!(WatchCategory::from_extension("png"), WatchCategory::Asset);
        assert_eq!(WatchCategory::from_extension("ron"), WatchCategory::Scene);
        assert_eq!(WatchCategory::from_extension("xyz"), WatchCategory::Other);
    }

    #[test]
    fn watcher_disabled_does_nothing() {
        let mut state = HotReloadState::new();
        let mut config = HotReloadConfig::default();
        config.enabled = false;
        // Even with large dt, no changes detected.
        assert_eq!(state.tick(100.0, &config), 0);
    }

    #[test]
    fn watcher_detects_file_change() {
        let dir = std::env::temp_dir().join("auratest_hotreload");
        let _ = std::fs::create_dir_all(&dir);
        let test_file = dir.join("test_scene.ron");

        // Create initial file.
        {
            let mut f = std::fs::File::create(&test_file).unwrap();
            writeln!(f, "initial").unwrap();
        }

        let mut state = HotReloadState::new();
        state.watch_file(&test_file, WatchCategory::Scene);
        let config = HotReloadConfig::default();

        // First poll: no changes (just registered).
        state.time_since_poll = 10.0; // Force poll
        assert_eq!(state.tick(0.0, &config), 0);

        // Modify the file.
        std::thread::sleep(std::time::Duration::from_millis(50));
        {
            let mut f = std::fs::File::create(&test_file).unwrap();
            writeln!(f, "modified").unwrap();
        }

        // Second poll: should detect change.
        state.time_since_poll = 10.0;
        let changes = state.tick(0.0, &config);
        assert!(changes > 0);
        assert!(state.has_changes());

        // Drain changes.
        let drained = state.drain_changes();
        assert!(!drained.is_empty());
        assert_eq!(drained[0].category, WatchCategory::Scene);
        assert!(!drained[0].is_new);
        assert!(!drained[0].is_deleted);

        // Cleanup.
        let _ = std::fs::remove_file(&test_file);
        let _ = std::fs::remove_dir(&dir);
    }

    #[test]
    fn status_summary() {
        let mut state = HotReloadState::new();
        assert!(state.status_summary().is_empty());

        state.pending_changes.push(FileChange {
            path: PathBuf::from("scene.ron"),
            category: WatchCategory::Scene,
            is_new: false,
            is_deleted: false,
        });
        assert_eq!(state.status_summary(), "scene.ron changed");
        assert_eq!(state.status_summary_es(), "scene.ron modificado");
    }
}
