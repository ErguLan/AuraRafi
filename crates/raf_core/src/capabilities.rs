//! Self-describing engine capabilities shared by the CLI, MCP and Agent.
//!
//! The JSON catalog remains the single authoring source for editor commands.
//! This module gives headless clients a lightweight, renderer/UI-independent
//! view of that catalog without depending on `raf_editor` or Egui.

use serde::{Deserialize, Serialize};

const BUILTIN_CATALOG: &str = include_str!("../../../assets/commands/catalog.json");

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CapabilityParameter {
    pub name: String,
    pub kind: String,
    #[serde(default)]
    pub required: bool,
    #[serde(default)]
    pub default: Option<String>,
    #[serde(default)]
    pub description_key: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CapabilityDefinition {
    pub name: String,
    #[serde(default)]
    pub aliases: Vec<String>,
    pub domain: String,
    pub category: String,
    pub description_key: String,
    #[serde(default)]
    pub parameters: Vec<CapabilityParameter>,
    #[serde(default)]
    pub examples: Vec<String>,
}

impl CapabilityDefinition {
    /// Conservative classification used by external clients before a
    /// project-specific executor is attached.
    pub fn is_read_only(&self) -> bool {
        let name = self.name.as_str();
        name == "help"
            || name == "commands"
            || name == "describe"
            || name == "history"
            || name.ends_with(".info")
            || name.ends_with(".list")
            || name.ends_with(".describe")
            || name.contains(".describe_")
            || name.ends_with(".search")
            || name == "workspace.read"
            || name == "workspace.search"
    }

    pub fn risk(&self) -> &'static str {
        if self.is_read_only() {
            "read"
        } else if self.name == "undo" || self.name == "redo" {
            "reversible_write"
        } else {
            "write"
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CapabilityCatalog {
    pub version: u32,
    pub capabilities: Vec<CapabilityDefinition>,
}

impl CapabilityCatalog {
    pub fn builtin() -> Self {
        let parsed: serde_json::Value = serde_json::from_str(BUILTIN_CATALOG)
            .expect("assets/commands/catalog.json must be valid JSON");
        let version = parsed
            .get("version")
            .and_then(serde_json::Value::as_u64)
            .unwrap_or_default() as u32;
        let capabilities = parsed
            .get("commands")
            .cloned()
            .and_then(|value| serde_json::from_value(value).ok())
            .unwrap_or_default();
        Self {
            version,
            capabilities,
        }
    }

    pub fn find(&self, name: &str) -> Option<&CapabilityDefinition> {
        let normalized = normalize(name);
        self.capabilities.iter().find(|capability| {
            normalize(&capability.name) == normalized
                || capability
                    .aliases
                    .iter()
                    .any(|alias| normalize(alias) == normalized)
        })
    }

    pub fn search(&self, query: &str) -> Vec<&CapabilityDefinition> {
        let query = query.trim().to_ascii_lowercase();
        let mut results: Vec<_> = self
            .capabilities
            .iter()
            .filter(|capability| {
                query.is_empty()
                    || capability.name.to_ascii_lowercase().contains(&query)
                    || capability
                        .aliases
                        .iter()
                        .any(|alias| alias.to_ascii_lowercase().contains(&query))
                    || capability.domain.to_ascii_lowercase().contains(&query)
                    || capability.category.to_ascii_lowercase().contains(&query)
                    || capability
                        .description_key
                        .to_ascii_lowercase()
                        .contains(&query)
                    || capability
                        .examples
                        .iter()
                        .any(|example| example.to_ascii_lowercase().contains(&query))
            })
            .collect();
        results.sort_by(|left, right| left.name.cmp(&right.name));
        results
    }

    pub fn for_domain(&self, domain: &str) -> Vec<&CapabilityDefinition> {
        self.capabilities
            .iter()
            .filter(|capability| capability.domain == "shared" || capability.domain == domain)
            .collect()
    }

    pub fn as_json(&self) -> serde_json::Value {
        serde_json::json!({
            "version": self.version,
            "capabilities": self.capabilities.iter().map(|capability| {
                serde_json::json!({
                    "name": capability.name,
                    "aliases": capability.aliases,
                    "domain": capability.domain,
                    "category": capability.category,
                    "description_key": capability.description_key,
                    "parameters": capability.parameters,
                    "examples": capability.examples,
                    "risk": capability.risk(),
                    "read_only": capability.is_read_only(),
                })
            }).collect::<Vec<_>>(),
        })
    }
}

fn normalize(value: &str) -> String {
    value
        .trim()
        .trim_start_matches('/')
        .to_ascii_lowercase()
        .replace('-', "_")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builtin_catalog_is_available_without_editor_or_ui() {
        let catalog = CapabilityCatalog::builtin();
        assert!(catalog.version >= 1);
        assert!(catalog.find("game.add").is_some());
        assert!(catalog.find("/game.add").is_some());
    }

    #[test]
    fn search_is_stable_and_classifies_risk() {
        let catalog = CapabilityCatalog::builtin();
        let results = catalog.search("scene");
        assert!(results.windows(2).all(|pair| pair[0].name <= pair[1].name));
        assert_eq!(catalog.find("game.describe_scene").unwrap().risk(), "read");
        assert_eq!(catalog.find("game.add").unwrap().risk(), "write");
    }
}
