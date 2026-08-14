//! Provider-neutral execution metadata for commands and agent harnesses.
//!
//! This is deliberately independent of scene/runtime implementations. It
//! lets the CLI, MCP adapter and editor agree on preview, revision and
//! recovery semantics before every domain has a full undo implementation.

use serde::{Deserialize, Serialize};
use serde_json::Value;
use uuid::Uuid;

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

/// Small in-memory ledger used by hosts to make revision and idempotency
/// checks deterministic. Domain hosts can persist richer records later.
#[derive(Debug, Clone)]
pub struct TransactionLedger {
    revision: Revision,
    records: Vec<TransactionRecord>,
    max_records: usize,
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
        }
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
}
