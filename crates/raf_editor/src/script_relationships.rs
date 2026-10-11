//! Optional, event-driven script reference index. Scene children are never modified.
use notify::{RecursiveMode, Watcher};
use raf_core::scene::{SceneGraph, SceneNodeId};
use raf_script::reference_analysis::{analyze_rhai, SceneReference};
use std::collections::HashMap;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver, SyncSender};
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc, Mutex,
};
use std::thread;
use std::time::Duration;
use winit::event_loop::EventLoopProxy;

const MAX_ATTACHMENTS: usize = 1024;
const MAX_FILES: usize = 256;
const MAX_SOURCE_BYTES: usize = 256 * 1024;
const MAX_TOTAL_BYTES: usize = 8 * 1024 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScriptLinkKind {
    Attached,
    Reference,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScriptLink {
    pub path: String,
    pub kind: ScriptLinkKind,
}
pub type ScriptLinks = HashMap<SceneNodeId, Vec<ScriptLink>>;

struct ScanResult {
    sources: HashMap<String, Vec<SceneReference>>,
    diagnostics: Vec<String>,
}
struct Worker {
    sender: SyncSender<()>,
    paths: Arc<Mutex<Vec<String>>>,
    result: Arc<Mutex<Option<ScanResult>>>,
    stop: Arc<AtomicBool>,
}
impl Drop for Worker {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Release);
        let _ = self.sender.try_send(());
    }
}

#[derive(Default)]
pub struct ScriptRelationshipIndex {
    root: Option<PathBuf>,
    worker: Option<Worker>,
    wakeup: Option<EventLoopProxy<()>>,
    paths: Vec<String>,
    sources: HashMap<String, Vec<SceneReference>>,
    revision: Option<u64>,
    pub diagnostics: Vec<String>,
}

impl ScriptRelationshipIndex {
    pub fn set_wakeup(&mut self, wakeup: Option<EventLoopProxy<()>>) {
        self.wakeup = wakeup;
    }

    /// Returns a fresh projection only on scene or source invalidation. Disabled is zero work.
    pub fn sync(
        &mut self,
        enabled: bool,
        root: Option<&Path>,
        scene: &SceneGraph,
    ) -> Option<ScriptLinks> {
        let root = enabled.then_some(root).flatten().map(Path::to_path_buf);
        if root != self.root {
            self.worker = None;
            self.root = root.clone();
            self.paths.clear();
            self.sources.clear();
            self.diagnostics.clear();
            self.revision = None;
            if let Some(root) = root {
                let (sender, requests) = mpsc::sync_channel(1);
                let state = Worker {
                    sender,
                    paths: Arc::new(Mutex::new(Vec::new())),
                    result: Arc::new(Mutex::new(None)),
                    stop: Arc::new(AtomicBool::new(false)),
                };
                let changed = state.sender.clone();
                let paths = state.paths.clone();
                let result = state.result.clone();
                let stop = state.stop.clone();
                let wakeup = self.wakeup.clone();
                match thread::Builder::new()
                    .name("raf-script-links".into())
                    .spawn(move || worker(root, requests, changed, paths, result, stop, wakeup))
                {
                    Ok(_) => self.worker = Some(state),
                    Err(error) => self
                        .diagnostics
                        .push(format!("Script index worker unavailable: {error}")),
                }
            }
            if self.root.is_none() {
                return Some(ScriptLinks::new());
            }
        }
        if self.root.is_none() {
            return None;
        }
        let mut changed = self.revision != Some(scene.document_revision());
        if let Some(worker) = &self.worker {
            // Single-slot publication and coalesced notifications bound both queues.
            if let Some(result) = worker.result.lock().ok().and_then(|mut slot| slot.take()) {
                self.sources = result.sources;
                self.diagnostics = result.diagnostics;
                changed = true;
            }
        }
        if !changed {
            return None;
        }
        self.revision = Some(scene.document_revision());
        let mut paths: Vec<String> = scene
            .iter()
            .filter(|(id, _)| scene.is_valid_node(*id))
            .flat_map(|(_, n)| n.scripts.iter().cloned())
            .take(MAX_ATTACHMENTS)
            .collect();
        paths.sort();
        paths.dedup();
        if paths.len() > MAX_FILES {
            self.diagnostics.push(format!("Script reference index is limited to {MAX_FILES} source files; remaining attachments keep their Attached rows"));
        }
        paths.truncate(MAX_FILES);
        if paths != self.paths {
            self.paths = paths;
            // Old source results may contain detached paths; projection uses current attachments only.
            if let Some(worker) = &self.worker {
                if let Ok(mut paths) = worker.paths.lock() {
                    *paths = self.paths.clone();
                }
                let _ = worker.sender.try_send(());
            }
        }
        let mut links = ScriptLinks::new();
        let lookup = raf_script::references::SceneLookup::new(scene);
        let mut attached = 0;
        let mut total_links = 0;
        for (owner, node) in scene.iter().filter(|(id, _)| scene.is_valid_node(*id)) {
            for path in &node.scripts {
                if attached >= MAX_ATTACHMENTS {
                    break;
                }
                attached += 1;
                insert(&mut links, owner, path, ScriptLinkKind::Attached);
                total_links += 1;
                if total_links >= 8192 {
                    break;
                }
                if let Some(refs) = self.sources.get(path) {
                    for reference in refs {
                        if total_links >= 8192 {
                            break;
                        }
                        if let Some(target) =
                            reference.resolve_with_source(&lookup, owner, Some(path))
                        {
                            if target != owner {
                                insert(&mut links, target, path, ScriptLinkKind::Reference);
                                total_links += 1;
                            }
                        }
                    }
                }
            }
            if attached >= MAX_ATTACHMENTS || total_links >= 8192 {
                break;
            }
        }
        Some(links)
    }

    pub fn resolve_open_path(&self, path: &str) -> Result<PathBuf, String> {
        let path = raf_script::runtime::resolve_script_path(self.root.as_deref(), path)?;
        // The existing Windows launcher has a shell fallback. Do not pass shell
        // metacharacters from user-authored paths into that fallback.
        #[cfg(target_os = "windows")]
        if path
            .to_string_lossy()
            .chars()
            .any(|c| matches!(c, '&' | '|' | '<' | '>' | '^' | '%' | '"' | '\n' | '\r'))
        {
            return Err("Linked script path contains unsupported shell metacharacters; open it directly in your editor".into());
        }
        Ok(path)
    }
}

fn insert(links: &mut ScriptLinks, id: SceneNodeId, path: &str, kind: ScriptLinkKind) {
    let rows = links.entry(id).or_default();
    if let Some(row) = rows.iter_mut().find(|row| row.path == path) {
        if kind == ScriptLinkKind::Attached {
            row.kind = kind;
        }
    } else {
        rows.push(ScriptLink {
            path: path.into(),
            kind,
        });
    }
}

fn worker(
    root: PathBuf,
    requests: Receiver<()>,
    changed: SyncSender<()>,
    paths: Arc<Mutex<Vec<String>>>,
    results: Arc<Mutex<Option<ScanResult>>>,
    stop: Arc<AtomicBool>,
    wakeup: Option<EventLoopProxy<()>>,
) {
    let mut watcher = notify::recommended_watcher(move |event: notify::Result<notify::Event>| {
        if event.as_ref().map_or(true, |event| {
            !matches!(event.kind, notify::EventKind::Access(_))
                && (event.paths.iter().any(|p| {
                    p.extension()
                        .is_some_and(|e| e.eq_ignore_ascii_case("rhai"))
                }) || matches!(
                    event.kind,
                    notify::EventKind::Modify(notify::event::ModifyKind::Name(_))
                        | notify::EventKind::Remove(_)
                ))
        }) {
            let _ = changed.try_send(());
        }
    });
    let watch_error = match &mut watcher {
        Ok(watcher) => watcher
            .watch(&root, RecursiveMode::Recursive)
            .err()
            .map(|e| e.to_string()),
        Err(error) => Some(error.to_string()),
    };
    while requests.recv().is_ok() {
        if stop.load(Ordering::Acquire) {
            break;
        }
        // One bounded debounce window; saves/atomic renames are batched without compiling.
        let until = std::time::Instant::now() + Duration::from_millis(150);
        while requests
            .recv_timeout(until.saturating_duration_since(std::time::Instant::now()))
            .is_ok()
        {
            if stop.load(Ordering::Acquire) {
                return;
            }
            if std::time::Instant::now() >= until {
                break;
            }
        }
        let mut result = ScanResult {
            sources: HashMap::new(),
            diagnostics: Vec::new(),
        };
        if let Some(error) = &watch_error {
            result.diagnostics.push(format!("Live script watching unavailable; attachment/scene refresh remains available: {error}"));
        }
        let paths = paths.lock().map(|p| p.clone()).unwrap_or_default();
        let mut bytes = 0;
        let mut cache: HashMap<PathBuf, Vec<SceneReference>> = HashMap::new();
        for path in paths.iter().take(MAX_FILES) {
            if stop.load(Ordering::Acquire) {
                return;
            }
            if !path.to_ascii_lowercase().ends_with(".rhai") {
                continue;
            }
            let source = (|| -> Result<Vec<SceneReference>, String> {
                let file = raf_script::runtime::resolve_script_path(Some(&root), path)?;
                if let Some(references) = cache.get(&file) {
                    return Ok(references.clone());
                }
                let mut source = String::new();
                std::fs::File::open(&file)
                    .map_err(|e| e.to_string())?
                    .take(MAX_SOURCE_BYTES as u64 + 1)
                    .read_to_string(&mut source)
                    .map_err(|e| e.to_string())?;
                bytes += source.len();
                if source.len() > MAX_SOURCE_BYTES || bytes > MAX_TOTAL_BYTES {
                    return Err("script reference scan byte budget exceeded".into());
                }
                let refs = analyze_rhai(&source);
                cache.insert(file, refs.clone());
                Ok(refs)
            })();
            match source {
                Ok(references) => {
                    result.sources.insert(path.clone(), references);
                }
                Err(error) => {
                    if result.diagnostics.len() < 64 {
                        result.diagnostics.push(format!("{path}: {error}"));
                    }
                }
            }
            if bytes >= MAX_TOTAL_BYTES {
                break;
            }
        }
        if stop.load(Ordering::Acquire) {
            break;
        }
        if let Ok(mut slot) = results.lock() {
            *slot = Some(result);
        }
        if let Some(proxy) = &wakeup {
            let _ = proxy.send_event(());
        }
    }
}
