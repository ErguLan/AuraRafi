//! Lazy, bounded progress I/O. No scene, script VM or graphics state enters this worker.
use raf_core::runtime_config::{valid_storage_name, ProgressLocation, ProgressSettings};
use raf_script::services::{
    ProgressDocument, ProgressOperation, ProgressOutcome, ProgressReceipt, ProgressRequest,
};
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    path::{Path, PathBuf},
    sync::mpsc,
};
use uuid::Uuid;

#[derive(Clone)]
pub struct StorageConfig {
    pub project: Uuid,
    pub project_root: PathBuf,
    pub custom_root: Option<PathBuf>,
    /// Stable development lanes isolate parallel windows and survive Stop/Play.
    pub development_slot: u32,
    pub policy: ProgressSettings,
}
pub struct StorageWorker {
    sender: mpsc::SyncSender<ProgressRequest>,
    receiver: mpsc::Receiver<ProgressReceipt>,
}
impl StorageWorker {
    pub fn new(config: StorageConfig) -> Result<Self, String> {
        let (sender, jobs) = mpsc::sync_channel::<ProgressRequest>(1);
        let (results, receiver) = mpsc::sync_channel(1);
        std::thread::Builder::new()
            .name("runtime-progress".into())
            .spawn(move || {
                let mut storage = DiskStorage {
                    config,
                    root: None,
                    observed: BTreeMap::new(),
                };
                while let Ok(request) = jobs.recv() {
                    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                        storage.execute(&request)
                    }))
                    .unwrap_or_else(|_| Err("progress.worker_failed".into()));
                    let outcome = result.unwrap_or_else(ProgressOutcome::Failed);
                    if results
                        .send(ProgressReceipt {
                            id: request.id,
                            revision: request.revision,
                            outcome,
                        })
                        .is_err()
                    {
                        break;
                    }
                }
            })
            .map_err(|e| format!("progress.worker: {e}"))?;
        Ok(Self { sender, receiver })
    }
    pub fn submit(&self, request: ProgressRequest) -> Result<(), ProgressReceipt> {
        self.sender.try_send(request).map_err(|error| {
            let request = match error {
                mpsc::TrySendError::Full(r) | mpsc::TrySendError::Disconnected(r) => r,
            };
            ProgressReceipt {
                id: request.id,
                revision: request.revision,
                outcome: ProgressOutcome::Failed("progress.worker_unavailable".into()),
            }
        })
    }
    pub fn poll(&self) -> Option<ProgressReceipt> {
        self.receiver.try_recv().ok()
    }
}
#[derive(Serialize, Deserialize)]
struct StoredProgress {
    project: Uuid,
    profile: String,
    generation: Uuid,
    document: ProgressDocument,
}
struct DiskStorage {
    config: StorageConfig,
    root: Option<PathBuf>,
    observed: BTreeMap<String, Option<Uuid>>,
}
impl DiskStorage {
    fn root(&mut self) -> Result<PathBuf, String> {
        if let Some(root) = &self.root {
            return Ok(root.clone());
        }
        self.config.policy.validate().map_err(str::to_owned)?;
        if !self.config.policy.enabled
            || self.config.project.is_nil()
            || self.config.development_slot > 3
        {
            return Err("progress.invalid_host_configuration".into());
        }
        let base = match self.config.policy.location {
            ProgressLocation::AppData => {
                let path = if cfg!(target_os = "windows") {
                    std::env::var_os("LOCALAPPDATA").map(PathBuf::from)
                } else if cfg!(target_os = "macos") {
                    std::env::var_os("HOME")
                        .map(|p| PathBuf::from(p).join("Library/Application Support"))
                } else {
                    std::env::var_os("XDG_DATA_HOME")
                        .map(PathBuf::from)
                        .or_else(|| {
                            std::env::var_os("HOME").map(|p| PathBuf::from(p).join(".local/share"))
                        })
                };
                path.filter(|p| p.is_absolute())
                    .ok_or("progress.appdata_unavailable")?
                    .join("AuraRafi/Games")
                    .join(self.config.project.to_string())
                    .join("saves")
            }
            ProgressLocation::Portable => self
                .config
                .project_root
                .canonicalize()
                .map_err(io_error)?
                .join("saves"),
            ProgressLocation::Custom => {
                let grant = self
                    .config
                    .custom_root
                    .as_ref()
                    .ok_or("progress.folder_grant_required")?;
                if !grant.is_absolute() {
                    return Err("progress.invalid_folder_grant".into());
                }
                grant
                    .canonicalize()
                    .map_err(io_error)?
                    .join(self.config.project.to_string())
            }
        };
        // Only the host chooses this root. Guests cannot supply paths or access production saves.
        create_checked(&base)?;
        let base = base.canonicalize().map_err(io_error)?;
        let root = base.join(format!(
            "development/play-{}",
            self.config.development_slot + 1
        ));
        create_confined(&base, &root)?;
        self.root = Some(root.clone());
        Ok(root)
    }
    fn execute(&mut self, request: &ProgressRequest) -> Result<ProgressOutcome, String> {
        if !valid_storage_name(&request.profile) || !valid_storage_name(&request.slot) {
            return Err("progress.invalid_name".into());
        }
        let root = self.root()?;
        let profile = root.join(&request.profile);
        if !profile.exists() && fs::read_dir(&root).map_err(io_error)?.take(17).count() >= 16 {
            return Err("progress.profile_limit".into());
        }
        create_confined(&root, &profile)?;
        let path = profile.join(format!("{}.ron", request.slot));
        let backup = profile.join(format!("{}.backup.ron", request.slot));
        let lock_path = profile.join(".writer.lock");
        reject_link(&lock_path)?;
        let lock = OpenOptions::new()
            .create(true)
            .truncate(false)
            .read(true)
            .write(true)
            .open(lock_path)
            .map_err(io_error)?;
        // OS locks are released on process death; no stale lock-file deletion heuristic.
        lock.try_lock()
            .map_err(|_| "progress.writer_busy".to_string())?;
        reject_link(&path)?;
        reject_link(&backup)?;
        let key = format!("{}/{}", request.profile, request.slot);
        match &request.operation {
            ProgressOperation::Load | ProgressOperation::LoadBackup => {
                let selected = if matches!(request.operation, ProgressOperation::LoadBackup) {
                    &backup
                } else {
                    &path
                };
                if !selected.exists() {
                    if selected == &path && backup.exists() {
                        return Err("progress.backup_available".into());
                    }
                    self.remember(key, None)?;
                    return Ok(ProgressOutcome::Missing);
                }
                let saved = self.read(selected, &request.profile)?;
                // Recovery is explicit, and never authorizes silently replacing a corrupt primary.
                if selected == &path {
                    self.remember(key, Some(saved.generation))?;
                }
                Ok(ProgressOutcome::Loaded(saved.document))
            }
            ProgressOperation::Save(document) => {
                document.validate().map_err(|e| e.to_string())?;
                let current = if path.exists() {
                    Some(self.read(&path, &request.profile)?.generation)
                } else {
                    None
                };
                if path.exists() && self.observed.get(&key).copied().flatten() != current {
                    return Err("progress.conflict_load_first".into());
                }
                if !path.exists() && backup.exists() {
                    return Err("progress.backup_available".into());
                }
                if current.is_none() {
                    let count = fs::read_dir(&profile)
                        .map_err(io_error)?
                        .take(512)
                        .filter_map(Result::ok)
                        .filter(|e| {
                            e.file_name()
                                .to_str()
                                .is_some_and(|n| n.ends_with(".ron") && !n.ends_with(".backup.ron"))
                        })
                        .count();
                    if count >= self.config.policy.max_slots as usize {
                        return Err("progress.slot_limit".into());
                    }
                }
                let generation = Uuid::new_v4();
                let stored = StoredProgress {
                    project: self.config.project,
                    profile: request.profile.clone(),
                    generation,
                    document: document.clone(),
                };
                let bytes =
                    ron::ser::to_string(&stored).map_err(|e| format!("progress.serialize: {e}"))?;
                if bytes.len() > self.config.policy.max_slot_bytes as usize {
                    return Err("progress.size_limit".into());
                }
                let temporary = profile.join(format!(".{}.pending", request.slot));
                reject_link(&temporary)?;
                let mut temporary_created = false;
                let result = (|| {
                    let mut file = OpenOptions::new()
                        .create_new(true)
                        .write(true)
                        .open(&temporary)
                        .map_err(io_error)?;
                    temporary_created = true;
                    file.write_all(bytes.as_bytes()).map_err(io_error)?;
                    file.sync_all().map_err(io_error)?;
                    drop(file);
                    if path.exists() {
                        fs::rename(&path, &backup).map_err(io_error)?;
                    }
                    if let Err(error) = fs::rename(&temporary, &path) {
                        if backup.exists() && !path.exists() {
                            fs::rename(&backup, &path).map_err(|restore| {
                                format!("progress.recovery_required: {error}; {restore}")
                            })?;
                        }
                        return Err(io_error(error));
                    }
                    // Directory durability is supported by the filesystem on Unix. Windows
                    // retains the flushed file plus the previous valid backup generation.
                    #[cfg(unix)]
                    File::open(&profile)
                        .and_then(|f| f.sync_all())
                        .map_err(io_error)?;
                    Ok(())
                })();
                if temporary_created && temporary.exists() {
                    let _ = fs::remove_file(&temporary);
                }
                result?;
                self.remember(key, Some(generation))?;
                Ok(ProgressOutcome::Saved)
            }
        }
    }
    fn remember(&mut self, key: String, generation: Option<Uuid>) -> Result<(), String> {
        if self.observed.len() >= 16 * 128 && !self.observed.contains_key(&key) {
            return Err("progress.slot_limit".into());
        }
        self.observed.insert(key, generation);
        Ok(())
    }
    fn read(&self, path: &Path, profile: &str) -> Result<StoredProgress, String> {
        let mut bytes = Vec::new();
        File::open(path)
            .map_err(io_error)?
            .take(self.config.policy.max_slot_bytes as u64 + 1)
            .read_to_end(&mut bytes)
            .map_err(io_error)?;
        if bytes.len() > self.config.policy.max_slot_bytes as usize {
            return Err("progress.size_limit".into());
        }
        let value: StoredProgress =
            ron::de::from_bytes(&bytes).map_err(|e| format!("progress.corrupt_document: {e}"))?;
        if value.project != self.config.project || value.profile != profile {
            return Err("progress.document_identity_mismatch".into());
        }
        value.document.validate().map_err(|e| e.to_string())?;
        Ok(value)
    }
}
fn io_error(error: std::io::Error) -> String {
    format!("progress.io: {error}")
}
fn reject_link(path: &Path) -> Result<(), String> {
    match fs::symlink_metadata(path) {
        Ok(meta) => {
            let mut link = meta.file_type().is_symlink();
            #[cfg(windows)]
            {
                use std::os::windows::fs::MetadataExt;
                link |= meta.file_attributes() & 0x400 != 0;
            }
            if link {
                return Err("progress.links_not_allowed".into());
            }
            Ok(())
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(io_error(error)),
    }
}
fn create_checked(path: &Path) -> Result<(), String> {
    // Inspect every existing ancestor before creating descendants; reject junctions too.
    for ancestor in path.ancestors() {
        reject_link(ancestor)?;
    }
    fs::create_dir_all(path).map_err(io_error)
}
fn create_confined(root: &Path, path: &Path) -> Result<(), String> {
    if !path.starts_with(root) {
        return Err("progress.path_escape".into());
    }
    create_checked(path)?;
    if !path
        .canonicalize()
        .map_err(io_error)?
        .starts_with(root.canonicalize().map_err(io_error)?)
    {
        return Err("progress.path_escape".into());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use raf_script::ScriptValue;
    struct Fixture(PathBuf);
    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }
    #[test]
    fn runtime_services_disk_generations_backup_confinement_and_session_ingress() {
        let fixture =
            Fixture(std::env::temp_dir().join(format!("raf-progress-test-{}", Uuid::new_v4())));
        fs::create_dir(&fixture.0).unwrap();
        let config = StorageConfig {
            project: Uuid::new_v4(),
            project_root: fixture.0.clone(),
            custom_root: None,
            development_slot: 0,
            policy: ProgressSettings {
                location: ProgressLocation::Portable,
                ..Default::default()
            },
        };
        let mut disk = DiskStorage {
            config: config.clone(),
            root: None,
            observed: BTreeMap::new(),
        };
        let mut document = ProgressDocument {
            format_version: 1,
            schema_version: 1,
            values: BTreeMap::from([("score".into(), ScriptValue::Int(1))]),
        };
        let save = |doc: ProgressDocument| ProgressRequest {
            id: 1,
            revision: 1,
            profile: "main".into(),
            slot: "manual".into(),
            operation: ProgressOperation::Save(doc),
        };
        assert!(matches!(
            disk.execute(&save(document.clone())).unwrap(),
            ProgressOutcome::Saved
        ));
        document.values.insert("score".into(), ScriptValue::Int(2));
        disk.execute(&save(document.clone())).unwrap();
        let mut contender = DiskStorage {
            config: config.clone(),
            root: None,
            observed: BTreeMap::new(),
        };
        assert!(contender
            .execute(&save(document.clone()))
            .unwrap_err()
            .contains("conflict"));
        let mut request = ProgressRequest {
            id: 2,
            revision: 2,
            profile: "main".into(),
            slot: "manual".into(),
            operation: ProgressOperation::Load,
        };
        assert!(matches!(
            contender.execute(&request).unwrap(),
            ProgressOutcome::Loaded(_)
        ));
        document.values.insert("score".into(), ScriptValue::Int(3));
        disk.execute(&save(document.clone())).unwrap();
        assert!(contender
            .execute(&save(document.clone()))
            .unwrap_err()
            .contains("conflict"));
        let primary = disk.root().unwrap().join("main/manual.ron");
        fs::write(&primary, b"corrupt test document").unwrap();
        assert!(disk.execute(&request).unwrap_err().contains("corrupt"));
        request.operation = ProgressOperation::LoadBackup;
        let ProgressOutcome::Loaded(backup) = disk.execute(&request).unwrap() else {
            panic!("backup missing")
        };
        assert_eq!(backup.values["score"], ScriptValue::Int(2));
        request.profile = "../escape".into();
        assert!(disk.execute(&request).is_err());
        let mut second_lane = DiskStorage {
            config: StorageConfig {
                development_slot: 1,
                ..config.clone()
            },
            root: None,
            observed: BTreeMap::new(),
        };
        request.profile = "main".into();
        request.operation = ProgressOperation::Load;
        assert!(matches!(
            second_lane.execute(&request).unwrap(),
            ProgressOutcome::Missing
        ));
        let mut registry =
            raf_core::session::ProjectSessionRegistry::new(raf_core::project::ProjectType::Game);
        registry.save(&fixture.0).unwrap();
        let session = registry.sessions[0].clone();
        let mut scene = raf_core::SceneGraph::new();
        scene.add_root("Authored");
        fs::write(
            fixture.0.join(&session.scene_file),
            ron::ser::to_string(&scene).unwrap(),
        )
        .unwrap();
        let source = crate::session_loader::SessionSource {
            root: fixture.0.clone(),
            project_id: config.project,
            project_name: "test".into(),
            project_settings: raf_core::project::ProjectSettings::default(),
            host_settings: crate::manifest::RuntimeHostSettings::from_engine(
                &raf_core::config::EngineSettings::default(),
            ),
            save_root_grant: None,
            development_slot: 0,
        };
        assert_eq!(
            source
                .load(session.id.0, Uuid::new_v4())
                .unwrap()
                .scene
                .len(),
            scene.len()
        );
        registry.sessions[0].scene_file = PathBuf::from("../escape.ron");
        registry.save(&fixture.0).unwrap();
        assert!(source
            .load(session.id.0, Uuid::new_v4())
            .err()
            .unwrap()
            .contains("path_escape"));
    }
}
