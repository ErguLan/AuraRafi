//! Worker-backed, recoverable saves of a mounted authoring document.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Component, Path, PathBuf};
use std::sync::mpsc::{self, Receiver};

use crate::electronics_controller::NativeElectronicsEditor;
use raf_core::project_store::{confined_path, read_recoverable, write_atomic};
use raf_core::session::ProjectSession;
use raf_core::{Project, SceneGraph, TransactionLedger};
use raf_nodes::NodeGraph;
use serde::{Deserialize, Serialize};

#[derive(Clone)]
pub(crate) struct DocumentSave {
    pub project: Project,
    pub session: ProjectSession,
    pub scene: SceneGraph,
    pub nodes: NodeGraph,
    pub electronics: Option<(raf_electronics::Schematic, raf_electronics::PcbLayout)>,
    pub ledger: TransactionLedger,
    pub expected: BTreeMap<PathBuf, Option<Vec<u8>>>,
}

#[derive(Serialize, Deserialize)]
struct FileChange {
    path: PathBuf,
    before: Option<Vec<u8>>,
    after: Vec<u8>,
}

#[derive(Serialize, Deserialize)]
struct SaveJournal {
    project_id: uuid::Uuid,
    document_id: uuid::Uuid,
    revision: u64,
    files: Vec<FileChange>,
}

pub(crate) struct SaveReceipt {
    pub revision: u64,
    pub files: BTreeMap<PathBuf, Option<Vec<u8>>>,
}

#[derive(Default)]
pub(crate) struct ProjectSaveQueue {
    receiver: Option<Receiver<Result<SaveReceipt, String>>>,
    request: Option<DocumentSave>,
    pub error: Option<String>,
    pub saved_revision: Option<u64>,
    pub expected: BTreeMap<PathBuf, Option<Vec<u8>>>,
    pub pending_call: Option<String>,
    pub pending_result: Option<raf_ai::agent_runtime::AgentToolResult>,
}

impl ProjectSaveQueue {
    pub fn with_files(expected: BTreeMap<PathBuf, Option<Vec<u8>>>) -> Self {
        Self {
            expected,
            ..Default::default()
        }
    }
    pub fn is_pending(&self) -> bool {
        self.receiver.is_some() || self.error.is_some()
    }
    pub fn is_running(&self) -> bool {
        self.receiver.is_some()
    }

    pub fn submit(&mut self, mut request: DocumentSave) {
        // Never replace the receiver of an in-flight save with a newer generation.
        if self.receiver.is_some() {
            return;
        }
        request.expected = self.expected.clone();
        self.request = Some(request.clone());
        self.error = None;
        let (sender, receiver) = mpsc::channel();
        self.receiver = Some(receiver);
        if let Err(error) = std::thread::Builder::new()
            .name("raf-agent-save".into())
            .spawn(move || {
                let _ = sender.send(commit(request));
            })
        {
            self.receiver = None;
            self.error = Some(error.to_string());
        }
    }

    pub fn poll(&mut self) -> bool {
        let Some(receiver) = self.receiver.as_ref() else {
            return false;
        };
        let result = match receiver.try_recv() {
            Ok(result) => result,
            Err(mpsc::TryRecvError::Empty) => return false,
            Err(mpsc::TryRecvError::Disconnected) => {
                Err("agent_workspace.save_worker_failed".into())
            }
        };
        self.receiver = None;
        match result {
            Ok(receipt) => {
                self.saved_revision = Some(receipt.revision);
                self.expected = receipt.files;
                self.request = None;
                self.error = None;
            }
            Err(error) => self.error = Some(error),
        }
        true
    }

    pub fn retry(&mut self) {
        if self.receiver.is_none() {
            if let Some(request) = self.request.clone() {
                self.submit(request);
            }
        }
    }

    pub fn wait_for_save(&mut self) -> Result<(), String> {
        if let Some(receiver) = self.receiver.take() {
            match receiver.recv() {
                Ok(Ok(receipt)) => {
                    self.saved_revision = Some(receipt.revision);
                    self.expected = receipt.files;
                    self.error = None;
                    self.request = None;
                }
                Ok(Err(error)) => self.error = Some(error),
                Err(_) => self.error = Some("agent_workspace.save_worker_failed".into()),
            }
        }
        self.error.clone().map_or(Ok(()), Err)
    }
}

impl DocumentSave {
    pub fn capture(
        project: &Project,
        session: &ProjectSession,
        scene: &SceneGraph,
        nodes: &NodeGraph,
        electronics: Option<&NativeElectronicsEditor>,
        ledger: &TransactionLedger,
    ) -> Self {
        Self {
            project: project.clone(),
            session: session.clone(),
            scene: scene.clone(),
            nodes: nodes.clone(),
            electronics: electronics.map(|e| (e.schematic().clone(), e.pcb().clone())),
            ledger: ledger.clone(),
            expected: BTreeMap::new(),
        }
    }
}

fn journal_path(project: &Project, session: &ProjectSession) -> Result<PathBuf, String> {
    confined_path(
        &project.path,
        &PathBuf::from(".aura_rafi").join(format!("agent-save-{}.ron", session.id.0)),
    )
}

fn relative_path(path: &Path) -> Result<(), String> {
    if path.as_os_str().is_empty()
        || path
            .components()
            .any(|p| !matches!(p, Component::Normal(_)))
    {
        return Err("agent_workspace.invalid_document_path".into());
    }
    Ok(())
}

pub(crate) fn document_files(session: &ProjectSession, electronics: bool) -> Vec<PathBuf> {
    let mut files = vec![session.scene_file.clone(), session.nodes_file.clone()];
    if electronics {
        files.extend([session.schematic_file.clone(), session.pcb_file.clone()]);
    }
    files
}

pub(crate) fn read_document_files(
    project: &Project,
    session: &ProjectSession,
) -> Result<BTreeMap<PathBuf, Option<Vec<u8>>>, String> {
    document_files(
        session,
        project.project_type == raf_core::project::ProjectType::Electronics,
    )
    .into_iter()
    .map(|path| {
        relative_path(&path)?;
        let bytes = read_recoverable(&confined_path(&project.path, &path)?)?;
        Ok((path, bytes))
    })
    .collect()
}

/// Recovery never overwrites a file changed outside the recorded transaction.
pub(crate) fn recover(project: &Project, session: &ProjectSession) -> Result<(), String> {
    let path = journal_path(project, session)?;
    if !path.exists() {
        return Ok(());
    }
    let bytes = fs::read(&path).map_err(|e| e.to_string())?;
    let journal: SaveJournal = ron::de::from_bytes(&bytes).map_err(|e| e.to_string())?;
    if journal.project_id != project.id || journal.document_id != session.id.0 {
        return Err("agent_workspace.invalid_document_path".into());
    }
    let allowed = document_files(
        session,
        project.project_type == raf_core::project::ProjectType::Electronics,
    );
    for file in &journal.files {
        relative_path(&file.path)?;
        if !allowed.contains(&file.path) {
            return Err("agent_workspace.invalid_document_path".into());
        }
        let current = read_recoverable(&confined_path(&project.path, &file.path)?)?;
        if current != file.before && current.as_deref() != Some(file.after.as_slice()) {
            return Err("agent_workspace.external_changes".into());
        }
    }
    for file in &journal.files {
        write_atomic(&confined_path(&project.path, &file.path)?, &file.after)?;
    }
    write_atomic(&confined_path(&project.path, &PathBuf::from(".aura_rafi").join(format!("saved-{}.json", session.id.0)))?,
        serde_json::to_vec(&serde_json::json!({"project_id": project.id, "document_id": session.id.0, "revision": journal.revision}))
            .map_err(|e| e.to_string())?.as_slice())?;
    fs::remove_file(path).map_err(|e| e.to_string())
}

fn commit(request: DocumentSave) -> Result<SaveReceipt, String> {
    recover(&request.project, &request.session)?;
    let pretty = ron::ser::PrettyConfig::default();
    let mut values = vec![
        (
            request.session.scene_file.clone(),
            ron::ser::to_string_pretty(&request.scene, pretty.clone())
                .map_err(|e| e.to_string())?
                .into_bytes(),
        ),
        (
            request.session.nodes_file.clone(),
            ron::ser::to_string_pretty(&request.nodes, pretty.clone())
                .map_err(|e| e.to_string())?
                .into_bytes(),
        ),
    ];
    if let Some((schematic, pcb)) = &request.electronics {
        values.push((
            request.session.schematic_file.clone(),
            ron::ser::to_string_pretty(schematic, pretty.clone())
                .map_err(|e| e.to_string())?
                .into_bytes(),
        ));
        values.push((
            request.session.pcb_file.clone(),
            ron::ser::to_string_pretty(pcb, pretty)
                .map_err(|e| e.to_string())?
                .into_bytes(),
        ));
    }
    let mut files = Vec::new();
    for (path, after) in values {
        relative_path(&path)?;
        let before = read_recoverable(&confined_path(&request.project.path, &path)?)?;
        if request
            .expected
            .get(&path)
            .is_some_and(|expected| *expected != before)
        {
            // A recovered interrupted save may already have published this generation.
            if before.as_deref() != Some(after.as_slice()) {
                return Err("agent_workspace.external_changes".into());
            }
        }
        files.push(FileChange {
            path,
            before,
            after,
        });
    }
    let journal = SaveJournal {
        project_id: request.project.id,
        document_id: request.session.id.0,
        revision: request.ledger.revision(),
        files,
    };
    let bytes = ron::ser::to_string(&journal).map_err(|e| e.to_string())?;
    write_atomic(
        &journal_path(&request.project, &request.session)?,
        bytes.as_bytes(),
    )?;
    recover(&request.project, &request.session)?;
    Ok(SaveReceipt {
        revision: request.ledger.revision(),
        files: read_document_files(&request.project, &request.session)?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Fixture {
        root: PathBuf,
        project: Project,
        session: ProjectSession,
    }
    impl Fixture {
        fn new() -> Self {
            let root =
                std::env::temp_dir().join(format!("raf-agent-save-{}", uuid::Uuid::new_v4()));
            fs::create_dir_all(&root).unwrap();
            let project = Project::create("Fixture", raf_core::ProjectType::Game, &root).unwrap();
            let registry = raf_core::session::ProjectSessionRegistry::load_or_legacy(
                &project.path,
                project.project_type,
            );
            Self {
                root,
                project,
                session: registry.active().unwrap().clone(),
            }
        }
    }
    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.root);
        }
    }

    #[test]
    fn agent_workspace_save_receipt_and_external_edit_guard() {
        let fixture = Fixture::new();
        let mut scene = SceneGraph::new();
        scene.add_root("Saved cube");
        let mut queue = ProjectSaveQueue {
            expected: read_document_files(&fixture.project, &fixture.session).unwrap(),
            ..Default::default()
        };
        let request = DocumentSave::capture(
            &fixture.project,
            &fixture.session,
            &scene,
            &NodeGraph::new("Main"),
            None,
            &TransactionLedger::new(),
        );
        queue.submit(request.clone());
        queue.wait_for_save().unwrap();
        assert_eq!(queue.saved_revision, Some(0));
        let target = fixture.project.path.join(&fixture.session.scene_file);
        let restored: SceneGraph = ron::de::from_bytes(&fs::read(&target).unwrap()).unwrap();
        assert_eq!(restored.all_live_ids().len(), 1);
        fs::write(&target, b"external generation").unwrap();
        queue.submit(request);
        assert!(queue.wait_for_save().is_err());
        assert_eq!(
            queue.error.as_deref(),
            Some("agent_workspace.external_changes")
        );
        assert_eq!(fs::read(&target).unwrap(), b"external generation");
    }

    #[test]
    fn agent_workspace_interrupted_group_rolls_forward_without_replaying_tools() {
        let fixture = Fixture::new();
        let expected = read_document_files(&fixture.project, &fixture.session).unwrap();
        let files = vec![
            FileChange {
                path: fixture.session.scene_file.clone(),
                before: expected[&fixture.session.scene_file].clone(),
                after: b"next scene".to_vec(),
            },
            FileChange {
                path: fixture.session.nodes_file.clone(),
                before: expected[&fixture.session.nodes_file].clone(),
                after: b"next nodes".to_vec(),
            },
        ];
        let journal = SaveJournal {
            project_id: fixture.project.id,
            document_id: fixture.session.id.0,
            revision: 7,
            files,
        };
        write_atomic(
            &journal_path(&fixture.project, &fixture.session).unwrap(),
            ron::ser::to_string(&journal).unwrap().as_bytes(),
        )
        .unwrap();
        write_atomic(
            &fixture.project.path.join(&fixture.session.scene_file),
            b"next scene",
        )
        .unwrap();
        recover(&fixture.project, &fixture.session).unwrap();
        let files = read_document_files(&fixture.project, &fixture.session).unwrap();
        assert_eq!(
            files[&fixture.session.scene_file].as_deref(),
            Some(b"next scene".as_slice())
        );
        assert_eq!(
            files[&fixture.session.nodes_file].as_deref(),
            Some(b"next nodes".as_slice())
        );
        assert!(!journal_path(&fixture.project, &fixture.session)
            .unwrap()
            .exists());
        recover(&fixture.project, &fixture.session).unwrap();
    }
}
