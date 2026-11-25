//! Imported asset catalog projections and transparent intent ranking.

use super::scene::scene_item_reference;
use super::support::{page, paged_data, string_arg};
use super::AgentObservationResult;
use crate::scene::SceneGraph;
use serde_json::{json, Value};
use std::collections::BTreeSet;

pub fn asset_inspect(
    scene: &SceneGraph,
    assets: &[String],
    args: &Value,
    catalog_pending: bool,
    catalog_error: Option<&str>,
) -> AgentObservationResult {
    let target = string_arg(args, "target")
        .or_else(|| string_arg(args, "path"))
        .or_else(|| string_arg(args, "asset"));
    let Some(target) = target.map(str::trim).filter(|target| !target.is_empty()) else {
        let mut result = AgentObservationResult::new(
            "Asset inspect",
            "No asset target was provided.",
            json!({"found": false}),
        );
        result
            .warnings
            .push("Provide target as an asset path.".to_string());
        return result;
    };
    let target = normalize_asset_path(target);
    let Some(path) = assets.iter().find(|path| {
        let path = normalize_asset_path(path);
        path == target || path.ends_with(&target) || target.ends_with(&path)
    }) else {
        return AgentObservationResult::new(
            "Asset inspect",
            format!("Asset '{target}' was not found in the project catalog."),
            json!({"found": false, "target": target}),
        );
    };
    let used = used_assets(scene);
    let normalized = normalize_asset_path(path);
    let is_used = used.contains(&normalized)
        || used
            .iter()
            .any(|source| normalized.ends_with(source) || source.ends_with(&normalized));
    let mut result = AgentObservationResult::new(
        "Asset inspect",
        format!("Inspected asset '{}'.", path),
        json!({
            "found": true,
            "path": path,
            "name": path.rsplit(['/', '\\']).next().unwrap_or(path),
            "kind": asset_kind(path),
            "used": is_used,
            "references": scene_asset_references(scene, path),
            "catalog_pending": catalog_pending,
        }),
    );
    if let Some(error) = catalog_error {
        result
            .warnings
            .push(format!("Asset catalog warning: {error}"));
    } else if catalog_pending {
        result
            .warnings
            .push("The asset catalog is still indexing; results may be incomplete.".to_string());
    }
    result
}

pub fn assets_catalog(
    scene: &SceneGraph,
    assets: &[String],
    args: &Value,
    catalog_pending: bool,
    catalog_error: Option<&str>,
) -> AgentObservationResult {
    let query = string_arg(args, "query")
        .unwrap_or("")
        .trim()
        .to_ascii_lowercase();
    let usage = string_arg(args, "usage")
        .unwrap_or("all")
        .trim()
        .to_ascii_lowercase();
    let kind = string_arg(args, "kind")
        .map(str::trim)
        .filter(|kind| !kind.is_empty())
        .map(str::to_ascii_lowercase);
    let used = used_assets(scene);
    let mut entries = assets
        .iter()
        .filter(|path| {
            let normalized = normalize_asset_path(path);
            let is_used = used.contains(&normalized)
                || used
                    .iter()
                    .any(|source| normalized.ends_with(source) || source.ends_with(&normalized));
            let usage_matches = match usage.as_str() {
                "used" => is_used,
                "unused" => !is_used,
                _ => true,
            };
            let kind_matches = kind
                .as_deref()
                .map(|requested| asset_kind_matches(path, requested))
                .unwrap_or(true);
            usage_matches && kind_matches && (query.is_empty() || normalized.contains(&query))
        })
        .map(|path| {
            let normalized = normalize_asset_path(path);
            let is_used = used.contains(&normalized)
                || used
                    .iter()
                    .any(|source| normalized.ends_with(source) || source.ends_with(&normalized));
            json!({
            "path": path,
            "name": path.rsplit(['/', '\\']).next().unwrap_or(path),
            "kind": asset_kind(path),
                "used": is_used,
                "references": scene_asset_references(scene, path),
            })
        })
        .collect::<Vec<_>>();
    entries.sort_by(|left, right| left["path"].to_string().cmp(&right["path"].to_string()));
    let total = entries.len();
    let (offset, limit) = page(args);
    let items = entries
        .into_iter()
        .skip(offset)
        .take(limit)
        .collect::<Vec<_>>();
    let mut result = AgentObservationResult::new(
        "Assets catalog",
        format!("{} asset(s) matched; {} returned.", total, items.len()),
        paged_data(
            items,
            total,
            offset,
            limit,
            json!({"query": query, "usage": usage, "kind": kind, "catalog_pending": catalog_pending}),
        ),
    );
    if let Some(error) = catalog_error {
        result
            .warnings
            .push(format!("Asset catalog warning: {error}"));
    } else if catalog_pending {
        result
            .warnings
            .push("The asset catalog is still indexing; results may be incomplete.".to_string());
    }
    result
}

/// Rank imported assets for an authoring intent without invoking a model or
/// crawling workspace internals. This remains a transparent filename/type
/// heuristic and reports its reasons so the Agent can decide whether to use a
/// real asset or fall back to procedural primitives.
pub fn assets_recommend(
    scene: &SceneGraph,
    assets: &[String],
    args: &Value,
    catalog_pending: bool,
    catalog_error: Option<&str>,
) -> AgentObservationResult {
    let intent = string_arg(args, "intent").unwrap_or("").trim();
    let preferred_kind = string_arg(args, "kind")
        .map(str::trim)
        .filter(|kind| !kind.is_empty())
        .map(str::to_ascii_lowercase);
    let limit = args
        .get("limit")
        .and_then(Value::as_u64)
        .unwrap_or(8)
        .clamp(1, 24) as usize;
    let mut tokens = semantic_tokens(intent);
    for (source, related) in [
        ("shelf", &["rack", "cabinet", "display", "estante"][..]),
        ("estante", &["shelf", "rack", "gondola"][..]),
        ("store", &["shop", "market", "retail", "tienda"][..]),
        ("tienda", &["store", "shop", "market", "retail"][..]),
        ("wall", &["pared", "panel"][..]),
        ("pared", &["wall", "panel"][..]),
    ] {
        if tokens.iter().any(|token| token == source) {
            tokens.extend(related.iter().map(|value| (*value).to_string()));
        }
    }
    tokens.sort();
    tokens.dedup();
    let used = used_assets(scene);
    let mut ranked = assets
        .iter()
        .filter_map(|path| {
            let normalized = normalize_asset_path(path);
            let kind = asset_kind(path);
            let is_used = used.contains(&normalized)
                || used
                    .iter()
                    .any(|source| normalized.ends_with(source) || source.ends_with(&normalized));
            let mut score = 0i32;
            let mut reasons = Vec::new();
            for token in &tokens {
                if normalized.contains(token) {
                    score += 10;
                    reasons.push(format!("name/path matches '{token}'"));
                }
            }
            if preferred_kind
                .as_deref()
                .is_some_and(|preferred| preferred == kind)
            {
                score += 8;
                reasons.push(format!("preferred kind '{kind}'"));
            }
            if !is_used {
                score += 1;
                reasons.push("currently unused".to_string());
            }
            (score > 0 || tokens.is_empty()).then(|| {
                json!({
                    "path": path,
                    "name": path.rsplit(['/', '\\']).next().unwrap_or(path),
                    "kind": kind,
                    "used": is_used,
                    "score": score,
                    "reasons": reasons,
                    "references": scene_asset_references(scene, path)
                })
            })
        })
        .collect::<Vec<_>>();
    ranked.sort_by(|left, right| {
        right["score"]
            .as_i64()
            .cmp(&left["score"].as_i64())
            .then_with(|| left["path"].as_str().cmp(&right["path"].as_str()))
    });
    ranked.truncate(limit);
    let mut result = AgentObservationResult::new(
        "Asset recommendations",
        format!(
            "Recommended {} imported asset(s) for '{}'.",
            ranked.len(),
            intent
        ),
        json!({
            "intent": intent,
            "kind": preferred_kind,
            "items": ranked,
            "catalog_pending": catalog_pending,
            "strategy": "transparent filename, semantic-token and asset-kind ranking"
        }),
    );
    if let Some(error) = catalog_error {
        result
            .warnings
            .push(format!("Asset catalog warning: {error}"));
    } else if catalog_pending {
        result.warnings.push(
            "The asset catalog is still indexing; recommendations may be incomplete.".to_string(),
        );
    }
    result
}

fn semantic_tokens(value: &str) -> Vec<String> {
    value
        .split(|character: char| !character.is_alphanumeric())
        .map(str::trim)
        .filter(|token| token.chars().count() >= 3)
        .map(str::to_ascii_lowercase)
        .collect()
}

pub(super) fn asset_kind(path: &str) -> &'static str {
    if is_script_path(path) {
        return "script";
    }
    match path
        .rsplit_once('.')
        .map(|(_, extension)| extension.to_ascii_lowercase())
        .as_deref()
    {
        Some("png") | Some("jpg") | Some("jpeg") | Some("webp") | Some("gif") => "image",
        Some("glb") | Some("gltf") | Some("obj") | Some("fbx") => "model",
        Some("wav") | Some("mp3") | Some("ogg") => "audio",
        Some("ron") | Some("json") | Some("toml") => "data",
        _ => "file",
    }
}

fn asset_kind_matches(path: &str, requested: &str) -> bool {
    let requested = match requested {
        "texture" => "image",
        "other" => "file",
        value => value,
    };
    asset_kind(path) == requested
}

pub(super) fn is_script_path(path: &str) -> bool {
    matches!(
        path.rsplit_once('.')
            .map(|(_, extension)| extension.to_ascii_lowercase())
            .as_deref(),
        Some("rhai") | Some("rs") | Some("lua") | Some("js") | Some("ts") | Some("cpp")
    )
}

pub(super) fn normalize_asset_path(path: &str) -> String {
    path.trim()
        .replace('\\', "/")
        .trim_start_matches("./")
        .to_ascii_lowercase()
}

fn used_assets(scene: &SceneGraph) -> BTreeSet<String> {
    scene
        .iter()
        .flat_map(|(_, node)| {
            node.source_asset
                .iter()
                .chain(node.base_color_texture.iter())
                .chain(node.animation_asset.iter())
                .chain(node.mesh_asset.as_ref().map(|mesh| &mesh.path))
                .chain(std::iter::once(&node.audio_source.clip).filter(|clip| !clip.is_empty()))
                .chain(node.scripts.iter())
        })
        .map(|asset| normalize_asset_path(asset))
        .collect()
}

fn scene_asset_references(scene: &SceneGraph, asset: &str) -> Vec<Value> {
    let asset = normalize_asset_path(asset);
    scene
        .iter()
        .filter(|(_, node)| {
            node.source_asset
                .iter()
                .chain(node.base_color_texture.iter())
                .chain(node.animation_asset.iter())
                .chain(node.mesh_asset.as_ref().map(|mesh| &mesh.path))
                .chain(std::iter::once(&node.audio_source.clip).filter(|clip| !clip.is_empty()))
                .chain(node.scripts.iter())
                .any(|source| {
                    let source = normalize_asset_path(source);
                    source == asset || source.ends_with(&asset) || asset.ends_with(&source)
                })
        })
        .filter_map(|(id, _)| scene_item_reference(scene, id))
        .collect()
}
