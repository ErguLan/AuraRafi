//! Script attachment projections for the shared scene graph.

use super::assets::is_script_path;
use super::scene::scene_item_reference;
use super::support::{page, paged_data, string_arg};
use super::AgentObservationResult;
use crate::scene::SceneGraph;
use serde_json::{json, Value};
use std::collections::BTreeMap;

pub fn scripts_catalog(
    scene: &SceneGraph,
    assets: &[String],
    args: &Value,
) -> AgentObservationResult {
    let query = string_arg(args, "query")
        .unwrap_or("")
        .trim()
        .to_ascii_lowercase();
    let mut references: BTreeMap<String, Vec<Value>> = BTreeMap::new();
    for asset in assets.iter().filter(|asset| is_script_path(asset)) {
        references.entry(asset.clone()).or_default();
    }
    for (id, node) in scene.iter() {
        for script in &node.scripts {
            references
                .entry(script.clone())
                .or_default()
                .push(scene_item_reference(scene, id).unwrap_or(Value::Null));
        }
    }
    let entries = references
        .into_iter()
        .filter(|(path, _)| query.is_empty() || path.to_ascii_lowercase().contains(&query))
        .map(|(path, entities)| {
            json!({"path": path, "name": path.rsplit(['/', '\\']).next().unwrap_or(&path), "attached_to": entities})
        })
        .collect::<Vec<_>>();
    let total = entries.len();
    let (offset, limit) = page(args);
    let items = entries
        .into_iter()
        .skip(offset)
        .take(limit)
        .collect::<Vec<_>>();
    AgentObservationResult::new(
        "Scripts catalog",
        format!("{} script(s) matched; {} returned.", total, items.len()),
        paged_data(items, total, offset, limit, json!({"query": query})),
    )
}
