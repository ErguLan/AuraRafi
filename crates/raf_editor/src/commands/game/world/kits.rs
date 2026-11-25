//! Built-in world kits and archetype placement helpers.

use super::super::query::parse_vec3_value;
use super::compile::CompiledWorld;
use super::*;

pub(super) fn slugify(value: &str) -> String {
    let mut slug = String::with_capacity(value.len());
    let mut previous_underscore = false;
    for character in value.chars() {
        let mapped = if character.is_ascii_alphanumeric() {
            character.to_ascii_lowercase()
        } else {
            '_'
        };
        if mapped == '_' {
            if previous_underscore {
                continue;
            }
            previous_underscore = true;
        } else {
            previous_underscore = false;
        }
        slug.push(mapped);
    }
    slug.trim_matches('_').to_string()
}

pub(super) fn unique_name(base: &str, used: &mut HashSet<String>) -> String {
    let base = display_name(base);
    let base = if base.is_empty() {
        "Generated".to_string()
    } else {
        base
    };
    if used.insert(base.to_ascii_lowercase()) {
        return base;
    }
    for index in 2..100_000 {
        let candidate = format!("{base}_{index}");
        if used.insert(candidate.to_ascii_lowercase()) {
            return candidate;
        }
    }
    format!("{base}_{}", Uuid::new_v4().simple())
}

pub(super) fn unique_key(base: &str, used: &mut HashSet<String>) -> String {
    let base = slugify(base);
    let base = if base.is_empty() {
        "world.generated".to_string()
    } else {
        base
    };
    if used.insert(base.clone()) {
        return base;
    }
    for index in 2..100_000 {
        let candidate = format!("{base}.{index}");
        if used.insert(candidate.clone()) {
            return candidate;
        }
    }
    format!("{base}.{}", Uuid::new_v4().simple())
}

pub(super) fn template_key(template: Option<&str>, fallback: &str, indices: &[usize]) -> String {
    let mut key = template.unwrap_or(fallback).to_string();
    for (index, value) in indices.iter().enumerate() {
        key = key.replace(&format!("{{{index}}}"), &value.to_string());
    }
    key = key
        .replace("{i}", &indices.first().copied().unwrap_or(0).to_string())
        .replace("{j}", &indices.get(1).copied().unwrap_or(0).to_string())
        .replace("{k}", &indices.get(2).copied().unwrap_or(0).to_string());
    key
}

pub(super) fn value_vec3(
    value: Option<&Value>,
    label: &str,
    default: Vec3,
) -> Result<Vec3, String> {
    match value {
        Some(value) => parse_vec3_value(value, label).map(Vec3::from_array),
        None => Ok(default),
    }
}

pub(super) fn value_count(value: Option<&Value>, label: &str) -> Result<Vec3, String> {
    let Some(value) = value else {
        return Ok(Vec3::ONE);
    };
    if let Some(count) = value.as_u64() {
        if count == 0 || count > 4096 {
            return Err(format!("{label} must be between 1 and 4096."));
        }
        return Ok(Vec3::new(count as f32, 1.0, 1.0));
    }
    let array = value
        .as_array()
        .ok_or_else(|| format!("{label} must be an integer or an array of 1 to 3 counts."))?;
    if array.is_empty() || array.len() > 3 {
        return Err(format!("{label} must contain 1 to 3 counts."));
    }
    let mut counts = Vec3::ONE;
    for (index, item) in array.iter().enumerate() {
        let count = item
            .as_u64()
            .ok_or_else(|| format!("{label}[{index}] must be a positive integer."))?;
        if count == 0 || count > 4096 {
            return Err(format!("{label}[{index}] must be between 1 and 4096."));
        }
        counts[index] = count as f32;
    }
    Ok(counts)
}

pub(super) fn value_spacing(
    value: Option<&Value>,
    label: &str,
    default: Vec3,
) -> Result<Vec3, String> {
    let Some(value) = value else {
        return Ok(default);
    };
    if let Some(number) = value.as_f64() {
        if !number.is_finite() {
            return Err(format!("{label} must be finite."));
        }
        return Ok(Vec3::splat(number as f32));
    }
    let array = value
        .as_array()
        .ok_or_else(|| format!("{label} must be a number or an array of 1 to 3 numbers."))?;
    if array.is_empty() || array.len() > 3 {
        return Err(format!("{label} must contain 1 to 3 numbers."));
    }
    let mut spacing = default;
    for (index, item) in array.iter().enumerate() {
        let number = item
            .as_f64()
            .ok_or_else(|| format!("{label}[{index}] must be a finite number."))?;
        if !number.is_finite() {
            return Err(format!("{label}[{index}] must be finite."));
        }
        spacing[index] = number as f32;
    }
    Ok(spacing)
}

fn builtin_archetype_parts(name: &str) -> Option<Vec<Value>> {
    let parts = match name.to_ascii_lowercase().as_str() {
        "floor" | "ground" | "suelo" => vec![serde_json::json!({
            "kind": "cube", "name": "floor",
            "transform": {"scale": [12.0, 0.2, 10.0]},
            "color_rgba": [96, 102, 108, 255],
            "tags": ["structure.floor"],
            "collider": {"type": "aabb"}
        })],
        "wall" | "wall_segment" | "muro" => vec![serde_json::json!({
            "kind": "cube", "name": "wall",
            "transform": {"scale": [4.0, 3.0, 0.2]},
            "color_rgba": [190, 190, 184, 255],
            "tags": ["structure.wall"],
            "collider": {"type": "aabb"}
        })],
        "door" | "door_frame" | "entrance" => vec![
            serde_json::json!({"kind":"cube","name":"post_left","transform":{"position":[-0.9,1.05,0],"scale":[0.18,2.1,0.18]},"color_rgba":[120,92,64,255],"tags":["structure.door"]}),
            serde_json::json!({"kind":"cube","name":"post_right","transform":{"position":[0.9,1.05,0],"scale":[0.18,2.1,0.18]},"color_rgba":[120,92,64,255],"tags":["structure.door"]}),
            serde_json::json!({"kind":"cube","name":"lintel","transform":{"position":[0,2.16,0],"scale":[2.0,0.18,0.18]},"color_rgba":[120,92,64,255],"tags":["structure.door"]}),
        ],
        "shelf" | "estante" | "gondola" => vec![
            serde_json::json!({"kind":"cube","name":"back","transform":{"position":[0,1.0,-0.5],"scale":[2.0,2.0,0.08]},"color_rgba":[112,88,62,255],"tags":["store.shelf"]}),
            serde_json::json!({"kind":"cube","name":"side_left","transform":{"position":[-1.0,1.0,0],"scale":[0.08,2.0,1.0]},"color_rgba":[112,88,62,255],"tags":["store.shelf"]}),
            serde_json::json!({"kind":"cube","name":"side_right","transform":{"position":[1.0,1.0,0],"scale":[0.08,2.0,1.0]},"color_rgba":[112,88,62,255],"tags":["store.shelf"]}),
            serde_json::json!({"kind":"cube","name":"shelf_top","transform":{"position":[0,2.0,0],"scale":[2.0,0.08,1.0]},"color_rgba":[138,112,80,255],"tags":["store.shelf"]}),
            serde_json::json!({"kind":"cube","name":"shelf_mid","transform":{"position":[0,1.0,0],"scale":[2.0,0.07,1.0]},"color_rgba":[138,112,80,255],"tags":["store.shelf"]}),
            serde_json::json!({"kind":"cube","name":"shelf_low","transform":{"position":[0,0.35,0],"scale":[2.0,0.07,1.0]},"color_rgba":[138,112,80,255],"tags":["store.shelf"]}),
        ],
        "checkout" | "counter" | "caja" => vec![
            serde_json::json!({"kind":"cube","name":"counter","transform":{"position":[0,0.55,0],"scale":[2.2,1.1,0.8]},"color_rgba":[168,160,148,255],"tags":["store.checkout"]}),
            serde_json::json!({"kind":"cube","name":"register","transform":{"position":[0.55,1.28,0],"scale":[0.5,0.35,0.4]},"color_rgba":[52,58,64,255],"tags":["store.checkout"]}),
        ],
        "street_lamp" | "lamp" | "farola" => vec![
            serde_json::json!({"kind":"cylinder","name":"post","transform":{"position":[0,1.75,0],"scale":[0.12,3.5,0.12]},"color_rgba":[54,58,62,255],"tags":["street.light"]}),
            serde_json::json!({"kind":"sphere","name":"bulb","transform":{"position":[0,3.6,0],"scale":[0.32,0.32,0.32]},"color_rgba":[255,226,156,255],"tags":["street.light"]}),
        ],
        "tree" | "arbol" => vec![
            serde_json::json!({"kind":"cylinder","name":"trunk","transform":{"position":[0,1.2,0],"scale":[0.28,2.4,0.28]},"color_rgba":[92,64,42,255],"tags":["nature.tree"]}),
            serde_json::json!({"kind":"sphere","name":"canopy","transform":{"position":[0,3.0,0],"scale":[2.0,1.8,2.0]},"color_rgba":[70,120,58,255],"tags":["nature.tree"]}),
        ],
        "bench" | "banco" => vec![
            serde_json::json!({"kind":"cube","name":"seat","transform":{"position":[0,0.48,0],"scale":[1.8,0.12,0.55]},"color_rgba":[132,92,56,255],"tags":["street.furniture"]}),
            serde_json::json!({"kind":"cube","name":"back","transform":{"position":[0,0.85,-0.22],"scale":[1.8,0.5,0.1]},"color_rgba":[132,92,56,255],"tags":["street.furniture"]}),
            serde_json::json!({"kind":"cube","name":"leg_left","transform":{"position":[-0.72,0.24,0],"scale":[0.1,0.48,0.5]},"color_rgba":[64,64,64,255],"tags":["street.furniture"]}),
            serde_json::json!({"kind":"cube","name":"leg_right","transform":{"position":[0.72,0.24,0],"scale":[0.1,0.48,0.5]},"color_rgba":[64,64,64,255],"tags":["street.furniture"]}),
        ],
        "crate" | "caja_madera" => vec![serde_json::json!({
            "kind":"cube","name":"crate","transform":{"scale":[1.0,1.0,1.0]},"color_rgba":[150,112,68,255],"tags":["prop.crate"],"collider":{"type":"aabb"}
        })],
        "platform" | "plataforma" => vec![serde_json::json!({
            "kind":"cube","name":"platform","transform":{"scale":[4.0,0.4,4.0]},"color_rgba":[96,112,132,255],"tags":["gameplay.platform"],"collider":{"type":"aabb"}
        })],
        "stairs" | "escalera" => (0..6)
            .map(|index| {
                serde_json::json!({
                    "kind":"cube",
                    "name":format!("step_{index}"),
                    "transform":{"position":[0.0,0.15 + index as f32 * 0.3, index as f32 * 0.32],"scale":[2.0,0.3,0.32]},
                    "color_rgba":[150,150,145,255],
                    "tags":["structure.stairs"]
                })
            })
            .collect(),
        "checkpoint" | "checkpoint_marker" => vec![
            serde_json::json!({"kind":"cylinder","name":"base","transform":{"position":[0,0.1,0],"scale":[0.9,0.2,0.9]},"color_rgba":[46,156,118,255],"tags":["gameplay.checkpoint"]}),
            serde_json::json!({"kind":"capsule","name":"marker","transform":{"position":[0,1.1,0],"scale":[0.6,1.1,0.6]},"color_rgba":[74,214,164,255],"tags":["gameplay.checkpoint"]}),
        ],
        "car" | "vehiculo" | "auto" => vec![
            serde_json::json!({"kind":"cube","name":"body","transform":{"position":[0,0.55,0],"scale":[1.9,0.55,4.0]},"color_rgba":[168,52,44,255],"tags":["vehicle.car"]}),
            serde_json::json!({"kind":"cube","name":"cabin","transform":{"position":[0,1.02,-0.1],"scale":[1.7,0.5,2.0]},"color_rgba":[74,88,102,255],"tags":["vehicle.car"]}),
            serde_json::json!({"kind":"cylinder","name":"wheel_left","transform":{"position":[-0.98,0.3,1.2],"rotation_deg":[0,0,90],"scale":[0.6,0.2,0.6]},"color_rgba":[36,36,38,255],"tags":["vehicle.car"]}),
            serde_json::json!({"kind":"cylinder","name":"wheel_right","transform":{"position":[0.98,0.3,1.2],"rotation_deg":[0,0,90],"scale":[0.6,0.2,0.6]},"color_rgba":[36,36,38,255],"tags":["vehicle.car"]}),
            serde_json::json!({"kind":"cylinder","name":"wheel_left_rear","transform":{"position":[-0.98,0.3,-1.2],"rotation_deg":[0,0,90],"scale":[0.6,0.2,0.6]},"color_rgba":[36,36,38,255],"tags":["vehicle.car"]}),
            serde_json::json!({"kind":"cylinder","name":"wheel_right_rear","transform":{"position":[0.98,0.3,-1.2],"rotation_deg":[0,0,90],"scale":[0.6,0.2,0.6]},"color_rgba":[36,36,38,255],"tags":["vehicle.car"]}),
        ],
        "aisle" | "pasillo" => vec![serde_json::json!({
            "kind":"plane","name":"aisle","transform":{"scale":[2.0,0.05,8.0]},"color_rgba":[196,178,118,255],"tags":["store.circulation","structure.circulation"],"collider":{"type":"none"}
        })],
        "spawn" | "spawn_point" => vec![serde_json::json!({
            "kind":"cylinder","name":"spawn","transform":{"scale":[0.8,0.05,0.8]},"color_rgba":[64,156,236,255],"tags":["gameplay.spawn"]
        })],
        "goal" | "meta" => vec![serde_json::json!({
            "kind":"sphere","name":"goal","transform":{"position":[0,1.0,0],"scale":[0.8,0.8,0.8]},"color_rgba":[240,196,64,255],"tags":["gameplay.goal"]
        })],
        _ => return None,
    };
    Some(parts)
}

pub(super) fn archetype_parts(
    name: &str,
    archetypes: &Map<String, Value>,
) -> Result<(Vec<Value>, Vec<String>), String> {
    if let Some(definition) = archetypes.get(name) {
        let parts = definition
            .get("parts")
            .and_then(Value::as_array)
            .cloned()
            .ok_or_else(|| format!("Archetype '{name}' requires a parts array."))?;
        if parts.is_empty() || parts.len() > 256 {
            return Err(format!(
                "Archetype '{name}' accepts between 1 and 256 parts."
            ));
        }
        let tags = definition
            .get("tags")
            .and_then(Value::as_array)
            .map(|tags| {
                tags.iter()
                    .filter_map(Value::as_str)
                    .map(str::to_string)
                    .collect()
            })
            .unwrap_or_default();
        return Ok((parts, tags));
    }
    let parts = builtin_archetype_parts(name).ok_or_else(|| {
        format!(
            "Unknown archetype '{name}'. Declare it in program.archetypes or use a built-in kit."
        )
    })?;
    Ok((parts, Vec::new()))
}

pub(super) fn archetype_text(value: Option<&Value>, label: &str) -> Result<String, String> {
    value
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
        .ok_or_else(|| format!("{label} requires a non-empty string."))
}

pub(super) fn place_archetype_instance(
    compiled: &mut CompiledWorld,
    archetypes: &Map<String, Value>,
    archetype: &str,
    instance_name: &str,
    instance_key: &str,
    parent: Option<&str>,
    transform: Value,
    extra_tags: &[String],
    used_names: &mut HashSet<String>,
    used_keys: &mut HashSet<String>,
) -> Result<(), String> {
    let (parts, archetype_tags) = archetype_parts(archetype, archetypes)?;
    let root_name = unique_name(instance_name, used_names);
    let root_key = unique_key(instance_key, used_keys);
    let mut tags = archetype_tags;
    for tag in extra_tags {
        if !tags.iter().any(|existing| existing == tag) {
            tags.push(tag.clone());
        }
    }
    compiled.groups.push(serde_json::json!({
        "name": root_name,
        "parent": parent,
        "stable_key": root_key,
        "tags": tags,
        "semantic_role": format!("kit.{archetype}"),
        "transform": transform,
    }));
    for (index, part) in parts.iter().enumerate() {
        let mut entity = part
            .as_object()
            .cloned()
            .ok_or_else(|| format!("Archetype '{archetype}' part {index} must be an object."))?;
        let part_name = entity
            .get("name")
            .and_then(Value::as_str)
            .map(str::to_string)
            .unwrap_or_else(|| format!("part_{index}"));
        entity.insert(
            "name".to_string(),
            Value::String(unique_name(&format!("{root_name}_{part_name}"), used_names)),
        );
        entity.insert("parent".to_string(), Value::String(root_name.clone()));
        let part_key = entity
            .get("stable_key")
            .and_then(Value::as_str)
            .map(str::to_string)
            .unwrap_or_else(|| format!("{root_key}.{}", slugify(&part_name)));
        entity.insert(
            "stable_key".to_string(),
            Value::String(unique_key(&part_key, used_keys)),
        );
        if !entity.contains_key("semantic_role") {
            entity.insert(
                "semantic_role".to_string(),
                Value::String(format!("kit.{archetype}.{}", slugify(&part_name))),
            );
        }
        compiled.entities.push(Value::Object(entity));
    }
    Ok(())
}

pub(super) fn mirror_compiled_entities(
    compiled: &mut CompiledWorld,
    operation: &Map<String, Value>,
    used_names: &mut HashSet<String>,
    used_keys: &mut HashSet<String>,
) -> Result<(), String> {
    let source = operation
        .get("source")
        .or_else(|| operation.get("group"))
        .or_else(|| operation.get("parent"))
        .and_then(Value::as_str)
        .ok_or_else(|| "mirror requires source/group/parent.".to_string())?;
    let source_name = compiled
        .groups
        .iter()
        .find(|group| {
            group.get("name").and_then(Value::as_str) == Some(source)
                || group.get("stable_key").and_then(Value::as_str) == Some(source)
        })
        .and_then(|group| group.get("name").and_then(Value::as_str))
        .unwrap_or(source)
        .to_string();
    let axis = operation
        .get("axis")
        .and_then(Value::as_str)
        .unwrap_or("x")
        .to_ascii_lowercase();
    let axis_index = match axis.as_str() {
        "x" => 0,
        "y" => 1,
        "z" => 2,
        other => return Err(format!("mirror axis must be x, y or z, got '{other}'.")),
    };
    let offset = operation
        .get("offset")
        .and_then(Value::as_f64)
        .unwrap_or(0.0);
    let indices = compiled
        .entities
        .iter()
        .enumerate()
        .filter(|(_, entity)| {
            entity.get("parent").and_then(Value::as_str) == Some(source_name.as_str())
        })
        .map(|(index, _)| index)
        .collect::<Vec<_>>();
    if indices.is_empty() {
        return Err(format!(
            "mirror source '{source_name}' has no compiled entities to mirror."
        ));
    }
    for index in indices {
        let mut entity = compiled.entities[index].clone();
        let old_name = entity
            .get("name")
            .and_then(Value::as_str)
            .unwrap_or("entity")
            .to_string();
        let old_key = entity
            .get("stable_key")
            .and_then(Value::as_str)
            .unwrap_or("world.mirror")
            .to_string();
        entity["name"] = Value::String(unique_name(&format!("{old_name}_mirror"), used_names));
        entity["stable_key"] = Value::String(unique_key(&format!("{old_key}.mirror"), used_keys));
        if let Some(position) = entity
            .get_mut("transform")
            .and_then(Value::as_object_mut)
            .and_then(|transform| transform.get_mut("position"))
            .and_then(Value::as_array_mut)
        {
            if let Some(value) = position.get(axis_index).and_then(Value::as_f64) {
                position[axis_index] = serde_json::json!(2.0 * offset - value);
            }
        }
        if let Some(role) = entity.get("semantic_role").and_then(Value::as_str) {
            entity["semantic_role"] = Value::String(format!("{role}.mirror"));
        }
        compiled.entities.push(entity);
    }
    Ok(())
}

pub(super) fn variant_compiled_entities(
    compiled: &mut CompiledWorld,
    archetypes: &Map<String, Value>,
    operation: &Map<String, Value>,
    parent: Option<&str>,
    used_names: &mut HashSet<String>,
    used_keys: &mut HashSet<String>,
) -> Result<(), String> {
    let variants = operation
        .get("archetypes")
        .or_else(|| operation.get("variants"))
        .and_then(Value::as_array)
        .ok_or_else(|| "variant requires archetypes/variants array.".to_string())?;
    if variants.is_empty() {
        return Err("variant requires at least one archetype.".to_string());
    }
    let variants = variants
        .iter()
        .map(|value| {
            value
                .as_str()
                .map(str::to_string)
                .ok_or_else(|| "variant archetypes must be strings.".to_string())
        })
        .collect::<Result<Vec<_>, _>>()?;
    let count = operation
        .get("count")
        .and_then(Value::as_u64)
        .unwrap_or(2)
        .clamp(1, 4096) as usize;
    let origin = value_vec3(
        operation
            .get("origin")
            .or_else(|| operation.get("position")),
        "variant.origin",
        Vec3::ZERO,
    )?;
    let spacing = value_spacing(
        operation.get("spacing"),
        "variant.spacing",
        Vec3::new(2.0, 0.0, 2.0),
    )?;
    let name = operation
        .get("name")
        .and_then(Value::as_str)
        .unwrap_or("Variant");
    let key = operation
        .get("key")
        .or_else(|| operation.get("stable_key"))
        .and_then(Value::as_str);
    let tags = operation
        .get("tags")
        .and_then(Value::as_array)
        .map(|tags| {
            tags.iter()
                .filter_map(Value::as_str)
                .map(str::to_string)
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    let parent = operation.get("parent").and_then(Value::as_str).or(parent);
    for index in 0..count {
        let archetype = &variants[index % variants.len()];
        let position = origin
            + Vec3::new(
                index as f32 * spacing.x,
                index as f32 * spacing.y,
                index as f32 * spacing.z,
            );
        let instance_key = template_key(
            key,
            &format!("world.variant.{}", slugify(archetype)),
            &[index],
        );
        let transform = serde_json::json!({
            "position": [position.x, position.y, position.z],
            "rotation_deg": operation.get("rotation_deg").cloned().unwrap_or_else(|| serde_json::json!([0, 0, 0])),
            "scale": operation.get("scale").cloned().unwrap_or_else(|| serde_json::json!([1, 1, 1])),
        });
        place_archetype_instance(
            compiled,
            archetypes,
            archetype,
            &format!("{name}_{index}"),
            &instance_key,
            parent,
            transform,
            &tags,
            used_names,
            used_keys,
        )?;
    }
    Ok(())
}
