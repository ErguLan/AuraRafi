//! World program compiler.

use super::kits::*;
use super::*;

pub(super) struct CompiledWorld {
    pub(super) groups: Vec<Value>,
    pub(super) entities: Vec<Value>,
    pub(super) updates: Vec<Value>,
    pub(super) deletions: Vec<Value>,
}

fn plan_operation_target(operation: &Map<String, Value>) -> Option<String> {
    operation
        .get("target")
        .and_then(Value::as_str)
        .map(str::to_string)
        .or_else(|| {
            operation
                .get("key")
                .or_else(|| operation.get("stable_key"))
                .and_then(Value::as_str)
                .map(|key| format!("key:{key}"))
        })
}

pub(super) fn compile_world_program(value: &Value) -> Result<CompiledWorld, String> {
    let object = value
        .as_object()
        .ok_or_else(|| "World program must be a JSON object.".to_string())?;
    let archetypes = object
        .get("archetypes")
        .and_then(Value::as_object)
        .cloned()
        .unwrap_or_default();
    let parent = object
        .get("parent")
        .and_then(Value::as_str)
        .map(str::to_string);
    let mut compiled = CompiledWorld {
        groups: Vec::new(),
        entities: Vec::new(),
        updates: Vec::new(),
        deletions: Vec::new(),
    };
    let mut used_names = HashSet::new();
    let mut used_keys = HashSet::new();

    if let Some(groups) = object.get("groups").and_then(Value::as_array) {
        if groups.len() > 512 {
            return Err("World program accepts at most 512 explicit groups.".to_string());
        }
        for (index, group) in groups.iter().enumerate() {
            let mut group = group
                .as_object()
                .cloned()
                .ok_or_else(|| format!("groups[{index}] must be an object."))?;
            let raw_name = group
                .get("name")
                .and_then(Value::as_str)
                .map(str::to_string)
                .unwrap_or_else(|| format!("Group_{index}"));
            let name = unique_name(&raw_name, &mut used_names);
            group.insert("name".to_string(), Value::String(name.clone()));
            if let Some(key) = group.get("stable_key").and_then(Value::as_str) {
                group.insert(
                    "stable_key".to_string(),
                    Value::String(unique_key(key, &mut used_keys)),
                );
            } else {
                group.insert(
                    "stable_key".to_string(),
                    Value::String(unique_key(
                        &format!("auto.group.{}", slugify(&name)),
                        &mut used_keys,
                    )),
                );
            }
            if !group.contains_key("parent") {
                if let Some(parent) = &parent {
                    group.insert("parent".to_string(), Value::String(parent.clone()));
                }
            }
            compiled.groups.push(Value::Object(group));
        }
    }
    if let Some(entities) = object.get("entities").and_then(Value::as_array) {
        if entities.len() > 4096 {
            return Err("World program accepts at most 4096 explicit entities.".to_string());
        }
        for (index, entity) in entities.iter().enumerate() {
            let mut entity = entity
                .as_object()
                .cloned()
                .ok_or_else(|| format!("entities[{index}] must be an object."))?;
            let raw_name = entity
                .get("name")
                .and_then(Value::as_str)
                .map(str::to_string)
                .unwrap_or_else(|| format!("Entity_{index}"));
            let name = unique_name(&raw_name, &mut used_names);
            entity.insert("name".to_string(), Value::String(name.clone()));
            if let Some(key) = entity.get("stable_key").and_then(Value::as_str) {
                entity.insert(
                    "stable_key".to_string(),
                    Value::String(unique_key(key, &mut used_keys)),
                );
            } else {
                entity.insert(
                    "stable_key".to_string(),
                    Value::String(unique_key(&format!("auto.{name}"), &mut used_keys)),
                );
            }
            if !entity.contains_key("parent") {
                if let Some(parent) = &parent {
                    entity.insert("parent".to_string(), Value::String(parent.clone()));
                }
            }
            compiled.entities.push(Value::Object(entity));
        }
    }

    let operations = object
        .get("operations")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    if operations.len() > 2048 {
        return Err("World program accepts at most 2048 operations.".to_string());
    }
    for (index, operation) in operations.iter().enumerate() {
        let operation = operation
            .as_object()
            .ok_or_else(|| format!("operations[{index}] must be an object."))?;
        let kind = operation
            .get("op")
            .or_else(|| operation.get("kind"))
            .and_then(Value::as_str)
            .unwrap_or("")
            .trim()
            .to_ascii_lowercase();
        match kind.as_str() {
            "group" | "folder" => {
                let raw_name = archetype_text(operation.get("name"), &format!("operations[{index}].name"))?;
                let name = unique_name(&raw_name, &mut used_names);
                let key = operation
                    .get("key")
                    .or_else(|| operation.get("stable_key"))
                    .and_then(Value::as_str)
                    .map(str::to_string)
                    .unwrap_or_else(|| format!("world.group.{}", slugify(&name)));
                compiled.groups.push(serde_json::json!({
                    "name": name,
                    "parent": operation.get("parent").and_then(Value::as_str).or(parent.as_deref()),
                    "stable_key": unique_key(&key, &mut used_keys),
                    "semantic_role": operation.get("semantic_role").and_then(Value::as_str),
                    "tags": operation.get("tags").cloned().unwrap_or(Value::Array(Vec::new())),
                    "transform": operation.get("transform").cloned().unwrap_or_else(|| serde_json::json!({})),
                }));
            }
            "place" | "instance" => {
                let archetype = archetype_text(
                    operation.get("archetype").or_else(|| operation.get("kit")),
                    &format!("operations[{index}].archetype"),
                )?;
                let raw_name = operation
                    .get("name")
                    .and_then(Value::as_str)
                    .map(str::to_string)
                    .unwrap_or_else(|| format!("{archetype}_{index}"));
                let fallback_key = format!("world.{}.{}", slugify(&archetype), slugify(&raw_name));
                let key = operation
                    .get("key")
                    .or_else(|| operation.get("stable_key"))
                    .and_then(Value::as_str)
                    .map(str::to_string)
                    .unwrap_or(fallback_key);
                let transform = operation
                    .get("transform")
                    .cloned()
                    .unwrap_or_else(|| {
                        serde_json::json!({
                            "position": operation.get("position").cloned().unwrap_or_else(|| serde_json::json!([0,0,0])),
                            "rotation_deg": operation.get("rotation_deg").cloned().unwrap_or_else(|| serde_json::json!([0,0,0])),
                            "scale": operation.get("scale").cloned().unwrap_or_else(|| serde_json::json!([1,1,1])),
                        })
                    });
                let tags = operation
                    .get("tags")
                    .and_then(Value::as_array)
                    .map(|tags| tags.iter().filter_map(Value::as_str).map(str::to_string).collect::<Vec<_>>())
                    .unwrap_or_default();
                place_archetype_instance(
                    &mut compiled,
                    &archetypes,
                    &archetype,
                    &raw_name,
                    &key,
                    operation
                        .get("parent")
                        .and_then(Value::as_str)
                        .or(parent.as_deref()),
                    transform,
                    &tags,
                    &mut used_names,
                    &mut used_keys,
                )?;
            }
            "grid" => {
                let archetype = archetype_text(
                    operation.get("archetype").or_else(|| operation.get("kit")),
                    &format!("operations[{index}].archetype"),
                )?;
                let counts = value_count(operation.get("count"), &format!("operations[{index}].count"))?;
                let spacing = value_spacing(
                    operation.get("spacing"),
                    &format!("operations[{index}].spacing"),
                    Vec3::splat(2.0),
                )?;
                let origin = value_vec3(
                    operation.get("origin").or_else(|| operation.get("position")),
                    &format!("operations[{index}].origin"),
                    Vec3::ZERO,
                )?;
                let total = counts.x as usize * counts.y as usize * counts.z as usize;
                if total > 4096 {
                    return Err(format!(
                        "operations[{index}] grid would create {total} instances; the per-operation budget is 4096."
                    ));
                }
                for x in 0..counts.x as usize {
                    for y in 0..counts.y as usize {
                        for z in 0..counts.z as usize {
                            let indices = [x, y, z];
                            let name = format!(
                                "{}_{index}_{x}_{y}_{z}",
                                operation
                                    .get("name")
                                    .and_then(Value::as_str)
                                    .unwrap_or(&archetype)
                            );
                            let key = template_key(
                                operation
                                    .get("key")
                                    .or_else(|| operation.get("stable_key"))
                                    .and_then(Value::as_str),
                                &format!("world.{}.{index}.{x}.{y}.{z}", slugify(&archetype)),
                                &indices,
                            );
                            let position = origin
                                + Vec3::new(x as f32 * spacing.x, y as f32 * spacing.y, z as f32 * spacing.z);
                            let transform = serde_json::json!({
                                "position": [position.x, position.y, position.z],
                                "rotation_deg": operation.get("rotation_deg").cloned().unwrap_or_else(|| serde_json::json!([0,0,0])),
                                "scale": operation.get("scale").cloned().unwrap_or_else(|| serde_json::json!([1,1,1])),
                            });
                            let tags = operation
                                .get("tags")
                                .and_then(Value::as_array)
                                .map(|tags| tags.iter().filter_map(Value::as_str).map(str::to_string).collect::<Vec<_>>())
                                .unwrap_or_default();
                            place_archetype_instance(
                                &mut compiled,
                                &archetypes,
                                &archetype,
                                &name,
                                &key,
                                operation.get("parent").and_then(Value::as_str).or(parent.as_deref()),
                                transform,
                                &tags,
                                &mut used_names,
                                &mut used_keys,
                            )?;
                        }
                    }
                }
            }
            "along" | "line" | "path" => {
                let archetype = archetype_text(
                    operation.get("archetype").or_else(|| operation.get("kit")),
                    &format!("operations[{index}].archetype"),
                )?;
                let points = operation
                    .get("points")
                    .or_else(|| operation.get("path"))
                    .and_then(Value::as_array)
                    .cloned()
                    .ok_or_else(|| format!("operations[{index}] requires points/path as an array of [x,y,z]."))?;
                if points.len() < 2 {
                    return Err(format!("operations[{index}] requires at least two path points."));
                }
                let spacing = operation
                    .get("spacing")
                    .and_then(Value::as_f64)
                    .unwrap_or(2.0);
                if !spacing.is_finite() || spacing <= 0.0 {
                    return Err(format!("operations[{index}].spacing must be greater than zero."));
                }
                let points = points
                    .iter()
                    .enumerate()
                    .map(|(point_index, point)| {
                        value_vec3(Some(point), &format!("operations[{index}].points[{point_index}]"), Vec3::ZERO)
                    })
                    .collect::<Result<Vec<_>, _>>()?;
                let mut instance = 0usize;
                for segment in points.windows(2) {
                    let start = segment[0];
                    let end = segment[1];
                    let length = start.distance(end);
                    let steps = (length / spacing as f32).ceil().max(1.0) as usize;
                    for step in 0..steps {
                        let t = if steps <= 1 { 0.0 } else { step as f32 / steps as f32 };
                        let position = start.lerp(end, t);
                        let name = format!(
                            "{}_{index}_{instance}",
                            operation
                                .get("name")
                                .and_then(Value::as_str)
                                .unwrap_or(&archetype)
                        );
                        let key = template_key(
                            operation
                                .get("key")
                                .or_else(|| operation.get("stable_key"))
                                .and_then(Value::as_str),
                            &format!("world.{}.{index}.{instance}", slugify(&archetype)),
                            &[instance, 0, 0],
                        );
                        let transform = serde_json::json!({
                            "position": [position.x, position.y, position.z],
                            "rotation_deg": operation.get("rotation_deg").cloned().unwrap_or_else(|| serde_json::json!([0,0,0])),
                            "scale": operation.get("scale").cloned().unwrap_or_else(|| serde_json::json!([1,1,1])),
                        });
                        let tags = operation
                            .get("tags")
                            .and_then(Value::as_array)
                            .map(|tags| tags.iter().filter_map(Value::as_str).map(str::to_string).collect::<Vec<_>>())
                            .unwrap_or_default();
                        place_archetype_instance(
                            &mut compiled,
                            &archetypes,
                            &archetype,
                            &name,
                            &key,
                            operation.get("parent").and_then(Value::as_str).or(parent.as_deref()),
                            transform,
                            &tags,
                            &mut used_names,
                            &mut used_keys,
                        )?;
                        instance += 1;
                        if instance >= 4096 {
                            return Err(format!("operations[{index}] exceeded the 4096 instance budget."));
                        }
                    }
                }
            }
            "scatter" | "random" => {
                let archetype = archetype_text(
                    operation.get("archetype").or_else(|| operation.get("kit")),
                    &format!("operations[{index}].archetype"),
                )?;
                let region = operation
                    .get("region")
                    .and_then(Value::as_object)
                    .ok_or_else(|| format!("operations[{index}].region requires min/max arrays."))?;
                let min = value_vec3(region.get("min"), &format!("operations[{index}].region.min"), Vec3::splat(-10.0))?;
                let max = value_vec3(region.get("max"), &format!("operations[{index}].region.max"), Vec3::splat(10.0))?;
                if min.x > max.x || min.y > max.y || min.z > max.z {
                    return Err(format!("operations[{index}].region min must be <= max."));
                }
                let count = operation
                    .get("count")
                    .and_then(Value::as_u64)
                    .unwrap_or(16)
                    .clamp(1, 4096) as usize;
                let seed = operation
                    .get("seed")
                    .and_then(Value::as_u64)
                    .unwrap_or(1)
                    .max(1);
                let mut state = seed;
                for instance in 0..count {
                    state ^= state << 13;
                    state ^= state >> 7;
                    state ^= state << 17;
                    let fx = (state & 0xffff) as f32 / 65535.0;
                    state ^= state << 13;
                    state ^= state >> 7;
                    state ^= state << 17;
                    let fz = (state & 0xffff) as f32 / 65535.0;
                    let position = Vec3::new(
                        min.x + (max.x - min.x) * fx,
                        min.y,
                        min.z + (max.z - min.z) * fz,
                    );
                    let name = format!(
                        "{}_{index}_{instance}",
                        operation
                            .get("name")
                            .and_then(Value::as_str)
                            .unwrap_or(&archetype)
                    );
                    let key = template_key(
                        operation.get("key").or_else(|| operation.get("stable_key")).and_then(Value::as_str),
                        &format!("world.{}.{index}.{instance}", slugify(&archetype)),
                        &[instance, 0, 0],
                    );
                    let transform = serde_json::json!({
                        "position": [position.x, position.y, position.z],
                        "rotation_deg": operation.get("rotation_deg").cloned().unwrap_or_else(|| serde_json::json!([0,0,0])),
                        "scale": operation.get("scale").cloned().unwrap_or_else(|| serde_json::json!([1,1,1])),
                    });
                    let tags = operation
                        .get("tags")
                        .and_then(Value::as_array)
                        .map(|tags| tags.iter().filter_map(Value::as_str).map(str::to_string).collect::<Vec<_>>())
                        .unwrap_or_default();
                    place_archetype_instance(
                        &mut compiled,
                        &archetypes,
                        &archetype,
                        &name,
                        &key,
                        operation.get("parent").and_then(Value::as_str).or(parent.as_deref()),
                        transform,
                        &tags,
                        &mut used_names,
                        &mut used_keys,
                    )?;
                }
            }
            "camera" => {
                let raw_name = operation
                    .get("name")
                    .and_then(Value::as_str)
                    .unwrap_or("Camera");
                let name = unique_name(raw_name, &mut used_names);
                let raw_key = operation
                    .get("key")
                    .or_else(|| operation.get("stable_key"))
                    .and_then(Value::as_str)
                    .map(str::to_string)
                    .unwrap_or_else(|| format!("world.camera.{}", slugify(raw_name)));
                let camera_key = unique_key(&raw_key, &mut used_keys);
                let transform = operation.get("transform").cloned().unwrap_or_else(|| {
                    serde_json::json!({
                        "position": operation.get("position").cloned().unwrap_or_else(|| serde_json::json!([0, 0, 0])),
                        "rotation_deg": operation.get("rotation_deg").cloned().unwrap_or_else(|| serde_json::json!([0, 0, 0])),
                        "scale": operation.get("scale").cloned().unwrap_or_else(|| serde_json::json!([1, 1, 1])),
                    })
                });
                let mut camera = operation
                    .get("camera")
                    .and_then(Value::as_object)
                    .cloned()
                    .unwrap_or_default();
                for (source, target) in [
                    ("fov", "fov_degrees"),
                    ("fov_degrees", "fov_degrees"),
                    ("near", "near"),
                    ("far", "far"),
                    ("orthographic", "orthographic"),
                    ("ortho_scale", "ortho_scale"),
                ] {
                    if let Some(value) = operation.get(source) {
                        camera.insert(target.to_string(), value.clone());
                    }
                }
                compiled.entities.push(serde_json::json!({
                    "kind": "empty",
                    "name": name,
                    "parent": operation.get("parent").and_then(Value::as_str).or(parent.as_deref()),
                    "transform": transform,
                    "camera": Value::Object(camera),
                    "stable_key": camera_key,
                    "tags": ["world.camera"],
                }));
            }
            "script" => {
                let target = plan_operation_target(operation)
                    .ok_or_else(|| format!("operations[{index}] script requires target or stable_key."))?;
                let scripts = if let Some(scripts) = operation.get("scripts").and_then(Value::as_array) {
                    scripts
                        .iter()
                        .filter_map(Value::as_str)
                        .map(str::to_string)
                        .collect::<Vec<_>>()
                } else if let Some(file) = operation.get("file").and_then(Value::as_str) {
                    vec![file.to_string()]
                } else {
                    return Err(format!(
                        "operations[{index}] script requires scripts array or file."
                    ));
                };
                if scripts.is_empty() {
                    return Err(format!("operations[{index}] script list cannot be empty."));
                }
                compiled.updates.push(serde_json::json!({
                    "target": target,
                    "scripts": scripts,
                }));
            }
            "mirror" => {
                mirror_compiled_entities(&mut compiled, operation, &mut used_names, &mut used_keys)?;
            }
            "variant" => {
                variant_compiled_entities(
                    &mut compiled,
                    &archetypes,
                    operation,
                    parent.as_deref(),
                    &mut used_names,
                    &mut used_keys,
                )?;
            }
            "update" => {
                let target = plan_operation_target(operation)
                    .ok_or_else(|| format!("operations[{index}] update requires target or stable_key."))?;
                let mut patch = operation.clone();
                for key in ["op", "kind", "target", "key", "stable_key"] {
                    patch.remove(key);
                }
                patch.insert("target".to_string(), Value::String(target));
                compiled.updates.push(Value::Object(patch));
            }
            "delete" | "remove" => {
                let targets = if let Some(targets) = operation.get("targets").and_then(Value::as_array) {
                    targets
                        .iter()
                        .filter_map(Value::as_str)
                        .map(str::to_string)
                        .collect::<Vec<_>>()
                } else {
                    plan_operation_target(operation).into_iter().collect()
                };
                if targets.is_empty() {
                    return Err(format!(
                        "operations[{index}] delete requires target/targets or stable_key."
                    ));
                }
                let optional = operation
                    .get("optional")
                    .and_then(Value::as_bool)
                    .unwrap_or(false);
                for target in targets {
                    compiled.deletions.push(serde_json::json!({
                        "target": target,
                        "optional": optional,
                    }));
                }
            }
            "assert" => {
                if let Some(max_entities) = operation.get("max_entities").and_then(Value::as_u64) {
                    if compiled.entities.len() as u64 > max_entities {
                        return Err(format!(
                            "World program produced {} entities, above its own max_entities assert ({}).",
                            compiled.entities.len(),
                            max_entities
                        ));
                    }
                }
            }
            other => {
                return Err(format!(
                    "operations[{index}] has unsupported op '{other}'. Use group, place, grid, along, scatter, camera, script, mirror, variant, update, delete or assert."
                ))
            }
        }
        if compiled.entities.len() > 4096 {
            return Err(
                "World program exceeded the 4096 entity budget. Split it into regions or raise the budget explicitly."
                    .to_string(),
            );
        }
    }
    let asserted_max = operations
        .iter()
        .filter_map(|operation| operation.get("max_entities").and_then(Value::as_u64))
        .min();
    if let Some(max_entities) = asserted_max {
        if compiled.entities.len() as u64 > max_entities {
            return Err(format!(
                "World program produced {} entities, above its own max_entities assert ({}).",
                compiled.entities.len(),
                max_entities
            ));
        }
    }
    Ok(compiled)
}
