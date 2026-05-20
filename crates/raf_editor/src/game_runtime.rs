use egui::Context;
use raf_core::project::ProjectSettings;
use raf_core::SceneGraph;

use crate::panels::node_editor::NodeEditorDocument;

#[derive(Debug, Clone, Default)]
pub struct RuntimeInputState;

impl RuntimeInputState {
    pub fn from_egui(_ctx: &Context) -> Self {
        Self
    }
}

#[derive(Debug, Default)]
pub struct RuntimeReport {
    pub logs: Vec<String>,
    pub errors: Vec<String>,
}

pub struct GameRuntimeState {
    pub scene: SceneGraph,
}

impl GameRuntimeState {
    pub fn start(
        source_scene: &SceneGraph,
        _node_document: &NodeEditorDocument,
        _assets_root: Option<std::path::PathBuf>,
        _settings: &ProjectSettings,
    ) -> (Self, RuntimeReport) {
        (
            Self {
                scene: source_scene.clone(),
            },
            RuntimeReport::default(),
        )
    }

    pub fn update(&mut self, _delta_time: f32, _input: RuntimeInputState) -> RuntimeReport {
        RuntimeReport::default()
    }
}
