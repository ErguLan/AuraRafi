//! Bounded, project-confined image decoding for authored RafUI documents.

use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::fs;
use std::path::{Component, Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc::{self, Receiver, SyncSender, TryRecvError, TrySendError};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use raf_assets::{decode_image, image_output_dimensions, DecodedImage};

pub(super) const MAX_PROJECT_IMAGE_KEYS: usize = 256;
const REQUEST_QUEUE_CAPACITY: usize = 8;
const COMPLETION_QUEUE_CAPACITY: usize = 4;
const MAX_PROJECT_IMAGE_DIMENSION: u32 = 1024;
const PROJECT_IMAGE_RETRY_COOLDOWN: Duration = Duration::from_secs(2);

#[derive(Debug)]
pub(super) struct ProjectImageCompletion {
    pub key: String,
    pub result: Result<DecodedImage, ProjectImageFailure>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum ProjectImageFailure {
    Missing,
    Error,
}

struct DecodeRequest {
    key: String,
}

pub(super) struct ProjectImageResourceLoader {
    request_sender: SyncSender<DecodeRequest>,
    completion_receiver: Receiver<ProjectImageCompletion>,
    cancellation: Arc<AtomicBool>,
    queued: VecDeque<String>,
    in_flight: Arc<Mutex<BTreeSet<String>>>,
    last_requested: Mutex<BTreeMap<String, Instant>>,
    max_image_bytes: Arc<AtomicU64>,
    wake_gate: Arc<Mutex<bool>>,
}

impl std::fmt::Debug for ProjectImageResourceLoader {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("ProjectImageResourceLoader")
            .field("queued_count", &self.queued.len())
            .field(
                "in_flight_count",
                &self.in_flight.lock().map(|set| set.len()).ok(),
            )
            .finish()
    }
}

impl ProjectImageResourceLoader {
    pub(super) fn new(
        project_root: &Path,
        keys: Vec<String>,
        max_image_bytes: u64,
        wakeup: Option<Arc<dyn Fn() + Send + Sync>>,
    ) -> Result<Self, String> {
        let project_root = fs::canonicalize(project_root).map_err(|error| {
            format!(
                "Unable to resolve project root '{}': {error}",
                project_root.display()
            )
        })?;
        if !project_root.is_dir() {
            return Err(format!(
                "Project root '{}' is not a directory.",
                project_root.display()
            ));
        }

        let (request_sender, request_receiver) =
            mpsc::sync_channel::<DecodeRequest>(REQUEST_QUEUE_CAPACITY);
        let (completion_sender, completion_receiver) =
            mpsc::sync_channel::<ProjectImageCompletion>(COMPLETION_QUEUE_CAPACITY);
        let cancellation = Arc::new(AtomicBool::new(false));
        let worker_cancellation = cancellation.clone();
        let wake_gate = Arc::new(Mutex::new(false));
        let worker_wake_gate = wake_gate.clone();
        let decode_budget = Arc::new(AtomicU64::new(max_image_bytes));
        let worker_decode_budget = decode_budget.clone();
        std::thread::Builder::new()
            .name("raf-ui-image-loader".into())
            .spawn(move || {
                while !worker_cancellation.load(Ordering::Acquire) {
                    let request = match request_receiver.recv_timeout(Duration::from_millis(50)) {
                        Ok(request) => request,
                        Err(mpsc::RecvTimeoutError::Timeout) => continue,
                        Err(mpsc::RecvTimeoutError::Disconnected) => break,
                    };
                    if worker_cancellation.load(Ordering::Acquire) {
                        break;
                    }
                    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                        decode_project_image(
                            &project_root,
                            &request.key,
                            worker_decode_budget.load(Ordering::Acquire),
                        )
                    }))
                    .unwrap_or(Err(ProjectImageFailure::Error));
                    if worker_cancellation.load(Ordering::Acquire) {
                        break;
                    }
                    if completion_sender
                        .send(ProjectImageCompletion {
                            key: request.key,
                            result,
                        })
                        .is_err()
                    {
                        break;
                    }
                    if let Some(wakeup) = &wakeup {
                        if let Ok(cancelled) = worker_wake_gate.lock() {
                            if !*cancelled && !worker_cancellation.load(Ordering::Acquire) {
                                wakeup();
                            }
                        }
                    }
                }
            })
            .map_err(|error| format!("Unable to start image loader worker: {error}"))?;

        Ok(Self {
            request_sender,
            completion_receiver,
            cancellation,
            queued: keys.into(),
            in_flight: Arc::new(Mutex::new(BTreeSet::new())),
            last_requested: Mutex::new(BTreeMap::new()),
            max_image_bytes: decode_budget,
            wake_gate,
        })
    }

    pub(super) fn dispatch_queued(&mut self) {
        while let Some(key) = self.queued.front().cloned() {
            match self
                .request_sender
                .try_send(DecodeRequest { key: key.clone() })
            {
                Ok(()) => {
                    self.queued.pop_front();
                    self.mark_in_flight(key);
                }
                Err(TrySendError::Full(_)) => break,
                Err(TrySendError::Disconnected(_)) => {
                    self.queued.clear();
                    break;
                }
            }
        }
    }

    pub(super) fn try_completion(&mut self) -> Option<ProjectImageCompletion> {
        let completion = match self.completion_receiver.try_recv() {
            Ok(completion) => completion,
            Err(TryRecvError::Empty | TryRecvError::Disconnected) => return None,
        };
        if let Ok(mut in_flight) = self.in_flight.lock() {
            in_flight.remove(&completion.key);
        }
        Some(completion)
    }

    pub(super) fn request_project_image(&self, key: &str) -> bool {
        let Ok(mut in_flight) = self.in_flight.lock() else {
            return false;
        };
        if in_flight.contains(key) {
            return false;
        }
        let now = Instant::now();
        let Ok(mut last_requested) = self.last_requested.lock() else {
            return false;
        };
        if last_requested
            .get(key)
            .is_some_and(|last| now.duration_since(*last) < PROJECT_IMAGE_RETRY_COOLDOWN)
        {
            return false;
        }
        match self.request_sender.try_send(DecodeRequest {
            key: key.to_string(),
        }) {
            Ok(()) => {
                in_flight.insert(key.to_string());
                last_requested.insert(key.to_string(), now);
                true
            }
            Err(TrySendError::Full(_) | TrySendError::Disconnected(_)) => false,
        }
    }

    pub(super) fn is_pending(&self, key: &str) -> bool {
        self.queued.iter().any(|queued| queued == key)
            || self
                .in_flight
                .lock()
                .is_ok_and(|in_flight| in_flight.contains(key))
    }

    pub(super) fn set_max_image_bytes(&self, max_image_bytes: u64) {
        self.max_image_bytes
            .store(max_image_bytes, Ordering::Release);
    }

    fn mark_in_flight(&self, key: String) {
        if let Ok(mut in_flight) = self.in_flight.lock() {
            in_flight.insert(key.clone());
        }
        if let Ok(mut last_requested) = self.last_requested.lock() {
            last_requested.insert(key, Instant::now());
        }
    }
}

impl Drop for ProjectImageResourceLoader {
    fn drop(&mut self) {
        if let Ok(mut cancelled) = self.wake_gate.lock() {
            *cancelled = true;
        }
        self.cancellation.store(true, Ordering::Release);
    }
}

/// Returns `None` for host-owned namespaces and non-file symbolic keys. A
/// recognized project key returns a path or a validation error, so malformed
/// paths report an error state instead of silently becoming a host resource.
pub(super) fn project_image_relative_path(key: &str) -> Option<Result<PathBuf, String>> {
    let raw_path = if let Some(path) = key.strip_prefix("project://") {
        path.to_string()
    } else if let Some(path) = key.strip_prefix("assets://") {
        format!("assets/{path}")
    } else {
        if key.starts_with("builtin://") || key.contains("://") {
            return None;
        }
        if !key.contains('/') && !key.contains('\\') && Path::new(key).extension().is_none() {
            return None;
        }
        key.to_string()
    };

    let portable_path = raw_path.replace('\\', "/");
    if portable_path.trim().is_empty() || portable_path.contains(':') {
        return Some(Err("Project image keys must be relative paths.".into()));
    }
    let path = PathBuf::from(portable_path);
    if path.is_absolute()
        || path.components().any(|component| {
            matches!(
                component,
                Component::Prefix(_) | Component::RootDir | Component::ParentDir
            )
        })
    {
        return Some(Err(
            "Project image key escapes the project root or is not relative.".into(),
        ));
    }
    Some(Ok(path))
}

fn decode_project_image(
    project_root: &Path,
    key: &str,
    max_image_bytes: u64,
) -> Result<DecodedImage, ProjectImageFailure> {
    let canonical_path = canonical_project_image_path(project_root, key)?;

    let max_dimension = ((max_image_bytes / 4) as f64).sqrt().floor() as u32;
    let max_dimension = max_dimension.clamp(1, MAX_PROJECT_IMAGE_DIMENSION);
    let (width, height) = image_output_dimensions(&canonical_path, max_dimension)
        .map_err(|_| ProjectImageFailure::Error)?;
    let decoded_bytes = u64::from(width)
        .saturating_mul(u64::from(height))
        .saturating_mul(4);
    if decoded_bytes == 0 || decoded_bytes > max_image_bytes {
        return Err(ProjectImageFailure::Error);
    }
    decode_image(&canonical_path, max_dimension).map_err(|_| ProjectImageFailure::Error)
}

fn canonical_project_image_path(
    project_root: &Path,
    key: &str,
) -> Result<PathBuf, ProjectImageFailure> {
    let relative_path = project_image_relative_path(key)
        .and_then(Result::ok)
        .ok_or(ProjectImageFailure::Error)?;
    let requested_path = project_root.join(relative_path);
    let canonical_path = fs::canonicalize(&requested_path).map_err(|error| {
        if error.kind() == std::io::ErrorKind::NotFound {
            ProjectImageFailure::Missing
        } else {
            ProjectImageFailure::Error
        }
    })?;
    if !canonical_path.starts_with(project_root) {
        return Err(ProjectImageFailure::Error);
    }
    if !canonical_path.is_file() {
        return Err(ProjectImageFailure::Missing);
    }
    Ok(canonical_path)
}

#[cfg(test)]
mod tests {
    use super::{canonical_project_image_path, project_image_relative_path, ProjectImageFailure};
    use std::fs;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn project_image_keys_are_portable_and_confined() {
        assert_eq!(
            project_image_relative_path("project://assets/ui/logo.png")
                .and_then(Result::ok)
                .unwrap()
                .to_string_lossy(),
            "assets/ui/logo.png"
        );
        assert_eq!(
            project_image_relative_path("assets://ui/logo.png")
                .and_then(Result::ok)
                .unwrap()
                .to_string_lossy(),
            "assets/ui/logo.png"
        );
        assert!(project_image_relative_path("../outside.png")
            .unwrap()
            .is_err());
        assert!(project_image_relative_path("project://C:/outside.png")
            .unwrap()
            .is_err());
        assert!(project_image_relative_path("builtin://icon/select").is_none());
    }

    #[cfg(unix)]
    #[test]
    fn project_image_symlinks_cannot_escape_the_root() {
        use std::os::unix::fs::symlink;

        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let temporary = std::env::temp_dir().join(format!("raf-ui-image-{unique}"));
        let project = temporary.join("project");
        let outside = temporary.join("outside.png");
        fs::create_dir_all(&project).unwrap();
        fs::write(&outside, b"not an image").unwrap();
        symlink(&outside, project.join("link.png")).unwrap();
        let canonical_project = fs::canonicalize(&project).unwrap();

        assert_eq!(
            canonical_project_image_path(&canonical_project, "link.png"),
            Err(ProjectImageFailure::Error)
        );

        fs::remove_dir_all(&temporary).unwrap();
    }
}
