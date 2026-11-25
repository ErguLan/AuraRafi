//! Versioned, editable subtree snapshots. No live inheritance or gameplay defaults.
use super::{LinkedAssembly, SceneGraph, SceneNode, SceneNodeId, VariableValue};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use uuid::Uuid;

pub const MAX_PREFAB_NODES: usize = 1024;
/// Maximum number of Assembly sources followed from one instance root.
pub const MAX_ASSEMBLY_DEPTH: usize = 8;
/// Total nodes accepted while walking an Assembly dependency graph.
pub const MAX_ASSEMBLY_TOTAL_NODES: usize = 4096;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScenePrefab {
    pub version: u32,
    /// Parent-before-child order; local parent/child indices are confined to this array.
    pub nodes: Vec<SceneNode>,
}

impl ScenePrefab {
    pub fn capture(scene: &SceneGraph, root: SceneNodeId) -> Result<Self, String> {
        let mut pending = vec![root];
        let mut ids = Vec::new();
        let mut seen = HashSet::new();
        while let Some(id) = pending.pop() {
            if !scene.is_valid_node(id) || !seen.insert(id) {
                return Err("Prefab hierarchy contains a missing node or cycle".into());
            }
            if ids.len() >= MAX_PREFAB_NODES {
                return Err("Prefab exceeds 1024 nodes".into());
            }
            let node = scene.get(id).expect("validated node");
            pending.extend(node.children.iter().rev().copied());
            ids.push(id);
        }
        let remap: HashMap<_, _> = ids
            .iter()
            .enumerate()
            .map(|(i, id)| (*id, SceneNodeId(i)))
            .collect();
        let nodes = ids
            .iter()
            .enumerate()
            .map(|(i, id)| {
                let mut node = scene.get(*id).expect("validated node").clone();
                node.parent = if i == 0 {
                    None
                } else {
                    node.parent.and_then(|id| remap.get(&id).copied())
                };
                node.children = node
                    .children
                    .iter()
                    .filter_map(|id| remap.get(id).copied())
                    .collect();
                node.entity_index = None;
                if i == 0 {
                    // A captured subtree must not keep pointing at its own
                    // source; nested instances deeper in the subtree may.
                    node.linked_assembly = None;
                }
                node
            })
            .collect();
        let prefab = Self { version: 1, nodes };
        prefab.validate()?;
        Ok(prefab)
    }

    pub fn validate(&self) -> Result<(), String> {
        if self.version != 1 || self.nodes.is_empty() || self.nodes.len() > MAX_PREFAB_NODES {
            return Err("Unsupported prefab version or node count (1..1024)".into());
        }
        let mut uuids = HashSet::new();
        let mut incoming = vec![0usize; self.nodes.len()];
        for (i, node) in self.nodes.iter().enumerate() {
            if node.name.trim().is_empty()
                || node.name.len() > 256
                || !uuids.insert(node.uuid)
                || node.uuid.is_nil()
                || !node.position.is_finite()
                || !node.rotation.is_finite()
                || !node.scale.is_finite()
            {
                return Err("Prefab contains invalid identity or transform".into());
            }
            if (i == 0 && node.parent.is_some())
                || (i > 0 && !node.parent.is_some_and(|parent| parent.0 < i))
            {
                return Err("Prefab parents must precede children and have one root".into());
            }
            for child in &node.children {
                if child.0 <= i
                    || child.0 >= self.nodes.len()
                    || self.nodes[child.0].parent != Some(SceneNodeId(i))
                {
                    return Err("Prefab child/parent links disagree".into());
                }
                incoming[child.0] += 1;
            }
            if node.variables.len() > 256
                || node.scripts.len() > 64
                || node.behavior_parameters.len() > 64
                || node
                    .behavior_parameters
                    .values()
                    .any(|values| values.len() > 256)
            {
                return Err("Prefab component budget exceeded".into());
            }
            if let Some(mesh) = &node.editable_mesh {
                mesh.validate()?;
            }
            if let Some(assembly) = &node.linked_assembly {
                assembly.validate().map_err(str::to_string)?;
            }
            if let Some(marker) = &node.spawn_marker {
                marker.validate().map_err(str::to_string)?;
            }
            node.surface.validate().map_err(str::to_string)?;
            for values in std::iter::once(&node.variables).chain(node.behavior_parameters.values())
            {
                let mut names = HashSet::new();
                for variable in values {
                    if variable.name.trim().is_empty()
                        || variable.name.len() > 128
                        || !names.insert(variable.name.as_str())
                        || matches!(&variable.value, VariableValue::Number(value) if !value.is_finite())
                        || matches!(&variable.value, VariableValue::Text(value) if value.len() > 4096)
                    {
                        return Err("Prefab contains invalid or duplicate parameters".into());
                    }
                }
            }
        }
        if incoming[0] != 0 || incoming.iter().skip(1).any(|count| *count != 1) {
            return Err("Prefab contains disconnected or duplicated children".into());
        }
        Ok(())
    }

    pub fn instantiate(
        &self,
        scene: &mut SceneGraph,
        parent: Option<SceneNodeId>,
    ) -> Result<SceneNodeId, String> {
        self.validate()?;
        if parent.is_some_and(|id| !scene.is_valid_node(id))
            || scene.len().saturating_add(self.nodes.len()) > 10_000
        {
            return Err("Invalid prefab parent or 10000-slot scene budget exceeded".into());
        }
        let identities: HashMap<_, _> = self
            .nodes
            .iter()
            .map(|n| (n.uuid, Uuid::new_v4()))
            .collect();
        let mut created: Vec<SceneNodeId> = Vec::with_capacity(self.nodes.len());
        for source in &self.nodes {
            let target_parent = source.parent.map(|p| created[p.0]).or(parent);
            let id = match target_parent {
                Some(parent) => scene.add_child(parent, &source.name),
                None => scene.add_root(&source.name),
            };
            let mut node = source.clone();
            node.uuid = identities[&source.uuid];
            node.parent = target_parent;
            node.children.clear();
            node.entity_index = None;
            // Reconciliation keys belong to the source; copies must not alias them.
            node.stable_key = None;
            remap_entity_references(&mut node, &identities);
            *scene.get_mut(id).expect("newly allocated node") = node;
            created.push(id);
        }
        Ok(created[0])
    }
}

/// Validate the project-relative source reference stored on an Assembly instance.
///
/// The `.prefab.ron` extension is a legacy format name; the product concept is
/// an Assembly. This function performs no filesystem I/O.
pub fn validate_assembly_source(source: &str) -> Result<(), String> {
    LinkedAssembly {
        source: source.to_string(),
        version: 1,
        source_hash: None,
    }
    .validate()
    .map_err(str::to_string)?;
    if source.len() > 512 {
        return Err("Assembly source path exceeds 512 bytes.".into());
    }
    if !source.to_ascii_lowercase().ends_with(".prefab.ron") {
        return Err(format!(
            "Assembly source must use the .prefab.ron extension: {source}"
        ));
    }
    Ok(())
}

/// Stable FNV-1a 64 hash of the serialized Assembly bytes.
///
/// The prefix matches the project asset content-hash convention. This is a
/// change fingerprint, not a security hash.
pub fn assembly_source_hash(bytes: &[u8]) -> String {
    let mut hash = 0xcbf2_9ce4_8422_2325u64;
    for byte in bytes {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    format!("fnv1a64:{hash:016x}")
}

/// Result of following nested Assembly links without touching the filesystem.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct AssemblyWalkSummary {
    /// Unique sources in load order (parents before dependencies).
    pub sources: Vec<String>,
    /// Total nodes across unique sources.
    pub nodes: usize,
}

/// Walk `linked_assembly.source` references through a caller-provided resolver.
///
/// `load` performs the actual read; `raf_core` never opens files. The walk
/// rejects invalid sources, missing/failed loads, cycles, nesting deeper than
/// [`MAX_ASSEMBLY_DEPTH`], and graphs larger than
/// [`MAX_ASSEMBLY_TOTAL_NODES`]. Shared dependencies are loaded once.
pub fn walk_assembly_dependencies<F>(
    root_source: &str,
    mut load: F,
) -> Result<AssemblyWalkSummary, String>
where
    F: FnMut(&str) -> Result<ScenePrefab, String>,
{
    let mut visited = HashSet::new();
    let mut path = Vec::new();
    let mut summary = AssemblyWalkSummary::default();
    visit_assembly_source(
        root_source,
        1,
        &mut load,
        &mut visited,
        &mut path,
        &mut summary,
    )?;
    Ok(summary)
}

fn visit_assembly_source<F>(
    source: &str,
    depth: usize,
    load: &mut F,
    visited: &mut HashSet<String>,
    path: &mut Vec<String>,
    summary: &mut AssemblyWalkSummary,
) -> Result<(), String>
where
    F: FnMut(&str) -> Result<ScenePrefab, String>,
{
    validate_assembly_source(source)?;
    if depth > MAX_ASSEMBLY_DEPTH {
        return Err(format!(
            "Assembly nesting exceeds {MAX_ASSEMBLY_DEPTH} levels at '{source}'."
        ));
    }
    if path.iter().any(|ancestor| ancestor == source) {
        let mut cycle = path.clone();
        cycle.push(source.to_string());
        return Err(format!("Assembly cycle detected: {}.", cycle.join(" -> ")));
    }
    if visited.contains(source) {
        return Ok(());
    }

    path.push(source.to_string());
    let prefab =
        load(source).map_err(|error| format!("Cannot load Assembly source '{source}': {error}"))?;
    prefab
        .validate()
        .map_err(|error| format!("Assembly source '{source}' is invalid: {error}"))?;
    let nodes = summary.nodes.saturating_add(prefab.nodes.len());
    if nodes > MAX_ASSEMBLY_TOTAL_NODES {
        path.pop();
        return Err(format!(
            "Assembly dependency graph exceeds the {MAX_ASSEMBLY_TOTAL_NODES}-node budget."
        ));
    }
    summary.nodes = nodes;
    summary.sources.push(source.to_string());

    for node in &prefab.nodes {
        if let Some(link) = &node.linked_assembly {
            link.validate().map_err(|error| {
                format!("Assembly source '{source}' contains an invalid link: {error}")
            })?;
            visit_assembly_source(&link.source, depth + 1, load, visited, path, summary)?;
        }
    }

    path.pop();
    visited.insert(source.to_string());
    Ok(())
}

/// Internal references follow copies; external UUIDs remain explicit external dependencies.
pub fn remap_entity_references(node: &mut SceneNode, identities: &HashMap<Uuid, Uuid>) {
    if let Some(target) = node.collider.body_owner.as_mut() {
        if let Some(&mapped) = identities.get(target) {
            *target = mapped;
        }
    }
    for joint in &mut node.physics_joints {
        if let Some(&target) = identities.get(&joint.target) {
            joint.target = target;
        }
    }
    for variable in node
        .variables
        .iter_mut()
        .chain(node.behavior_parameters.values_mut().flatten())
    {
        if let VariableValue::Entity(Some(id)) = &mut variable.value {
            if let Some(replacement) = identities.get(id) {
                *id = *replacement;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn copies_remap_internal_parameters_keep_external_references_and_reject_bad_links() {
        let mut scene = SceneGraph::new();
        let root = scene.add_root("Rig");
        let camera = scene.add_child(root, "Camera");
        let external = Uuid::new_v4();
        let camera_uuid = scene.get(camera).unwrap().uuid;
        scene
            .get_mut(root)
            .unwrap()
            .set_variable("camera", VariableValue::Entity(Some(camera_uuid)));
        scene
            .get_mut(root)
            .unwrap()
            .set_variable("target", VariableValue::Entity(Some(external)));
        let prefab = ScenePrefab::capture(&scene, root).unwrap();
        let encoded = ron::to_string(&prefab).unwrap();
        let mut prefab: ScenePrefab = ron::from_str(&encoded).unwrap();
        let copy = prefab.instantiate(&mut scene, None).unwrap();
        let copied_camera = scene.get(copy).unwrap().children[0];
        assert_ne!(scene.get(copied_camera).unwrap().uuid, camera_uuid);
        assert_eq!(
            scene.get(copy).unwrap().get_variable("camera"),
            Some(&VariableValue::Entity(Some(
                scene.get(copied_camera).unwrap().uuid
            )))
        );
        assert_eq!(
            scene.get(copy).unwrap().get_variable("target"),
            Some(&VariableValue::Entity(Some(external)))
        );
        prefab.nodes[0].children.push(SceneNodeId(0));
        let count = scene.len();
        assert!(prefab.instantiate(&mut scene, None).is_err());
        assert_eq!(scene.len(), count);
    }

    fn linked_node(name: &str, source: &str) -> SceneNode {
        let mut node = SceneNode::new(name);
        node.linked_assembly = Some(LinkedAssembly {
            source: source.into(),
            version: 1,
            source_hash: None,
        });
        node
    }

    fn prefab(nodes: Vec<SceneNode>) -> ScenePrefab {
        ScenePrefab { version: 1, nodes }
    }

    fn chain_prefab(len: usize, link: Option<&str>) -> ScenePrefab {
        let mut nodes = Vec::with_capacity(len);
        for index in 0..len {
            let mut node = SceneNode::new(&format!("N{index}"));
            if index > 0 {
                node.parent = Some(SceneNodeId(index - 1));
            }
            nodes.push(node);
        }
        for index in 1..len {
            nodes[index - 1].children.push(SceneNodeId(index));
        }
        if let Some(link) = link {
            nodes[0].linked_assembly = Some(LinkedAssembly {
                source: link.into(),
                version: 1,
                source_hash: None,
            });
        }
        prefab(nodes)
    }

    #[test]
    fn assembly_sources_and_hashes_are_confined_and_stable() {
        assert!(validate_assembly_source("parts/rig.prefab.ron").is_ok());
        for invalid in [
            "",
            "/parts/rig.prefab.ron",
            "../rig.prefab.ron",
            "parts/../rig.prefab.ron",
            "parts\\rig.prefab.ron",
            "C:/rig.prefab.ron",
            "parts/rig.ron",
        ] {
            assert!(
                validate_assembly_source(invalid).is_err(),
                "{invalid} must be rejected"
            );
        }

        let mut root = SceneNode::new("Root");
        root.linked_assembly = Some(LinkedAssembly {
            source: "parts/rig.prefab.ron".into(),
            version: 1,
            source_hash: Some(assembly_source_hash(b"one")),
        });
        prefab(vec![root]).validate().unwrap();

        let invalid = prefab(vec![linked_node("Root", "../escape.prefab.ron")]);
        assert!(invalid.validate().is_err());

        assert_eq!(assembly_source_hash(b"one"), assembly_source_hash(b"one"));
        assert_ne!(assembly_source_hash(b"one"), assembly_source_hash(b"two"));
        assert!(assembly_source_hash(b"one").starts_with("fnv1a64:"));
    }

    #[test]
    fn assembly_walk_detects_cycles_counts_shared_dependencies_once_and_stops_at_depth() {
        let mut root = SceneNode::new("Root");
        root.children = vec![SceneNodeId(1), SceneNodeId(2)];
        let mut left = linked_node("Left", "shared.prefab.ron");
        left.parent = Some(SceneNodeId(0));
        let mut right = linked_node("Right", "shared.prefab.ron");
        right.parent = Some(SceneNodeId(0));
        let shared = prefab(vec![SceneNode::new("Shared")]);

        let mut shared_loads = 0usize;
        let summary = walk_assembly_dependencies("root.prefab.ron", |source| match source {
            "root.prefab.ron" => Ok(prefab(vec![root.clone(), left.clone(), right.clone()])),
            "shared.prefab.ron" => {
                shared_loads += 1;
                Ok(shared.clone())
            }
            other => Err(format!("unexpected {other}")),
        })
        .unwrap();
        assert_eq!(shared_loads, 1);
        assert_eq!(summary.sources.len(), 2);
        assert_eq!(summary.nodes, 4);

        let a = prefab(vec![linked_node("A", "b.prefab.ron")]);
        let b = prefab(vec![linked_node("B", "a.prefab.ron")]);
        let error = walk_assembly_dependencies("a.prefab.ron", |source| match source {
            "a.prefab.ron" => Ok(a.clone()),
            "b.prefab.ron" => Ok(b.clone()),
            other => Err(format!("unexpected {other}")),
        })
        .unwrap_err();
        assert!(error.contains("cycle detected"), "{error}");

        let depth_error = walk_assembly_dependencies("s0.prefab.ron", |source| {
            let index = source
                .trim_start_matches('s')
                .trim_end_matches(".prefab.ron")
                .parse::<usize>()
                .unwrap();
            let link = (index < MAX_ASSEMBLY_DEPTH).then(|| format!("s{}.prefab.ron", index + 1));
            Ok(chain_prefab(1, link.as_deref()))
        })
        .unwrap_err();
        assert!(depth_error.contains("exceeds 8 levels"), "{depth_error}");
    }

    #[test]
    fn assembly_walk_enforces_the_total_node_budget() {
        let sources = (0..5)
            .map(|index| format!("part{index}.prefab.ron"))
            .collect::<Vec<_>>();
        let error = walk_assembly_dependencies("part0.prefab.ron", |source| {
            let index = sources
                .iter()
                .position(|candidate| candidate == source)
                .ok_or("unexpected source")?;
            let link = sources.get(index + 1).map(String::as_str);
            Ok(chain_prefab(900, link))
        })
        .unwrap_err();
        assert!(error.contains("node budget"), "{error}");
    }
}
