//! Provider-neutral execution metadata for commands and agent harnesses.
//!
//! This is deliberately independent of scene/runtime implementations. It
//! lets the CLI, MCP adapter and editor agree on preview, revision and
//! recovery semantics before every domain has a full undo implementation.

use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::fs;
use std::path::Path;
use uuid::Uuid;

/// File name for the small, project-scoped command state used by attached
/// clients. Undo snapshots remain editor-memory-only; this file only keeps
/// the optimistic-concurrency clock and the last observed document identity.
pub const AGENT_STATE_FILE: &str = "agent_state.json";

pub type Revision = u64;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct TransactionId(pub Uuid);

impl TransactionId {
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }
}

impl Default for TransactionId {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct UndoToken(pub Uuid);

impl UndoToken {
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }
}

impl Default for UndoToken {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ExecutionBudget {
    #[serde(default)]
    pub max_tool_calls: Option<u32>,
    #[serde(default)]
    pub max_milliseconds: Option<u64>,
    /// Maximum number of scene operations a single semantic build may stage.
    /// This is separate from transport frame size and lets an agent ask for a
    /// safe preflight before constructing a large scene.
    #[serde(default)]
    pub max_scene_operations: Option<u32>,
    /// Maximum live scene nodes allowed after a bounded authoring operation.
    /// Hosts that expose a scene can preflight this before mutating it.
    #[serde(default)]
    pub max_scene_entities: Option<u32>,
    /// Maximum serialized result size requested by an adapter. Adapters may
    /// return a compact result and expose details as an artifact instead.
    #[serde(default)]
    pub max_result_bytes: Option<u32>,
    #[serde(default)]
    pub profile: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ArtifactRef {
    pub id: String,
    pub kind: String,
    pub uri: String,
    #[serde(default)]
    pub label: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct VerificationSummary {
    pub status: String,
    #[serde(default)]
    pub checks: Vec<String>,
    #[serde(default)]
    pub failures: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TransactionRecord {
    pub id: TransactionId,
    pub revision_before: Revision,
    pub revision_after: Revision,
    pub changed: bool,
    #[serde(default)]
    pub diff: Option<Value>,
    #[serde(default)]
    pub undo_token: Option<UndoToken>,
    #[serde(default)]
    pub idempotency_key: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
struct PersistedTransactionState {
    #[serde(default = "default_state_version")]
    version: u16,
    #[serde(default)]
    revision: Revision,
    #[serde(default)]
    document_fingerprint: Option<u64>,
}

fn default_state_version() -> u16 {
    1
}

/// Small in-memory ledger used by hosts to make revision and idempotency
/// checks deterministic. Domain hosts can persist richer records later.
#[derive(Debug, Clone)]
pub struct TransactionLedger {
    revision: Revision,
    records: Vec<TransactionRecord>,
    max_records: usize,
    document_fingerprint: Option<u64>,
}

impl Default for TransactionLedger {
    fn default() -> Self {
        Self::new()
    }
}

impl TransactionLedger {
    pub fn new() -> Self {
        Self {
            revision: 0,
            records: Vec::new(),
            max_records: 256,
            document_fingerprint: None,
        }
    }

    /// Rehydrate only the durable part of a ledger. Transaction records and
    /// undo tokens intentionally do not cross process boundaries because they
    /// refer to editor-memory snapshots that no longer exist after a restart.
    pub fn load_for_project(project_path: &Path, document_fingerprint: u64) -> Self {
        let mut ledger = Self::load_persisted_for_project(project_path);
        // A changed on-disk document must invalidate an old expected revision
        // even when the previous editor process was closed normally.
        ledger.observe_document(document_fingerprint);
        ledger
    }

    /// Rehydrate the durable clock without observing a scene. This variant is
    /// used by the stateless CLI, where loading a project does not mount the
    /// live editor scene graph.
    pub fn load_persisted_for_project(project_path: &Path) -> Self {
        let state_path = project_path
            .join(crate::ipc::ATTACH_DIRECTORY)
            .join(AGENT_STATE_FILE);
        let backup_path = state_path.with_extension("json.bak");
        let persisted = [state_path, backup_path].into_iter().find_map(|path| {
            fs::read_to_string(path)
                .ok()
                .and_then(|raw| serde_json::from_str::<PersistedTransactionState>(&raw).ok())
        });
        let mut ledger = Self::new();
        if let Some(state) = persisted {
            ledger.revision = state.revision;
            ledger.document_fingerprint = state.document_fingerprint;
        }
        ledger
    }

    /// Persist the concurrency clock next to the editor attach descriptor.
    /// The temporary file is published with a replace-and-restore sequence so
    /// a Windows rename failure does not silently delete the previous state.
    /// This is deliberately independent from project document saving.
    pub fn persist_for_project(&self, project_path: &Path) -> Result<(), String> {
        let directory = project_path.join(crate::ipc::ATTACH_DIRECTORY);
        fs::create_dir_all(&directory)
            .map_err(|error| format!("Unable to create {}: {error}", directory.display()))?;
        let path = directory.join(AGENT_STATE_FILE);
        let temporary = directory.join("agent_state.json.tmp");
        let backup = directory.join("agent_state.json.bak");
        let state = PersistedTransactionState {
            version: default_state_version(),
            revision: self.revision,
            document_fingerprint: self.document_fingerprint,
        };
        let data = serde_json::to_string_pretty(&state)
            .map_err(|error| format!("Agent state encode: {error}"))?;
        fs::write(&temporary, data)
            .map_err(|error| format!("Unable to write {}: {error}", temporary.display()))?;
        let had_existing = path.exists();
        if had_existing {
            let _ = fs::remove_file(&backup);
            if let Err(error) = fs::rename(&path, &backup) {
                let _ = fs::remove_file(&temporary);
                return Err(format!("Unable to stage {}: {error}", path.display()));
            }
        }
        if let Err(error) = fs::rename(&temporary, &path) {
            let _ = fs::remove_file(&temporary);
            if had_existing {
                let _ = fs::rename(&backup, &path);
            }
            return Err(format!("Unable to publish {}: {error}", path.display()));
        }
        if had_existing {
            let _ = fs::remove_file(&backup);
        }
        Ok(())
    }

    /// Observe a document mutation made outside the command gateway. The
    /// first observation establishes identity; every later identity change
    /// advances the revision exactly once until the caller marks it known.
    pub fn observe_document(&mut self, fingerprint: u64) -> bool {
        match self.document_fingerprint {
            None => {
                self.document_fingerprint = Some(fingerprint);
                false
            }
            Some(previous) if previous == fingerprint => false,
            Some(_) => {
                self.revision = self.revision.saturating_add(1);
                self.document_fingerprint = Some(fingerprint);
                self.records.clear();
                true
            }
        }
    }

    /// Mark a fingerprint as the result of a mutation already recorded by the
    /// command gateway. This prevents the next redraw from counting that same
    /// command twice.
    pub fn mark_document(&mut self, fingerprint: u64) {
        self.document_fingerprint = Some(fingerprint);
    }

    pub fn document_fingerprint(&self) -> Option<u64> {
        self.document_fingerprint
    }

    pub fn revision(&self) -> Revision {
        self.revision
    }

    pub fn check_expected(&self, expected: Option<Revision>) -> Result<(), String> {
        if let Some(expected) = expected {
            if expected != self.revision {
                return Err(format!(
                    "Revision conflict: expected {expected}, current {}.",
                    self.revision
                ));
            }
        }
        Ok(())
    }

    pub fn find_idempotency_key(&self, key: Option<&str>) -> Option<&TransactionRecord> {
        let key = key?.trim();
        if key.is_empty() {
            return None;
        }
        self.records
            .iter()
            .rev()
            .find(|record| record.idempotency_key.as_deref() == Some(key))
    }

    pub fn record(
        &mut self,
        id: TransactionId,
        changed: bool,
        diff: Option<Value>,
        idempotency_key: Option<String>,
    ) -> TransactionRecord {
        self.record_internal(id, changed, diff, idempotency_key, true)
    }

    /// Records a transaction for a host that cannot yet execute undo. The
    /// revision and diff remain useful to an agent, but no false undo affordance
    /// is exposed to the caller.
    pub fn record_without_undo(
        &mut self,
        id: TransactionId,
        changed: bool,
        diff: Option<Value>,
        idempotency_key: Option<String>,
    ) -> TransactionRecord {
        self.record_internal(id, changed, diff, idempotency_key, false)
    }

    fn record_internal(
        &mut self,
        id: TransactionId,
        changed: bool,
        diff: Option<Value>,
        idempotency_key: Option<String>,
        undo_supported: bool,
    ) -> TransactionRecord {
        let revision_before = self.revision;
        if changed {
            self.revision = self.revision.saturating_add(1);
        }
        let record = TransactionRecord {
            id,
            revision_before,
            revision_after: self.revision,
            changed,
            diff,
            undo_token: (changed && undo_supported).then(UndoToken::new),
            idempotency_key,
        };
        self.records.push(record.clone());
        if self.records.len() > self.max_records {
            self.records.remove(0);
        }
        record
    }

    pub fn records(&self) -> &[TransactionRecord] {
        &self.records
    }

    /// Attach a token only after the host has installed a real rollback
    /// snapshot. This keeps the generic gateway honest while allowing an
    /// editor-owned host to advertise undo after it has proven the snapshot
    /// is available.
    pub fn attach_undo_token(&mut self, id: TransactionId, token: UndoToken) -> bool {
        let Some(record) = self
            .records
            .iter_mut()
            .rev()
            .find(|record| record.id == id && record.changed && record.undo_token.is_none())
        else {
            return false;
        };
        record.undo_token = Some(token);
        true
    }

    /// Attach the semantic diff observed by a host after the domain executor
    /// has committed its mutation. Generic command handlers may not know the
    /// document-level diff while they are running, but the host can still
    /// make idempotent replays return the same evidence as the first call.
    pub fn attach_diff(&mut self, id: TransactionId, diff: Value) -> bool {
        let Some(record) = self
            .records
            .iter_mut()
            .rev()
            .find(|record| record.id == id && record.changed && record.diff.is_none())
        else {
            return false;
        };
        record.diff = Some(diff);
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ledger_revisions_only_advance_for_changes() {
        let mut ledger = TransactionLedger::new();
        let preview = ledger.record(TransactionId::new(), false, None, None);
        assert_eq!(preview.revision_before, 0);
        assert_eq!(preview.revision_after, 0);
        let change = ledger.record(
            TransactionId::new(),
            true,
            Some(serde_json::json!({"created": ["entity"]})),
            Some("once".to_string()),
        );
        assert_eq!(change.revision_after, 1);
        assert!(change.undo_token.is_some());
        assert!(ledger.find_idempotency_key(Some("once")).is_some());
    }

    #[test]
    fn expected_revision_conflicts_are_explicit() {
        let mut ledger = TransactionLedger::new();
        ledger.record(TransactionId::new(), true, None, None);
        assert!(ledger.check_expected(Some(1)).is_ok());
        assert!(ledger.check_expected(Some(0)).is_err());
    }

    #[test]
    fn hosts_without_undo_do_not_advertise_a_fake_token() {
        let mut ledger = TransactionLedger::new();
        let record = ledger.record_without_undo(TransactionId::new(), true, None, None);
        assert!(record.undo_token.is_none());
        assert_eq!(record.revision_after, 1);
    }

    #[test]
    fn host_can_attach_real_undo_token_to_changed_record() {
        let mut ledger = TransactionLedger::new();
        let id = TransactionId::new();
        let record = ledger.record_without_undo(id, true, None, None);
        let token = UndoToken::new();
        assert!(ledger.attach_undo_token(id, token));
        assert_eq!(ledger.records().last().unwrap().undo_token, Some(token));
        assert!(!ledger.attach_undo_token(id, UndoToken::new()));
        assert_eq!(record.revision_after, 1);
    }

    #[test]
    fn host_can_attach_document_diff_for_idempotent_replay() {
        let mut ledger = TransactionLedger::new();
        let id = TransactionId::new();
        ledger.record_without_undo(id, true, None, Some("diff".to_string()));
        let diff = serde_json::json!({"created": ["entity:one"]});
        assert!(ledger.attach_diff(id, diff.clone()));
        assert_eq!(ledger.records().last().unwrap().diff, Some(diff.clone()));
        assert!(!ledger.attach_diff(id, serde_json::json!({"updated": []})));
        assert_eq!(ledger.records().last().unwrap().diff, Some(diff));
    }

    #[test]
    fn persisted_revision_survives_restart_and_document_changes_advance_once() {
        let project_path = std::env::temp_dir().join(format!("raf-agent-state-{}", Uuid::new_v4()));
        std::fs::create_dir_all(&project_path).unwrap();

        let mut ledger = TransactionLedger::new();
        assert!(!ledger.observe_document(7));
        ledger.record_without_undo(TransactionId::new(), true, None, None);
        ledger.persist_for_project(&project_path).unwrap();

        let loaded = TransactionLedger::load_persisted_for_project(&project_path);
        assert_eq!(loaded.revision(), 1);
        assert_eq!(loaded.document_fingerprint(), Some(7));

        let mut observed = TransactionLedger::load_for_project(&project_path, 8);
        assert_eq!(observed.revision(), 2);
        assert_eq!(observed.document_fingerprint(), Some(8));
        assert!(!observed.observe_document(8));

        let _ = std::fs::remove_dir_all(project_path);
    }
}
