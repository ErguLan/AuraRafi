//! Bounded, provider-neutral project perception for Agent, CLI and MCP.
//!
//! The model should not discover a scene by walking arbitrary project files.
//! This module exposes the small semantic view that every adapter can share:
//! stable entity references, hierarchy, bounded queries, asset usage and
//! verification. It deliberately has no editor, renderer or transport
//! dependency.

use crate::transaction::VerificationSummary;
use serde::{Deserialize, Serialize};
use serde_json::Value;

mod assets;
mod audit;
mod compact;
mod diagnostics;
mod project;
mod scene;
mod scripts;
mod spatial;
mod support;
#[cfg(test)]
mod tests;

pub const DEFAULT_PAGE_SIZE: usize = 48;
pub const MAX_PAGE_SIZE: usize = 128;

pub use assets::{asset_inspect, assets_catalog, assets_recommend};
pub use audit::{scene_design_audit, scene_navigation_audit};
pub use compact::{compact_result_data, compact_result_lines};
pub use diagnostics::{scene_diff, scene_fingerprint, scene_verify};
pub use project::{project_health, project_health_scoped};
pub use scene::{
    display_name, display_path, resolve_target, scene_context, scene_digest, scene_inspect,
    scene_outline, scene_query, selection_info, world_bounds,
};
pub use scripts::scripts_catalog;
pub use spatial::{scene_check_overlaps, scene_spatial_map};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AgentObservationResult {
    pub title: String,
    pub summary: String,
    pub data: Value,
    #[serde(default)]
    pub warnings: Vec<String>,
    #[serde(default)]
    pub verification: Option<VerificationSummary>,
}

impl AgentObservationResult {
    fn new(title: impl Into<String>, summary: impl Into<String>, data: Value) -> Self {
        Self {
            title: title.into(),
            summary: summary.into(),
            data,
            warnings: Vec::new(),
            verification: None,
        }
    }

    /// Return the machine-level outcome of an observation. Human-readable
    /// warnings may be non-fatal, but a failed verification or an explicit
    /// `found: false` must never be reported as a successful tool call.
    pub fn is_success(&self) -> bool {
        let found = self
            .data
            .get("found")
            .and_then(Value::as_bool)
            .unwrap_or(true);
        let verification_failed = self
            .verification
            .as_ref()
            .is_some_and(|verification| verification.status == "failed");
        found && !verification_failed
    }
}
