//! Native Agent asset tools. Reuses the image worker and existing Assets catalog.
//! External studios remain user-trusted, separately installed command-line apps.

use raf_ai::agent_runtime::{AgentToolResult, PendingToolCall, ToolExecutionMode};
use raf_ai::{
    AssetImageGenerationMode, AssetImageGenerationQueue, AssetImageJobStatus, AssetImageProvider,
    AssetImageRequest, AssetImageSize, AssetLocalPngStyle,
};
use raf_core::{ai::ExternalAssetStudio, i18n::t, Language};
use serde_json::{json, Value};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use uuid::Uuid;

enum PendingAsset {
    Image {
        job: Uuid,
        root: PathBuf,
    },
    Install {
        child: Child,
        directory: PathBuf,
        entry_point: String,
    },
}

#[derive(Default)]
pub(crate) struct AgentAssetTools {
    queue: AssetImageGenerationQueue,
    pending: HashMap<String, PendingAsset>,
    catalog_dirty: bool,
    command_jobs: HashMap<String, (PathBuf, Option<Result<AgentToolResult, String>>)>,
}

impl AgentAssetTools {
    /// Console and attached CLI/MCP use the same execution path as Agent tools.
    /// These are file jobs, not scene transactions or scene Undo records.
    pub fn command(
        &mut self,
        request: &raf_core::EngineCommandRequest,
        root: Option<&Path>,
        studio: &ExternalAssetStudio,
        provider: Option<&AssetImageProvider>,
        model: &str,
        revision: u64,
        language: Language,
    ) -> raf_core::EngineCommandResponse {
        use raf_core::EngineCommandResponse;
        let mut response = EngineCommandResponse::error(request.id, t("app.assets", language), "");
        response.revision = revision;
        let outcome = (|| -> Result<Value, String> {
            request.validate()?;
            if request.idempotency_key.is_some() {
                return Err("Asset jobs deduplicate by request ID; reuse it instead of setting an idempotency key".into());
            }
            if request
                .expected_revision
                .is_some_and(|expected| expected != revision)
            {
                return Err("Project revision changed; inspect the project before retrying".into());
            }
            let root = root
                .ok_or_else(|| t("app.agent_asset_project_required", language))?
                .canonicalize()
                .map_err(|error| error.to_string())?;
            if matches!(
                request.name.as_str(),
                "asset.image_status" | "asset.cancel_image"
            ) {
                let id = required_string(&request.params, "job")?;
                let (job_root, result) = self.command_jobs.get(id).ok_or("Unknown asset job")?;
                if *job_root != root {
                    return Err("Asset job belongs to a different project".into());
                }
                if request.name == "asset.cancel_image" {
                    if request.dry_run {
                        return Ok(json!({"preview": true, "job": id}));
                    }
                    if !request.confirm {
                        return Err("Confirmation required to cancel an asset job".into());
                    }
                    self.cancel_call(id);
                    self.command_jobs.get_mut(id).unwrap().1 =
                        Some(Err(t("app.agent_asset_cancelled", language)));
                    return Ok(json!({"job": id, "status": "cancelled"}));
                }
                return Ok(match result {
                    None => json!({"job": id, "status": "running"}),
                    Some(Ok(result)) => {
                        json!({"job": id, "status": "ready", "result": result.data})
                    }
                    Some(Err(error)) => json!({"job": id, "status": "failed", "error": error}),
                });
            }
            let tool = match request.name.as_str() {
                "asset.generate_image" | "asset.image" => "asset_generate_image",
                "asset.generate_local_png" | "asset.local_png" | "asset.icon" => {
                    "asset_generate_local_png"
                }
                "asset.studio_install" => "external_studio_install",
                "asset.studio_generate" => "external_studio_generate",
                _ => return Err("Unknown asset command".into()),
            };
            if !request.dry_run && !request.confirm {
                return Err("Confirmation required to start an asset job".into());
            }
            if request.transaction_id.is_some() {
                return Err("Asset file generation is not a scene transaction".into());
            }
            let id = format!("command:{}", request.id.0);
            if self.command_jobs.contains_key(&id) {
                return Ok(json!({"job": id, "status": "accepted"}));
            }
            let result = self.execute(
                &id,
                tool,
                &request.params,
                if request.dry_run {
                    ToolExecutionMode::Preview
                } else {
                    ToolExecutionMode::Apply
                },
                Some(&root),
                studio,
                provider,
                model,
                language,
            )?;
            if request.dry_run {
                return Ok(json!({"preview": true, "result": result.data}));
            }
            // Bound retained status history without dropping running jobs.
            if self.command_jobs.len() >= 64 {
                if let Some(old) = self
                    .command_jobs
                    .iter()
                    .find(|(_, (_, result))| result.is_some())
                    .map(|(id, _)| id.clone())
                {
                    self.command_jobs.remove(&old);
                }
            }
            let pending = self.is_pending(&id);
            self.command_jobs.insert(
                id.clone(),
                (root, if pending { None } else { Some(Ok(result)) }),
            );
            Ok(json!({"job": id, "status": if pending { "running" } else { "ready" }}))
        })();
        match outcome {
            Ok(data) => {
                response.ok = true;
                response.lines.clear();
                response.data = data;
            }
            Err(error) => response.lines = vec![asset_error(language, &error)],
        }
        response
    }

    pub fn poll_commands(
        &mut self,
        root: Option<&Path>,
        language: Language,
    ) -> Vec<Result<AgentToolResult, String>> {
        let root = root.and_then(|root| root.canonicalize().ok());
        let ids: Vec<_> = self
            .command_jobs
            .iter()
            .filter(|(_, (_, result))| result.is_none())
            .map(|(id, (path, _))| (id.clone(), path.clone()))
            .collect();
        let mut completed = Vec::new();
        for (id, path) in ids {
            let result = if root.as_ref() != Some(&path) {
                self.cancel_call(&id);
                Some(Err(t("app.agent_asset_cancelled", language)))
            } else {
                self.poll(&id, language)
            };
            if let Some(result) = result {
                self.command_jobs.get_mut(&id).unwrap().1 = Some(result.clone());
                completed.push(result);
            }
        }
        completed
    }

    fn cancel_call(&mut self, id: &str) {
        match self.pending.remove(id) {
            Some(PendingAsset::Image { job, .. }) => {
                let _ = self.queue.cancel(job);
                self.queue.forget_completed(job);
            }
            Some(PendingAsset::Install { mut child, .. }) => {
                let _ = child.kill();
                let _ = child.wait();
            }
            None => {}
        }
    }
    pub fn is_pending(&self, id: &str) -> bool {
        self.pending.contains_key(id)
    }

    pub fn execute(
        &mut self,
        id: &str,
        name: &str,
        args: &Value,
        mode: ToolExecutionMode,
        root: Option<&Path>,
        studio: &ExternalAssetStudio,
        provider: Option<&AssetImageProvider>,
        model: &str,
        language: Language,
    ) -> Result<AgentToolResult, String> {
        if mode != ToolExecutionMode::Apply {
            let mut result = AgentToolResult::success(
                t("app.agent_asset_preview", language),
                json!({"tool": name, "arguments": args, "network_request": false}),
            );
            result.preview = true;
            return Ok(result);
        }
        let root = root
            .ok_or_else(|| t("app.agent_asset_project_required", language))?
            .canonicalize()
            .map_err(|error| asset_error(language, &error.to_string()))?;
        if name == "external_studio_install" {
            let child = install_studio(studio).map_err(|error| asset_error(language, &error))?;
            if let Some(child) = child {
                self.pending.insert(
                    id.to_string(),
                    PendingAsset::Install {
                        child,
                        directory: PathBuf::from(&studio.directory),
                        entry_point: studio.entry_point.clone(),
                    },
                );
                return Ok(AgentToolResult::success(
                    t("app.agent_studio_installing", language),
                    Value::Null,
                ));
            }
            return Ok(AgentToolResult::success(
                t("app.agent_studio_installed", language),
                json!({"installed": true}),
            ));
        }
        let prompt = required_string(args, "prompt")?;
        let output_name = required_string(args, "name")?;
        let local = name == "asset_generate_local_png";
        let size = match args.get("size").and_then(Value::as_str).unwrap_or("square") {
            "square" => AssetImageSize::Square,
            "landscape" => AssetImageSize::Landscape,
            "portrait" => AssetImageSize::Portrait,
            _ => return Err("size must be square, landscape, or portrait".into()),
        };
        let style = match args.get("style").and_then(Value::as_str).unwrap_or("icon") {
            "icon" => AssetLocalPngStyle::Icon,
            "badge" => AssetLocalPngStyle::Badge,
            "sprite" => AssetLocalPngStyle::Sprite,
            "texture" => AssetLocalPngStyle::Texture,
            _ => return Err("style must be icon, badge, sprite, or texture".into()),
        };
        let request = AssetImageRequest {
            prompt: prompt.to_string(),
            output_name: output_name.to_string(),
            model: if local {
                "local-png".into()
            } else {
                args.get("model")
                    .and_then(Value::as_str)
                    .unwrap_or(model)
                    .to_string()
            },
            generation_mode: if local {
                AssetImageGenerationMode::LocalPng
            } else {
                AssetImageGenerationMode::Remote
            },
            local_style: style,
            size,
            transparent: args
                .get("transparent")
                .and_then(Value::as_bool)
                .unwrap_or(local),
        };
        let job = if local {
            crate::commands::assets::enqueue_image(&root, &mut self.queue, None, request)
        } else {
            let provider =
                provider.ok_or_else(|| t("app.agent_asset_provider_required", language))?;
            if name == "external_studio_generate" {
                self.queue.start_external(&root, request, provider, studio)
            } else {
                crate::commands::assets::enqueue_image(
                    &root,
                    &mut self.queue,
                    Some(provider),
                    request,
                )
            }
        }
        .map_err(|error| asset_error(language, &error))?;
        self.pending
            .insert(id.to_string(), PendingAsset::Image { job, root });
        Ok(AgentToolResult::success(
            t("app.agent_asset_generating", language),
            json!({"job": job}),
        ))
    }

    pub fn poll(
        &mut self,
        id: &str,
        language: Language,
    ) -> Option<Result<AgentToolResult, String>> {
        self.queue.poll();
        let pending = self.pending.get_mut(id)?;
        let result = match pending {
            PendingAsset::Image { job, root } => match self.queue.snapshot(*job)?.status {
                AssetImageJobStatus::Running => return None,
                AssetImageJobStatus::Ready(asset) => {
                    if let Err(error) = image::open(&asset.image_path) {
                        self.pending.remove(id);
                        self.queue.forget_completed(asset.job_id);
                        return Some(Err(asset_error(language, &format!("PNG decode: {error}"))));
                    }
                    let path = match asset.image_path.strip_prefix(root.as_path()) {
                        Ok(path) => path.to_string_lossy().replace('\\', "/"),
                        Err(_) => {
                            self.pending.remove(id);
                            return Some(Err(t("app.agent_asset_failed", language)));
                        }
                    };
                    self.catalog_dirty = true;
                    let mut result = AgentToolResult::success(
                        t("app.agent_asset_ready", language),
                        json!({"image_path": path, "model": asset.model, "generation_mode": asset.generation_mode,
                                "catalog_refresh_requested": true}),
                    );
                    result.references.push(path);
                    result.changed = true;
                    result.verification = Some(
                        json!({"status": "passed", "checks": ["file_exists", "png_decode", "project_scoped"]}),
                    );
                    Ok(result)
                }
                AssetImageJobStatus::Failed(error) => Err(asset_error(language, &error)),
                AssetImageJobStatus::Cancelled => Err(t("app.agent_asset_cancelled", language)),
            },
            PendingAsset::Install {
                child,
                directory,
                entry_point,
            } => match child.try_wait() {
                Ok(None) => return None,
                Ok(Some(status))
                    if status.success() && valid_entry_point(directory, entry_point) =>
                {
                    Ok(AgentToolResult::success(
                        t("app.agent_studio_installed", language),
                        json!({"installed": true, "directory": directory}),
                    ))
                }
                _ => Err(t("app.agent_studio_install_failed", language)),
            },
        };
        if let Some(PendingAsset::Image { job, .. }) = self.pending.remove(id) {
            self.queue.forget_completed(job);
        }
        Some(result)
    }

    /// Stop/project/chat changes cannot leave an image request running invisibly.
    pub fn cancel_abandoned(&mut self, calls: &[PendingToolCall]) {
        let active: HashSet<_> = calls
            .iter()
            .filter(|call| call.result.is_none())
            .map(|call| call.id.as_str())
            .collect();
        let abandoned: Vec<_> = self
            .pending
            .keys()
            .filter(|id| {
                !active.contains(id.as_str()) && !self.command_jobs.contains_key(id.as_str())
            })
            .cloned()
            .collect();
        for id in abandoned {
            self.cancel_call(&id);
        }
    }

    pub fn take_catalog_dirty(&mut self) -> bool {
        std::mem::take(&mut self.catalog_dirty)
    }
}

impl Drop for AgentAssetTools {
    fn drop(&mut self) {
        self.command_jobs.clear();
        self.cancel_abandoned(&[]);
    }
}

fn required_string<'a>(args: &'a Value, key: &str) -> Result<&'a str, String> {
    args.get(key)
        .and_then(Value::as_str)
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| format!("{key} must be a nonempty string"))
}

fn asset_error(language: Language, detail: &str) -> String {
    format!("{}: {detail}", t("app.agent_asset_failed", language))
}

fn valid_entry_point(directory: &Path, entry: &str) -> bool {
    let Ok(root) = directory.canonicalize() else {
        return false;
    };
    root.join(entry)
        .canonicalize()
        .is_ok_and(|path| path.is_file() && path.starts_with(root))
}

fn install_studio(studio: &ExternalAssetStudio) -> Result<Option<Child>, String> {
    if !studio.enabled {
        return Err("External studio is disabled in AI Providers settings".into());
    }
    let directory = Path::new(&studio.directory);
    if studio.entry_point.trim().is_empty()
        || !directory.is_absolute()
        || directory.parent().is_none()
    {
        return Err("Configure an absolute installation directory and an entry point in AI Providers settings".into());
    }
    if directory.exists() {
        let root = directory
            .canonicalize()
            .map_err(|error| error.to_string())?;
        let entry = root.join(&studio.entry_point).canonicalize().map_err(|_| "Directory already exists without the configured entry point; it will not be overwritten".to_string())?;
        if !entry.is_file() || !entry.starts_with(root) {
            return Err("Entry point escapes the studio installation".into());
        }
        return Ok(None);
    }
    // No URLs from tool arguments, credential-bearing URLs, scripts or package hooks.
    let url = studio.repository_url.trim();
    if !url.starts_with("https://")
        || url.contains('@')
        || url.contains('?')
        || url.contains('#')
        || url.chars().any(char::is_whitespace)
    {
        return Err("Configure a public HTTPS Git repository without embedded credentials".into());
    }
    let parent = directory.parent().unwrap();
    if !parent.is_dir() {
        return Err("Studio installation parent directory does not exist".into());
    }
    let mut command = Command::new("git");
    let disabled_hooks = if cfg!(windows) {
        "core.hooksPath=NUL"
    } else {
        "core.hooksPath=/dev/null"
    };
    command
        .args([
            "-c",
            disabled_hooks,
            "clone",
            "--quiet",
            "--depth",
            "1",
            "--single-branch",
            "--",
        ])
        .arg(url)
        .arg(directory)
        .env("GIT_TERMINAL_PROMPT", "0")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x08000000);
    }
    command
        .spawn()
        .map(Some)
        .map_err(|error| format!("Studio download: {error}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn attached_asset_commands_require_confirmation_and_preview_has_no_side_effects() {
        let root = std::env::temp_dir().join(format!("raf-asset-command-{}", Uuid::new_v4()));
        std::fs::create_dir(&root).unwrap();
        let mut tools = AgentAssetTools::default();
        let mut request = raf_core::EngineCommandRequest::new(
            "asset.generate_local_png",
            json!({"prompt":"folder icon", "name":"folder"}),
            raf_core::CommandSource::Mcp,
        );
        let studio = ExternalAssetStudio::default();
        assert!(
            !tools
                .command(
                    &request,
                    Some(&root),
                    &studio,
                    None,
                    "",
                    4,
                    Language::English
                )
                .ok
        );
        request.dry_run = true;
        let result = tools.command(
            &request,
            Some(&root),
            &studio,
            None,
            "",
            4,
            Language::English,
        );
        assert!(result.ok && result.data["preview"] == true);
        assert_eq!(result.revision, 4);
        assert!(!result.changed && !result.undo_available);
        assert!(tools.pending.is_empty() && tools.command_jobs.is_empty());
        assert_eq!(std::fs::read_dir(&root).unwrap().count(), 0);
        request.expected_revision = Some(3);
        assert!(
            !tools
                .command(
                    &request,
                    Some(&root),
                    &studio,
                    None,
                    "",
                    4,
                    Language::English
                )
                .ok
        );
        std::fs::remove_dir(root).unwrap();
    }

    #[test]
    fn preview_never_installs_or_generates_and_existing_directories_are_not_overwritten() {
        let mut tools = AgentAssetTools::default();
        let result = tools
            .execute(
                "preview",
                "external_studio_install",
                &json!({}),
                ToolExecutionMode::Preview,
                None,
                &ExternalAssetStudio::default(),
                None,
                "",
                Language::English,
            )
            .unwrap();
        assert!(result.preview);
        assert!(!tools.is_pending("preview"));
        let root = std::env::temp_dir().join(format!("raf-studio-preserve-{}", Uuid::new_v4()));
        std::fs::create_dir(&root).unwrap();
        let marker = root.join("keep.txt");
        std::fs::write(&marker, "existing user file").unwrap();
        let studio = ExternalAssetStudio {
            enabled: true,
            directory: root.to_string_lossy().into_owned(),
            entry_point: "missing.js".into(),
            repository_url: "https://example.invalid/studio.git".into(),
            ..ExternalAssetStudio::default()
        };
        assert!(install_studio(&studio).is_err());
        assert_eq!(
            std::fs::read_to_string(&marker).unwrap(),
            "existing user file"
        );
        std::fs::remove_file(marker).unwrap();
        std::fs::remove_dir(root).unwrap();
    }

    #[test]
    fn local_png_runs_through_the_real_worker_and_marks_the_existing_catalog_dirty() {
        let root = std::env::temp_dir().join(format!("raf-agent-image-{}", Uuid::new_v4()));
        std::fs::create_dir(&root).unwrap();
        let mut tools = AgentAssetTools::default();
        tools
            .execute(
                "local",
                "asset_generate_local_png",
                &json!({"prompt":"folder icon", "name":"folder"}),
                ToolExecutionMode::Apply,
                Some(&root),
                &ExternalAssetStudio::default(),
                None,
                "",
                Language::English,
            )
            .unwrap();
        assert!(tools.is_pending("local"));
        let start = std::time::Instant::now();
        let result = loop {
            if let Some(result) = tools.poll("local", Language::English) {
                break result.unwrap();
            }
            assert!(
                start.elapsed().as_secs() < 20,
                "image worker did not finish"
            );
            std::thread::sleep(std::time::Duration::from_millis(20));
        };
        assert!(result.ok && result.changed);
        assert!(result.references[0].starts_with("assets/generated/folder_"));
        assert!(root.join(&result.references[0]).is_file());
        assert!(!tools.is_pending("local"));
        assert!(tools.take_catalog_dirty());
        assert!(!tools.take_catalog_dirty());
        // Only this test's uniquely created directory is removed.
        std::fs::remove_dir_all(root).unwrap();
    }
}
