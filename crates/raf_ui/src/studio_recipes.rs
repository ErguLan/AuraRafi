//! RafUI Studio's reusable component vocabulary.
//!
//! A recipe is a named visual contract, not a second widget hierarchy. Surface
//! authors still build ordinary `UiNode`s; the catalog tells them which
//! geometry, semantic class, states, and accessibility obligations are
//! already solved.

use serde::{Deserialize, Serialize};

pub const UI_STUDIO_RECIPE_VERSION: u32 = 1;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum UiStudioRecipeKind {
    TechnicalToolbar,
    IconButton,
    SegmentedControl,
    FloatingActionRail,
    TreeRow,
    InspectorField,
    EditorTab,
    Tooltip,
    PanelHeader,
    EmptyState,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct UiStudioRecipeSpec {
    pub kind: UiStudioRecipeKind,
    pub name: String,
    pub semantic_class: String,
    pub purpose: String,
    pub min_size: [f32; 2],
    pub requires_tooltip: bool,
    pub requires_accessibility_label: bool,
    pub supports_keyboard_focus: bool,
}

impl UiStudioRecipeSpec {
    fn new(
        kind: UiStudioRecipeKind,
        name: &str,
        semantic_class: &str,
        purpose: &str,
        min_size: [f32; 2],
        requires_tooltip: bool,
        requires_accessibility_label: bool,
        supports_keyboard_focus: bool,
    ) -> Self {
        Self {
            kind,
            name: name.to_string(),
            semantic_class: semantic_class.to_string(),
            purpose: purpose.to_string(),
            min_size,
            requires_tooltip,
            requires_accessibility_label,
            supports_keyboard_focus,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct UiStudioRecipeCatalog {
    pub version: u32,
    pub recipes: Vec<UiStudioRecipeSpec>,
}

impl Default for UiStudioRecipeCatalog {
    fn default() -> Self {
        Self {
            version: UI_STUDIO_RECIPE_VERSION,
            recipes: vec![
                UiStudioRecipeSpec::new(
                    UiStudioRecipeKind::TechnicalToolbar,
                    "Technical toolbar",
                    "technical-toolbar",
                    "Compact command row for viewport and editor actions.",
                    [0.0, 32.0],
                    false,
                    false,
                    false,
                ),
                UiStudioRecipeSpec::new(
                    UiStudioRecipeKind::IconButton,
                    "Icon button",
                    "icon-button",
                    "Small command target with a stable tooltip and accessible label.",
                    [32.0, 32.0],
                    true,
                    true,
                    true,
                ),
                UiStudioRecipeSpec::new(
                    UiStudioRecipeKind::SegmentedControl,
                    "Segmented control",
                    "segmented-control",
                    "Mutually exclusive view or mode selection.",
                    [0.0, 28.0],
                    false,
                    true,
                    true,
                ),
                UiStudioRecipeSpec::new(
                    UiStudioRecipeKind::FloatingActionRail,
                    "Floating action rail",
                    "floating-action-rail",
                    "Small lower-canvas action cluster that stays out of the scene.",
                    [32.0, 32.0],
                    true,
                    true,
                    true,
                ),
                UiStudioRecipeSpec::new(
                    UiStudioRecipeKind::TreeRow,
                    "Tree row",
                    "tree-row",
                    "Hierarchy item with selection, visibility, and overflow affordances.",
                    [0.0, 28.0],
                    false,
                    true,
                    true,
                ),
                UiStudioRecipeSpec::new(
                    UiStudioRecipeKind::InspectorField,
                    "Inspector field",
                    "inspector-field",
                    "Label/value pair with stable spacing and compact validation state.",
                    [0.0, 28.0],
                    false,
                    true,
                    true,
                ),
                UiStudioRecipeSpec::new(
                    UiStudioRecipeKind::EditorTab,
                    "Editor tab",
                    "editor-tab",
                    "Persistent bottom or context tab with an active edge.",
                    [72.0, 32.0],
                    false,
                    true,
                    true,
                ),
                UiStudioRecipeSpec::new(
                    UiStudioRecipeKind::Tooltip,
                    "Tooltip",
                    "tooltip",
                    "Compact neutral overlay placed below the pointer or anchor.",
                    [52.0, 22.0],
                    false,
                    false,
                    false,
                ),
                UiStudioRecipeSpec::new(
                    UiStudioRecipeKind::PanelHeader,
                    "Panel header",
                    "panel-header",
                    "Quiet title row shared by panels and inspector sections.",
                    [0.0, 32.0],
                    false,
                    false,
                    false,
                ),
                UiStudioRecipeSpec::new(
                    UiStudioRecipeKind::EmptyState,
                    "Empty state",
                    "empty-state",
                    "Calm explanation for a valid surface with no current selection.",
                    [160.0, 64.0],
                    false,
                    false,
                    false,
                ),
            ],
        }
    }
}

impl UiStudioRecipeCatalog {
    pub fn get(&self, kind: UiStudioRecipeKind) -> Option<&UiStudioRecipeSpec> {
        self.recipes.iter().find(|recipe| recipe.kind == kind)
    }

    pub fn contains_class(&self, class_name: &str) -> bool {
        self.recipes
            .iter()
            .any(|recipe| recipe.semantic_class == class_name)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn catalog_covers_the_editor_vocabulary() {
        let catalog = UiStudioRecipeCatalog::default();
        assert_eq!(catalog.version, UI_STUDIO_RECIPE_VERSION);
        assert!(catalog.contains_class("icon-button"));
        assert!(catalog.contains_class("inspector-field"));
        assert!(catalog.contains_class("tooltip"));
    }
}
