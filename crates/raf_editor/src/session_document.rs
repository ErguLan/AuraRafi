//! Session-scoped project document persistence.
//!
//! This module keeps active-session file paths out of `app.rs`. It owns no UI
//! and does not decide which session is active; that remains an editor action.

use std::fs;

use raf_core::project::Project;
use raf_core::scene::SceneGraph;
use raf_core::session::ProjectSession;
use raf_render::bridge::EditorCameraBlock;
use raf_ui::UiDocument;

use crate::panels::node_editor::NodeEditorDocument;

pub struct GameSessionDocument {
    pub scene: Option<SceneGraph>,
    pub nodes: NodeEditorDocument,
    pub ui_document: UiDocument,
    pub editor_camera: Option<EditorCameraBlock>,
}

pub fn load_game_session(project: &Project, session: &ProjectSession) -> GameSessionDocument {
    let scene_path = session.path(&project.path, &session.scene_file);
    let nodes_path = session.path(&project.path, &session.nodes_file);
    let ui_path = session.path(&project.path, &session.ui_document_file);
    let camera_file = session.editor_camera_file();
    let camera_path = session.path(&project.path, &camera_file);

    let scene = scene_path
        .exists()
        .then(|| SceneGraph::load_ron(&scene_path));
    let nodes = fs::read_to_string(nodes_path)
        .ok()
        .and_then(|raw| ron::from_str::<NodeEditorDocument>(&raw).ok())
        .unwrap_or_default();
    let ui_document = load_ui_document(&ui_path, session);
    let editor_camera = fs::read_to_string(camera_path)
        .ok()
        .and_then(|raw| ron::from_str::<EditorCameraBlock>(&raw).ok());

    GameSessionDocument {
        scene,
        nodes,
        ui_document,
        editor_camera,
    }
}

pub fn save_game_session(
    project: &Project,
    session: &ProjectSession,
    scene: &SceneGraph,
    nodes: &NodeEditorDocument,
    ui_document: &UiDocument,
    editor_camera: &EditorCameraBlock,
) -> Result<(), String> {
    session
        .ensure_storage(&project.path)
        .map_err(|error| format!("session storage: {error}"))?;
    let scene_path = session.path(&project.path, &session.scene_file);
    scene
        .save_ron(&scene_path)
        .map_err(|error| format!("{}: {error}", session.scene_file.display()))?;
    write_ron(
        &session.path(&project.path, &session.nodes_file),
        nodes,
        &session.nodes_file,
    )?;
    save_ui_document(
        &session.path(&project.path, &session.ui_document_file),
        ui_document,
        &session.ui_document_file,
    )?;
    let camera_file = session.editor_camera_file();
    write_ron(
        &session.path(&project.path, &camera_file),
        editor_camera,
        &camera_file,
    )
}

pub fn load_ui_document(path: &std::path::Path, session: &ProjectSession) -> UiDocument {
    fs::read_to_string(path)
        .ok()
        .and_then(|raw| ron::from_str::<UiDocument>(&raw).ok())
        .unwrap_or_else(|| UiDocument::blank(format!("{} UI", session.name)))
}

pub fn save_ui_document(
    path: &std::path::Path,
    document: &UiDocument,
    relative_path: &std::path::Path,
) -> Result<(), String> {
    write_ron(path, document, relative_path)
}

fn write_ron<T: serde::Serialize>(
    path: &std::path::Path,
    value: &T,
    relative_path: &std::path::Path,
) -> Result<(), String> {
    let parent = path
        .parent()
        .ok_or_else(|| format!("{} has no parent", relative_path.display()))?;
    fs::create_dir_all(parent).map_err(|error| format!("{}: {error}", parent.display()))?;
    let raw = ron::ser::to_string_pretty(value, ron::ser::PrettyConfig::default())
        .map_err(|error| format!("{} serialize: {error}", relative_path.display()))?;
    fs::write(path, raw).map_err(|error| format!("{}: {error}", relative_path.display()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use raf_core::project::ProjectType;
    use raf_core::session::ProjectSession;
    use std::path::PathBuf;

    #[test]
    fn missing_ui_document_starts_blank_without_templates() {
        let session = ProjectSession::new("World", raf_core::session::ProjectSessionKind::World);
        let document = load_ui_document(&PathBuf::from("this-file-does-not-exist.ron"), &session);
        assert!(document.root.children.is_empty());
        assert_eq!(document.name, "World UI");
        let _ = ProjectType::Game;
    }
}
