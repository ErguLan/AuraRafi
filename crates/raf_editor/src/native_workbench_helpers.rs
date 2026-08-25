//! Pure helper functions for the native workbench.
//!
//! Keeping fingerprints, validation and small parsing policies here prevents
//! the lifecycle coordinator from becoming a second domain monolith.

use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};

use crate::panels::inspector_surface::{InspectorSection, InspectorViewState};
use crate::panels::viewport_toolbar_surface::{
    ViewportRenderStyle, ViewportTool, ViewportToolbarState, ViewportViewMode,
};
use raf_core::project::BuildingStyle;
use raf_core::scene::{SceneGraph, SceneNodeId};
use raf_core::session::ProjectSessionRegistry;
use raf_nodes::{NodeGraph, NodeId};

pub(crate) fn unique_session_name(registry: &ProjectSessionRegistry, base: &str) -> String {
    let base = base.trim();
    if !registry
        .sessions
        .iter()
        .any(|session| session.name.eq_ignore_ascii_case(base))
    {
        return base.to_string();
    }
    for index in 2..=10_000 {
        let candidate = format!("{base} {index}");
        if !registry
            .sessions
            .iter()
            .any(|session| session.name.eq_ignore_ascii_case(&candidate))
        {
            return candidate;
        }
    }
    format!("{base} {}", uuid::Uuid::new_v4())
}

pub(crate) fn default_toolbar_state() -> ViewportToolbarState {
    ViewportToolbarState {
        select_mode: true,
        tool: ViewportTool::Select,
        render_style: ViewportRenderStyle::Solid,
        polygons_visible: false,
        grid_visible: true,
        labels_visible: true,
        view_mode: ViewportViewMode::View3d,
        view_menu_open: false,
        shading_menu_open: false,
        primitive_menu_open: false,
        building_style: BuildingStyle::Free,
        building_menu_open: false,
        compact: false,
    }
}

pub(crate) fn hierarchy_drop_target_from_hovered(hovered: Option<&str>) -> Option<SceneNodeId> {
    let suffix = hovered?.strip_prefix("hierarchy.row.")?;
    let raw_id = suffix.split('.').next()?;
    raw_id.parse::<usize>().ok().map(SceneNodeId)
}

pub(crate) fn hierarchy_drop_target_is_valid(
    scene: &SceneGraph,
    sources: &[SceneNodeId],
    target: Option<SceneNodeId>,
) -> bool {
    let Some(target) = target else {
        return true;
    };
    if !scene.is_valid_node(target) || sources.contains(&target) {
        return false;
    }
    let mut current = Some(target);
    while let Some(id) = current {
        if sources.contains(&id) {
            return false;
        }
        current = scene.get(id).and_then(|node| node.parent);
    }
    true
}

pub(crate) fn numeric_commit_field(target_id: &str) -> Option<(String, String)> {
    let value = target_id.strip_suffix(".input")?;
    let mut parts = value.split('.');
    let prefix = parts.next()?;
    let label = parts.next()?;
    let axis = parts.next()?;
    if prefix != "inspector" || !matches!(label, "position" | "rotation" | "scale") {
        return None;
    }
    if !matches!(axis, "x" | "y" | "z") {
        return None;
    }
    Some((format!("{label}.{axis}"), format!("{value}.text")))
}

pub(crate) fn inspector_section_from_slug(slug: &str) -> Option<InspectorSection> {
    Some(match slug {
        "identity" => InspectorSection::Identity,
        "transform" => InspectorSection::Transform,
        "appearance" => InspectorSection::Appearance,
        "components" => InspectorSection::Components,
        "variables" => InspectorSection::Variables,
        "audio" => InspectorSection::Audio,
        "physics" => InspectorSection::Physics,
        "metadata" => InspectorSection::Metadata,
        "debug" => InspectorSection::Debug,
        _ => return None,
    })
}

pub(crate) fn set_inspector_section(
    view: &mut InspectorViewState,
    section: InspectorSection,
    value: bool,
) {
    match section {
        InspectorSection::Identity => view.identity = value,
        InspectorSection::Transform => view.transform = value,
        InspectorSection::Appearance => view.appearance = value,
        InspectorSection::Components => view.components = value,
        InspectorSection::Variables => view.variables = value,
        InspectorSection::Audio => view.audio = value,
        InspectorSection::Physics => view.physics = value,
        InspectorSection::Metadata => view.metadata = value,
        InspectorSection::Debug => view.debug = value,
    }
}

pub(crate) fn hierarchy_fingerprint(scene: &SceneGraph) -> u64 {
    let mut hasher = DefaultHasher::new();
    scene.roots().hash(&mut hasher);
    for (id, node) in scene.iter() {
        id.0.hash(&mut hasher);
        node.uuid.hash(&mut hasher);
        node.name.hash(&mut hasher);
        node.parent.map(|parent| parent.0).hash(&mut hasher);
        node.children.hash(&mut hasher);
        node.visible.hash(&mut hasher);
        node.locked.hash(&mut hasher);
        node.is_folder.hash(&mut hasher);
    }
    hasher.finish()
}

pub(crate) fn node_graph_fingerprint(graph: &NodeGraph, selected: Option<NodeId>) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    graph.name.hash(&mut hasher);
    graph.nodes.len().hash(&mut hasher);
    graph.connections.len().hash(&mut hasher);
    for node in &graph.nodes {
        node.id.hash(&mut hasher);
        node.name.hash(&mut hasher);
        node.position[0].to_bits().hash(&mut hasher);
        node.position[1].to_bits().hash(&mut hasher);
    }
    selected.hash(&mut hasher);
    hasher.finish()
}
