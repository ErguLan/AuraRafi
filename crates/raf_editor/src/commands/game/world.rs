//! World Program compiler, plan journal and persistent world brief.
//!
//! Kept as a child module of commands::game so the command kernel stays
//! readable without widening the public surface.

use super::*;

// ---------------------------------------------------------------------------
// World Program, plan journal and persistent world brief
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub(crate) struct PlannedWorld {
    pub(crate) id: String,
    pub(crate) hash: String,
    pub(crate) status: String,
    pub(crate) mode: String,
    pub(crate) parent: Option<Value>,
    pub(crate) design_profile: Option<String>,
    pub(crate) groups: Vec<Value>,
    pub(crate) entities: Vec<Value>,
    pub(crate) updates: Vec<Value>,
    pub(crate) deletions: Vec<Value>,
    pub(crate) fingerprint: std::collections::BTreeMap<String, Value>,
    pub(crate) persisted_path: Option<String>,
    pub(crate) project_root: Option<String>,
}

struct CompiledWorld {
    groups: Vec<Value>,
    entities: Vec<Value>,
    updates: Vec<Value>,
    deletions: Vec<Value>,
}

thread_local! {
    static ACTIVE_PROJECT_ROOT: RefCell<Option<PathBuf>> = const { RefCell::new(None) };
}

/// Bind the current command dispatch to a project root so plan artifacts and
/// the world brief stay inside the active project. The value is thread-local:
/// editor UI dispatch and attached IPC dispatch never exchange it implicitly.
pub fn set_project_root(root: Option<PathBuf>) {
    ACTIVE_PROJECT_ROOT.with(|slot| *slot.borrow_mut() = root);
}

pub fn clear_project_root() {
    ACTIVE_PROJECT_ROOT.with(|slot| *slot.borrow_mut() = None);
}

/// Scoped project binding for plan artifacts and the world brief.
pub struct ProjectRootGuard {
    previous: Option<PathBuf>,
}

impl Drop for ProjectRootGuard {
    fn drop(&mut self) {
        ACTIVE_PROJECT_ROOT.with(|slot| *slot.borrow_mut() = self.previous.take());
    }
}

pub fn bind_project_root(root: Option<PathBuf>) -> ProjectRootGuard {
    let previous = ACTIVE_PROJECT_ROOT.with(|slot| {
        let mut slot = slot.borrow_mut();
        std::mem::replace(&mut *slot, root)
    });
    ProjectRootGuard { previous }
}

pub(crate) fn active_project_root() -> Option<PathBuf> {
    ACTIVE_PROJECT_ROOT.with(|slot| slot.borrow().clone())
}

fn plan_cache() -> &'static Mutex<HashMap<String, PlannedWorld>> {
    static CACHE: OnceLock<Mutex<HashMap<String, PlannedWorld>>> = OnceLock::new();
    CACHE.get_or_init(|| Mutex::new(HashMap::new()))
}

fn world_directory(root: &Path) -> PathBuf {
    root.join(".aura_rafi").join("world")
}

fn plan_path(root: &Path, id: &str) -> PathBuf {
    world_directory(root).join("plans").join(format!("{id}.json"))
}

fn brief_path(root: &Path) -> PathBuf {
    world_directory(root).join("brief.json")
}

fn journal_path(root: &Path) -> PathBuf {
    world_directory(root).join("journal.jsonl")
}

fn sha256_hex(bytes: &[u8]) -> String {
    let digest = ring::digest::digest(&ring::digest::SHA256, bytes);
    let mut output = String::with_capacity(digest.as_ref().len() * 2);
    for byte in digest.as_ref() {
        use std::fmt::Write as _;
        let _ = write!(&mut output, "{byte:02x}");
    }
    output
}

fn plan_json(plan: &PlannedWorld) -> Value {
    serde_json::json!({
        "schema": "raf.world.plan",
        "version": 1,
        "id": plan.id,
        "hash": plan.hash,
        "status": plan.status,
        "mode": plan.mode,
        "parent": plan.parent,
        "design_profile": plan.design_profile,
        "groups": plan.groups,
        "entities": plan.entities,
        "updates": plan.updates,
        "deletions": plan.deletions,
        "fingerprint": plan.fingerprint,
        "persisted_path": plan.persisted_path,
        "project_root": plan.project_root,
    })
}

fn persist_plan(plan: &mut PlannedWorld) -> Result<(), String> {
    let Some(root) = active_project_root() else {
        return Ok(());
    };
    let directory = world_directory(&root).join("plans");
    std::fs::create_dir_all(&directory)
        .map_err(|error| format!("Could not create the world plan directory: {error}"))?;
    let path = plan_path(&root, &plan.id);
    plan.persisted_path = Some(
        path.strip_prefix(&root)
            .unwrap_or(&path)
            .to_string_lossy()
            .replace('\\', "/"),
    );
    let encoded = serde_json::to_vec_pretty(&plan_json(plan))
        .map_err(|error| format!("Could not encode the world plan: {error}"))?;
    std::fs::write(&path, encoded)
        .map_err(|error| format!("Could not write {}: {error}", path.display()))?;
    append_journal(
        &root,
        serde_json::json!({
            "event": "plan_created",
            "id": plan.id,
            "hash": plan.hash,
            "mode": plan.mode,
            "groups": plan.groups.len(),
            "entities": plan.entities.len(),
        }),
    );
    Ok(())
}

fn update_persisted_plan(plan: &PlannedWorld) {
    let Some(root) = active_project_root() else {
        return;
    };
    let path = plan_path(&root, &plan.id);
    if let Ok(encoded) = serde_json::to_vec_pretty(&plan_json(plan)) {
        let _ = std::fs::write(path, encoded);
    }
    append_journal(
        &root,
        serde_json::json!({
            "event": "plan_status",
            "id": plan.id,
            "status": plan.status,
            "revision_fingerprint": sha256_hex(
                serde_json::to_string(&plan.fingerprint).unwrap_or_default().as_bytes()
            ),
        }),
    );
}

fn append_journal(root: &Path, entry: Value) {
    let path = journal_path(root);
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    if let Ok(mut file) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)
    {
        let _ = writeln!(file, "{entry}");
    }
}

fn load_planned_world(id: &str) -> Result<PlannedWorld, String> {
    if let Ok(cache) = plan_cache().lock() {
        if let Some(plan) = cache.get(id) {
            return Ok(plan.clone());
        }
    }
    let Some(root) = active_project_root() else {
        return Err(format!("Unknown world plan '{id}'. Create it with game.plan first."));
    };
    let path = plan_path(&root, id);
    let bytes = std::fs::read(&path)
        .map_err(|error| format!("World plan '{id}' was not found ({}): {error}", path.display()))?;
    let value: Value = serde_json::from_slice(&bytes)
        .map_err(|error| format!("World plan '{id}' is not valid JSON: {error}"))?;
    let plan = PlannedWorld {
        id: id.to_string(),
        hash: value
            .get("hash")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string(),
        status: value
            .get("status")
            .and_then(Value::as_str)
            .unwrap_or("planned")
            .to_string(),
        mode: value
            .get("mode")
            .and_then(Value::as_str)
            .unwrap_or("build")
            .to_string(),
        parent: value.get("parent").cloned().filter(|value| !value.is_null()),
        design_profile: value
            .get("design_profile")
            .and_then(Value::as_str)
            .map(str::to_string),
        groups: value
            .get("groups")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default(),
        entities: value
            .get("entities")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default(),
        updates: value
            .get("updates")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default(),
        deletions: value
            .get("deletions")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default(),
        fingerprint: value
            .get("fingerprint")
            .cloned()
            .and_then(|value| serde_json::from_value(value).ok())
            .unwrap_or_default(),
        persisted_path: value
            .get("persisted_path")
            .and_then(Value::as_str)
            .map(str::to_string),
        project_root: value
            .get("project_root")
            .and_then(Value::as_str)
            .map(str::to_string),
    };
    if let Ok(mut cache) = plan_cache().lock() {
        cache.insert(plan.id.clone(), plan.clone());
    }
    Ok(plan)
}

fn slugify(value: &str) -> String {
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

fn unique_name(base: &str, used: &mut HashSet<String>) -> String {
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

fn unique_key(base: &str, used: &mut HashSet<String>) -> String {
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

fn template_key(template: Option<&str>, fallback: &str, indices: &[usize]) -> String {
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

fn value_vec3(value: Option<&Value>, label: &str, default: Vec3) -> Result<Vec3, String> {
    match value {
        Some(value) => parse_vec3_value(value, label).map(Vec3::from_array),
        None => Ok(default),
    }
}

fn value_count(value: Option<&Value>, label: &str) -> Result<Vec3, String> {
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

fn value_spacing(value: Option<&Value>, label: &str, default: Vec3) -> Result<Vec3, String> {
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

fn archetype_parts(
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
            return Err(format!("Archetype '{name}' accepts between 1 and 256 parts."));
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

fn archetype_text(value: Option<&Value>, label: &str) -> Result<String, String> {
    value
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
        .ok_or_else(|| format!("{label} requires a non-empty string."))
}

fn place_archetype_instance(
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

fn mirror_compiled_entities(
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
        entity["stable_key"] =
            Value::String(unique_key(&format!("{old_key}.mirror"), used_keys));
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

fn variant_compiled_entities(
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
        operation.get("origin").or_else(|| operation.get("position")),
        "variant.origin",
        Vec3::ZERO,
    )?;
    let spacing = value_spacing(operation.get("spacing"), "variant.spacing", Vec3::new(2.0, 0.0, 2.0))?;
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
    let parent = operation
        .get("parent")
        .and_then(Value::as_str)
        .or(parent);
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

fn compile_world_program(value: &Value) -> Result<CompiledWorld, String> {
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

/// Plans are applied through the same validated semantic commands as a manual
/// build, but the plan may contain thousands of entities. The previous commit
/// path forwarded the whole plan to `game.build`, whose 32-group and
/// 128-entity contract is correct for one interactive build but wrong for a
/// compiled world. Plan chunks keep every individual command inside that
/// contract while the plan itself is one command, one history entry and one
/// undo token at the adapter boundary.
const PLAN_GROUP_CHUNK: usize = 32;
const PLAN_ENTITY_CHUNK: usize = 128;

fn plan_chunk_command(
    plan: &PlannedWorld,
    key: &str,
    chunk: &[Value],
) -> ParsedCommand {
    let mut params = serde_json::json!({ key: chunk });
    if let Some(parent) = &plan.parent {
        params["parent"] = parent.clone();
    }
    parsed_command_from_params(
        if plan.mode == "reconcile" {
            "game.reconcile"
        } else {
            "game.build"
        },
        params,
    )
}

fn run_plan_chunk(
    plan: &PlannedWorld,
    scene: &mut SceneGraph,
    selection: &mut SceneSelectionState,
    viewport: &mut dyn GameViewportPort,
    key: &str,
    chunk: &[Value],
) -> Result<CommandOutput, String> {
    let command = plan_chunk_command(plan, key, chunk);
    let mut context = GameCommandContext {
        scene,
        selection,
        viewport,
    };
    let output = if plan.mode == "reconcile" {
        reconcile_scene(&command, &mut context)
    } else {
        build_scene(&command, &mut context)
    };
    if matches!(output.level, crate::commands::output::CommandLevel::Error) {
        return Err(output.lines.join(" "));
    }
    Ok(output)
}

/// Apply a compiled plan in bounded chunks and roll the whole document back if
/// any chunk fails. The caller owns the single transaction/undo scope; this
/// function only guarantees all-or-nothing scene state inside that scope.
fn resolve_plan_target(scene: &SceneGraph, reference: &str) -> Option<SceneNodeId> {
    if let Some(key) = reference.strip_prefix("key:") {
        return scene
            .iter()
            .find(|(_, node)| {
                node.stable_key
                    .as_deref()
                    .is_some_and(|stable| stable.eq_ignore_ascii_case(key))
            })
            .map(|(id, _)| id);
    }
    if let Ok(id) = crate::commands::behavior::resolve(scene, reference) {
        return Some(id);
    }
    scene
        .iter()
        .find(|(_, node)| {
            node.stable_key
                .as_deref()
                .is_some_and(|stable| stable.eq_ignore_ascii_case(reference))
        })
        .map(|(id, _)| id)
}

pub(crate) fn apply_plan_to_scene(
    plan: &PlannedWorld,
    scene: &mut SceneGraph,
    selection: &mut SceneSelectionState,
    viewport: &mut dyn GameViewportPort,
) -> Result<CommandOutput, String> {
    if plan.groups.is_empty()
        && plan.entities.is_empty()
        && plan.updates.is_empty()
        && plan.deletions.is_empty()
    {
        return Err("The world plan contains no groups, entities, updates or deletions.".to_string());
    }
    let original_scene = scene.clone();
    let original_selection = selection.clone();
    let original_viewport_selection = viewport.selected_ids();
    let ordered_groups = match order_build_groups(
        plan.groups.clone(),
        scene,
        plan.parent.as_ref(),
    ) {
        Ok(groups) => groups,
        Err(error) => return Err(error),
    };
    let mut chunks: Vec<(&'static str, Vec<Value>)> = Vec::new();
    for chunk in ordered_groups.chunks(PLAN_GROUP_CHUNK) {
        chunks.push(("groups", chunk.to_vec()));
    }
    for chunk in plan.entities.chunks(PLAN_ENTITY_CHUNK) {
        chunks.push(("entities", chunk.to_vec()));
    }
    let mut outputs = Vec::with_capacity(chunks.len());
    for (key, chunk) in &chunks {
        match run_plan_chunk(plan, scene, selection, viewport, key, chunk) {
            Ok(output) => outputs.push(output),
            Err(error) => {
                *scene = original_scene;
                *selection = original_selection;
                viewport.set_selected_ids(original_viewport_selection);
                return Err(error);
            }
        }
    }
    let mut changed = false;
    let mut updated = 0usize;
    let mut deleted = 0usize;
    let post_result = (|| -> Result<(), String> {
        let mut context = GameCommandContext {
            scene,
            selection,
            viewport,
        };
        for update in &plan.updates {
            let target = update
                .get("target")
                .and_then(Value::as_str)
                .ok_or_else(|| "Plan update requires target.".to_string())?;
            let Some(id) = resolve_plan_target(context.scene, target) else {
                return Err(format!("Plan update target '{target}' was not found."));
            };
            let uuid = context
                .scene
                .get(id)
                .ok_or_else(|| format!("Plan update target '{target}' disappeared."))?
                .uuid;
            let mut params = update.clone();
            if let Some(object) = params.as_object_mut() {
                object.insert("target".to_string(), Value::String(uuid.to_string()));
            }
            let command = parsed_command_from_params("game.update", params);
            let output = update_entity(&command, &mut context);
            if matches!(output.level, crate::commands::output::CommandLevel::Error) {
                return Err(format!("Plan update '{target}' failed: {}", output.lines.join(" ")));
            }
            updated += 1;
        }
        for deletion in &plan.deletions {
            let target = deletion
                .get("target")
                .and_then(Value::as_str)
                .ok_or_else(|| "Plan deletion requires target.".to_string())?;
            let optional = deletion
                .get("optional")
                .and_then(Value::as_bool)
                .unwrap_or(false);
            match resolve_plan_target(context.scene, target) {
                Some(id) => {
                    if !context.scene.remove_node(id) {
                        return Err(format!("Plan deletion target '{target}' could not be removed."));
                    }
                    deleted += 1;
                }
                None if optional => {}
                None => return Err(format!("Plan deletion target '{target}' was not found.")),
            }
        }
        Ok(())
    })();
    if let Err(error) = post_result {
        *scene = original_scene;
        *selection = original_selection;
        viewport.set_selected_ids(original_viewport_selection);
        return Err(error);
    }
    let mut created = 0usize;
    let mut created_ids = Vec::new();
    let mut lines = Vec::new();
    for output in &outputs {
        changed |= output.changed;
        if let Some(ids) = output.json.get("created_ids").and_then(Value::as_array) {
            created += ids.len();
            created_ids.extend(ids.iter().take(64).cloned());
        }
        if let Some(reconcile) = output.json.get("reconcile") {
            created += reconcile
                .get("created")
                .and_then(Value::as_u64)
                .unwrap_or(0) as usize;
            updated += reconcile
                .get("updated")
                .and_then(Value::as_u64)
                .unwrap_or(0) as usize;
        }
        lines.extend(output.lines.iter().take(2).cloned());
    }
    created_ids.truncate(64);
    let mut result = CommandOutput::info(
        "World plan applied",
        vec![
            format!("mode: {}", plan.mode),
            format!("chunks: {}", chunks.len()),
            format!("groups: {}", plan.groups.len()),
            format!("entities: {}", plan.entities.len()),
            format!("updates: {}", plan.updates.len()),
            format!("deletions: {}", plan.deletions.len()),
            format!("created: {created}"),
            format!("updated: {updated}"),
            format!("deleted: {deleted}"),
        ],
        serde_json::json!({
            "ok": true,
            "atomic": true,
            "mode": plan.mode,
            "chunks": chunks.len(),
            "group_chunk_size": PLAN_GROUP_CHUNK,
            "entity_chunk_size": PLAN_ENTITY_CHUNK,
            "groups": plan.groups.len(),
            "entities": plan.entities.len(),
            "updates": plan.updates.len(),
            "deletions": plan.deletions.len(),
            "created": created,
            "updated": updated,
            "deleted": deleted,
            "created_ids": created_ids,
            "chunk_lines": lines,
            "undo_scope": "plan",
        }),
    );
    result.changed = changed || updated > 0 || deleted > 0;
    Ok(result)
}

fn stage_plan(scene: &SceneGraph, plan: &PlannedWorld) -> Result<(CommandOutput, SceneGraph), String> {
    let mut staged = scene.clone();
    let mut selection = SceneSelectionState::default();
    let mut viewport = HeadlessGameViewportPort::default();
    let output = apply_plan_to_scene(plan, &mut staged, &mut selection, &mut viewport)?;
    Ok((output, staged))
}

fn plan_command_output(request_id: u64, plan: &PlannedWorld, stage: &CommandOutput) -> CommandOutput {
    let mut output = CommandOutput::info(
        "World plan ready",
        vec![
            format!("plan: {}", plan.id),
            format!("mode: {}", plan.mode),
            format!("groups: {}", plan.groups.len()),
            format!("entities: {}", plan.entities.len()),
            format!("updates: {}", plan.updates.len()),
            format!("deletions: {}", plan.deletions.len()),
            format!("hash: {}", plan.hash),
        ],
        serde_json::json!({
            "ok": true,
            "plan_id": plan.id,
            "plan_hash": plan.hash,
            "status": plan.status,
            "mode": plan.mode,
            "groups": plan.groups.len(),
            "entities": plan.entities.len(),
            "updates": plan.updates.len(),
            "deletions": plan.deletions.len(),
            "preview": {
                "title": stage.title,
                "chunks": stage.json.get("chunks").cloned(),
                "lines": stage.lines.iter().take(8).cloned().collect::<Vec<_>>(),
                "group_sample": plan.groups.iter().take(16).cloned().collect::<Vec<_>>(),
                "entity_sample": plan.entities.iter().take(16).cloned().collect::<Vec<_>>(),
            },
            "persisted_path": plan.persisted_path,
            "next": "Call game.commit with this plan_id, then verify and capture the result.",
            "request_id": request_id,
        }),
    );
    output.changed = false;
    output
}

pub(crate) fn execute_plan(request: &ParsedCommand, ctx: &mut GameCommandContext<'_>) -> CommandOutput {
    let result = (|| -> Result<CommandOutput, String> {
        let program = request
            .structured_arg("program")
            .cloned()
            .or_else(|| {
                request
                    .arg("program")
                    .and_then(|raw| serde_json::from_str::<Value>(raw).ok())
            })
            .or_else(|| {
                request
                    .structured_args
                    .clone()
                    .filter(|value| value.get("operations").is_some() || value.get("entities").is_some() || value.get("groups").is_some())
            })
            .ok_or_else(|| {
                "game.plan requires a program object with archetypes/groups/entities/operations.".to_string()
            })?;
        let compiled = compile_world_program(&program)?;
        let mode = program
            .get("mode")
            .or_else(|| request.structured_arg("mode"))
            .and_then(Value::as_str)
            .unwrap_or("build")
            .trim()
            .to_ascii_lowercase();
        if !matches!(mode.as_str(), "build" | "reconcile") {
            return Err("World plan mode must be build or reconcile.".to_string());
        }
        let parent = program
            .get("parent")
            .cloned()
            .or_else(|| request.structured_arg("parent").cloned())
            .filter(|value| !value.is_null());
        let design_profile = program
            .get("design_profile")
            .or_else(|| request.structured_arg("design_profile"))
            .and_then(Value::as_str)
            .map(str::to_string);
        let mut plan = PlannedWorld {
            id: format!("plan_{}", Uuid::new_v4().simple()),
            hash: sha256_hex(
                serde_json::to_string(&serde_json::json!({
                    "mode": mode,
                    "parent": parent,
                    "design_profile": design_profile,
                    "groups": compiled.groups,
                    "entities": compiled.entities,
                    "updates": compiled.updates,
                    "deletions": compiled.deletions,
                }))
                .unwrap_or_default()
                .as_bytes(),
            ),
            status: "planned".to_string(),
            mode,
            parent,
            design_profile,
            groups: compiled.groups,
            entities: compiled.entities,
            updates: compiled.updates,
            deletions: compiled.deletions,
            fingerprint: raf_core::agent_context::scene_fingerprint(ctx.scene),
            persisted_path: None,
            project_root: active_project_root().map(|root| {
                root.to_string_lossy().replace('\\', "/")
            }),
        };
        let (stage, staged_scene) = stage_plan(ctx.scene, &plan)?;
        let overlaps = raf_core::agent_context::scene_check_overlaps(
            &staged_scene,
            &serde_json::json!({"max_pairs": 32}),
            &[],
        );
        let overlap_data = overlaps.data;
        let overlap_count = overlap_data
            .get("overlap_count")
            .and_then(Value::as_u64)
            .unwrap_or(0);
        let design_audit = plan
            .design_profile
            .as_deref()
            .map(|profile| {
                raf_core::agent_context::scene_design_audit(
                    &staged_scene,
                    &serde_json::json!({"design_profile": profile, "root": Value::Null}),
                    &[],
                )
            });
        let navigation = raf_core::agent_context::scene_navigation_audit(
            &staged_scene,
            program.get("navigation").unwrap_or(&Value::Null),
            &[],
        );
        let pipeline = world_pipeline(
            &program,
            &plan,
            &stage,
            &overlap_data,
            overlap_count,
            design_audit.as_ref(),
            &navigation,
        );
        if let Err(error) = persist_plan(&mut plan) {
            return Err(error);
        }
        if let Ok(mut cache) = plan_cache().lock() {
            cache.insert(plan.id.clone(), plan.clone());
        }
        let mut output = plan_command_output(0, &plan, &stage);
        output.json["overlaps"] = overlap_data;
        output.json["overlap_count"] = Value::Number(overlap_count.into());
        if let Some(audit) = design_audit {
            output.json["design_audit"] = serde_json::json!({
                "status": if audit.is_success() { "passed" } else { "failed" },
                "summary": audit.summary,
                "warnings": audit.warnings,
            });
        }
        output.json["navigation"] = navigation.data.clone();
        output.json["pipeline"] = pipeline;
        let mut warnings = overlaps.warnings;
        warnings.extend(navigation.warnings.iter().cloned());
        if overlap_count > 0 {
            warnings.push(format!(
                "The plan contains {overlap_count} conservative AABB overlap(s); inspect the preview before commit."
            ));
        }
        if !warnings.is_empty() {
            output.lines.extend(warnings.iter().cloned());
            output.json["warnings"] = serde_json::json!(warnings);
        }
        Ok(output)
    })();
    result.unwrap_or_else(|error| CommandOutput::error("World plan", error))
}

fn world_pipeline(
    program: &Value,
    plan: &PlannedWorld,
    stage: &CommandOutput,
    overlap_data: &Value,
    overlap_count: u64,
    design_audit: Option<&raf_core::agent_context::AgentObservationResult>,
    navigation: &raf_core::agent_context::AgentObservationResult,
) -> Value {
    let mut gates = Vec::new();
    gates.push(serde_json::json!({
        "gate": "compile",
        "status": "passed",
        "detail": {"groups": plan.groups.len(), "entities": plan.entities.len()},
    }));
    gates.push(serde_json::json!({
        "gate": "simulate",
        "status": "passed",
        "detail": {
            "chunks": stage.json.get("chunks").cloned().unwrap_or(Value::Null),
            "ready": stage.json.get("ok").and_then(Value::as_bool).unwrap_or(true),
        },
    }));
    let budget = program.get("budget").and_then(Value::as_object);
    let max_entities = budget
        .and_then(|budget| budget.get("entities"))
        .and_then(Value::as_u64);
    let max_ops = budget
        .and_then(|budget| budget.get("ops"))
        .and_then(Value::as_u64);
    let operation_count = program
        .get("operations")
        .and_then(Value::as_array)
        .map(|operations| operations.len() as u64)
        .unwrap_or(0);
    let entity_count = plan.entities.len() as u64;
    let budget_status = match (max_entities, max_ops) {
        (None, None) => "not_declared",
        _ => {
            if max_entities.is_some_and(|max| entity_count > max)
                || max_ops.is_some_and(|max| operation_count > max)
            {
                "failed"
            } else {
                "passed"
            }
        }
    };
    gates.push(serde_json::json!({
        "gate": "budget",
        "status": budget_status,
        "detail": {
            "entities": entity_count,
            "max_entities": max_entities,
            "operations": operation_count,
            "max_ops": max_ops,
        },
    }));
    gates.push(serde_json::json!({
        "gate": "overlaps",
        "status": if overlap_count == 0 { "passed" } else { "warning" },
        "detail": {
            "overlap_count": overlap_count,
            "pairs": overlap_data.get("pairs").cloned().unwrap_or(Value::Null),
        },
    }));
    let design_status = match design_audit {
        None => "not_run",
        Some(audit) if audit.is_success() => "passed",
        Some(_) => "failed",
    };
    gates.push(serde_json::json!({
        "gate": "design",
        "status": design_status,
        "detail": {
            "profile": plan.design_profile.clone(),
            "summary": design_audit.map(|audit| audit.summary.clone()),
        },
    }));
    let navigation_status = navigation
        .data
        .get("status")
        .and_then(Value::as_str)
        .unwrap_or("not_checked");
    let navigation_gate = match navigation_status {
        "passed" => "passed",
        "failed" => "failed",
        _ => "warning",
    };
    gates.push(serde_json::json!({
        "gate": "navigation",
        "status": navigation_gate,
        "detail": {
            "status": navigation_status,
            "reachable_goals": navigation.data.get("reachable_goals").cloned().unwrap_or(Value::Null),
            "goals": navigation.data.get("goals").cloned().unwrap_or(Value::Null),
        },
    }));
    let mut assertion_failures = Vec::new();
    let mut assertions_declared = false;
    let reachable_goals = navigation
        .data
        .get("reachable_goals")
        .and_then(Value::as_u64)
        .unwrap_or(0);
    if let Some(operations) = program.get("operations").and_then(Value::as_array) {
        for operation in operations {
            if operation.get("op").and_then(Value::as_str) != Some("assert") {
                continue;
            }
            if let Some(no_overlaps) = operation.get("no_overlaps").and_then(Value::as_bool) {
                assertions_declared = true;
                if no_overlaps && overlap_count > 0 {
                    assertion_failures.push(format!(
                        "no_overlaps was requested but the staged plan has {overlap_count} overlap(s)."
                    ));
                }
            }
            if let Some(max_overlaps) = operation.get("max_overlaps").and_then(Value::as_u64) {
                assertions_declared = true;
                if overlap_count > max_overlaps {
                    assertion_failures.push(format!(
                        "max_overlaps={max_overlaps} but the staged plan has {overlap_count} overlap(s)."
                    ));
                }
            }
            if let Some(min_reachable) = operation
                .get("min_reachable_goals")
                .and_then(Value::as_u64)
            {
                assertions_declared = true;
                if reachable_goals < min_reachable {
                    assertion_failures.push(format!(
                        "min_reachable_goals={min_reachable} but only {reachable_goals} goal(s) are reachable."
                    ));
                }
            }
        }
    }
    gates.push(serde_json::json!({
        "gate": "assertions",
        "status": if !assertions_declared {
            "not_declared"
        } else if assertion_failures.is_empty() {
            "passed"
        } else {
            "failed"
        },
        "detail": {
            "failures": assertion_failures,
            "reachable_goals": reachable_goals,
            "overlap_count": overlap_count,
        },
    }));
    let failed = gates
        .iter()
        .filter(|gate| gate.get("status").and_then(Value::as_str) == Some("failed"))
        .count();
    let warnings = gates
        .iter()
        .filter(|gate| gate.get("status").and_then(Value::as_str) == Some("warning"))
        .count();
    serde_json::json!({
        "status": if failed > 0 { "failed" } else if warnings > 0 { "warning" } else { "passed" },
        "failed": failed,
        "warnings": warnings,
        "gates": gates,
    })
}

pub(crate) fn execute_cancel(request: &ParsedCommand) -> CommandOutput {
    let result = (|| -> Result<CommandOutput, String> {
        let plan_id = text_argument(request, "plan_id")
            .or_else(|| text_argument(request, "id"))
            .ok_or_else(|| "game.cancel requires plan_id.".to_string())?;
        if !plan_id
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || matches!(character, '_' | '-'))
        {
            return Err("plan_id contains unsupported characters.".to_string());
        }
        let mut plan = load_planned_world(&plan_id)?;
        if plan.status == "committed" {
            return Err(format!(
                "Plan '{plan_id}' is already committed and cannot be cancelled. Use Undo instead."
            ));
        }
        if plan.status == "cancelled" {
            return Ok(CommandOutput::info(
                "World plan already cancelled",
                vec![format!("plan: {plan_id}")],
                serde_json::json!({"ok": true, "plan_id": plan_id, "cancelled": true, "replayed": true}),
            ));
        }
        plan.status = "cancelled".to_string();
        update_persisted_plan(&plan);
        if let Ok(mut cache) = plan_cache().lock() {
            cache.insert(plan.id.clone(), plan.clone());
        }
        if let Some(root) = active_project_root() {
            append_journal(
                &root,
                serde_json::json!({"event": "plan_cancelled", "plan_id": plan.id}),
            );
        }
        Ok(CommandOutput::info(
            "World plan cancelled",
            vec![format!("plan: {plan_id}")],
            serde_json::json!({
                "ok": true,
                "plan_id": plan.id,
                "plan_hash": plan.hash,
                "cancelled": true,
                "changed": false,
            }),
        ))
    })();
    result.unwrap_or_else(|error| CommandOutput::error("World cancel", error))
}

pub(crate) fn execute_commit(request: &ParsedCommand, ctx: &mut GameCommandContext<'_>) -> CommandOutput {
    let result = (|| -> Result<CommandOutput, String> {
        let plan_id = text_argument(request, "plan_id")
            .or_else(|| text_argument(request, "id"))
            .ok_or_else(|| "game.commit requires plan_id.".to_string())?;
        if !plan_id
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || matches!(character, '_' | '-'))
        {
            return Err("plan_id contains unsupported characters.".to_string());
        }
        let mut plan = load_planned_world(&plan_id)?;
        if plan.status == "cancelled" {
            return Err(format!(
                "Plan '{plan_id}' was cancelled. Compile a fresh plan before committing."
            ));
        }
        if let (Some(plan_root), Some(current_root)) =
            (plan.project_root.as_deref(), active_project_root())
        {
            let current_root = current_root.to_string_lossy().replace('\\', "/");
            if plan_root != current_root {
                return Err(format!(
                    "Plan '{plan_id}' belongs to a different project. Compile a new plan in the active project."
                ));
            }
        }
        if plan.status == "committed" {
            return Ok(CommandOutput::info(
                "World plan already committed",
                vec![format!("plan: {plan_id}")],
                serde_json::json!({
                    "ok": true,
                    "plan_id": plan_id,
                    "plan_hash": plan.hash,
                    "replayed": true,
                    "changed": false,
                }),
            ));
        }
        let current = raf_core::agent_context::scene_fingerprint(ctx.scene);
        if plan.fingerprint != current {
            return Err(format!(
                "The scene changed after plan '{plan_id}' was created. Create a fresh plan against revision evidence before committing."
            ));
        }
        let mut output = apply_plan_to_scene(&plan, ctx.scene, ctx.selection, ctx.viewport)?;
        plan.status = "committed".to_string();
        if let Ok(mut cache) = plan_cache().lock() {
            cache.insert(plan.id.clone(), plan.clone());
        }
        update_persisted_plan(&plan);
        output.title = "World plan committed".to_string();
        output.lines.insert(0, format!("plan: {plan_id}"));
        if let Some(object) = output.json.as_object_mut() {
            object.insert("plan_id".to_string(), Value::String(plan_id.clone()));
            object.insert(
                "world_plan".to_string(),
                serde_json::json!({
                    "id": plan.id,
                    "hash": plan.hash,
                    "status": plan.status,
                    "mode": plan.mode,
                    "groups": plan.groups.len(),
                    "entities": plan.entities.len(),
                    "persisted_path": plan.persisted_path,
                    "undo_scope": "plan",
                }),
            );
        }
        Ok(output)
    })();
    result.unwrap_or_else(|error| CommandOutput::error("World commit", error))
}

pub(crate) fn execute_world_brief(request: &ParsedCommand) -> CommandOutput {
    let result = (|| -> Result<CommandOutput, String> {
        let Some(root) = active_project_root() else {
            return Err(
                "The world brief requires an active project root. Open the project first.".to_string(),
            );
        };
        let path = brief_path(&root);
        let action = text_argument(request, "action")
            .or_else(|| {
                request
                    .name
                    .strip_suffix(".write")
                    .map(|_| "write".to_string())
            })
            .or_else(|| {
                request
                    .name
                    .strip_suffix(".read")
                    .map(|_| "read".to_string())
            })
            .unwrap_or_else(|| "read".to_string())
            .to_ascii_lowercase();
        match action.as_str() {
            "read" | "get" | "show" => {
                let content = std::fs::read(&path)
                    .ok()
                    .and_then(|bytes| String::from_utf8(bytes).ok())
                    .unwrap_or_default();
                let brief = if content.trim().is_empty() {
                    Value::Null
                } else {
                    serde_json::from_str(&content).unwrap_or_else(|_| Value::String(content.clone()))
                };
                Ok(CommandOutput::info(
                    "World brief",
                    vec![format!(
                        "brief: {}",
                        if content.is_empty() { "not set" } else { "loaded" }
                    )],
                    serde_json::json!({
                        "ok": true,
                        "found": !content.is_empty(),
                        "path": path.strip_prefix(&root).unwrap_or(&path).to_string_lossy().replace('\\', "/"),
                        "brief": brief,
                    }),
                ))
            }
            "write" | "set" | "save" => {
                let brief = request
                    .structured_arg("brief")
                    .cloned()
                    .or_else(|| request.structured_arg("content").cloned())
                    .or_else(|| {
                        request
                            .arg("brief")
                            .and_then(|raw| serde_json::from_str::<Value>(raw).ok())
                    })
                    .or_else(|| request.arg("content").map(|raw| Value::String(raw.to_string())))
                    .ok_or_else(|| "world.brief write requires a brief object or content string.".to_string())?;
                if brief.to_string().len() > 64 * 1024 {
                    return Err("World brief must be at most 64 KiB.".to_string());
                }
                if let Some(parent) = path.parent() {
                    std::fs::create_dir_all(parent)
                        .map_err(|error| format!("Could not create the world directory: {error}"))?;
                }
                let encoded = serde_json::to_vec_pretty(&brief)
                    .map_err(|error| format!("Could not encode the world brief: {error}"))?;
                std::fs::write(&path, encoded)
                    .map_err(|error| format!("Could not write the world brief: {error}"))?;
                append_journal(
                    &root,
                    serde_json::json!({"event": "brief_updated", "path": "world/brief.json"}),
                );
                Ok(CommandOutput::changed(
                    "World brief saved",
                    vec![format!(
                        "path: {}",
                        path.strip_prefix(&root).unwrap_or(&path).to_string_lossy().replace('\\', "/")
                    )],
                    serde_json::json!({
                        "ok": true,
                        "saved": true,
                        "path": path.strip_prefix(&root).unwrap_or(&path).to_string_lossy().replace('\\', "/"),
                        "brief": brief,
                    }),
                ))
            }
            other => Err(format!(
                "Unknown world.brief action '{other}'. Use read or write."
            )),
        }
    })();
    result.unwrap_or_else(|error| CommandOutput::error("World brief", error))
}

// ---------------------------------------------------------------------------
// Reusable recipes captured from committed plans
// ---------------------------------------------------------------------------

fn recipes_directory(root: &Path) -> PathBuf {
    world_directory(root).join("recipes")
}

fn recipe_slug(value: &str) -> Result<String, String> {
    let slug = slugify(value);
    if slug.is_empty() {
        return Err("Recipe name must contain at least one letter or digit.".to_string());
    }
    if slug.len() > 64 {
        return Err("Recipe name must be at most 64 characters.".to_string());
    }
    Ok(slug)
}

fn load_recipe(root: &Path, name: &str) -> Result<(PathBuf, Value), String> {
    let slug = recipe_slug(name)?;
    let path = recipes_directory(root).join(format!("{slug}.json"));
    let bytes = std::fs::read(&path).map_err(|error| {
        format!("Recipe '{slug}' was not found in the project: {error}")
    })?;
    let recipe = serde_json::from_slice::<Value>(&bytes)
        .map_err(|error| format!("Recipe '{slug}' is not valid JSON: {error}"))?;
    if !recipe.is_object() {
        return Err(format!("Recipe '{slug}' must be a JSON object."));
    }
    Ok((path, recipe))
}

fn recipe_summary(slug: &str, recipe: &Value) -> Value {
    serde_json::json!({
        "name": slug,
        "title": recipe.get("name").cloned().unwrap_or(Value::String(slug.to_string())),
        "description": recipe.get("description").cloned().unwrap_or(Value::Null),
        "source_plan_id": recipe.get("source_plan_id").cloned().unwrap_or(Value::Null),
        "mode": recipe.get("mode").cloned().unwrap_or(Value::String("build".to_string())),
        "groups": recipe.get("groups").and_then(Value::as_array).map(Vec::len).unwrap_or(0),
        "entities": recipe.get("entities").and_then(Value::as_array).map(Vec::len).unwrap_or(0),
        "tags": recipe.get("tags").cloned().unwrap_or(Value::Array(Vec::new())),
    })
}

pub(crate) fn execute_world_recipe(
    request: &ParsedCommand,
    ctx: &mut GameCommandContext<'_>,
) -> CommandOutput {
    let result = (|| -> Result<CommandOutput, String> {
        let root = active_project_root()
            .ok_or_else(|| "World recipes require an active project root.".to_string())?;
        let action = text_argument(request, "action")
            .unwrap_or_else(|| "list".to_string())
            .to_ascii_lowercase();
        match action.as_str() {
            "save" | "capture" => {
                let name = text_argument(request, "name")
                    .or_else(|| text_argument(request, "recipe"))
                    .ok_or_else(|| "world.recipe save requires name.".to_string())?;
                let slug = recipe_slug(&name)?;
                let plan_id = text_argument(request, "plan_id")
                    .ok_or_else(|| "world.recipe save requires plan_id.".to_string())?;
                let plan = load_planned_world(&plan_id)?;
                let source_plan_id = plan.id.clone();
                let recipe = serde_json::json!({
                    "version": 1,
                    "name": name,
                    "slug": slug,
                    "description": text_argument(request, "description").unwrap_or_default(),
                    "source_plan_id": source_plan_id,
                    "mode": plan.mode,
                    "parent": plan.parent,
                    "design_profile": plan.design_profile,
                    "groups": plan.groups,
                    "entities": plan.entities,
                    "updates": plan.updates,
                    "deletions": plan.deletions,
                    "tags": request.structured_arg("tags").cloned().unwrap_or_else(|| Value::Array(Vec::new())),
                });
                let encoded = serde_json::to_vec_pretty(&recipe)
                    .map_err(|error| format!("Could not encode recipe '{slug}': {error}"))?;
                if encoded.len() > 8 * 1024 * 1024 {
                    return Err("Recipe exceeds the 8 MiB project budget.".to_string());
                }
                let directory = recipes_directory(&root);
                std::fs::create_dir_all(&directory)
                    .map_err(|error| format!("Could not create the recipe directory: {error}"))?;
                let path = directory.join(format!("{slug}.json"));
                std::fs::write(&path, encoded)
                    .map_err(|error| format!("Could not write recipe '{slug}': {error}"))?;
                append_journal(
                    &root,
                    serde_json::json!({"event": "recipe_saved", "name": slug, "source_plan_id": source_plan_id}),
                );
                Ok(CommandOutput::changed(
                    "World recipe saved",
                    vec![
                        format!("recipe: {slug}"),
                        format!("source plan: {source_plan_id}"),
                    ],
                    serde_json::json!({
                        "ok": true,
                        "saved": true,
                        "recipe": recipe_summary(&slug, &recipe),
                        "path": path.strip_prefix(&root).unwrap_or(&path).to_string_lossy().replace('\\', "/"),
                    }),
                ))
            }
            "list" => {
                let directory = recipes_directory(&root);
                let mut recipes = Vec::new();
                if let Ok(entries) = std::fs::read_dir(&directory) {
                    for entry in entries.flatten() {
                        let path = entry.path();
                        if path.extension().and_then(|extension| extension.to_str()) != Some("json") {
                            continue;
                        }
                        let Some(slug) = path.file_stem().and_then(|stem| stem.to_str()) else {
                            continue;
                        };
                        let Ok(bytes) = std::fs::read(&path) else {
                            continue;
                        };
                        let Ok(recipe) = serde_json::from_slice::<Value>(&bytes) else {
                            continue;
                        };
                        recipes.push(recipe_summary(slug, &recipe));
                    }
                }
                recipes.sort_by(|left, right| {
                    left.get("name")
                        .and_then(Value::as_str)
                        .cmp(&right.get("name").and_then(Value::as_str))
                });
                recipes.truncate(128);
                let count = recipes.len();
                Ok(CommandOutput::info(
                    "World recipes",
                    vec![format!("recipes: {count}")],
                    serde_json::json!({
                        "ok": true,
                        "count": count,
                        "recipes": recipes,
                    }),
                ))
            }
            "show" | "read" => {
                let name = text_argument(request, "name")
                    .or_else(|| text_argument(request, "recipe"))
                    .ok_or_else(|| "world.recipe show requires name.".to_string())?;
                let (path, recipe) = load_recipe(&root, &name)?;
                let slug = recipe_slug(&name)?;
                Ok(CommandOutput::info(
                    "World recipe",
                    vec![format!("recipe: {slug}")],
                    serde_json::json!({
                        "ok": true,
                        "recipe": recipe,
                        "path": path.strip_prefix(&root).unwrap_or(&path).to_string_lossy().replace('\\', "/"),
                    }),
                ))
            }
            "delete" | "remove" => {
                let name = text_argument(request, "name")
                    .or_else(|| text_argument(request, "recipe"))
                    .ok_or_else(|| "world.recipe delete requires name.".to_string())?;
                let slug = recipe_slug(&name)?;
                let path = recipes_directory(&root).join(format!("{slug}.json"));
                let existed = path.exists();
                if existed {
                    std::fs::remove_file(&path)
                        .map_err(|error| format!("Could not remove recipe '{slug}': {error}"))?;
                    append_journal(
                        &root,
                        serde_json::json!({"event": "recipe_deleted", "name": slug}),
                    );
                }
                Ok(CommandOutput {
                    changed: existed,
                    ..CommandOutput::info(
                        "World recipe deleted",
                        vec![format!("recipe: {slug}")],
                        serde_json::json!({
                            "ok": true,
                            "existed": existed,
                            "name": slug,
                        }),
                    )
                })
            }
            "apply" | "instantiate" => {
                let name = text_argument(request, "name")
                    .or_else(|| text_argument(request, "recipe"))
                    .ok_or_else(|| "world.recipe apply requires name.".to_string())?;
                let slug = recipe_slug(&name)?;
                let (_path, recipe) = load_recipe(&root, &slug)?;
                let program = serde_json::json!({
                    "version": 1,
                    "mode": recipe.get("mode").cloned().unwrap_or_else(|| Value::String("build".to_string())),
                    "parent": recipe.get("parent").cloned().unwrap_or(Value::Null),
                    "design_profile": recipe.get("design_profile").cloned().unwrap_or(Value::Null),
                    "groups": recipe.get("groups").cloned().unwrap_or_else(|| Value::Array(Vec::new())),
                    "entities": recipe.get("entities").cloned().unwrap_or_else(|| Value::Array(Vec::new())),
                    "updates": recipe.get("updates").cloned().unwrap_or_else(|| Value::Array(Vec::new())),
                    "deletions": recipe.get("deletions").cloned().unwrap_or_else(|| Value::Array(Vec::new())),
                });
                let plan_request = parsed_command_from_params(
                    "game.plan",
                    serde_json::json!({"program": program}),
                );
                let mut output = execute_plan(&plan_request, ctx);
                if matches!(
                    output.level,
                    crate::commands::output::CommandLevel::Error
                ) {
                    return Ok(output);
                }
                let plan_id = output.json.get("plan_id").cloned().unwrap_or(Value::Null);
                output.lines.insert(0, format!("recipe: {slug}"));
                if let Some(object) = output.json.as_object_mut() {
                    object.insert(
                        "recipe".to_string(),
                        serde_json::json!({
                            "name": slug,
                            "source_plan_id": recipe.get("source_plan_id").cloned().unwrap_or(Value::Null),
                        }),
                    );
                }
                append_journal(
                    &root,
                    serde_json::json!({"event": "recipe_applied", "name": slug, "plan_id": plan_id}),
                );
                Ok(output)
            }
            other => Err(format!(
                "Unknown world.recipe action '{other}'. Use save, list, show, delete or apply."
            )),
        }
    })();
    result.unwrap_or_else(|error| CommandOutput::error("World recipe", error))
}

// ---------------------------------------------------------------------------
// Sandboxed authoring script
// ---------------------------------------------------------------------------

#[derive(Default)]
struct ScriptWorld {
    groups: Vec<Value>,
    operations: Vec<Value>,
    parent: Option<String>,
}

impl ScriptWorld {
    fn push(&mut self, mut operation: Value) {
        if let Some(parent) = self.parent.as_deref() {
            if operation.get("parent").is_none() {
                operation["parent"] = Value::String(parent.to_string());
            }
        }
        self.operations.push(operation);
    }

    fn program(&self) -> Value {
        serde_json::json!({
            "version": 1,
            "groups": self.groups,
            "operations": self.operations,
        })
    }
}

fn script_error(message: impl Into<String>) -> Box<rhai::EvalAltResult> {
    message.into().into()
}

fn world_script_program(source: &str, scene: &SceneGraph) -> Result<(Value, usize), String> {
    if source.trim().is_empty() {
        return Err("world.script requires a non-empty Rhai script.".to_string());
    }
    if source.len() > 64 * 1024 {
        return Err("Authoring scripts are limited to 64 KiB.".to_string());
    }
    let mut engine = rhai::Engine::new_raw();
    engine.set_max_operations(200_000);
    engine.set_max_call_levels(32);
    engine.set_max_variables(256);
    engine.set_max_functions(128);
    engine.set_max_string_size(64 * 1024);
    engine.set_max_array_size(8192);
    engine.set_max_map_size(1024);
    engine.set_max_expr_depths(32, 32);
    engine.disable_symbol("eval");
    let state = std::sync::Arc::new(Mutex::new(ScriptWorld::default()));
    let scene_text = std::sync::Arc::new(
        scene
            .iter()
            .map(|(_, node)| {
                let mut text = node.name.to_ascii_lowercase();
                if let Some(role) = node.semantic_role.as_deref() {
                    text.push(' ');
                    text.push_str(&role.to_ascii_lowercase());
                }
                if let Some(key) = node.stable_key.as_deref() {
                    text.push(' ');
                    text.push_str(&key.to_ascii_lowercase());
                }
                for tag in &node.tags {
                    text.push(' ');
                    text.push_str(&tag.to_ascii_lowercase());
                }
                text
            })
            .collect::<Vec<_>>(),
    );

    let mut world_module = rhai::Module::new();
    let texts = scene_text.clone();
    world_module.set_native_fn(
        "count",
        move |query: rhai::ImmutableString| -> Result<i64, Box<rhai::EvalAltResult>> {
            let query = query.trim().to_ascii_lowercase();
            if query.is_empty() {
                return Ok(0);
            }
            Ok(texts
                .iter()
                .filter(|text| text.contains(&query))
                .count() as i64)
        },
    );
    let world = state.clone();
    world_module.set_native_fn(
        "group",
        move |name: rhai::ImmutableString| -> Result<(), Box<rhai::EvalAltResult>> {
            let mut world = world
                .lock()
                .map_err(|_| script_error("Authoring script state was poisoned."))?;
            if world.groups.len() >= 512 {
                return Err(script_error("The plan already has 512 groups."));
            }
            world.groups.push(serde_json::json!({"name": name.to_string()}));
            Ok(())
        },
    );
    let world = state.clone();
    world_module.set_native_fn(
        "parent",
        move |name: rhai::ImmutableString| -> Result<(), Box<rhai::EvalAltResult>> {
            let mut world = world
                .lock()
                .map_err(|_| script_error("Authoring script state was poisoned."))?;
            world.parent = Some(name.to_string());
            Ok(())
        },
    );
    let world = state.clone();
    world_module.set_native_fn("parent_clear", move || -> Result<(), Box<rhai::EvalAltResult>> {
        let mut world = world
            .lock()
            .map_err(|_| script_error("Authoring script state was poisoned."))?;
        world.parent = None;
        Ok(())
    });
    let world = state.clone();
    world_module.set_native_fn(
        "camera",
        move |name: rhai::ImmutableString, x: f64, y: f64, z: f64, fov: f64| -> Result<(), Box<rhai::EvalAltResult>> {
            if !fov.is_finite() || !(1.0..=179.0).contains(&fov) {
                return Err(script_error("world::camera fov must be between 1 and 179."));
            }
            world.lock().map_err(|_| script_error("Authoring script state was poisoned."))?.push(serde_json::json!({
                "op": "camera",
                "name": name.to_string(),
                "position": [x, y, z],
                "fov": fov,
            }));
            Ok(())
        },
    );
    let world = state.clone();
    world_module.set_native_fn(
        "script",
        move |target: rhai::ImmutableString, file: rhai::ImmutableString| -> Result<(), Box<rhai::EvalAltResult>> {
            if file.trim().is_empty() {
                return Err(script_error("world::script requires a script file."));
            }
            world.lock().map_err(|_| script_error("Authoring script state was poisoned."))?.push(serde_json::json!({
                "op": "script",
                "target": target.to_string(),
                "file": file.to_string(),
            }));
            Ok(())
        },
    );
    let world = state.clone();
    world_module.set_native_fn(
        "place",
        move |archetype: rhai::ImmutableString, x: f64, y: f64, z: f64| -> Result<(), Box<rhai::EvalAltResult>> {
            world.lock().map_err(|_| script_error("Authoring script state was poisoned."))?.push(serde_json::json!({
                "op": "place",
                "archetype": archetype.to_string(),
                "position": [x, y, z],
            }));
            Ok(())
        },
    );
    let world = state.clone();
    world_module.set_native_fn(
        "place_named",
        move |archetype: rhai::ImmutableString, name: rhai::ImmutableString, x: f64, y: f64, z: f64| -> Result<(), Box<rhai::EvalAltResult>> {
            world.lock().map_err(|_| script_error("Authoring script state was poisoned."))?.push(serde_json::json!({
                "op": "place",
                "archetype": archetype.to_string(),
                "name": name.to_string(),
                "position": [x, y, z],
            }));
            Ok(())
        },
    );
    let world = state.clone();
    world_module.set_native_fn(
        "grid",
        move |archetype: rhai::ImmutableString, count_x: i64, count_z: i64, spacing_x: f64, spacing_z: f64| -> Result<(), Box<rhai::EvalAltResult>> {
            push_script_grid(&world, archetype.as_str(), count_x, count_z, spacing_x, spacing_z, 0.0, 0.0)
        },
    );
    let world = state.clone();
    world_module.set_native_fn(
        "grid_at",
        move |archetype: rhai::ImmutableString, count_x: i64, count_z: i64, spacing_x: f64, spacing_z: f64, origin_x: f64, origin_z: f64| -> Result<(), Box<rhai::EvalAltResult>> {
            push_script_grid(&world, archetype.as_str(), count_x, count_z, spacing_x, spacing_z, origin_x, origin_z)
        },
    );
    let world = state.clone();
    world_module.set_native_fn(
        "line",
        move |archetype: rhai::ImmutableString, x1: f64, z1: f64, x2: f64, z2: f64, spacing: f64| -> Result<(), Box<rhai::EvalAltResult>> {
            if !spacing.is_finite() || spacing <= 0.0 {
                return Err(script_error("world::line spacing must be greater than zero."));
            }
            world.lock().map_err(|_| script_error("Authoring script state was poisoned."))?.push(serde_json::json!({
                "op": "along",
                "archetype": archetype.to_string(),
                "points": [[x1, 0.0, z1], [x2, 0.0, z2]],
                "spacing": spacing,
            }));
            Ok(())
        },
    );
    let world = state.clone();
    world_module.set_native_fn(
        "scatter",
        move |archetype: rhai::ImmutableString, count: i64, min_x: f64, min_z: f64, max_x: f64, max_z: f64, seed: i64| -> Result<(), Box<rhai::EvalAltResult>> {
            if count <= 0 || count > 4096 {
                return Err(script_error("world::scatter count must be between 1 and 4096."));
            }
            if min_x > max_x || min_z > max_z {
                return Err(script_error("world::scatter min must be <= max on both axes."));
            }
            world.lock().map_err(|_| script_error("Authoring script state was poisoned."))?.push(serde_json::json!({
                "op": "scatter",
                "archetype": archetype.to_string(),
                "count": count,
                "region": {"min": [min_x, 0.0, min_z], "max": [max_x, 0.0, max_z]},
                "seed": seed.max(1),
            }));
            Ok(())
        },
    );
    let world = state.clone();
    world_module.set_native_fn(
        "assert_max_entities",
        move |max: i64| -> Result<(), Box<rhai::EvalAltResult>> {
            if max < 0 {
                return Err(script_error("world::assert_max_entities requires a positive budget."));
            }
            let mut world = world
                .lock()
                .map_err(|_| script_error("Authoring script state was poisoned."))?;
            world.push(serde_json::json!({"op": "assert", "max_entities": max}));
            Ok(())
        },
    );
    let world = state.clone();
    world_module.set_native_fn(
        "emit",
        move || -> Result<String, Box<rhai::EvalAltResult>> {
            let world = world
                .lock()
                .map_err(|_| script_error("Authoring script state was poisoned."))?;
            serde_json::to_string(&world.program())
                .map_err(|error| script_error(format!("Could not emit the plan: {error}")))
        },
    );

    engine.register_static_module("world", world_module.into());

    engine
        .run(source)
        .map_err(|error| format!("Authoring script failed: {error}"))?;
    let program = {
        let state = state
            .lock()
            .map_err(|_| "Authoring script state was poisoned.".to_string())?;
        state.program()
    };
    let operation_count = program
        .get("operations")
        .and_then(Value::as_array)
        .map(Vec::len)
        .unwrap_or(0);
    if operation_count == 0 {
        return Err(
            "The authoring script did not emit any operation. Call world::group, world::place, world::grid, world::line or world::scatter."
                .to_string(),
        );
    }
    compile_world_program(&program)?;
    Ok((program, operation_count))
}

fn push_script_grid(
    world: &std::sync::Arc<Mutex<ScriptWorld>>,
    archetype: &str,
    count_x: i64,
    count_z: i64,
    spacing_x: f64,
    spacing_z: f64,
    origin_x: f64,
    origin_z: f64,
) -> Result<(), Box<rhai::EvalAltResult>> {
    if count_x <= 0 || count_z <= 0 || count_x.saturating_mul(count_z) > 4096 {
        return Err(script_error(
            "world::grid requires positive counts with at most 4096 instances.",
        ));
    }
    if !spacing_x.is_finite() || !spacing_z.is_finite() {
        return Err(script_error("world::grid spacing must be finite."));
    }
    world.lock().map_err(|_| script_error("Authoring script state was poisoned."))?.push(serde_json::json!({
        "op": "grid",
        "archetype": archetype,
        "count": [count_x, 1, count_z],
        "spacing": [spacing_x, 0.0, spacing_z],
        "origin": [origin_x, 0.0, origin_z],
    }));
    Ok(())
}

fn read_authoring_script(root: &Path, relative: &str) -> Result<String, String> {
    let root = root
        .canonicalize()
        .map_err(|error| format!("Could not resolve the project root: {error}"))?;
    let candidate = Path::new(relative);
    let candidate = if candidate.is_absolute() {
        candidate.to_path_buf()
    } else {
        root.join(candidate)
    };
    let candidate = candidate
        .canonicalize()
        .map_err(|error| format!("Could not resolve script file '{relative}': {error}"))?;
    if !candidate.starts_with(&root) {
        return Err("Authoring scripts must live inside the active project.".to_string());
    }
    let bytes = std::fs::read(&candidate)
        .map_err(|error| format!("Could not read script file '{relative}': {error}"))?;
    if bytes.len() > 64 * 1024 {
        return Err("Authoring scripts are limited to 64 KiB.".to_string());
    }
    String::from_utf8(bytes)
        .map_err(|error| format!("Script file '{relative}' must be UTF-8: {error}"))
}

pub(crate) fn execute_world_script(
    request: &ParsedCommand,
    ctx: &mut GameCommandContext<'_>,
) -> CommandOutput {
    let result = (|| -> Result<CommandOutput, String> {
        let root = active_project_root()
            .ok_or_else(|| "World scripts require an active project root.".to_string())?;
        let (source, origin) = if let Some(source) = request
            .structured_arg("script")
            .and_then(Value::as_str)
            .map(str::to_string)
            .or_else(|| request.arg("script").map(str::to_string))
        {
            (source, "inline".to_string())
        } else if let Some(file) = text_argument(request, "file")
            .or_else(|| text_argument(request, "path"))
        {
            let source = read_authoring_script(&root, &file)?;
            (source, file)
        } else {
            return Err(
                "world.script requires script=<rhai source> or file=<project-relative .rhai>."
                    .to_string(),
            );
        };
        let script_hash = sha256_hex(source.as_bytes());
        let script_path = format!(
            ".aura_rafi/world_scripts/authoring-{}.rhai",
            &script_hash[..12.min(script_hash.len())]
        );
        let absolute_script = root.join(&script_path);
        if let Some(parent) = absolute_script.parent() {
            std::fs::create_dir_all(parent).map_err(|error| {
                format!("Could not create the world script directory: {error}")
            })?;
        }
        if !absolute_script.exists() {
            std::fs::write(&absolute_script, source.as_bytes())
                .map_err(|error| format!("Could not save the world script: {error}"))?;
        }
        let (program, operation_count) = world_script_program(&source, ctx.scene)?;
        let group_count = program
            .get("groups")
            .and_then(Value::as_array)
            .map(Vec::len)
            .unwrap_or(0);
        let plan_request = parsed_command_from_params(
            "game.plan",
            serde_json::json!({"program": program}),
        );
        let mut output = execute_plan(&plan_request, ctx);
        if matches!(
            output.level,
            crate::commands::output::CommandLevel::Error
        ) {
            return Ok(output);
        }
        let plan_id = output.json.get("plan_id").cloned().unwrap_or(Value::Null);
        output
            .lines
            .insert(0, format!("script: {operation_count} operation(s)"));
        if let Some(object) = output.json.as_object_mut() {
            object.insert(
                "script".to_string(),
                serde_json::json!({
                    "origin": origin,
                    "operations": operation_count,
                    "groups": group_count,
                    "path": script_path,
                    "sandbox": "rhai-authoring-v1",
                }),
            );
        }
        append_journal(
            &root,
            serde_json::json!({
                "event": "world_script_planned",
                "origin": origin,
                "operations": operation_count,
                "script": script_path,
                "plan_id": plan_id,
            }),
        );
        Ok(output)
    })();
    result.unwrap_or_else(|error| CommandOutput::error("World script", error))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commands::output::CommandLevel;

    fn temp_project(label: &str) -> PathBuf {
        let root = std::env::temp_dir().join(format!(
            "raf-{label}-{}",
            uuid::Uuid::new_v4().simple()
        ));
        std::fs::create_dir_all(&root).unwrap();
        root
    }

    #[test]
    fn authoring_script_emits_reviewable_plan_and_rejects_sandbox_escape() {
        let root = temp_project("world-script");
        let _guard = bind_project_root(Some(root.clone()));
        let mut scene = SceneGraph::new();
        let mut selection = SceneSelectionState::default();
        let mut viewport = HeadlessGameViewportPort::default();
        let mut ctx = GameCommandContext {
            scene: &mut scene,
            selection: &mut selection,
            viewport: &mut viewport,
        };
        let command = parsed_command_from_params(
            "world.script",
            serde_json::json!({
                "script": "world::group(\"Scripted\"); world::group(\"Lamps\"); world::parent(\"Lamps\"); world::grid(\"street_lamp\", 3, 1, 6.0, 6.0); world::parent_clear(); world::place(\"crate\", 0.0, 0.0, 4.0);"
            }),
        );
        let output = execute_world_script(&command, &mut ctx);
        assert!(
            !matches!(output.level, CommandLevel::Error),
            "{:?}",
            output.lines
        );
        assert_eq!(output.json["script"]["sandbox"], "rhai-authoring-v1");
        assert!(output.json["script"]["operations"].as_u64().unwrap() >= 2);
        assert_eq!(output.json["script"]["groups"], 2);
        let script_path = output.json["script"]["path"].as_str().unwrap();
        assert!(root.join(script_path).exists());
        let plan_id = output.json["plan_id"].as_str().unwrap();
        let plan_path = root
            .join(".aura_rafi/world/plans")
            .join(format!("{plan_id}.json"));
        assert!(plan_path.exists());
        let plan: Value = serde_json::from_slice(&std::fs::read(&plan_path).unwrap()).unwrap();
        let groups = plan["groups"].as_array().unwrap();
        assert!(groups.iter().any(|group| group["name"] == "Lamps"));
        assert!(plan["entities"].as_array().unwrap().len() >= 4);
        let gates = output.json["pipeline"]["gates"].as_array().unwrap();
        assert!(gates.iter().any(|gate| gate["gate"] == "navigation"));
        assert!(gates.iter().any(|gate| gate["gate"] == "budget"));
        assert!(gates.iter().any(|gate| gate["gate"] == "design"));

        let escape = parsed_command_from_params(
            "world.script",
            serde_json::json!({"script": "eval(\"1 + 1\");"}),
        );
        let escaped = execute_world_script(&escape, &mut ctx);
        assert!(matches!(escaped.level, CommandLevel::Error));
        assert!(
            escaped.lines.join(" ").to_lowercase().contains("script"),
            "{:?}",
            escaped.lines
        );

        let over_budget = parsed_command_from_params(
            "world.script",
            serde_json::json!({
                "script": "world::assert_max_entities(1); world::grid(\"crate\", 3, 1, 2.0, 2.0);"
            }),
        );
        let over_budget = execute_world_script(&over_budget, &mut ctx);
        assert!(matches!(over_budget.level, CommandLevel::Error));
        assert!(
            over_budget.lines.join(" ").contains("max_entities"),
            "{:?}",
            over_budget.lines
        );
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn recipes_capture_list_apply_and_delete_compiled_plans() {
        let root = temp_project("world-recipe");
        let _guard = bind_project_root(Some(root.clone()));
        let mut scene = SceneGraph::new();
        let mut selection = SceneSelectionState::default();
        let mut viewport = HeadlessGameViewportPort::default();
        let mut ctx = GameCommandContext {
            scene: &mut scene,
            selection: &mut selection,
            viewport: &mut viewport,
        };
        let plan = execute_plan(
            &parsed_command_from_params(
                "game.plan",
                serde_json::json!({
                    "program": {
                        "operations": [
                            {"op": "group", "name": "RecipeRoot", "key": "recipe.root"},
                            {"op": "place", "archetype": "crate", "parent": "RecipeRoot", "position": [1, 0, 1], "key": "recipe.crate"}
                        ]
                    }
                }),
            ),
            &mut ctx,
        );
        assert!(
            !matches!(plan.level, CommandLevel::Error),
            "{:?}",
            plan.lines
        );
        let plan_id = plan.json["plan_id"].as_str().unwrap().to_string();

        let save = execute_world_recipe(
            &parsed_command_from_params(
                "world.recipe",
                serde_json::json!({
                    "action": "save",
                    "name": "demo_recipe",
                    "plan_id": plan_id,
                    "description": "A captured crate."
                }),
            ),
            &mut ctx,
        );
        assert!(save.changed, "{:?}", save.lines);
        assert!(root
            .join(".aura_rafi/world/recipes/demo_recipe.json")
            .exists());

        let list = execute_world_recipe(
            &parsed_command_from_params(
                "world.recipe",
                serde_json::json!({"action": "list"}),
            ),
            &mut ctx,
        );
        assert_eq!(list.json["count"], 1);
        assert_eq!(list.json["recipes"][0]["name"], "demo_recipe");

        let apply = execute_world_recipe(
            &parsed_command_from_params(
                "world.recipe",
                serde_json::json!({"action": "apply", "name": "demo_recipe"}),
            ),
            &mut ctx,
        );
        assert!(
            !matches!(apply.level, CommandLevel::Error),
            "{:?}",
            apply.lines
        );
        assert_ne!(apply.json["plan_id"], plan_id);
        assert_eq!(apply.json["recipe"]["name"], "demo_recipe");
        assert_eq!(apply.json["entities"], 1);

        let delete = execute_world_recipe(
            &parsed_command_from_params(
                "world.recipe",
                serde_json::json!({"action": "delete", "name": "demo_recipe"}),
            ),
            &mut ctx,
        );
        assert!(delete.changed);
        assert!(!root
            .join(".aura_rafi/world/recipes/demo_recipe.json")
            .exists());
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn pipeline_reports_budget_failures_before_commit() {
        let root = temp_project("world-pipeline");
        let _guard = bind_project_root(Some(root.clone()));
        let mut scene = SceneGraph::new();
        let mut selection = SceneSelectionState::default();
        let mut viewport = HeadlessGameViewportPort::default();
        let mut ctx = GameCommandContext {
            scene: &mut scene,
            selection: &mut selection,
            viewport: &mut viewport,
        };
        let output = execute_plan(
            &parsed_command_from_params(
                "game.plan",
                serde_json::json!({
                    "program": {
                        "budget": {"entities": 1, "ops": 8},
                        "operations": [
                            {"op": "place", "archetype": "crate", "position": [0, 0, 0]},
                            {"op": "place", "archetype": "crate", "position": [2, 0, 0]}
                        ]
                    }
                }),
            ),
            &mut ctx,
        );
        assert!(
            !matches!(output.level, CommandLevel::Error),
            "{:?}",
            output.lines
        );
        assert_eq!(output.json["pipeline"]["status"], "failed");
        let gates = output.json["pipeline"]["gates"].as_array().unwrap();
        let budget = gates
            .iter()
            .find(|gate| gate["gate"] == "budget")
            .unwrap();
        assert_eq!(budget["status"], "failed");
        assert_eq!(budget["detail"]["entities"], 2);
        assert_eq!(budget["detail"]["max_entities"], 1);
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn authoring_script_can_emit_camera_and_script_operations() {
        let root = temp_project("world-script-camera");
        let _guard = bind_project_root(Some(root.clone()));
        let mut scene = SceneGraph::new();
        let mut selection = SceneSelectionState::default();
        let mut viewport = HeadlessGameViewportPort::default();
        let mut ctx = GameCommandContext {
            scene: &mut scene,
            selection: &mut selection,
            viewport: &mut viewport,
        };
        let output = execute_world_script(
            &parsed_command_from_params(
                "world.script",
                serde_json::json!({
                    "script": "world::group(\"Rig\"); world::camera(\"Cam\", 0.0, 2.0, 0.0, 55.0); world::script(\"Cam\", \"scripts/probe.rhai\");"
                }),
            ),
            &mut ctx,
        );
        assert!(!matches!(output.level, CommandLevel::Error), "{:?}", output.lines);
        let plan_id = output.json["plan_id"].as_str().unwrap();
        let plan_path = root
            .join(".aura_rafi/world/plans")
            .join(format!("{plan_id}.json"));
        let plan: Value = serde_json::from_slice(&std::fs::read(&plan_path).unwrap()).unwrap();
        assert!(plan["entities"]
            .as_array()
            .unwrap()
            .iter()
            .any(|entity| entity["camera"].is_object()));
        assert_eq!(plan["updates"].as_array().unwrap().len(), 1);
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn plan_supports_mirror_variant_update_and_delete_atomically() {
        let root = temp_project("world-ops");
        let _guard = bind_project_root(Some(root.clone()));
        let mut scene = SceneGraph::new();
        let keep = scene.add_root("Keep");
        scene.get_mut(keep).unwrap().stable_key = Some("keep.crate".to_string());
        let remove = scene.add_root("DeleteMe");
        scene.get_mut(remove).unwrap().stable_key = Some("delete.me".to_string());
        let mut selection = SceneSelectionState::default();
        let mut viewport = HeadlessGameViewportPort::default();
        let mut ctx = GameCommandContext {
            scene: &mut scene,
            selection: &mut selection,
            viewport: &mut viewport,
        };
        let planned = execute_plan(
            &parsed_command_from_params(
                "game.plan",
                serde_json::json!({
                    "program": {
                        "operations": [
                            {"op": "group", "name": "World", "key": "world"},
                            {"op": "place", "archetype": "crate", "parent": "World", "name": "Base", "key": "world.base", "position": [2, 0, 0]},
                            {"op": "mirror", "source": "Base", "axis": "x", "offset": 0},
                            {"op": "variant", "archetypes": ["crate", "bench"], "count": 3, "parent": "World", "name": "Mix", "origin": [0, 0, 4], "spacing": [2, 0, 0]},
                            {"op": "update", "key": "keep.crate", "transform": {"position": [5, 0, 5]}},
                            {"op": "delete", "key": "delete.me"}
                        ]
                    }
                }),
            ),
            &mut ctx,
        );
        assert!(!matches!(planned.level, CommandLevel::Error), "{:?}", planned.lines);
        assert_eq!(planned.json["updates"], 1);
        assert_eq!(planned.json["deletions"], 1);
        let plan_id = planned.json["plan_id"].as_str().unwrap().to_string();
        let plan_path = root
            .join(".aura_rafi/world/plans")
            .join(format!("{plan_id}.json"));
        let persisted: Value = serde_json::from_slice(&std::fs::read(&plan_path).unwrap()).unwrap();
        assert_eq!(persisted["updates"].as_array().unwrap().len(), 1);
        assert_eq!(persisted["deletions"].as_array().unwrap().len(), 1);
        let committed = execute_commit(
            &parsed_command_from_params(
                "game.commit",
                serde_json::json!({"plan_id": plan_id}),
            ),
            &mut ctx,
        );
        assert!(!matches!(committed.level, CommandLevel::Error), "{:?}", committed.lines);
        assert_eq!(committed.json["deleted"], 1);
        assert_eq!(committed.json["updated"], 1);
        let keep_position = ctx.scene.get(keep).unwrap().position;
        assert!((keep_position - glam::Vec3::new(5.0, 0.0, 5.0)).length() < 0.001);
        assert!(!ctx.scene.is_valid_node(remove));
        let names = ctx
            .scene
            .iter()
            .map(|(_, node)| node.name.clone())
            .collect::<Vec<_>>();
        assert!(names.iter().any(|name| name.ends_with("_mirror")));
        assert!(names.iter().any(|name| name.starts_with("Mix_0")));
        assert!(names.iter().any(|name| name.starts_with("Mix_1")));
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn authoring_script_can_query_existing_scene_tags() {
        let root = temp_project("world-script-query");
        let _guard = bind_project_root(Some(root.clone()));
        let mut scene = SceneGraph::new();
        let lamp = scene.add_root("ExistingLamp");
        scene
            .get_mut(lamp)
            .unwrap()
            .tags
            .push("street.light".to_string());
        let mut selection = SceneSelectionState::default();
        let mut viewport = HeadlessGameViewportPort::default();
        let mut ctx = GameCommandContext {
            scene: &mut scene,
            selection: &mut selection,
            viewport: &mut viewport,
        };
        let output = execute_world_script(
            &parsed_command_from_params(
                "world.script",
                serde_json::json!({
                    "script": "world::group(\"Start\"); if world::count(\"street.light\") > 0 { world::group(\"Found\"); } world::place(\"crate\", 0.0, 0.0, 0.0);"
                }),
            ),
            &mut ctx,
        );
        assert!(!matches!(output.level, CommandLevel::Error), "{:?}", output.lines);
        let plan_id = output.json["plan_id"].as_str().unwrap();
        let plan_path = root
            .join(".aura_rafi/world/plans")
            .join(format!("{plan_id}.json"));
        let plan: Value = serde_json::from_slice(&std::fs::read(&plan_path).unwrap()).unwrap();
        assert!(plan["groups"]
            .as_array()
            .unwrap()
            .iter()
            .any(|group| group["name"] == "Found"));
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn asserted_overlap_constraint_fails_the_pipeline_gate() {
        let root = temp_project("world-assert");
        let _guard = bind_project_root(Some(root.clone()));
        let mut scene = SceneGraph::new();
        let mut selection = SceneSelectionState::default();
        let mut viewport = HeadlessGameViewportPort::default();
        let mut ctx = GameCommandContext {
            scene: &mut scene,
            selection: &mut selection,
            viewport: &mut viewport,
        };
        let output = execute_plan(
            &parsed_command_from_params(
                "game.plan",
                serde_json::json!({
                    "program": {
                        "operations": [
                            {"op": "place", "archetype": "crate", "position": [0, 0, 0]},
                            {"op": "place", "archetype": "crate", "position": [0, 0, 0]},
                            {"op": "assert", "no_overlaps": true}
                        ]
                    }
                }),
            ),
            &mut ctx,
        );
        assert!(!matches!(output.level, CommandLevel::Error), "{:?}", output.lines);
        let gates = output.json["pipeline"]["gates"].as_array().unwrap();
        let assertions = gates
            .iter()
            .find(|gate| gate["gate"] == "assertions")
            .unwrap();
        assert_eq!(assertions["status"], "failed");
        assert_eq!(output.json["pipeline"]["status"], "failed");
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn cancelled_plan_cannot_be_committed() {
        let root = temp_project("world-cancel");
        let _guard = bind_project_root(Some(root.clone()));
        let mut scene = SceneGraph::new();
        let mut selection = SceneSelectionState::default();
        let mut viewport = HeadlessGameViewportPort::default();
        let mut ctx = GameCommandContext {
            scene: &mut scene,
            selection: &mut selection,
            viewport: &mut viewport,
        };
        let plan = execute_plan(
            &parsed_command_from_params(
                "game.plan",
                serde_json::json!({
                    "program": {
                        "operations": [
                            {"op": "place", "archetype": "crate", "position": [0, 0, 0]}
                        ]
                    }
                }),
            ),
            &mut ctx,
        );
        assert!(!matches!(plan.level, CommandLevel::Error), "{:?}", plan.lines);
        let plan_id = plan.json["plan_id"].as_str().unwrap().to_string();
        let cancel = execute_cancel(&parsed_command_from_params(
            "game.cancel",
            serde_json::json!({"plan_id": plan_id.clone()}),
        ));
        assert!(!matches!(cancel.level, CommandLevel::Error), "{:?}", cancel.lines);
        assert_eq!(cancel.json["cancelled"], true);
        let commit = execute_commit(
            &parsed_command_from_params(
                "game.commit",
                serde_json::json!({"plan_id": plan_id}),
            ),
            &mut ctx,
        );
        assert!(matches!(commit.level, CommandLevel::Error));
        assert!(
            commit.lines.join(" ").contains("cancelled"),
            "{:?}",
            commit.lines
        );
        assert!(ctx.scene.all_live_ids().is_empty());
        let _ = std::fs::remove_dir_all(root);
    }
}


