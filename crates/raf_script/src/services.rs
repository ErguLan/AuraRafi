//! Runtime services contain data and requests only. Platform I/O belongs to the host.
use crate::{ScriptContext, ScriptError, ScriptResult, ScriptValue};
use raf_core::runtime_config::{valid_storage_name, ProgressSettings};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use uuid::Uuid;

pub use raf_core::runtime_config::CursorCapture;
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CursorIntent {
    pub capture: CursorCapture,
    pub visible: bool,
}
impl Default for CursorIntent {
    fn default() -> Self {
        Self {
            capture: CursorCapture::Free,
            visible: true,
        }
    }
}

pub const PROGRESS_FORMAT_VERSION: u32 = 1;
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProgressDocument {
    pub format_version: u32,
    pub schema_version: u32,
    pub values: BTreeMap<String, ScriptValue>,
}
impl ProgressDocument {
    pub fn validate(&self) -> ScriptResult<()> {
        if self.format_version != PROGRESS_FORMAT_VERSION
            || self.schema_version == 0
            || self.values.len() > 256
        {
            return Err(invalid("progress.invalid_document"));
        }
        for (key, value) in &self.values {
            validate_key(key)?;
            value.validate_event_payload()?;
        }
        Ok(())
    }
}
#[derive(Debug, Clone)]
pub enum ProgressOperation {
    Save(ProgressDocument),
    Load,
    LoadBackup,
}
#[derive(Debug, Clone)]
pub struct ProgressRequest {
    pub id: u64,
    pub revision: u64,
    pub profile: String,
    pub slot: String,
    pub operation: ProgressOperation,
}
#[derive(Debug)]
pub enum ProgressOutcome {
    Saved,
    Loaded(ProgressDocument),
    Missing,
    Failed(String),
}
#[derive(Debug)]
pub struct ProgressReceipt {
    pub id: u64,
    pub revision: u64,
    pub outcome: ProgressOutcome,
}

#[derive(Debug, Clone)]
pub struct ProgressState {
    policy: ProgressSettings,
    profile: String,
    values: BTreeMap<String, ScriptValue>,
    sizes: BTreeMap<String, usize>,
    revision: u64,
    dirty: bool,
    next_id: u64,
    pending: Option<ProgressRequest>,
    busy: bool,
    loaded: Option<ProgressDocument>,
    status: &'static str,
    error: Option<String>,
    last_id: u64,
    autosave_elapsed: f64,
}
impl Default for ProgressState {
    fn default() -> Self {
        Self {
            policy: ProgressSettings::default(),
            profile: "main".into(),
            values: BTreeMap::new(),
            sizes: BTreeMap::new(),
            revision: 0,
            dirty: false,
            next_id: 1,
            pending: None,
            busy: false,
            loaded: None,
            status: "idle",
            error: None,
            last_id: 0,
            autosave_elapsed: 0.0,
        }
    }
}
fn invalid(code: &str) -> ScriptError {
    ScriptError::InvalidArgument(code.into())
}
fn validate_key(key: &str) -> ScriptResult<()> {
    if key.is_empty() || key.len() > 128 {
        Err(invalid("progress.invalid_key"))
    } else {
        Ok(())
    }
}
impl ProgressState {
    pub fn configure(&mut self, policy: ProgressSettings) -> ScriptResult<()> {
        policy.validate().map_err(invalid)?;
        self.policy = policy;
        Ok(())
    }
    fn require_enabled(&self) -> ScriptResult<()> {
        if self.policy.enabled {
            Ok(())
        } else {
            Err(invalid("progress.disabled"))
        }
    }
    pub fn set(&mut self, key: &str, value: ScriptValue) -> ScriptResult<()> {
        self.require_enabled()?;
        validate_key(key)?;
        value.validate_event_payload()?;
        if self.values.get(key) == Some(&value) {
            return Ok(());
        }
        if self.values.len() >= 256 && !self.values.contains_key(key) {
            return Err(invalid("progress.key_limit"));
        }
        let size = serde_json::to_vec(&value)
            .map_err(|_| invalid("progress.invalid_value"))?
            .len()
            + key.len()
            + 16;
        let total: usize = self.sizes.values().sum();
        if total - self.sizes.get(key).copied().unwrap_or(0) + size
            > self.policy.max_slot_bytes.saturating_sub(512) as usize
        {
            return Err(invalid("progress.size_limit"));
        }
        self.sizes.insert(key.into(), size);
        self.values.insert(key.into(), value);
        self.changed();
        Ok(())
    }
    pub fn get(&self, key: &str, fallback: ScriptValue) -> ScriptValue {
        self.values.get(key).cloned().unwrap_or(fallback)
    }
    pub fn remove(&mut self, key: &str) -> ScriptResult<()> {
        self.require_enabled()?;
        validate_key(key)?;
        if self.values.remove(key).is_some() {
            self.sizes.remove(key);
            self.changed();
        }
        Ok(())
    }
    pub fn clear(&mut self) -> ScriptResult<()> {
        self.require_enabled()?;
        if !self.values.is_empty() {
            self.values.clear();
            self.sizes.clear();
            self.changed();
        }
        Ok(())
    }
    /// Explicitly abandon in-memory edits without deleting or overwriting a disk slot.
    pub fn discard(&mut self) -> ScriptResult<()> {
        self.require_enabled()?;
        if self.busy {
            return Err(invalid("progress.busy"));
        }
        self.values.clear();
        self.sizes.clear();
        self.loaded = None;
        self.changed();
        self.dirty = false;
        self.status = "discarded";
        self.error = None;
        self.autosave_elapsed = 0.0;
        Ok(())
    }
    fn changed(&mut self) {
        self.revision = self.revision.saturating_add(1);
        self.dirty = true;
    }
    pub fn set_profile(&mut self, name: &str) -> ScriptResult<()> {
        self.require_enabled()?;
        if name == self.profile {
            return Ok(());
        }
        if !valid_storage_name(name) || self.busy || self.dirty {
            return Err(invalid("progress.profile_requires_clean_idle_state"));
        }
        self.profile = name.into();
        self.values.clear();
        self.sizes.clear();
        self.loaded = None;
        self.status = "idle";
        self.error = None;
        self.autosave_elapsed = 0.0;
        Ok(())
    }
    pub fn request(&mut self, slot: &str, load: bool) -> ScriptResult<u64> {
        self.require_enabled()?;
        if !valid_storage_name(slot) {
            return Err(invalid("progress.invalid_slot"));
        }
        if self.busy {
            return Err(invalid("progress.busy"));
        }
        let id = self.next_id;
        self.next_id = self
            .next_id
            .checked_add(1)
            .ok_or_else(|| invalid("progress.request_limit"))?;
        let operation = if load {
            self.loaded = None;
            ProgressOperation::Load
        } else {
            ProgressOperation::Save(ProgressDocument {
                format_version: PROGRESS_FORMAT_VERSION,
                schema_version: self.policy.schema_version,
                values: self.values.clone(),
            })
        };
        self.pending = Some(ProgressRequest {
            id,
            revision: self.revision,
            profile: self.profile.clone(),
            slot: slot.into(),
            operation,
        });
        self.busy = true;
        self.last_id = id;
        self.status = "pending";
        self.error = None;
        Ok(id)
    }
    pub fn take_request(&mut self) -> Option<ProgressRequest> {
        self.pending.take()
    }
    pub fn request_backup(&mut self, slot: &str) -> ScriptResult<u64> {
        let id = self.request(slot, true)?;
        if let Some(request) = &mut self.pending {
            request.operation = ProgressOperation::LoadBackup;
        }
        Ok(id)
    }
    pub fn complete(&mut self, receipt: ProgressReceipt) {
        if receipt.id != self.last_id {
            return;
        }
        self.busy = false;
        self.error = None;
        match receipt.outcome {
            ProgressOutcome::Saved => {
                self.status = "saved";
                if receipt.revision == self.revision {
                    self.dirty = false;
                }
            }
            ProgressOutcome::Loaded(document) => {
                self.status = if document.schema_version == self.policy.schema_version {
                    "loaded"
                } else {
                    "schema_mismatch"
                };
                self.loaded = Some(document);
            }
            ProgressOutcome::Missing => {
                self.status = "missing";
            }
            ProgressOutcome::Failed(error) => {
                self.status = "error";
                self.error = Some(error);
            }
        }
    }
    /// Loading stages a document. Restoration is always a separate authored decision.
    pub fn apply_loaded(&mut self) -> ScriptResult<()> {
        self.require_enabled()?;
        if self.busy {
            return Err(invalid("progress.busy"));
        }
        let document = self
            .loaded
            .as_ref()
            .ok_or_else(|| invalid("progress.no_loaded_document"))?;
        if document.schema_version != self.policy.schema_version {
            return Err(invalid("progress.schema_mismatch"));
        }
        document.validate()?;
        let values = document.values.clone();
        let mut candidate = ProgressState::default();
        candidate.configure(self.policy.clone())?;
        for (key, value) in values {
            candidate.set(&key, value)?;
        }
        self.values = candidate.values;
        self.sizes = candidate.sizes;
        self.changed();
        self.dirty = false;
        self.status = "restored";
        Ok(())
    }
    pub fn loaded_get(&self, key: &str, fallback: ScriptValue) -> ScriptValue {
        self.loaded
            .as_ref()
            .and_then(|d| d.values.get(key))
            .cloned()
            .unwrap_or(fallback)
    }
    pub fn report(&self) -> ScriptValue {
        ScriptValue::Record(BTreeMap::from([
            ("request".into(), ScriptValue::Int(self.last_id as i64)),
            ("status".into(), ScriptValue::String(self.status.into())),
            (
                "error".into(),
                self.error
                    .clone()
                    .map(ScriptValue::String)
                    .unwrap_or_default(),
            ),
            ("dirty".into(), ScriptValue::Bool(self.dirty)),
            ("busy".into(), ScriptValue::Bool(self.busy)),
            ("profile".into(), ScriptValue::String(self.profile.clone())),
            (
                "schema".into(),
                ScriptValue::Int(self.policy.schema_version as i64),
            ),
            (
                "loaded_schema".into(),
                ScriptValue::Int(self.loaded.as_ref().map_or(0, |d| d.schema_version) as i64),
            ),
        ]))
    }
    pub fn autosave(&mut self, dt: f64) {
        if !self.policy.enabled || !self.policy.autosave {
            return;
        }
        self.autosave_elapsed += dt.clamp(0.0, 0.25);
        if self.autosave_elapsed >= self.policy.autosave_seconds as f64 && self.dirty && !self.busy
        {
            let slot = self.policy.autosave_slot.clone();
            if self.request(&slot, false).is_ok() {
                self.autosave_elapsed = 0.0;
            }
        }
    }
    pub fn busy(&self) -> bool {
        self.busy
    }
}

#[derive(Debug, Default)]
pub struct RuntimeServices {
    pub cursor: CursorIntent,
    pub effective_cursor: CursorCapture,
    pub progress: ProgressState,
    pub current_session: Option<Uuid>,
    pub requested_session: Option<Uuid>,
    pub transition_status: &'static str,
    pub transition_error: Option<String>,
}
impl ScriptContext<'_> {
    pub fn set_cursor(&mut self, capture: CursorCapture, visible: bool) -> ScriptResult<()> {
        if self.view.role == crate::view::ScriptRole::Server {
            return Err(invalid("input.no_local_view"));
        }
        self.view.services.cursor = CursorIntent { capture, visible };
        Ok(())
    }
    pub fn request_session(&mut self, session: Uuid) -> ScriptResult<()> {
        if session.is_nil() || matches!(self.view.services.transition_status, "pending" | "loading")
        {
            return Err(invalid("runtime.invalid_or_busy_transition"));
        }
        self.view.services.requested_session = Some(session);
        self.view.services.transition_status = "pending";
        self.view.services.transition_error = None;
        Ok(())
    }
    pub fn session_report(&self) -> ScriptValue {
        ScriptValue::Record(BTreeMap::from([
            (
                "current".into(),
                self.view
                    .services
                    .current_session
                    .map(|id| ScriptValue::String(id.to_string()))
                    .unwrap_or_default(),
            ),
            (
                "status".into(),
                ScriptValue::String(self.view.services.transition_status.into()),
            ),
            (
                "error".into(),
                self.view
                    .services
                    .transition_error
                    .clone()
                    .map(ScriptValue::String)
                    .unwrap_or_default(),
            ),
        ]))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn runtime_services_progress_is_explicit_bounded_and_preserves_new_dirty_data() {
        let mut state = ProgressState::default();
        assert!(state.set("invalid", ScriptValue::Handle(3)).is_err());
        state.set("score", ScriptValue::Int(1)).unwrap();
        state.autosave(1000.0);
        assert!(state.take_request().is_none());
        assert!(state.request("../escape", false).is_err());
        let id = state.request("manual", false).unwrap();
        let request = state.take_request().unwrap();
        assert!(state.request("other", false).is_err());
        state.set("score", ScriptValue::Int(2)).unwrap();
        state.complete(ProgressReceipt {
            id,
            revision: request.revision,
            outcome: ProgressOutcome::Saved,
        });
        assert!(state.dirty);
        assert!(state.set_profile("other").is_err());
        let id = state.request("manual", true).unwrap();
        let request = state.take_request().unwrap();
        let mut document = ProgressDocument {
            format_version: 1,
            schema_version: 1,
            values: BTreeMap::from([("score".into(), ScriptValue::Int(5))]),
        };
        state.complete(ProgressReceipt {
            id,
            revision: request.revision,
            outcome: ProgressOutcome::Loaded(document.clone()),
        });
        assert_eq!(state.get("score", ScriptValue::None), ScriptValue::Int(2));
        state.apply_loaded().unwrap();
        assert_eq!(state.get("score", ScriptValue::None), ScriptValue::Int(5));
        document.schema_version = 2;
        let id = state.request("manual", true).unwrap();
        state.take_request();
        state.complete(ProgressReceipt {
            id,
            revision: 0,
            outcome: ProgressOutcome::Loaded(document),
        });
        assert!(state.apply_loaded().is_err());
        assert_eq!(
            state.loaded_get("score", ScriptValue::None),
            ScriptValue::Int(5)
        );
        assert_eq!(state.get("score", ScriptValue::None), ScriptValue::Int(5));
    }
}
