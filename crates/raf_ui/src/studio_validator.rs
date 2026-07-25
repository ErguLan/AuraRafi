//! Truth validation for RafUI Studio.
//!
//! This validates the UI contract against the command and localization names
//! supplied by the application. It never invents or executes backend actions.

use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};

use crate::studio_diagnostics::{
    action_command, inspect_document, UiStudioDiagnostic, UiStudioDiagnosticCode,
    UiStudioDiagnosticSeverity, UiStudioDocumentReport,
};
use crate::{UiDocument, UiNode};

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct UiStudioCommandRegistry {
    pub commands: BTreeSet<String>,
}

impl UiStudioCommandRegistry {
    pub fn from_names<I, S>(names: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        Self {
            commands: names.into_iter().map(Into::into).collect(),
        }
    }

    pub fn contains(&self, name: &str) -> bool {
        self.commands.contains(name)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UiStudioValidationOptions {
    #[serde(default)]
    pub command_registry: UiStudioCommandRegistry,
    #[serde(default)]
    pub localization_keys: BTreeSet<String>,
    #[serde(default = "default_true")]
    pub require_accessibility_labels: bool,
    #[serde(default = "default_true")]
    pub require_icon_tooltips: bool,
}

fn default_true() -> bool {
    true
}

impl Default for UiStudioValidationOptions {
    fn default() -> Self {
        Self {
            command_registry: UiStudioCommandRegistry::default(),
            localization_keys: BTreeSet::new(),
            require_accessibility_labels: true,
            require_icon_tooltips: true,
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct UiStudioValidationReport {
    pub document: UiStudioDocumentReport,
    pub diagnostics: Vec<UiStudioDiagnostic>,
}

impl UiStudioValidationReport {
    pub fn is_clean(&self) -> bool {
        self.document.is_clean()
            && !self
                .diagnostics
                .iter()
                .any(|diagnostic| diagnostic.severity == UiStudioDiagnosticSeverity::Error)
    }
}

pub fn validate_document(
    document: &UiDocument,
    options: &UiStudioValidationOptions,
) -> UiStudioValidationReport {
    let document_report = inspect_document(document);
    let mut diagnostics = Vec::new();
    validate_node(&document.root, options, &mut diagnostics);
    UiStudioValidationReport {
        document: document_report,
        diagnostics,
    }
}

fn validate_node(
    node: &UiNode,
    options: &UiStudioValidationOptions,
    diagnostics: &mut Vec<UiStudioDiagnostic>,
) {
    if options.require_accessibility_labels
        && node.interactive
        && node.accessibility_label_key.is_none()
    {
        diagnostics.push(UiStudioDiagnostic {
            severity: UiStudioDiagnosticSeverity::Error,
            code: UiStudioDiagnosticCode::InteractiveWithoutAccessibility,
            node_id: node.id.clone(),
            message: "Interactive node cannot ship without an accessibility label key.".to_string(),
        });
    }
    if options.require_icon_tooltips
        && node
            .classes
            .iter()
            .any(|class_name| class_name == "icon-button" || class_name == "floating-action-rail")
        && node.tooltip_key.is_none()
    {
        diagnostics.push(UiStudioDiagnostic {
            severity: UiStudioDiagnosticSeverity::Error,
            code: UiStudioDiagnosticCode::IconButtonWithoutTooltip,
            node_id: node.id.clone(),
            message: "Icon command cannot ship without a tooltip key.".to_string(),
        });
    }
    for binding in &node.event_handlers {
        if let Some(command) = action_command(&binding.action) {
            if !options.command_registry.commands.is_empty()
                && !options.command_registry.contains(command)
            {
                diagnostics.push(UiStudioDiagnostic {
                    severity: UiStudioDiagnosticSeverity::Error,
                    code: UiStudioDiagnosticCode::UnknownCommand,
                    node_id: node.id.clone(),
                    message: format!(
                        "Action '{command}' is not present in the application registry."
                    ),
                });
            }
        }
    }
    if let Some(key) = node_text_key(node) {
        if !options.localization_keys.is_empty() && !options.localization_keys.contains(key) {
            diagnostics.push(UiStudioDiagnostic {
                severity: UiStudioDiagnosticSeverity::Warning,
                code: UiStudioDiagnosticCode::MissingLocalizationKey,
                node_id: node.id.clone(),
                message: format!("Localization key '{key}' is not in the supplied catalog."),
            });
        }
    }
    for child in &node.children {
        validate_node(child, options, diagnostics);
    }
}

fn node_text_key(node: &UiNode) -> Option<&str> {
    node.text_key
        .as_deref()
        .or_else(|| node.tooltip_key.as_deref())
        .filter(|key| !key.is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{UiEventBinding, UiEventKind, UiNode, UiNodeKind};

    #[test]
    fn validator_rejects_actions_outside_the_known_application_contract() {
        let mut document = UiDocument::blank("truth");
        document
            .add_node(
                "root",
                UiNode::new("save", UiNodeKind::Button)
                    .with_accessibility_label_key("save")
                    .with_event(UiEventBinding::command(UiEventKind::Click, "project.save")),
            )
            .unwrap();
        let report = validate_document(
            &document,
            &UiStudioValidationOptions {
                command_registry: UiStudioCommandRegistry::from_names(["project.open"]),
                ..UiStudioValidationOptions::default()
            },
        );
        assert!(!report.is_clean());
        assert!(report
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code == UiStudioDiagnosticCode::UnknownCommand));
    }
}
