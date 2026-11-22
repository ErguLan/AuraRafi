//! Persistent, reusable transform clips in the project Assets catalog.

use raf_core::scene::{animation::AnimationRig, SceneGraph};
use std::collections::HashMap;
use std::io::Read;
use std::path::{Component, Path, PathBuf};

pub const SUFFIX: &str = ".animation.json";
const MAX_BYTES: u64 = 8 * 1024 * 1024;
const MAX_SCENE_KEYS: usize = 262_144;

pub fn is_animation_asset(path: &str) -> bool {
    path.to_ascii_lowercase().ends_with(SUFFIX)
}

pub fn reference(row: &str) -> String {
    crate::authoring_catalog::normalized_reference(row)
}

fn assets_dir(project: &Path, create: bool) -> Result<PathBuf, String> {
    let project = project.canonicalize().map_err(|error| error.to_string())?;
    let assets = project.join("assets");
    if create {
        std::fs::create_dir_all(&assets).map_err(|error| error.to_string())?;
    }
    let assets = assets.canonicalize().map_err(|error| error.to_string())?;
    if !assets.starts_with(&project) || assets == project {
        return Err("Animation assets must resolve inside the project".into());
    }
    Ok(assets)
}

pub fn load(project: &Path, row: &str) -> Result<AnimationRig, String> {
    let reference = reference(row);
    let relative = Path::new(&reference);
    if !is_animation_asset(&reference)
        || !relative
            .components()
            .all(|part| matches!(part, Component::Normal(_)))
    {
        return Err("Expected a project-relative .animation.json asset".into());
    }
    let assets = assets_dir(project, false)?;
    let file = assets
        .join(relative)
        .canonicalize()
        .map_err(|error| error.to_string())?;
    if !file.starts_with(&assets) || !file.is_file() {
        return Err("Animation asset resolves outside project assets or is not a file".into());
    }
    let mut data = Vec::new();
    std::fs::File::open(file)
        .map_err(|error| error.to_string())?
        .take(MAX_BYTES + 1)
        .read_to_end(&mut data)
        .map_err(|error| error.to_string())?;
    if data.len() as u64 > MAX_BYTES {
        return Err("Animation asset exceeds the file budget".into());
    }
    let rig: AnimationRig = serde_json::from_slice(&data).map_err(|error| error.to_string())?;
    rig.validate()?;
    Ok(rig)
}

pub fn create(project: &Path, label: &str, rig: &AnimationRig) -> Result<String, String> {
    rig.validate()?;
    let stem: String = label
        .chars()
        .take(48)
        .map(|c| {
            if c.is_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '-'
            }
        })
        .collect();
    let id = uuid::Uuid::new_v4().simple().to_string();
    let name = format!("{}-{}{SUFFIX}", stem.trim_matches('-'), &id[..12]);
    let data = serde_json::to_vec_pretty(rig).map_err(|error| error.to_string())?;
    if data.len() as u64 > MAX_BYTES {
        return Err("Animation asset exceeds the file budget".into());
    }
    let file = assets_dir(project, true)?.join(&name);
    if file.exists() {
        return Err("Animation asset already exists".into());
    }
    raf_core::project_store::write_atomic(&file, &data)?;
    Ok(name)
}

pub fn save(
    project: &Path,
    row: &str,
    expected: &AnimationRig,
    rig: &AnimationRig,
) -> Result<(), String> {
    if &load(project, row)? != expected {
        return Err("Animation asset changed on disk. Reopen it from Assets.".into());
    }
    rig.validate()?;
    let data = serde_json::to_vec_pretty(rig).map_err(|error| error.to_string())?;
    if data.len() as u64 > MAX_BYTES {
        return Err("Animation asset exceeds the file budget".into());
    }
    let assets = assets_dir(project, false)?;
    let file = assets
        .join(reference(row))
        .canonicalize()
        .map_err(|error| error.to_string())?;
    if !file.starts_with(&assets) {
        return Err("Animation asset resolves outside project assets".into());
    }
    raf_core::project_store::write_atomic(&file, &data)
}

/// Hydrates references on an isolated Play or preview snapshot.
pub fn resolve_scene(project: &Path, scene: &mut SceneGraph) -> Result<(), String> {
    let links: Vec<_> = scene
        .iter()
        .filter(|(_, node)| !node.name.is_empty())
        .filter_map(|(id, node)| node.animation_asset.as_ref().map(|path| (id, path.clone())))
        .collect();
    let mut loaded: HashMap<String, AnimationRig> = HashMap::new();
    let mut keys = 0;
    for (id, path) in links {
        let path = reference(&path);
        let rig = match loaded.entry(path.clone()) {
            std::collections::hash_map::Entry::Occupied(entry) => entry.into_mut(),
            std::collections::hash_map::Entry::Vacant(entry) => {
                entry.insert(load(project, &path).map_err(|error| format!("{path}: {error}"))?)
            }
        };
        keys += rig
            .layers
            .iter()
            .flat_map(|layer| &layer.tracks)
            .map(|track| track.keys.len())
            .sum::<usize>();
        if keys > MAX_SCENE_KEYS {
            return Err("Linked animations exceed the scene key budget".into());
        }
        if let Some(node) = scene.get_mut(id) {
            node.animation = Some(rig.clone());
        }
    }
    Ok(())
}
