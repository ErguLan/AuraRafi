//! RafUI Studio: the shared authoring and visual-quality contract for RafUI.
//!
//! RafUI Studio keeps design work predictable by exposing recipes, structured
//! inspection, controlled edits, density reports, diagnostics, snapshots, and
//! application-truth validation through one renderer-agnostic API.

use serde::{Deserialize, Serialize};

use crate::studio_diagnostics::{inspect_document, UiStudioDocumentReport};
use crate::studio_inspector::{
    inspect_document as inspect_nodes, inspect_node_by_id, UiStudioNodeInspection,
};
use crate::studio_quality::{UiStudioDensityReport, UiStudioDpiMatrix, UiStudioDpiReport};
pub use crate::studio_recipes::UiStudioRecipeKind;
use crate::studio_recipes::{UiStudioRecipeCatalog, UiStudioRecipeSpec};
use crate::studio_snapshots::{
    UiStudioGoldenSnapshot, UiStudioSnapshotCase, UiStudioSnapshotResult,
};
use crate::studio_validator::{
    validate_document, UiStudioValidationOptions, UiStudioValidationReport,
};
use crate::{UiDocument, UiEnvironment, UiLayout, UiStylePatch, UiTextStyle};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum UiStudioEdit {
    SetTextKey {
        node_id: String,
        value: Option<String>,
    },
    SetTooltipKey {
        node_id: String,
        value: Option<String>,
    },
    SetAccessibilityLabelKey {
        node_id: String,
        value: Option<String>,
    },
    SetClass {
        node_id: String,
        class_name: String,
        enabled: bool,
    },
    SetLayout {
        node_id: String,
        layout: UiLayout,
    },
    PatchStyle {
        node_id: String,
        patch: UiStylePatch,
    },
    SetTextStyle {
        node_id: String,
        style: Option<UiTextStyle>,
    },
}

impl UiStudioEdit {
    pub fn node_id(&self) -> &str {
        match self {
            Self::SetTextKey { node_id, .. }
            | Self::SetTooltipKey { node_id, .. }
            | Self::SetAccessibilityLabelKey { node_id, .. }
            | Self::SetClass { node_id, .. }
            | Self::SetLayout { node_id, .. }
            | Self::PatchStyle { node_id, .. }
            | Self::SetTextStyle { node_id, .. } => node_id,
        }
    }

    pub fn apply(&self, document: &mut UiDocument) -> Result<(), String> {
        let node = document
            .find_node_mut(self.node_id())
            .ok_or_else(|| format!("RafUI Studio could not find node '{}'.", self.node_id()))?;
        match self {
            Self::SetTextKey { value, .. } => node.text_key = value.clone(),
            Self::SetTooltipKey { value, .. } => node.tooltip_key = value.clone(),
            Self::SetAccessibilityLabelKey { value, .. } => {
                node.accessibility_label_key = value.clone()
            }
            Self::SetClass {
                class_name,
                enabled,
                ..
            } => {
                if *enabled {
                    if !node.classes.iter().any(|class| class == class_name) {
                        node.classes.push(class_name.clone());
                    }
                } else {
                    node.classes.retain(|class| class != class_name);
                }
            }
            Self::SetLayout { layout, .. } => node.layout = layout.clone(),
            Self::PatchStyle { patch, .. } => patch.apply_to(&mut node.style),
            Self::SetTextStyle { style, .. } => node.text_style = *style,
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct UiStudioRecipe {
    pub kind: UiStudioRecipeKind,
    pub spec: UiStudioRecipeSpec,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct UiStudioTextPreview {
    pub studio: String,
    pub recipe_version: u32,
    pub document_name: String,
    pub node_count: usize,
    pub selected_density: UiStudioDensityReport,
    pub recipes: Vec<UiStudioRecipeSpec>,
    pub dpi_cases: Vec<UiStudioDpiReport>,
    pub diagnostic_count: usize,
    pub diagnostics_clean: bool,
}

impl UiStudioTextPreview {
    pub fn lines(&self) -> Vec<String> {
        let mut lines = vec![
            "RafUI Studio".to_string(),
            format!("document: {}", self.document_name),
            format!("nodes: {}", self.node_count),
            format!(
                "selected density: {:.2}x geometry | {:.2}x text | {:.2}x icons",
                self.selected_density.contract.geometry_scale,
                self.selected_density.contract.text_scale,
                self.selected_density.contract.icon_scale
            ),
            format!("recipes (v{}):", self.recipe_version),
        ];
        lines.extend(self.recipes.iter().map(|recipe| {
            format!(
                "  - {} [{}] min={}x{}",
                recipe.name, recipe.semantic_class, recipe.min_size[0], recipe.min_size[1]
            )
        }));
        lines.push("dpi matrix:".to_string());
        lines.extend(self.dpi_cases.iter().map(|report| {
            format!(
                "  - {} -> target={}x{} | geometry={:?} text={:?} icons={:?}",
                report.case.label(),
                report.physical_size[0],
                report.physical_size[1],
                report.density.geometry_snap,
                report.density.text_sampling,
                report.density.icon_sampling
            )
        }));
        lines.push(format!(
            "diagnostics: {} ({})",
            self.diagnostic_count,
            if self.diagnostics_clean {
                "clean"
            } else {
                "review"
            }
        ));
        lines
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct RafUiStudio {
    pub recipes: UiStudioRecipeCatalog,
    pub dpi_matrix: UiStudioDpiMatrix,
}

impl Default for RafUiStudio {
    fn default() -> Self {
        Self {
            recipes: UiStudioRecipeCatalog::default(),
            dpi_matrix: UiStudioDpiMatrix::default(),
        }
    }
}

impl RafUiStudio {
    pub fn new(logical_size: [u32; 2]) -> Self {
        Self {
            dpi_matrix: UiStudioDpiMatrix::new(logical_size),
            ..Self::default()
        }
    }

    pub fn recipe(&self, kind: UiStudioRecipeKind) -> Option<UiStudioRecipe> {
        self.recipes
            .get(kind)
            .cloned()
            .map(|spec| UiStudioRecipe { kind, spec })
    }

    pub fn inspect(&self, document: &UiDocument) -> Vec<UiStudioNodeInspection> {
        inspect_nodes(document)
    }

    pub fn inspect_node(
        &self,
        document: &UiDocument,
        node_id: &str,
    ) -> Option<UiStudioNodeInspection> {
        inspect_node_by_id(document, node_id)
    }

    pub fn diagnose(&self, document: &UiDocument) -> UiStudioDocumentReport {
        inspect_document(document)
    }

    pub fn validate(
        &self,
        document: &UiDocument,
        options: &UiStudioValidationOptions,
    ) -> UiStudioValidationReport {
        validate_document(document, options)
    }

    pub fn density_report(&self, environment: UiEnvironment) -> UiStudioDensityReport {
        UiStudioDensityReport::from_environment(environment)
    }

    pub fn text_preview(
        &self,
        document: &UiDocument,
        environment: UiEnvironment,
    ) -> UiStudioTextPreview {
        let diagnostics = self.diagnose(document);
        UiStudioTextPreview {
            studio: "RafUI Studio".to_string(),
            recipe_version: self.recipes.version,
            document_name: document.name.clone(),
            node_count: diagnostics.node_count,
            selected_density: self.density_report(environment),
            recipes: self.recipes.recipes.clone(),
            dpi_cases: self.dpi_matrix.reports(),
            diagnostic_count: diagnostics.diagnostics.len(),
            diagnostics_clean: diagnostics.is_clean(),
        }
    }

    pub fn structural_snapshot(
        &self,
        name: impl Into<String>,
        document: &UiDocument,
        environment: UiEnvironment,
    ) -> UiStudioSnapshotCase {
        UiStudioSnapshotCase::from_document(name, document, environment)
    }

    pub fn compare_golden(
        &self,
        golden: &UiStudioGoldenSnapshot,
        actual_rgba: &[u8],
    ) -> Result<UiStudioSnapshotResult, String> {
        golden.compare(actual_rgba)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{UiNode, UiNodeKind};

    #[test]
    fn controlled_edit_changes_only_the_retained_document() {
        let studio = RafUiStudio::new([640, 360]);
        let mut document = UiDocument::blank("editor");
        document
            .add_node("root", UiNode::new("toolbar", UiNodeKind::Toolbar))
            .unwrap();
        UiStudioEdit::SetClass {
            node_id: "toolbar".to_string(),
            class_name: "technical-toolbar".to_string(),
            enabled: true,
        }
        .apply(&mut document)
        .unwrap();
        assert!(studio
            .inspect_node(&document, "toolbar")
            .unwrap()
            .classes
            .contains(&"technical-toolbar".to_string()));
    }
}
