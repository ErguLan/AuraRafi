//! Worker-backed project and asset discovery for the Game workbench.
//!
//! The editor used to enumerate project folders from the UI frame path. This
//! catalog keeps discovery off the UI thread and publishes only immutable
//! display rows back to the application boundary.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{self, Receiver, TryRecvError};
use std::sync::Arc;
use std::thread;

use raf_ui::UiIconId;

use crate::panels::project_surface::ProjectTreeEntry;

const MAX_ASSET_ROWS: usize = 2_048;
const MAX_PROJECT_ROWS: usize = 4_096;
const MAX_SCAN_DEPTH: u8 = 24;

#[derive(Debug)]
struct CatalogResult {
    generation: u64,
    root: PathBuf,
    assets: Vec<String>,
    project_entries: Vec<ProjectTreeEntry>,
    import_report: Option<(usize, usize)>,
    error: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ProjectCatalogResults {
    pub assets: Vec<String>,
    pub project_entries: Vec<ProjectTreeEntry>,
}

/// Retained display catalog owned by the editor application.
///
/// `sync_project` only schedules work when the active project changes. The
/// worker result is accepted only for the current generation, so a late scan
/// from a previous project can never replace current rows.
#[derive(Debug)]
pub struct ProjectCatalog {
    active_root: Option<PathBuf>,
    generation: u64,
    receiver: Option<Receiver<CatalogResult>>,
    worker_generation: Arc<AtomicU64>,
    assets: Vec<String>,
    project_entries: Vec<ProjectTreeEntry>,
    pending: bool,
    revision: u64,
    import_report: Option<(usize, usize)>,
    error: Option<String>,
}

impl Default for ProjectCatalog {
    fn default() -> Self {
        Self {
            active_root: None,
            generation: 0,
            receiver: None,
            worker_generation: Arc::new(AtomicU64::new(0)),
            assets: Vec::new(),
            project_entries: Vec::new(),
            pending: false,
            revision: 0,
            import_report: None,
            error: None,
        }
    }
}

impl ProjectCatalog {
    /// Schedule a scan for the active project if its root changed.
    pub fn sync_project(&mut self, project_root: Option<&Path>) {
        let next_root = project_root.map(Path::to_path_buf);
        if self.active_root == next_root {
            return;
        }

        self.active_root = next_root.clone();
        self.generation = self.generation.wrapping_add(1);
        self.worker_generation
            .store(self.generation, Ordering::Release);
        self.receiver = None;
        self.assets.clear();
        self.project_entries.clear();
        self.pending = false;
        self.import_report = None;
        self.error = None;
        self.revision = self.revision.wrapping_add(1);

        let Some(root) = next_root else {
            return;
        };

        let generation = self.generation;
        let worker_generation = Arc::clone(&self.worker_generation);
        let (sender, receiver) = mpsc::channel();
        self.receiver = Some(receiver);
        self.pending = true;

        let spawn = thread::Builder::new()
            .name("raf-project-catalog".to_string())
            .spawn(move || {
                let result = discover_catalog(generation, root, None, &worker_generation);
                let _ = sender.send(result);
            });
        if let Err(error) = spawn {
            self.receiver = None;
            self.pending = false;
            self.error = Some(format!("Unable to start project catalog worker: {error}"));
            self.revision = self.revision.wrapping_add(1);
        }
    }

    /// Poll completed worker output without blocking the frame.
    pub fn poll(&mut self) -> bool {
        let mut changed = false;
        let Some(receiver) = self.receiver.as_ref() else {
            return false;
        };

        loop {
            match receiver.try_recv() {
                Ok(result) => {
                    if result.generation == self.generation
                        && self.active_root.as_ref() == Some(&result.root)
                    {
                        self.assets = result.assets;
                        self.project_entries = result.project_entries;
                        self.pending = false;
                        self.import_report = result.import_report;
                        self.error = result.error;
                        self.revision = self.revision.wrapping_add(1);
                        changed = true;
                        // The sender normally disconnects immediately after
                        // publishing this one-shot result. Stop polling now so
                        // that expected disconnect cannot overwrite a valid
                        // catalog with a false worker failure.
                        break;
                    }
                }
                Err(TryRecvError::Empty) => break,
                Err(TryRecvError::Disconnected) => {
                    if self.pending {
                        self.pending = false;
                        self.error = Some(
                            "Project catalog worker stopped before publishing results.".to_string(),
                        );
                        self.revision = self.revision.wrapping_add(1);
                        changed = true;
                    }
                    break;
                }
            }
        }

        if !self.pending {
            self.receiver = None;
        }
        changed
    }

    /// Re-scan the current project without blocking the editor frame.
    pub fn refresh(&mut self) {
        let Some(root) = self.active_root.clone() else {
            return;
        };
        self.active_root = None;
        self.sync_project(Some(&root));
    }

    /// Copies external files into the project's assets folder on the catalog
    /// worker, then publishes a fresh catalog snapshot. The UI thread never
    /// performs the copy or directory scan.
    pub fn import_external_files(&mut self, files: Vec<PathBuf>) {
        let Some(root) = self.active_root.clone() else {
            return;
        };
        self.generation = self.generation.wrapping_add(1);
        self.worker_generation
            .store(self.generation, Ordering::Release);
        let generation = self.generation;
        let worker_generation = Arc::clone(&self.worker_generation);
        self.receiver = None;
        self.pending = true;
        self.error = None;
        self.import_report = None;
        self.revision = self.revision.wrapping_add(1);
        let (sender, receiver) = mpsc::channel();
        self.receiver = Some(receiver);
        let spawn = thread::Builder::new()
            .name("raf-project-asset-import".to_string())
            .spawn(move || {
                let (imported, skipped, cancelled, import_error) =
                    import_files(&root, &files, generation, &worker_generation);
                if cancelled {
                    return;
                }
                let mut result = discover_catalog(
                    generation,
                    root,
                    Some((imported, skipped)),
                    &worker_generation,
                );
                if result.error.is_none() {
                    result.error = import_error;
                }
                let _ = sender.send(result);
            });
        if let Err(error) = spawn {
            self.receiver = None;
            self.pending = false;
            self.error = Some(format!("Unable to start asset import worker: {error}"));
            self.revision = self.revision.wrapping_add(1);
        }
    }

    /// Takes the last completed import report, if any.
    pub fn take_import_report(&mut self) -> Option<(usize, usize)> {
        self.import_report.take()
    }

    pub fn is_pending(&self) -> bool {
        self.pending
    }

    pub fn revision(&self) -> u64 {
        self.revision
    }

    pub fn error(&self) -> Option<&str> {
        self.error.as_deref()
    }

    pub fn is_ready(&self) -> bool {
        !self.pending && self.error.is_none()
    }

    pub fn assets(&self) -> &[String] {
        &self.assets
    }

    pub fn project_entries(&self) -> &[ProjectTreeEntry] {
        &self.project_entries
    }

    /// Queries the immutable catalog snapshot without touching the filesystem.
    /// While a worker is pending, this returns the last published rows.
    pub fn search_results(&self, query: &str) -> ProjectCatalogResults {
        let query = query.trim().to_ascii_lowercase();
        let matches = |value: &str| query.is_empty() || value.to_ascii_lowercase().contains(&query);
        ProjectCatalogResults {
            assets: self
                .assets
                .iter()
                .filter(|asset| matches(asset))
                .cloned()
                .collect(),
            project_entries: self
                .project_entries
                .iter()
                .filter(|entry| matches(&entry.label))
                .cloned()
                .collect(),
        }
    }
}

fn discover_catalog(
    generation: u64,
    root: PathBuf,
    import_report: Option<(usize, usize)>,
    worker_generation: &AtomicU64,
) -> CatalogResult {
    let error = match std::fs::metadata(&root) {
        Ok(metadata) if metadata.is_dir() => None,
        Ok(_) => Some(format!(
            "Project path is not a directory: {}",
            root.display()
        )),
        Err(error) => Some(format!(
            "Unable to read project path {}: {error}",
            root.display()
        )),
    };
    if error.is_some() || worker_generation.load(Ordering::Acquire) != generation {
        return CatalogResult {
            generation,
            root,
            assets: Vec::new(),
            project_entries: Vec::new(),
            import_report,
            error,
        };
    }
    CatalogResult {
        generation,
        root: root.clone(),
        assets: scan_assets(&root),
        project_entries: scan_project_entries(&root),
        import_report,
        error: None,
    }
}

fn scan_assets(root: &Path) -> Vec<String> {
    let assets_root = root.join("assets");
    let mut rows = Vec::new();
    collect_asset_files(&assets_root, &assets_root, 0, &mut rows);
    rows.sort_unstable_by_key(|row| row.to_ascii_lowercase());
    rows.truncate(MAX_ASSET_ROWS);
    rows
}

fn collect_asset_files(root: &Path, current: &Path, depth: u8, rows: &mut Vec<String>) {
    if depth > MAX_SCAN_DEPTH || rows.len() >= MAX_ASSET_ROWS {
        return;
    }
    let Ok(mut entries) =
        std::fs::read_dir(current).map(|entries| entries.flatten().collect::<Vec<_>>())
    else {
        return;
    };
    entries.sort_by_key(|entry| entry.file_name().to_string_lossy().to_ascii_lowercase());

    for entry in entries {
        if rows.len() >= MAX_ASSET_ROWS {
            return;
        }
        let Ok(file_type) = entry.file_type() else {
            continue;
        };
        let name = entry.file_name().to_string_lossy().to_string();
        if should_skip_name(&name) {
            continue;
        }
        if file_type.is_dir() {
            collect_asset_files(root, &entry.path(), depth.saturating_add(1), rows);
        } else if file_type.is_file() {
            let entry_path = entry.path();
            let Ok(relative) = entry_path.strip_prefix(root) else {
                continue;
            };
            let relative = relative.to_string_lossy().replace('\\', "/");
            rows.push(relative);
        }
    }
}

fn scan_project_entries(root: &Path) -> Vec<ProjectTreeEntry> {
    let mut rows = Vec::new();
    collect_project_entries(root, root, 0, &mut rows);
    rows.truncate(MAX_PROJECT_ROWS);
    rows
}

fn collect_project_entries(
    root: &Path,
    current: &Path,
    depth: u8,
    rows: &mut Vec<ProjectTreeEntry>,
) {
    if depth > MAX_SCAN_DEPTH || rows.len() >= MAX_PROJECT_ROWS {
        return;
    }
    let Ok(mut entries) =
        std::fs::read_dir(current).map(|entries| entries.flatten().collect::<Vec<_>>())
    else {
        return;
    };
    entries.sort_by_key(|entry| {
        (
            !entry.file_type().map(|kind| kind.is_dir()).unwrap_or(false),
            entry.file_name().to_string_lossy().to_ascii_lowercase(),
        )
    });

    for entry in entries {
        if rows.len() >= MAX_PROJECT_ROWS {
            return;
        }
        let Ok(file_type) = entry.file_type() else {
            continue;
        };
        let name = entry.file_name().to_string_lossy().to_string();
        if should_skip_name(&name) || name.eq_ignore_ascii_case("agent_history.ron") {
            continue;
        }
        let entry_path = entry.path();
        let Ok(relative) = entry_path.strip_prefix(root) else {
            continue;
        };
        let relative = relative.to_string_lossy().replace('\\', "/");
        rows.push(ProjectTreeEntry {
            label: relative,
            icon: if file_type.is_dir() {
                UiIconId::Folder
            } else {
                project_file_icon(&name)
            },
            depth,
        });
        if file_type.is_dir() {
            collect_project_entries(root, &entry.path(), depth.saturating_add(1), rows);
        }
    }
}

fn import_files(
    root: &Path,
    files: &[PathBuf],
    generation: u64,
    worker_generation: &AtomicU64,
) -> (usize, usize, bool, Option<String>) {
    let assets_root = root.join("assets");
    if worker_generation.load(Ordering::Acquire) != generation {
        return (0, files.len(), true, None);
    }
    if let Err(error) = std::fs::create_dir_all(&assets_root) {
        return (
            0,
            files.len(),
            false,
            Some(format!(
                "Unable to create project assets directory: {error}"
            )),
        );
    }

    let mut imported = 0;
    let mut skipped = 0;
    for source in files {
        if worker_generation.load(Ordering::Acquire) != generation {
            return (imported, skipped, true, None);
        }
        let Ok(metadata) = std::fs::metadata(source) else {
            skipped += 1;
            continue;
        };
        if !metadata.is_file() {
            skipped += 1;
            continue;
        }
        let Some(file_name) = source.file_name().and_then(|name| name.to_str()) else {
            skipped += 1;
            continue;
        };
        if should_skip_name(file_name) {
            skipped += 1;
            continue;
        }
        let destination = unique_asset_destination(&assets_root, file_name);
        if worker_generation.load(Ordering::Acquire) != generation {
            return (imported, skipped, true, None);
        }
        if std::fs::copy(source, destination).is_ok() {
            imported += 1;
        } else {
            skipped += 1;
        }
    }
    (imported, skipped, false, None)
}

fn unique_asset_destination(root: &Path, file_name: &str) -> PathBuf {
    let first = root.join(file_name);
    if !first.exists() {
        return first;
    }
    let path = Path::new(file_name);
    let stem = path
        .file_stem()
        .and_then(|value| value.to_str())
        .unwrap_or("asset");
    let extension = path.extension().and_then(|value| value.to_str());
    for index in 1..=10_000 {
        let candidate_name = extension
            .map(|extension| format!("{stem}_{index}.{extension}"))
            .unwrap_or_else(|| format!("{stem}_{index}"));
        let candidate = root.join(candidate_name);
        if !candidate.exists() {
            return candidate;
        }
    }
    root.join(format!("{stem}_imported"))
}

fn should_skip_name(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    matches!(
        lower.as_str(),
        ".git" | ".aura_rafi" | "target" | "cache" | "caches" | ".cache"
    ) || (name.starts_with('.') && lower != ".ai")
}

fn project_file_icon(name: &str) -> UiIconId {
    match Path::new(name)
        .extension()
        .and_then(|extension| extension.to_str())
        .map(|extension| extension.to_ascii_lowercase())
        .as_deref()
    {
        Some("ron") | Some("scene") => UiIconId::Scene,
        Some("rhai") | Some("rs") | Some("lua") | Some("cpp") => UiIconId::Node,
        // The current semantic icon registry does not yet expose dedicated
        // image/audio IDs. Keep the classification truthful without inventing
        // a second renderer asset path; the icon family can be extended in the
        // dedicated icon phase.
        Some("png") | Some("jpg") | Some("jpeg") | Some("webp") | Some("svg") => UiIconId::Assets,
        Some("wav") | Some("mp3") | Some("ogg") => UiIconId::Project,
        _ => UiIconId::Project,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hidden_engine_directories_are_not_project_rows() {
        assert!(should_skip_name(".git"));
        assert!(should_skip_name("target"));
        assert!(!should_skip_name(".ai"));
    }

    #[test]
    fn file_icons_are_semantic() {
        assert_eq!(project_file_icon("main.ron"), UiIconId::Scene);
        assert_eq!(project_file_icon("icon.png"), UiIconId::Assets);
        assert_eq!(project_file_icon("script.rs"), UiIconId::Node);
    }

    #[test]
    fn published_result_is_not_overwritten_by_expected_disconnect() {
        let root = PathBuf::from("catalog-test");
        let (sender, receiver) = mpsc::channel();
        sender
            .send(CatalogResult {
                generation: 7,
                root: root.clone(),
                assets: vec!["models/shelf.glb".to_string()],
                project_entries: Vec::new(),
                import_report: None,
                error: None,
            })
            .unwrap();
        drop(sender);
        let mut catalog = ProjectCatalog {
            active_root: Some(root),
            generation: 7,
            receiver: Some(receiver),
            pending: true,
            ..ProjectCatalog::default()
        };

        assert!(catalog.poll());
        assert_eq!(catalog.assets(), &["models/shelf.glb"]);
        assert!(catalog.error().is_none());
        assert!(!catalog.is_pending());
    }
}
