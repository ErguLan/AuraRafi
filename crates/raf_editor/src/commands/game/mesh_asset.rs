//! Static mesh asset commands: import, inspect, place and remove.
//!
//! Host filesystem paths are accepted only when the caller explicitly sets
//! `host=true` (the editor import action). Project-relative paths always resolve
//! below the active project's `assets` directory.

mod skeletal;
pub(crate) use skeletal::place_rig;

use raf_assets::asset_paths;
use raf_assets::importer::{import_model_package_with_destination, ModelImportMetadata};
use raf_assets::model_asset::{load_static_model, ModelDependency, ModelMaterial};
use raf_core::scene::MeshAssetRef;
use serde_json::{json, Value};
use std::path::Path;

use super::query::*;
use super::transform::parse_transform_patch;
use super::world::active_project_root;
use super::*;

pub(super) fn execute(command: &ParsedCommand, ctx: &mut GameCommandContext<'_>) -> CommandOutput {
    let Some(action) = text_argument(command, "action")
        .or_else(|| command.first_positional().map(str::to_string))
        .map(|action| action.trim().to_ascii_lowercase())
        .filter(|action| !action.is_empty())
    else {
        return CommandOutput::error(
            "Mesh asset",
            "Missing action. Use action=import, inspect, place, rig or remove.",
        );
    };
    match action.as_str() {
        "import" => import(command),
        "inspect" => inspect(command),
        "place" | "add" => place(command, ctx),
        "rig" => skeletal::place(command, ctx),
        "remove" | "clear" => remove(command, ctx),
        other => CommandOutput::error(
            "Mesh asset",
            format!(
                "Unknown mesh asset action '{other}'. Use import, inspect, place, rig or remove."
            ),
        ),
    }
}

fn path_argument(command: &ParsedCommand) -> Option<String> {
    text_argument(command, "path")
        .or_else(|| text_argument(command, "source"))
        .or_else(|| text_argument(command, "asset"))
        .or_else(|| command.positional.get(1).cloned())
        .map(|path| path.trim().to_string())
        .filter(|path| !path.is_empty())
}

fn import(command: &ParsedCommand) -> CommandOutput {
    let Some(raw_path) = path_argument(command) else {
        return CommandOutput::error(
            "Import model",
            "Missing path. Use a project-relative asset path or an explicit host path from the editor.",
        );
    };
    let Some(root) = active_project_root() else {
        return CommandOutput::error(
            "Import model",
            "Importing a model requires an active project.",
        );
    };

    let is_host_path = Path::new(&raw_path).is_absolute();
    let source = if is_host_path {
        let host_allowed = match bool_argument(command, "host", false) {
            Ok(value) => value,
            Err(error) => return CommandOutput::error("Import model", error),
        };
        if !host_allowed {
            return CommandOutput::error(
                "Import model",
                "Absolute host paths are accepted only from the editor import action (host=true). Use a project-relative asset path.",
            );
        }
        let source = PathBuf::from(&raw_path);
        if !source.is_file() {
            return CommandOutput::error(
                "Import model",
                format!("Host file '{}' does not exist.", source.display()),
            );
        }
        source
    } else {
        match asset_paths::resolve_under_project(&root, &raw_path) {
            Ok(path) => path,
            Err(error) => return CommandOutput::error("Import model", error),
        }
    };
    if !source.is_file() {
        return CommandOutput::error(
            "Import model",
            format!("Model source '{}' does not exist.", source.display()),
        );
    }

    let destination = match text_argument(command, "destination")
        .or_else(|| text_argument(command, "dest"))
        .or_else(|| text_argument(command, "folder"))
    {
        Some(destination) => match asset_paths::normalize_directory_reference(&destination) {
            Ok(destination) => Some(destination),
            Err(error) => return CommandOutput::error("Import model", error),
        },
        None => None,
    };

    let assets_dir = root.join("assets");
    let report =
        match import_model_package_with_destination(&source, &assets_dir, destination.as_deref()) {
            Ok(report) => report,
            Err(error) => return CommandOutput::error("Import model", error),
        };
    let metadata = report.metadata.clone();
    CommandOutput::info(
        format!("Imported model {}", report.mesh_asset.path),
        vec![
            format!("package: {}", report.package),
            format!("files: {}", report.copied_files.len()),
            format!("triangles: {}", metadata.triangles),
            format!("vertices: {}", metadata.vertices),
            format!("materials: {}", metadata.materials.len()),
            format!("hash: {}", metadata.content_hash),
        ],
        json!({
            "ok": true,
            "mesh_asset": report.mesh_asset,
            "package": report.package,
            "copied_files": report.copied_files,
            "metadata": metadata,
            "warnings": report.warnings
        }),
    )
}

fn inspect(command: &ParsedCommand) -> CommandOutput {
    let Some(raw_reference) = text_argument(command, "target").or_else(|| path_argument(command))
    else {
        return CommandOutput::error(
            "Inspect model",
            "Missing target path for the static model to inspect.",
        );
    };
    if Path::new(&raw_reference).is_absolute() {
        return CommandOutput::error(
            "Inspect model",
            "Inspect requires a project-relative asset path. Import host files first.",
        );
    }
    let Some(root) = active_project_root() else {
        return CommandOutput::error(
            "Inspect model",
            "Inspecting a model requires an active project.",
        );
    };
    let reference = match asset_paths::normalize_reference(&raw_reference) {
        Ok(reference) => reference,
        Err(error) => return CommandOutput::error("Inspect model", error),
    };
    let file = match asset_paths::resolve_under_project(&root, &reference) {
        Ok(file) => file,
        Err(error) => return CommandOutput::error("Inspect model", error),
    };
    if !file.is_file() {
        return CommandOutput::error(
            "Inspect model",
            format!("Model '{}' was not found in the project assets.", reference),
        );
    }
    let model = match load_static_model(&file) {
        Ok(model) => model,
        Err(error) => return CommandOutput::error("Inspect model", error),
    };
    let package = reference
        .rsplit_once('/')
        .map(|(directory, _)| directory)
        .unwrap_or("");
    let materials = prefixed_materials(&model.materials, package);
    let dependencies = prefixed_dependencies(&model.dependencies, package);
    let dimensions = model.bounds.dimensions();
    CommandOutput::info(
        format!("Inspected model {reference}"),
        vec![
            format!("triangles: {}", model.triangles),
            format!("vertices: {}", model.vertex_count()),
            format!("meshes: {}", model.meshes.len()),
            format!("materials: {}", materials.len()),
            format!("dependencies: {}", dependencies.len()),
            format!(
                "dimensions: [{:.3}, {:.3}, {:.3}]",
                dimensions[0], dimensions[1], dimensions[2]
            ),
        ],
        json!({
            "ok": true,
            "path": reference,
            "bounds": {
                "min": model.bounds.min,
                "max": model.bounds.max,
                "dimensions": dimensions
            },
            "triangles": model.triangles,
            "vertices": model.vertex_count(),
            "meshes": model.meshes.len(),
            "materials": materials,
            "dependencies": dependencies,
            "warnings": model.warnings
        }),
    )
}

fn prefixed_materials(materials: &[ModelMaterial], package: &str) -> Vec<Value> {
    materials
        .iter()
        .cloned()
        .map(|mut material| {
            if let Some(texture) = material.base_color_texture.take() {
                material.base_color_texture = Some(prefix_reference(package, &texture));
            }
            serde_json::to_value(&material).unwrap_or(Value::Null)
        })
        .collect()
}

fn prefixed_dependencies(dependencies: &[ModelDependency], package: &str) -> Vec<Value> {
    dependencies
        .iter()
        .cloned()
        .map(|mut dependency| {
            dependency.path = prefix_reference(package, &dependency.path);
            serde_json::to_value(&dependency).unwrap_or(Value::Null)
        })
        .collect()
}

fn prefix_reference(package: &str, reference: &str) -> String {
    if package.is_empty() || reference.starts_with("embedded:") {
        reference.to_string()
    } else {
        format!("{package}/{reference}")
    }
}

fn place(command: &ParsedCommand, ctx: &mut GameCommandContext<'_>) -> CommandOutput {
    let mut mesh_asset = match mesh_asset_argument(command) {
        Ok(mesh_asset) => mesh_asset,
        Err(error) => return CommandOutput::error("Place model", error),
    };
    if let Err(error) = mesh_asset.validate() {
        return CommandOutput::error("Place model", error.to_string());
    }
    if let Some(root) = active_project_root() {
        match asset_paths::resolve_under_project(&root, &mesh_asset.path) {
            Ok(file) if file.is_file() => {}
            Ok(_) => {
                return CommandOutput::error(
                    "Place model",
                    format!(
                        "Model '{}' was not found in the project assets. Import it first or pass an explicit mesh_asset object for a cached reference.",
                        mesh_asset.path
                    ),
                )
            }
            Err(error) => return CommandOutput::error("Place model", error),
        }
    }

    let name = text_argument(command, "name")
        .map(|name| display_name(&name))
        .filter(|name| !name.is_empty())
        .unwrap_or_else(|| {
            Path::new(&mesh_asset.path)
                .file_stem()
                .and_then(|stem| stem.to_str())
                .map(str::to_string)
                .unwrap_or_else(|| "Model".to_string())
        });
    let parent = match parent_target(command, ctx.scene) {
        Ok(parent) => parent,
        Err(error) => return CommandOutput::error("Place model", error),
    };
    let transform = match parse_transform_patch(command) {
        Ok(transform) => transform,
        Err(error) => return CommandOutput::error("Place model", error),
    };
    let position = transform.position_or(Vec3::ZERO);
    let rotation = transform.rotation_or(Vec3::ZERO);
    let scale = transform.scale_or(Vec3::ONE);
    if !position.is_finite()
        || !rotation.is_finite()
        || !scale.is_finite()
        || scale.x <= 0.0
        || scale.y <= 0.0
        || scale.z <= 0.0
    {
        return CommandOutput::error(
            "Place model",
            "Model transform requires finite values and a positive scale.",
        );
    }

    let id = match parent {
        Some(parent) => ctx
            .scene
            .add_child_with_primitive(parent, &name, Primitive::Empty),
        None => ctx.scene.add_root_with_primitive(&name, Primitive::Empty),
    };
    let metadata = active_project_root().and_then(|root| {
        let file = asset_paths::resolve_under_project(&root, &mesh_asset.path).ok()?;
        ModelImportMetadata::read_for_asset_file(&file)
    });
    if let Some(metadata) = metadata.as_ref() {
        mesh_asset.bounds = Some([
            metadata.bounds.min[0],
            metadata.bounds.min[1],
            metadata.bounds.min[2],
            metadata.bounds.max[0],
            metadata.bounds.max[1],
            metadata.bounds.max[2],
        ]);
        mesh_asset.content_hash = Some(metadata.content_hash.clone());
        mesh_asset.import_scale = metadata.import_scale;
    }
    if let Some(node) = ctx.scene.get_mut(id) {
        node.position = position;
        node.rotation = rotation;
        node.scale = scale;
        node.mesh_asset = Some(mesh_asset.clone());
        node.source_asset = Some(mesh_asset.path.clone());
        if let Some(metadata) = metadata.as_ref() {
            if let Some(material) = metadata.materials.first() {
                let [r, g, b, a] = material.base_color;
                node.color = NodeColor::rgba(
                    (r.clamp(0.0, 1.0) * 255.0).round() as u8,
                    (g.clamp(0.0, 1.0) * 255.0).round() as u8,
                    (b.clamp(0.0, 1.0) * 255.0).round() as u8,
                    (a.clamp(0.0, 1.0) * 255.0).round() as u8,
                );
                if let Some(texture) = material
                    .base_color_texture
                    .as_deref()
                    .filter(|texture| !texture.starts_with("embedded:"))
                {
                    let package = mesh_asset
                        .path
                        .rsplit_once('/')
                        .map(|(directory, _)| directory)
                        .unwrap_or("");
                    node.base_color_texture = Some(if package.is_empty() {
                        texture.to_string()
                    } else {
                        format!("{package}/{texture}")
                    });
                }
            }
        }
    }
    select_ids(ctx, vec![id]);
    let node = ctx.scene.get(id).expect("placed model node exists");
    let mut data = node_json(id, node, ctx.scene);
    if let Some(object) = data.as_object_mut() {
        object.insert(
            "mesh_asset".to_string(),
            serde_json::to_value(&mesh_asset).unwrap_or(Value::Null),
        );
        if let Some(metadata) = metadata {
            object.insert(
                "model".to_string(),
                json!({
                    "bounds": metadata.bounds,
                    "triangles": metadata.triangles,
                    "vertices": metadata.vertices,
                    "materials": metadata.materials,
                    "dependencies": metadata.dependencies
                }),
            );
        }
    }
    CommandOutput::changed(
        format!("Placed model {}", node.name),
        node_detail_lines(id, node, ctx.scene),
        data,
    )
}

fn mesh_asset_argument(command: &ParsedCommand) -> Result<MeshAssetRef, String> {
    if let Some(value) = command.structured_arg("mesh_asset") {
        if !value.is_null() {
            let mesh_asset: MeshAssetRef = serde_json::from_value(value.clone())
                .map_err(|error| format!("mesh_asset is invalid: {error}"))?;
            return Ok(mesh_asset);
        }
    }
    let Some(raw_path) = path_argument(command) else {
        return Err("Missing path or mesh_asset object for the static model to place.".to_string());
    };
    let normalized = asset_paths::normalize_reference(&raw_path)?;
    Ok(MeshAssetRef::new(normalized))
}

fn remove(command: &ParsedCommand, ctx: &mut GameCommandContext<'_>) -> CommandOutput {
    let Some(id) = resolve_target(command, ctx.scene, ctx.selection) else {
        return CommandOutput::error("Remove mesh asset", "Target not found.");
    };
    let removed = ctx.scene.get_mut(id).and_then(|node| {
        node.skin = None;
        node.mesh_asset.take()
    });
    select_ids(ctx, vec![id]);
    let node = ctx.scene.get(id).expect("target node exists");
    let mut data = node_json(id, node, ctx.scene);
    if let Some(object) = data.as_object_mut() {
        object.insert("mesh_asset".to_string(), Value::Null);
        if let Some(removed) = &removed {
            object.insert(
                "removed_mesh_asset".to_string(),
                serde_json::to_value(removed).unwrap_or(Value::Null),
            );
        }
    }
    match removed {
        Some(removed) => CommandOutput::changed(
            format!("Removed mesh asset from {}", node.name),
            vec![format!("removed: {}", removed.path)],
            data,
        ),
        None => CommandOutput::info(
            format!("{} has no mesh asset", node.name),
            vec!["No mesh_asset was assigned to the target.".to_string()],
            data,
        ),
    }
}
