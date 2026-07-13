//! On-demand Python worker for generated image assets.
//!
//! The worker is intentionally isolated from the editor frame loop. Rust owns
//! project paths, job state, validation, and metadata contracts; Python only
//! performs the provider request when an explicit generation command starts it.

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

fn default_model() -> String {
    "gpt-image-2".to_string()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AssetImageSize {
    Square,
    Landscape,
    Portrait,
}

impl AssetImageSize {
    pub fn provider_value(self) -> &'static str {
        match self {
            Self::Square => "1024x1024",
            Self::Landscape => "1536x1024",
            Self::Portrait => "1024x1536",
        }
    }
}

impl Default for AssetImageSize {
    fn default() -> Self {
        Self::Square
    }
}

/// Selects whether an asset comes from the remote image model or the local
/// deterministic PNG generator.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AssetImageGenerationMode {
    Remote,
    LocalPng,
}

impl Default for AssetImageGenerationMode {
    fn default() -> Self {
        Self::Remote
    }
}

/// Local PNG presets are intentionally simple and cheap. They are useful for
/// editor icons, placeholder sprites, badges, and reference textures.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AssetLocalPngStyle {
    Icon,
    Badge,
    Sprite,
    Texture,
}

impl Default for AssetLocalPngStyle {
    fn default() -> Self {
        Self::Icon
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AssetImageRequest {
    pub prompt: String,
    #[serde(default = "default_model")]
    pub model: String,
    #[serde(default)]
    pub generation_mode: AssetImageGenerationMode,
    #[serde(default)]
    pub local_style: AssetLocalPngStyle,
    #[serde(default)]
    pub size: AssetImageSize,
    #[serde(default)]
    pub transparent: bool,
    pub output_name: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GeneratedImageAsset {
    pub job_id: Uuid,
    pub prompt: String,
    pub model: String,
    pub generation_mode: AssetImageGenerationMode,
    pub local_style: AssetLocalPngStyle,
    pub size: AssetImageSize,
    pub transparent: bool,
    pub image_path: PathBuf,
    pub metadata_path: PathBuf,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone)]
pub struct AssetImageWorkerConfig {
    pub python_executable: PathBuf,
    pub script_path: PathBuf,
}

impl Default for AssetImageWorkerConfig {
    fn default() -> Self {
        let python_executable = default_python_executable();
        Self {
            python_executable,
            script_path: PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("../../tools/ai_asset_worker/generate_image.py"),
        }
    }
}

fn default_python_executable() -> PathBuf {
    if let Some(path) = std::env::var_os("RAF_PYTHON_EXECUTABLE") {
        return PathBuf::from(path);
    }

    #[cfg(windows)]
    {
        if let Some(program_files) = std::env::var_os("ProgramFiles") {
            let kicad_root = PathBuf::from(program_files).join("KiCad");
            if let Ok(entries) = fs::read_dir(kicad_root) {
                let mut candidates = entries
                    .flatten()
                    .map(|entry| entry.path())
                    .collect::<Vec<_>>();
                candidates.sort();
                for directory in candidates.into_iter().rev() {
                    let candidate = directory.join("bin/python.exe");
                    if candidate.is_file() {
                        return candidate;
                    }
                }
            }
        }
    }

    PathBuf::from("python")
}

#[derive(Debug)]
pub struct AssetImageJob {
    id: Uuid,
    child: Child,
    request: AssetImageRequest,
    project_root: PathBuf,
    image_path: PathBuf,
    metadata_path: PathBuf,
    request_path: PathBuf,
    result_path: PathBuf,
}

#[derive(Debug, Clone)]
pub enum AssetImageJobStatus {
    Running,
    Ready(GeneratedImageAsset),
    Failed(String),
    Cancelled,
}

#[derive(Debug, Clone)]
pub struct AssetImageJobSnapshot {
    pub id: Uuid,
    pub request: AssetImageRequest,
    pub status: AssetImageJobStatus,
}

#[derive(Debug, Default)]
pub struct AssetImageGenerationQueue {
    worker: AssetImageWorker,
    jobs: HashMap<Uuid, AssetImageJob>,
    completed: HashMap<Uuid, AssetImageJobSnapshot>,
}

impl AssetImageGenerationQueue {
    pub fn start(
        &mut self,
        project_root: &Path,
        request: AssetImageRequest,
    ) -> Result<Uuid, String> {
        let job = self.worker.start(project_root, request)?;
        let id = job.id();
        self.jobs.insert(id, job);
        Ok(id)
    }

    pub fn poll(&mut self) -> Vec<AssetImageJobSnapshot> {
        let finished_ids = self
            .jobs
            .iter_mut()
            .filter_map(|(id, job)| job.poll().map(|result| (*id, result)))
            .collect::<Vec<_>>();
        let mut finished = Vec::with_capacity(finished_ids.len());
        for (id, result) in finished_ids {
            let Some(job) = self.jobs.remove(&id) else {
                continue;
            };
            let status = match result {
                Ok(asset) => AssetImageJobStatus::Ready(asset),
                Err(error) => AssetImageJobStatus::Failed(error),
            };
            let snapshot = AssetImageJobSnapshot {
                id,
                request: job.request().clone(),
                status,
            };
            self.completed.insert(id, snapshot.clone());
            finished.push(snapshot);
        }
        finished
    }

    pub fn snapshot(&self, id: Uuid) -> Option<AssetImageJobSnapshot> {
        self.jobs
            .get(&id)
            .map(|job| AssetImageJobSnapshot {
                id,
                request: job.request().clone(),
                status: AssetImageJobStatus::Running,
            })
            .or_else(|| self.completed.get(&id).cloned())
    }

    pub fn cancel(&mut self, id: Uuid) -> Result<bool, String> {
        let Some(mut job) = self.jobs.remove(&id) else {
            return Ok(false);
        };
        job.cancel()?;
        self.completed.insert(
            id,
            AssetImageJobSnapshot {
                id,
                request: job.request().clone(),
                status: AssetImageJobStatus::Cancelled,
            },
        );
        Ok(true)
    }
}

impl AssetImageJob {
    pub fn id(&self) -> Uuid {
        self.id
    }

    pub fn request(&self) -> &AssetImageRequest {
        &self.request
    }

    /// Returns `None` while Python is still working.
    pub fn poll(&mut self) -> Option<Result<GeneratedImageAsset, String>> {
        match self.child.try_wait() {
            Ok(None) => None,
            Err(error) => Some(Err(format!("image worker status: {error}"))),
            Ok(Some(status)) => {
                let result = read_worker_result(&self.result_path).and_then(|worker_result| {
                    if !status.success() || !worker_result.ok {
                        return Err(worker_result.error.unwrap_or_else(|| {
                            "image worker failed without a result".to_string()
                        }));
                    }
                    validate_png(&self.image_path)?;
                    ensure_inside_project(&self.project_root, &self.image_path)?;
                    ensure_inside_project(&self.project_root, &self.metadata_path)?;
                    Ok(GeneratedImageAsset {
                        job_id: self.id,
                        prompt: self.request.prompt.clone(),
                        model: self.request.model.clone(),
                        generation_mode: self.request.generation_mode,
                        local_style: self.request.local_style,
                        size: self.request.size,
                        transparent: self.request.transparent,
                        image_path: self.image_path.clone(),
                        metadata_path: self.metadata_path.clone(),
                        created_at: Utc::now(),
                    })
                });
                let _ = fs::remove_file(&self.request_path);
                let _ = fs::remove_file(&self.result_path);
                Some(result)
            }
        }
    }

    pub fn cancel(&mut self) -> Result<(), String> {
        self.child
            .kill()
            .map_err(|error| format!("image worker stop: {error}"))?;
        let _ = self.child.wait();
        let _ = fs::remove_file(&self.request_path);
        let _ = fs::remove_file(&self.result_path);
        Ok(())
    }
}

#[derive(Debug, Clone)]
pub struct AssetImageWorker {
    config: AssetImageWorkerConfig,
}

impl Default for AssetImageWorker {
    fn default() -> Self {
        Self::new(AssetImageWorkerConfig::default())
    }
}

impl AssetImageWorker {
    pub fn new(config: AssetImageWorkerConfig) -> Self {
        Self { config }
    }

    pub fn start(
        &self,
        project_root: &Path,
        request: AssetImageRequest,
    ) -> Result<AssetImageJob, String> {
        validate_request(&request)?;
        if !self.config.script_path.is_file() {
            return Err(format!(
                "image worker script is missing: {}",
                self.config.script_path.display()
            ));
        }

        let project_root = project_root
            .canonicalize()
            .map_err(|error| format!("project root: {error}"))?;
        let job_id = Uuid::new_v4();
        let generated_dir = project_root.join("assets/generated");
        let staging_dir = generated_dir.join(".staging");
        fs::create_dir_all(&staging_dir).map_err(|error| format!("image staging: {error}"))?;

        let asset_stem = sanitize_asset_stem(&request.output_name)?;
        let image_path = generated_dir.join(format!("{asset_stem}.png"));
        let metadata_path = generated_dir.join(format!("{asset_stem}.asset.json"));
        ensure_inside_project(&project_root, &image_path)?;
        ensure_inside_project(&project_root, &metadata_path)?;

        let request_path = staging_dir.join(format!("{job_id}.request.json"));
        let result_path = staging_dir.join(format!("{job_id}.result.json"));
        let payload = PythonImageRequest {
            prompt: request.prompt.clone(),
            model: request.model.clone(),
            generation_mode: request.generation_mode,
            local_style: request.local_style,
            size: request.size.provider_value().to_string(),
            transparent: request.transparent,
            output_path: image_path.clone(),
            metadata_path: metadata_path.clone(),
        };
        write_json(&request_path, &payload)?;

        let child = Command::new(&self.config.python_executable)
            .arg(&self.config.script_path)
            .arg("--request")
            .arg(&request_path)
            .arg("--result")
            .arg(&result_path)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|error| format!("image worker start: {error}"))?;

        Ok(AssetImageJob {
            id: job_id,
            child,
            request,
            project_root,
            image_path,
            metadata_path,
            request_path,
            result_path,
        })
    }
}

#[derive(Debug, Serialize)]
struct PythonImageRequest {
    prompt: String,
    model: String,
    generation_mode: AssetImageGenerationMode,
    local_style: AssetLocalPngStyle,
    size: String,
    transparent: bool,
    output_path: PathBuf,
    metadata_path: PathBuf,
}

#[derive(Debug, Deserialize)]
struct PythonImageResult {
    ok: bool,
    error: Option<String>,
}

fn validate_request(request: &AssetImageRequest) -> Result<(), String> {
    if request.prompt.trim().is_empty() {
        return Err("image prompt is empty".to_string());
    }
    if request.prompt.len() > 8_000 {
        return Err("image prompt exceeds 8000 characters".to_string());
    }
    if request.model.trim().is_empty() {
        return Err("image model is empty".to_string());
    }
    let _ = sanitize_asset_stem(&request.output_name)?;
    Ok(())
}

fn sanitize_asset_stem(name: &str) -> Result<String, String> {
    let stem = name
        .trim()
        .chars()
        .filter_map(|character| {
            if character.is_ascii_alphanumeric() {
                Some(character.to_ascii_lowercase())
            } else if matches!(character, '-' | '_' | ' ') {
                Some('_')
            } else {
                None
            }
        })
        .collect::<String>()
        .trim_matches('_')
        .to_string();
    if stem.is_empty() {
        return Err("image output name has no valid characters".to_string());
    }
    Ok(stem.chars().take(72).collect())
}

fn write_json<T: Serialize>(path: &Path, value: &T) -> Result<(), String> {
    let raw =
        serde_json::to_vec_pretty(value).map_err(|error| format!("image request json: {error}"))?;
    fs::write(path, raw).map_err(|error| format!("image request write: {error}"))
}

fn read_worker_result(path: &Path) -> Result<PythonImageResult, String> {
    let raw = fs::read(path).map_err(|error| format!("image worker result: {error}"))?;
    serde_json::from_slice(&raw).map_err(|error| format!("image worker result json: {error}"))
}

fn validate_png(path: &Path) -> Result<(), String> {
    let bytes = fs::read(path).map_err(|error| format!("generated image missing: {error}"))?;
    if bytes.len() < 24 || bytes[..8] != [137, 80, 78, 71, 13, 10, 26, 10] {
        return Err("generated image is not a PNG".to_string());
    }
    Ok(())
}

fn ensure_inside_project(project_root: &Path, path: &Path) -> Result<(), String> {
    if path.starts_with(project_root) {
        Ok(())
    } else {
        Err("generated asset path escapes the project".to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn output_names_stay_inside_the_generated_asset_folder() {
        assert_eq!(
            sanitize_asset_stem("Forest Rock 01").unwrap(),
            "forest_rock_01"
        );
        assert!(sanitize_asset_stem("../../escape").is_ok());
        assert!(sanitize_asset_stem("...").is_err());
    }

    #[test]
    fn request_requires_prompt_and_safe_name() {
        let request = AssetImageRequest {
            prompt: String::new(),
            model: default_model(),
            generation_mode: AssetImageGenerationMode::Remote,
            local_style: AssetLocalPngStyle::Icon,
            size: AssetImageSize::Square,
            transparent: false,
            output_name: "icon".to_string(),
        };
        assert!(validate_request(&request).is_err());
    }

    #[test]
    fn generated_images_default_to_the_remote_gpt_image_two_path() {
        let request = AssetImageRequest {
            prompt: "orange folder icon".to_string(),
            model: default_model(),
            generation_mode: AssetImageGenerationMode::default(),
            local_style: AssetLocalPngStyle::default(),
            size: AssetImageSize::default(),
            transparent: true,
            output_name: "folder_icon".to_string(),
        };

        assert_eq!(request.model, "gpt-image-2");
        assert_eq!(request.generation_mode, AssetImageGenerationMode::Remote);
    }
}
