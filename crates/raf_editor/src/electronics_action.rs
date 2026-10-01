//! Strongly-typed command actions for the Electronics workspace.
//!
//! Provides compile-time safety and exhaustive matching over editor operations,
//! eliminating brittle stringly-typed commands while remaining 100% compatible
//! with string-based retained UI event dispatch.

use raf_electronics::CadSurfaceKind;

use crate::electronics_controller::ElectronicsTool;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ElectronicsAction {
    // Tool selection
    SelectTool(ElectronicsTool),

    // View & Camera
    FitView,
    ToggleGrid,
    ToggleLabels,
    ZoomIn,
    ZoomOut,

    // Document Operations
    Rotate,
    Delete,
    Undo,
    Redo,
    SaveProject,

    // Workspace Mode
    SetSurface(CadSurfaceKind),
    SyncPcb,

    // Analysis
    RunDrc,
    RunSimulation,
    CancelAnalysis,

    // Context Menu Operations
    ContextDelete,
    ContextRoute,
    ContextSelect,
    ContextDuplicate,
    ContextCancel,

    // Parameterized Actions
    CommitInspectorValue(String),
    SelectNavigator(usize),
    InspectNavigator(usize),
    SelectTab(String),
}

impl ElectronicsAction {
    /// Parses an incoming string command (from a UI button click or shortcut)
    /// into a strongly-typed `ElectronicsAction`.
    pub fn parse(command: &str) -> Option<Self> {
        match command {
            "electronics.select" => Some(Self::SelectTool(ElectronicsTool::Select)),
            "electronics.pan" => Some(Self::SelectTool(ElectronicsTool::Pan)),
            "electronics.wire" => Some(Self::SelectTool(ElectronicsTool::Wire)),
            "electronics.route" => Some(Self::SelectTool(ElectronicsTool::Route)),
            "electronics.place" => Some(Self::SelectTool(ElectronicsTool::Place)),
            "electronics.board-outline" => Some(Self::SelectTool(ElectronicsTool::BoardOutline)),
            "electronics.fit" => Some(Self::FitView),
            "electronics.grid.toggle" => Some(Self::ToggleGrid),
            "electronics.labels.toggle" => Some(Self::ToggleLabels),
            "electronics.zoom-in" => Some(Self::ZoomIn),
            "electronics.zoom-out" => Some(Self::ZoomOut),
            "electronics.rotate" => Some(Self::Rotate),
            "electronics.analysis.drc" => Some(Self::RunDrc),
            "electronics.analysis.simulation" => Some(Self::RunSimulation),
            "electronics.analysis.cancel" => Some(Self::CancelAnalysis),
            "electronics.mode.schematic" => Some(Self::SetSurface(CadSurfaceKind::Schematic)),
            "electronics.mode.pcb" => Some(Self::SetSurface(CadSurfaceKind::Pcb)),
            "electronics.pcb.sync" => Some(Self::SyncPcb),
            "edit.undo" => Some(Self::Undo),
            "edit.redo" => Some(Self::Redo),
            "edit.delete" => Some(Self::Delete),
            "project.save" => Some(Self::SaveProject),
            "electronics.context.delete" => Some(Self::ContextDelete),
            "electronics.context.route" => Some(Self::ContextRoute),
            "electronics.context.select" | "electronics.context.properties" => {
                Some(Self::ContextSelect)
            }
            "electronics.context.duplicate" => Some(Self::ContextDuplicate),
            "electronics.context.cancel" => Some(Self::ContextCancel),
            _ => {
                if let Some(val) = command.strip_prefix("electronics.inspector.value.commit:") {
                    Some(Self::CommitInspectorValue(val.to_string()))
                } else if let Some(idx) = command.strip_prefix("electronics.navigator.select.") {
                    idx.parse::<usize>().ok().map(Self::SelectNavigator)
                } else if let Some(idx) = command.strip_prefix("electronics.navigator.inspect.") {
                    idx.parse::<usize>().ok().map(Self::InspectNavigator)
                } else if let Some(tab) = command.strip_prefix("electronics.tab.") {
                    Some(Self::SelectTab(tab.to_string()))
                } else {
                    None
                }
            }
        }
    }

    /// Converts this action back to its canonical event command string.
    pub fn to_command_string(&self) -> String {
        match self {
            Self::SelectTool(ElectronicsTool::Select) => "electronics.select".to_string(),
            Self::SelectTool(ElectronicsTool::Pan) => "electronics.pan".to_string(),
            Self::SelectTool(ElectronicsTool::Wire) => "electronics.wire".to_string(),
            Self::SelectTool(ElectronicsTool::Route) => "electronics.route".to_string(),
            Self::SelectTool(ElectronicsTool::Place) => "electronics.place".to_string(),
            Self::SelectTool(ElectronicsTool::BoardOutline) => {
                "electronics.board-outline".to_string()
            }
            Self::FitView => "electronics.fit".to_string(),
            Self::ToggleGrid => "electronics.grid.toggle".to_string(),
            Self::ToggleLabels => "electronics.labels.toggle".to_string(),
            Self::ZoomIn => "electronics.zoom-in".to_string(),
            Self::ZoomOut => "electronics.zoom-out".to_string(),
            Self::Rotate => "electronics.rotate".to_string(),
            Self::RunDrc => "electronics.analysis.drc".to_string(),
            Self::RunSimulation => "electronics.analysis.simulation".to_string(),
            Self::CancelAnalysis => "electronics.analysis.cancel".to_string(),
            Self::SetSurface(CadSurfaceKind::Schematic) => "electronics.mode.schematic".to_string(),
            Self::SetSurface(CadSurfaceKind::Pcb) => "electronics.mode.pcb".to_string(),
            Self::SyncPcb => "electronics.pcb.sync".to_string(),
            Self::Undo => "edit.undo".to_string(),
            Self::Redo => "edit.redo".to_string(),
            Self::Delete => "edit.delete".to_string(),
            Self::SaveProject => "project.save".to_string(),
            Self::ContextDelete => "electronics.context.delete".to_string(),
            Self::ContextRoute => "electronics.context.route".to_string(),
            Self::ContextSelect => "electronics.context.select".to_string(),
            Self::ContextDuplicate => "electronics.context.duplicate".to_string(),
            Self::ContextCancel => "electronics.context.cancel".to_string(),
            Self::CommitInspectorValue(val) => {
                format!("electronics.inspector.value.commit:{val}")
            }
            Self::SelectNavigator(idx) => format!("electronics.navigator.select.{idx}"),
            Self::InspectNavigator(idx) => format!("electronics.navigator.inspect.{idx}"),
            Self::SelectTab(tab) => format!("electronics.tab.{tab}"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_standard_and_parameterized_commands() {
        assert_eq!(
            ElectronicsAction::parse("electronics.wire"),
            Some(ElectronicsAction::SelectTool(ElectronicsTool::Wire))
        );
        assert_eq!(
            ElectronicsAction::parse("electronics.inspector.value.commit:10k"),
            Some(ElectronicsAction::CommitInspectorValue("10k".to_string()))
        );
        assert_eq!(
            ElectronicsAction::parse("electronics.navigator.select.3"),
            Some(ElectronicsAction::SelectNavigator(3))
        );
        assert_eq!(
            ElectronicsAction::parse("unknown.command"),
            None
        );
    }

    #[test]
    fn round_trips_to_command_string() {
        let action = ElectronicsAction::CommitInspectorValue("4.7uF".to_string());
        let cmd = action.to_command_string();
        assert_eq!(cmd, "electronics.inspector.value.commit:4.7uF");
        assert_eq!(ElectronicsAction::parse(&cmd), Some(action));
    }
}
