//! Data-only authoring diagnostics for RafUI Studio.

use std::collections::HashSet;

use serde::{Deserialize, Serialize};

use crate::{UiAction, UiDocument, UiNode, UiNodeKind};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum UiStudioDiagnosticSeverity {
    Info,
    Warning,
    Error,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum UiStudioDiagnosticCode {
    DuplicateNodeId,
    InteractiveWithoutAccessibility,
    IconButtonWithoutTooltip,
    InteractiveWithoutAction,
    EmptyTextKey,
    EmptyTooltipKey,
    InvalidFixedSize,
    MissingTextInputBinding,
    UnknownCommand,
    MissingLocalizationKey,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UiStudioDiagnostic {
    pub severity: UiStudioDiagnosticSeverity,
    pub code: UiStudioDiagnosticCode,
    pub node_id: String,
    pub message: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct UiStudioDocumentReport {
    pub node_count: usize,
    pub interactive_count: usize,
    pub diagnostics: Vec<UiStudioDiagnostic>,
}

impl UiStudioDocumentReport {
    pub fn errors(&self) -> usize {
        self.diagnostics
            .iter()
            .filter(|diagnostic| diagnostic.severity == UiStudioDiagnosticSeverity::Error)
            .count()
    }

    pub fn warnings(&self) -> usize {
        self.diagnostics
            .iter()
            .filter(|diagnostic| diagnostic.severity == UiStudioDiagnosticSeverity::Warning)
            .count()
    }

    pub fn is_clean(&self) -> bool {
        self.errors() == 0
    }
}

pub fn inspect_document(document: &UiDocument) -> UiStudioDocumentReport {
    let mut report = UiStudioDocumentReport::default();
    let mut ids = HashSet::new();
    inspect_node(&document.root, &mut ids, &mut report);
    report
}

fn inspect_node(node: &UiNode, ids: &mut HashSet<String>, report: &mut UiStudioDocumentReport) {
    report.node_count += 1;
    if node.interactive {
        report.interactive_count += 1;
    }
    if !ids.insert(node.id.clone()) {
        report.diagnostics.push(UiStudioDiagnostic {
            severity: UiStudioDiagnosticSeverity::Error,
            code: UiStudioDiagnosticCode::DuplicateNodeId,
            node_id: node.id.clone(),
            message: "Node id is duplicated; input and style resolution are ambiguous.".to_string(),
        });
    }
    if node.interactive && node.accessibility_label_key.is_none() {
        report.diagnostics.push(UiStudioDiagnostic {
            severity: UiStudioDiagnosticSeverity::Warning,
            code: UiStudioDiagnosticCode::InteractiveWithoutAccessibility,
            node_id: node.id.clone(),
            message: "Interactive node has no accessibility label key.".to_string(),
        });
    }
    if node
        .classes
        .iter()
        .any(|class_name| class_name == "icon-button" || class_name == "floating-action-rail")
        && node.tooltip_key.is_none()
    {
        report.diagnostics.push(UiStudioDiagnostic {
            severity: UiStudioDiagnosticSeverity::Warning,
            code: UiStudioDiagnosticCode::IconButtonWithoutTooltip,
            node_id: node.id.clone(),
            message: "Icon command has no tooltip key.".to_string(),
        });
    }
    if node.interactive && node.event_handlers.is_empty() {
        report.diagnostics.push(UiStudioDiagnostic {
            severity: UiStudioDiagnosticSeverity::Warning,
            code: UiStudioDiagnosticCode::InteractiveWithoutAction,
            node_id: node.id.clone(),
            message: "Interactive node has no event binding.".to_string(),
        });
    }
    if node.text_key.as_deref().is_some_and(str::is_empty) {
        report.diagnostics.push(UiStudioDiagnostic {
            severity: UiStudioDiagnosticSeverity::Error,
            code: UiStudioDiagnosticCode::EmptyTextKey,
            node_id: node.id.clone(),
            message: "Text key is present but empty.".to_string(),
        });
    }
    if node.tooltip_key.as_deref().is_some_and(str::is_empty) {
        report.diagnostics.push(UiStudioDiagnostic {
            severity: UiStudioDiagnosticSeverity::Error,
            code: UiStudioDiagnosticCode::EmptyTooltipKey,
            node_id: node.id.clone(),
            message: "Tooltip key is present but empty.".to_string(),
        });
    }
    if node
        .layout
        .basis
        .iter()
        .any(|value| !value.is_finite() || *value < 0.0)
    {
        report.diagnostics.push(UiStudioDiagnostic {
            severity: UiStudioDiagnosticSeverity::Error,
            code: UiStudioDiagnosticCode::InvalidFixedSize,
            node_id: node.id.clone(),
            message: "Layout basis contains a negative or non-finite dimension.".to_string(),
        });
    }
    if node.kind == UiNodeKind::TextInput
        && node
            .control
            .text_input()
            .is_some_and(|input| input.value_key.is_empty())
    {
        report.diagnostics.push(UiStudioDiagnostic {
            severity: UiStudioDiagnosticSeverity::Error,
            code: UiStudioDiagnosticCode::MissingTextInputBinding,
            node_id: node.id.clone(),
            message: "Text input has no transient value binding.".to_string(),
        });
    }
    for child in &node.children {
        inspect_node(child, ids, report);
    }
}

pub(crate) fn action_command(action: &UiAction) -> Option<&str> {
    match action {
        UiAction::Command { name } => Some(name.as_str()),
        UiAction::OpenMenu { id } => Some(id.as_str()),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{UiEventBinding, UiEventKind, UiNode, UiStyle};

    #[test]
    fn diagnostics_catch_missing_icon_contract() {
        let mut document = UiDocument::blank("diagnostics");
        document
            .add_node(
                "root",
                UiNode::new("grid", UiNodeKind::Button)
                    .with_class("icon-button")
                    .with_style(UiStyle::transparent())
                    .with_event(UiEventBinding::command(UiEventKind::Click, "grid")),
            )
            .unwrap();
        let report = inspect_document(&document);
        assert!(report
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code == UiStudioDiagnosticCode::IconButtonWithoutTooltip));
    }
}
