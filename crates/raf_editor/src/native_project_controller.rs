//! Project/session persistence used by the native editor shell.
//!
//! This module owns file-backed project concerns only. It does not own Winit,
//! RafUI surfaces, scene editing or the Electronics domain.

use raf_core::project::{Project, ProjectType};
use raf_core::scene::SceneGraph;
use raf_core::session::{ProjectSession, ProjectSessionRegistry, SessionId};
use raf_nodes::NodeGraph;

pub(crate) fn initial_project() -> Option<Project> {
    let path = std::env::args_os().nth(1)?;
    Project::load(std::path::Path::new(&path)).ok()
}

pub(crate) fn game_capabilities(project_type: ProjectType) -> Vec<String> {
    if project_type != ProjectType::Game {
        return Vec::new();
    }
    [
        "game.add",
        "game.select",
        "game.rename",
        "game.delete",
        "game.duplicate",
        "game.set_transform",
        "game.move",
        "game.rotate",
        "game.scale",
        "game.color",
        "game.arrange_grid",
        "game.generate_prefab",
        "game.describe_scene",
        "game.focus",
        "game.update",
        "game.batch",
        "game.create_group",
        "game.reparent",
        "game.build",
        "game.reconcile",
        "game.repair",
    ]
    .into_iter()
    .map(str::to_string)
    .collect()
}

pub(crate) fn electronics_capabilities() -> Vec<String> {
    [
        "electronics.add_part",
        "electronics.wire",
        "electronics.set_value",
        "electronics.rotate",
        "electronics.delete",
        "electronics.select",
        "electronics.generate_circuit",
        "electronics.autolayout",
        "electronics.drc",
        "electronics.diagnose",
        "electronics.simulate",
        "electronics.netlist",
        "electronics.bom",
        "electronics.describe",
        "pcb.sync",
        "pcb.route_airwire",
        "pcb.set_board",
        "pcb.move",
        "pcb.rotate",
        "pcb.describe",
    ]
    .into_iter()
    .map(str::to_string)
    .collect()
}

/// Commands that can be queried without selecting a domain executor.
///
/// The attached endpoint is project-scoped, but status/capability/session
/// inspection is not a Game or Electronics mutation. Keeping this list here
/// prevents the transport boundary from accidentally routing metadata into a
/// domain parser.
pub(crate) fn project_capabilities(project_type: ProjectType) -> Vec<String> {
    let mut capabilities = vec![
        "engine.status".to_string(),
        "engine.context".to_string(),
        "scene.outline".to_string(),
        "scene.query".to_string(),
        "scene.spatial_map".to_string(),
        "scene.design_audit".to_string(),
        "scene.inspect".to_string(),
        "selection.get".to_string(),
        "assets.catalog".to_string(),
        "assets.inspect".to_string(),
        "scripts.catalog".to_string(),
        "project.health".to_string(),
        "scene.verify".to_string(),
        "capabilities.list".to_string(),
        "capabilities.search".to_string(),
        "project.info".to_string(),
        "project.save".to_string(),
        "session.list".to_string(),
        "workspace.describe".to_string(),
        "task.list".to_string(),
        "task.get".to_string(),
        "task.events".to_string(),
        "task.cancel".to_string(),
    ];
    if project_type == ProjectType::Game {
        capabilities.push("viewport.capture".to_string());
        capabilities.push("transaction.undo".to_string());
    }
    capabilities.extend(match project_type {
        ProjectType::Game => game_capabilities(project_type),
        ProjectType::Electronics => electronics_capabilities(),
    });
    capabilities
}

pub(crate) fn initial_scene(project: Option<&Project>) -> SceneGraph {
    let Some(project) = project else {
        return SceneGraph::new();
    };
    let registry = ProjectSessionRegistry::load_or_legacy(&project.path, project.project_type);
    let Some(session) = registry.active() else {
        return SceneGraph::new();
    };
    SceneGraph::load_ron(&session.path(&project.path, &session.scene_file))
}

pub(crate) fn initial_scene_for_session(project: &Project, session_id: SessionId) -> SceneGraph {
    let registry = ProjectSessionRegistry::load_or_legacy(&project.path, project.project_type);
    let Some(session) = registry
        .sessions
        .iter()
        .find(|session| session.id == session_id)
    else {
        return SceneGraph::new();
    };
    SceneGraph::load_ron(&session.path(&project.path, &session.scene_file))
}

pub(crate) fn initial_node_graph(project: Option<&Project>) -> NodeGraph {
    let Some(project) = project else {
        return NodeGraph::new("Main");
    };
    let registry = ProjectSessionRegistry::load_or_legacy(&project.path, project.project_type);
    let Some(session) = registry.active() else {
        return NodeGraph::new("Main");
    };
    let path = session.path(&project.path, &session.nodes_file);
    std::fs::read_to_string(path)
        .ok()
        .and_then(|raw| ron::from_str::<NodeGraph>(&raw).ok())
        .unwrap_or_else(|| NodeGraph::new("Main"))
}

pub(crate) fn initial_node_graph_for_session(
    project: &Project,
    session_id: SessionId,
) -> NodeGraph {
    let registry = ProjectSessionRegistry::load_or_legacy(&project.path, project.project_type);
    let Some(session) = registry
        .sessions
        .iter()
        .find(|session| session.id == session_id)
    else {
        return NodeGraph::new("Main");
    };
    let path = session.path(&project.path, &session.nodes_file);
    std::fs::read_to_string(path)
        .ok()
        .and_then(|raw| ron::from_str::<NodeGraph>(&raw).ok())
        .unwrap_or_else(|| NodeGraph::new("Main"))
}

pub(crate) fn save_project_document(
    project: &Project,
    scene: &SceneGraph,
    node_graph: &NodeGraph,
) -> Result<(), String> {
    project.save().map_err(|error| error.to_string())?;
    let registry = ProjectSessionRegistry::load_or_legacy(&project.path, project.project_type);
    save_project_document_for_session(project, registry.active_session, scene, node_graph)
}

pub(crate) fn save_project_document_for_session(
    project: &Project,
    session_id: SessionId,
    scene: &SceneGraph,
    node_graph: &NodeGraph,
) -> Result<(), String> {
    let registry = ProjectSessionRegistry::load_or_legacy(&project.path, project.project_type);
    let session: ProjectSession = registry
        .sessions
        .iter()
        .find(|session| session.id == session_id)
        .cloned()
        .ok_or_else(|| "project session was not found".to_string())?;
    session
        .ensure_storage(&project.path)
        .map_err(|error| format!("session storage: {error}"))?;
    scene
        .save_ron(&session.path(&project.path, &session.scene_file))
        .map_err(|error| format!("scene save: {error}"))?;
    let nodes_path = session.path(&project.path, &session.nodes_file);
    let nodes = ron::ser::to_string_pretty(node_graph, ron::ser::PrettyConfig::default())
        .map_err(|error| format!("nodes serialize: {error}"))?;
    std::fs::write(nodes_path, nodes).map_err(|error| format!("nodes save: {error}"))
}
