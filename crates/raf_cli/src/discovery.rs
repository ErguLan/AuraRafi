//! Discovery of running AuraRafi editors.
//!
//! The editor publishes a per-project attach descriptor at
//! `<project>/.aura_rafi/agent_endpoint.json` while it runs. This module
//! turns those scattered descriptors into one global view: it walks the
//! recent-projects registry, probes each descriptor with a short loopback
//! connect, and reports which editors are actually alive right now.
//!
//! With this, `raf editors` answers "what is running?" and `raf attach`
//! without a path can auto-connect when exactly one editor is live.

use raf_core::ipc::EndpointDescriptor;
use raf_core::project::RecentProjects;
use serde_json::json;
use std::net::{SocketAddr, TcpStream};
use std::path::{Path, PathBuf};
use std::time::Duration;

/// Loopback connects in well under a millisecond when a listener exists.
/// A few hundred milliseconds is generous and still fails instantly next to
/// the previous OS-level timeout on a dead port.
const PROBE_TIMEOUT: Duration = Duration::from_millis(400);

#[derive(Debug, Clone)]
pub struct DiscoveredEditor {
    pub descriptor: EndpointDescriptor,
    pub project_path: PathBuf,
    pub project_name: String,
    pub alive: bool,
}

impl DiscoveredEditor {
    pub fn to_json(&self) -> serde_json::Value {
        json!({
            "project_name": self.project_name,
            "project_path": self.project_path,
            "address": self.descriptor.address,
            "process_id": self.descriptor.process_id,
            "project_type": self.descriptor.project_type,
            "editor_state": self.descriptor.editor_state,
            "session_name": self.descriptor.session_name,
            "revision": self.descriptor.revision,
            "capabilities": self.descriptor.capabilities,
            "alive": self.alive,
        })
    }
}

fn registry_dir() -> Option<PathBuf> {
    dirs_next::config_dir().map(|dir| dir.join("AuraRafi"))
}

/// Recent project paths, newest first. The registry is the same one the Hub
/// uses, so every project the user actually opens is already listed here.
fn recent_project_paths() -> Vec<(String, PathBuf)> {
    let Some(dir) = registry_dir() else {
        return Vec::new();
    };
    let entries: Vec<(String, PathBuf)> = RecentProjects::load(&dir)
        .projects
        .into_iter()
        .map(|entry| (entry.name, entry.path))
        .collect();
    // The registry stores most-recent-first already; keep that order stable.
    entries
}

/// TCP liveness probe. A successful connect proves an editor listener owns
/// the port; the connection is dropped immediately without a handshake.
fn probe_alive(address: &str) -> bool {
    let Ok(address) = address.parse::<SocketAddr>() else {
        return false;
    };
    TcpStream::connect_timeout(&address, PROBE_TIMEOUT).is_ok()
}

/// Collects candidate project paths: the recent registry plus any explicit
/// extra path (for example the current working directory).
fn candidate_paths(extra: Option<&Path>) -> Vec<(String, PathBuf)> {
    let mut seen = std::collections::BTreeSet::new();
    let mut candidates = Vec::new();
    for (name, path) in recent_project_paths() {
        if seen.insert(path.clone()) {
            candidates.push((name, path));
        }
    }
    if let Some(path) = extra {
        if seen.insert(path.to_path_buf()) {
            candidates.push((
                path.file_name()
                    .map(|name| name.to_string_lossy().to_string())
                    .unwrap_or_default(),
                path.to_path_buf(),
            ));
        }
    }
    candidates
}

/// Scans every known project for a published attach descriptor and probes
/// whether its editor still listens.
pub fn discover_editors(extra: Option<&Path>) -> Vec<DiscoveredEditor> {
    let mut found = Vec::new();
    for (name, path) in candidate_paths(extra) {
        let Ok(descriptor) = EndpointDescriptor::load_from_project(&path) else {
            continue;
        };
        let alive = probe_alive(&descriptor.address);
        found.push(DiscoveredEditor {
            descriptor,
            project_path: path,
            project_name: name,
            alive,
        });
    }
    found
}

pub fn live_editors(editors: &[DiscoveredEditor]) -> Vec<&DiscoveredEditor> {
    editors.iter().filter(|editor| editor.alive).collect()
}

/// Chooses the editor to auto-attach to: exactly one live editor succeeds;
/// zero or several produce actionable errors instead of guessing.
pub fn pick_single_live(extra: Option<&Path>) -> Result<DiscoveredEditor, String> {
    let editors = discover_editors(extra);
    let live = live_editors(&editors);
    match live.len() {
        1 => Ok(live[0].clone()),
        0 => Err(format!(
            "No live AuraRafi editor found across {} known project(s). \
             Open a project first or pass --project PATH.",
            editors.len()
        )),
        _ => {
            let listing = live
                .iter()
                .map(|editor| format!("  - {}", editor.project_path.display()))
                .collect::<Vec<_>>()
                .join("\n");
            Err(format!(
                "{} editors are live; disambiguate with --project PATH:\n{listing}",
                live.len()
            ))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn probe_rejects_unroutable_loopback_port_quickly() {
        // Port 1 on loopback is never owned by Rafi in tests; the probe must
        // return false instead of erroring or hanging past PROBE_TIMEOUT.
        assert!(!probe_alive("127.0.0.1:1"));
    }

    #[test]
    fn probe_invalid_address_is_not_alive() {
        assert!(!probe_alive("not-an-address"));
    }
}
