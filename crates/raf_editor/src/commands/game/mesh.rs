//! Persistent Tab Mode mesh editing commands.
//!
//! The command surface is intentionally index-based so the Agent, the CLI and
//! the viewport all mutate the same `SceneNode::editable_mesh` payload.

use serde::de::DeserializeOwned;
use serde_json::json;

use super::query::{node_json, resolve_text_target, select_ids, text_argument};
use super::*;

const MESH_ACTIONS: &[&str] = &[
    "inspect",
    "enter",
    "set",
    "move",
    "scale",
    "extrude",
    "inset",
    "subdivide",
    "delete",
    "revert",
    "apply",
];

pub(super) fn execute(command: &ParsedCommand, ctx: &mut GameCommandContext<'_>) -> CommandOutput {
    let result = (|| -> Result<CommandOutput, String> {
        let action = text_argument(command, "action")
            .unwrap_or_else(|| "inspect".into())
            .trim()
            .to_ascii_lowercase();
        let target_raw = text_argument(command, "target").unwrap_or_else(|| "selected".into());
        let id = if target_raw == "selected" {
            ctx.selection.selected_node
        } else {
            resolve_text_target(&target_raw, ctx.scene)
        }
        .ok_or_else(|| format!("Mesh target '{target_raw}' not found."))?;
        let node = ctx.scene.get(id).ok_or("Mesh target is not a live node.")?;

        match action.as_str() {
            "inspect" => {
                let include_topology = bool_arg(command, "topology");
                Ok(CommandOutput::info(
                    "Mesh topology",
                    vec![format!("id: {}", id.0), format!("entity: {}", node.name)],
                    mesh_json(id, node, ctx.scene, include_topology),
                ))
            }
            "enter" => {
                if node.mesh_asset.is_some() && node.editable_mesh.is_none() {
                    return Err(
                        "Imported GLB/GLTF meshes are not editable yet. Use a primitive or an already authored editable mesh."
                            .into(),
                    );
                }
                if node.is_folder {
                    return Err("Folders have no mesh topology to edit.".into());
                }
                let mut mesh = match node.editable_mesh.clone() {
                    Some(mesh) => mesh,
                    None => raf_render::editable::EditableMesh::from_primitive(node.primitive)
                        .to_scene_data(),
                };
                mesh.touch();
                mesh.validate()
                    .map_err(|error| format!("Mesh is invalid: {error}"))?;
                if let Some(node) = ctx.scene.get_mut(id) {
                    node.editable_mesh = Some(mesh);
                }
                select_ids(ctx, vec![id]);
                let node = ctx.scene.get(id).expect("live mesh node");
                Ok(CommandOutput::changed(
                    "Mesh editing enabled",
                    vec![
                        format!("id: {}", id.0),
                        format!("entity: {}", node.name),
                        format!(
                            "vertices: {}",
                            node.editable_mesh.as_ref().map_or(0, |m| m.vertices.len())
                        ),
                        format!(
                            "faces: {}",
                            node.editable_mesh.as_ref().map_or(0, |m| m.face_count())
                        ),
                    ],
                    mesh_json(id, node, ctx.scene, false),
                ))
            }
            "set" => {
                let vertices: Vec<[f32; 3]> = json_argument(command, "vertices")?
                    .ok_or("mesh.set requires vertices=[[x,y,z], ...].")?;
                let indices: Vec<u32> = json_argument(command, "indices")?
                    .ok_or("mesh.set requires indices=[a,b,c, ...].")?;
                let mut mesh = raf_core::scene::EditableMeshData::new(vertices, indices)
                    .map_err(|error| format!("Mesh is invalid: {error}"))?;
                mesh.touch();
                if let Some(node) = ctx.scene.get_mut(id) {
                    node.editable_mesh = Some(mesh);
                }
                select_ids(ctx, vec![id]);
                let node = ctx.scene.get(id).expect("live mesh node");
                Ok(CommandOutput::changed(
                    "Mesh topology replaced",
                    vec![format!("id: {}", id.0)],
                    mesh_json(id, node, ctx.scene, false),
                ))
            }
            "move" | "scale" | "extrude" | "inset" | "subdivide" | "delete" => {
                let mut mesh = node.editable_mesh.clone().ok_or_else(|| {
                    "This entity has no editable mesh. Run action=enter first.".to_string()
                })?;
                let vertices = match json_argument::<Vec<u32>>(command, "indices")? {
                    Some(indices) => indices,
                    None => json_argument::<Vec<u32>>(command, "vertices")?.unwrap_or_default(),
                };
                let faces = json_argument::<Vec<u32>>(command, "faces")?.unwrap_or_default();
                let outcome = match action.as_str() {
                    "move" => {
                        let delta: [f32; 3] = vector_argument(command, "delta")?
                            .ok_or("mesh.move requires delta=[x,y,z].")?;
                        let targets = if vertices.is_empty() {
                            (0..mesh.vertices.len() as u32).collect::<Vec<_>>()
                        } else {
                            vertices
                        };
                        mesh.move_vertices(&targets, delta)
                            .map_err(|error| format!("Mesh move failed: {error}"))?;
                        None
                    }
                    "scale" => {
                        let factor: [f32; 3] =
                            vector_argument(command, "factor")?.unwrap_or([1.0, 1.0, 1.0]);
                        let center = vector_argument(command, "center")?
                            .unwrap_or_else(|| selection_center(&mesh, &vertices).to_array());
                        let targets = if vertices.is_empty() {
                            (0..mesh.vertices.len() as u32).collect::<Vec<_>>()
                        } else {
                            vertices
                        };
                        mesh.scale_vertices(&targets, center, factor)
                            .map_err(|error| format!("Mesh scale failed: {error}"))?;
                        None
                    }
                    "extrude" => {
                        let distance = scalar_argument(command, "distance").unwrap_or_default();
                        Some(
                            mesh.extrude_faces(&required_faces(command, &faces)?, distance)
                                .map_err(|error| format!("Mesh extrude failed: {error}"))?,
                        )
                    }
                    "inset" => {
                        let amount = scalar_argument(command, "amount").unwrap_or(0.2);
                        Some(
                            mesh.inset_faces(&required_faces(command, &faces)?, amount)
                                .map_err(|error| format!("Mesh inset failed: {error}"))?,
                        )
                    }
                    "subdivide" => Some(
                        mesh.subdivide_faces(&required_faces(command, &faces)?)
                            .map_err(|error| format!("Mesh subdivide failed: {error}"))?,
                    ),
                    "delete" => {
                        let removed = mesh
                            .delete_faces(&required_faces(command, &faces)?)
                            .map_err(|error| format!("Mesh delete failed: {error}"))?;
                        if mesh.is_empty() {
                            if let Some(node) = ctx.scene.get_mut(id) {
                                node.editable_mesh = None;
                            }
                            select_ids(ctx, vec![id]);
                            let node = ctx.scene.get(id).expect("live mesh node");
                            return Ok(CommandOutput::changed(
                                "Mesh cleared",
                                vec![format!("id: {}", id.0), format!("removed_faces: {removed}")],
                                mesh_json(id, node, ctx.scene, false),
                            ));
                        }
                        None
                    }
                    _ => unreachable!("mesh action matched above"),
                };
                mesh.touch();
                mesh.validate()
                    .map_err(|error| format!("Mesh is invalid: {error}"))?;
                if let Some(node) = ctx.scene.get_mut(id) {
                    node.editable_mesh = Some(mesh.clone());
                }
                select_ids(ctx, vec![id]);
                let node = ctx.scene.get(id).expect("live mesh node");
                let mut data = mesh_json(id, node, ctx.scene, false);
                if let Some(outcome) = outcome {
                    data["created_vertices"] = json!(outcome.created_vertices);
                    data["created_faces"] = json!(outcome.created_faces);
                }
                Ok(CommandOutput::changed(
                    "Mesh topology edited",
                    vec![
                        format!("id: {}", id.0),
                        format!("action: {action}"),
                        format!("vertices: {}", mesh.vertices.len()),
                        format!("faces: {}", mesh.face_count()),
                    ],
                    data,
                ))
            }
            "revert" => {
                let before = node.editable_mesh.clone();
                if let Some(node) = ctx.scene.get_mut(id) {
                    node.editable_mesh = None;
                }
                let node = ctx.scene.get(id).expect("live mesh node");
                if before.is_none() {
                    return Ok(CommandOutput::info(
                        "Mesh already uses its parametric primitive",
                        vec![format!("id: {}", id.0)],
                        mesh_json(id, node, ctx.scene, false),
                    ));
                }
                Ok(CommandOutput::changed(
                    "Mesh reverted to the parametric primitive",
                    vec![format!("id: {}", id.0)],
                    mesh_json(id, node, ctx.scene, false),
                ))
            }
            "apply" => {
                let mesh = node
                    .editable_mesh
                    .as_ref()
                    .ok_or("This entity has no editable mesh to apply.")?;
                mesh.validate()
                    .map_err(|error| format!("Mesh is invalid: {error}"))?;
                Ok(CommandOutput::info(
                    "Mesh topology accepted",
                    vec![format!("id: {}", id.0)],
                    mesh_json(id, node, ctx.scene, false),
                ))
            }
            other => Err(format!(
                "Unknown mesh action '{other}'. Use {}.",
                MESH_ACTIONS.join(", ")
            )),
        }
    })();
    result.unwrap_or_else(|error| CommandOutput::error("Mesh", error))
}

fn required_faces(command: &ParsedCommand, faces: &[u32]) -> Result<Vec<u32>, String> {
    if !faces.is_empty() {
        return Ok(faces.to_vec());
    }
    // A single selected face can be provided as `face=0` for shorter Agent calls.
    if let Some(face) = command
        .arg("face")
        .and_then(|value| value.parse::<u32>().ok())
    {
        return Ok(vec![face]);
    }
    Err("This action requires faces=[0,1,...] or face=0.".to_string())
}

fn selection_center(mesh: &raf_core::scene::EditableMeshData, vertices: &[u32]) -> glam::Vec3 {
    let mut sum = glam::Vec3::ZERO;
    let mut count = 0.0;
    for &index in vertices {
        if let Some(position) = mesh.vertex_position(index) {
            sum += position;
            count += 1.0;
        }
    }
    if count > 0.0 {
        sum / count
    } else {
        mesh.bounds()
            .map(|(min, max)| (min + max) * 0.5)
            .unwrap_or(glam::Vec3::ZERO)
    }
}

fn mesh_json(
    id: raf_core::scene::SceneNodeId,
    node: &raf_core::scene::SceneNode,
    scene: &raf_core::scene::SceneGraph,
    include_topology: bool,
) -> serde_json::Value {
    let mut data = node_json(id, node, scene);
    if let Some(mesh) = &node.editable_mesh {
        let (min, max) = mesh
            .bounds()
            .unwrap_or((glam::Vec3::ZERO, glam::Vec3::ZERO));
        data["has_editable_mesh"] = json!(true);
        data["editable_mesh"] = json!({
            "revision": mesh.revision,
            "vertex_count": mesh.vertices.len(),
            "face_count": mesh.face_count(),
            "bounds_min": [min.x, min.y, min.z],
            "bounds_max": [max.x, max.y, max.z],
        });
        if include_topology {
            data["editable_mesh"]["positions"] = json!(mesh.vertices);
            data["editable_mesh"]["indices"] = json!(mesh.indices);
        }
    } else {
        data["has_editable_mesh"] = json!(false);
        data["editable_mesh"] = serde_json::Value::Null;
    }
    data
}

fn json_argument<T: DeserializeOwned>(
    command: &ParsedCommand,
    name: &str,
) -> Result<Option<T>, String> {
    if let Some(value) = command.structured_arg(name) {
        return serde_json::from_value(value.clone())
            .map(Some)
            .map_err(|error| format!("{name} is invalid: {error}"));
    }
    let Some(raw) = command.arg(name) else {
        return Ok(None);
    };
    serde_json::from_str(raw)
        .map(Some)
        .map_err(|error| format!("{name} must be JSON: {error}"))
}

fn vector_argument(command: &ParsedCommand, name: &str) -> Result<Option<[f32; 3]>, String> {
    if let Some(value) = command.structured_arg(name) {
        return serde_json::from_value(value.clone())
            .map(Some)
            .map_err(|error| format!("{name} must be [x,y,z]: {error}"));
    }
    let Some(raw) = command.arg(name) else {
        return Ok(None);
    };
    if let Ok(value) = serde_json::from_str::<[f32; 3]>(raw) {
        return Ok(Some(value));
    }
    let parts = raw
        .split(',')
        .map(|part| part.trim().parse::<f32>())
        .collect::<Result<Vec<_>, _>>()
        .map_err(|_| format!("{name} must be [x,y,z] or x,y,z."))?;
    if parts.len() != 3 {
        return Err(format!("{name} must contain exactly three numbers."));
    }
    Ok(Some([parts[0], parts[1], parts[2]]))
}

fn scalar_argument(command: &ParsedCommand, name: &str) -> Option<f32> {
    command
        .arg(name)
        .and_then(|value| value.trim().parse::<f32>().ok())
        .or_else(|| {
            command
                .structured_arg(name)
                .and_then(serde_json::Value::as_f64)
                .map(|value| value as f32)
        })
}

fn bool_arg(command: &ParsedCommand, name: &str) -> bool {
    command
        .arg(name)
        .is_some_and(|value| matches!(value, "true" | "1" | "yes" | "on"))
}
