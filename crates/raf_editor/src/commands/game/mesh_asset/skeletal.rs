use super::*;
use raf_assets::skeletal_model;
use raf_core::scene::{animation::AnimationRig, mesh_asset::SkinBinding};
use std::collections::HashMap;

pub(super) fn place(command: &ParsedCommand, ctx: &mut GameCommandContext<'_>) -> CommandOutput {
    let result = (|| -> Result<_, String> {
        let project = active_project_root().ok_or("Skeletal placement requires an open project")?;
        let path = path_argument(command).ok_or("A model asset path is required")?;
        let parent = parent_target(command, ctx.scene)?;
        let clip = command
            .arg("clip")
            .map(|index| index.parse::<usize>().map_err(|error| error.to_string()))
            .transpose()?;
        let mut scene = ctx.scene.clone();
        let (id, clips) = place_rig(&mut scene, &project, &path, parent, clip)?;
        *ctx.scene = scene;
        ctx.selection.selected_node = Some(id);
        ctx.selection.selected_nodes = vec![id];
        ctx.viewport.set_selected_ids(vec![id]);
        let entry = ctx.scene.get(id).ok_or("The placed skeleton is missing")?;
        let mut data = node_json(id, entry, ctx.scene);
        data["clips"] = json!(clips);
        Ok(data)
    })();
    match result {
        Ok(data) => CommandOutput::changed("Placed skeletal model", Vec::new(), data),
        Err(error) => CommandOutput::error("Place skeletal model", error),
    }
}

pub(crate) fn place_rig(
    scene: &mut SceneGraph,
    project: &Path,
    asset: &str,
    parent: Option<SceneNodeId>,
    clip: Option<usize>,
) -> Result<(SceneNodeId, Vec<String>), String> {
    let path = asset_paths::normalize_reference(asset)?;
    let file = asset_paths::resolve_under_project(project, &path)?;
    if parent.is_some_and(|id| !scene.is_valid_node(id)) {
        return Err("The model parent was removed".into());
    }
    let model = skeletal_model::load(&file, true)?;
    let names = model.clips.iter().map(|layer| layer.name.clone()).collect();
    let animation = if model.clips.is_empty() && clip.is_none() {
        None
    } else {
        let layer = model
            .clips
            .get(clip.unwrap_or(0))
            .ok_or("The requested skeletal clip does not exist")?
            .clone();
        let rig = AnimationRig {
            layers: vec![layer],
            impact: None,
            contacts: Vec::new(),
            motion_match: None,
            physics_follow: Vec::new(),
        };
        rig.validate()?;
        Some(rig)
    };
    let name = file
        .file_stem()
        .and_then(|name| name.to_str())
        .unwrap_or("Rig");
    let root = match parent {
        Some(parent) => scene.add_child(parent, name),
        None => scene.add_root(name),
    };
    let metadata = ModelImportMetadata::read_for_asset_file(&file);
    let mut ids = HashMap::new();
    for node in &model.nodes {
        let parent = node
            .parent
            .and_then(|source| ids.get(&source).copied())
            .unwrap_or(root);
        let id = scene.add_child(parent, &node.name);
        ids.insert(node.source, id);
        let placed = scene.get_mut(id).expect("just inserted");
        placed.position = node.translation;
        placed.rotation = node.rotation;
        placed.scale = node.scale;
    }
    let model_inverse = scene.world_matrix(root).inverse();
    for mesh in model.meshes {
        let palette: Vec<_> = mesh
            .joints
            .iter()
            .zip(&mesh.inverse_bind)
            .map(|(joint, inverse)| model_inverse * scene.world_matrix(ids[joint]) * *inverse)
            .collect();
        let normals = vec![glam::Mat3::IDENTITY; palette.len()];
        let bounds = mesh
            .vertices
            .iter()
            .map(|vertex| skeletal_model::deform_vertex(vertex, &palette, &normals).0)
            .fold(
                (Vec3::splat(f32::INFINITY), Vec3::splat(f32::NEG_INFINITY)),
                |(min, max), vertex| (min.min(vertex), max.max(vertex)),
            );
        let id = scene.add_child(root, &format!("Mesh [{}]", mesh.source));
        let placed = scene.get_mut(id).expect("just inserted");
        let mut reference = MeshAssetRef::new(&path);
        reference.node = Some(mesh.source);
        if bounds.0.is_finite() && bounds.1.is_finite() {
            reference.bounds = Some([
                bounds.0.x, bounds.0.y, bounds.0.z, bounds.1.x, bounds.1.y, bounds.1.z,
            ]);
        }
        reference.content_hash = metadata
            .as_ref()
            .map(|metadata| metadata.content_hash.clone());
        placed.mesh_asset = Some(reference);
        placed.skin = Some(SkinBinding {
            joints: mesh
                .joints
                .iter()
                .map(|index| model.paths[index].clone())
                .collect(),
        });
        placed.locked = true;
    }
    let owner = scene.get_mut(root).expect("just inserted");
    owner.source_asset = Some(path);
    owner.animation = animation;
    Ok((root, names))
}
