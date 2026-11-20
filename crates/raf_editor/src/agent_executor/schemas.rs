//! Provider schemas and localized tool descriptions shared by the Agent tools.

use raf_ai::openai_client::{OpenAiFunction, OpenAiTool};
use raf_core::i18n::t;
use raf_core::Language;
use serde_json::{Map, Value};

use crate::commands::catalog::CommandDefinition;

pub fn sanitize_tool_name(name: &str) -> String {
    let sanitized = name
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() || character == '_' {
                character
            } else {
                '_'
            }
        })
        .collect::<String>();
    if sanitized
        .chars()
        .next()
        .is_some_and(|character| character.is_ascii_digit())
    {
        format!("_{sanitized}")
    } else {
        sanitized
    }
}

pub(super) fn command_to_openai_tool(
    sanitized_name: &str,
    command: &CommandDefinition,
    language: Language,
) -> OpenAiTool {
    let mut properties = Map::new();
    let mut required = Vec::new();
    for parameter in &command.parameters {
        let description = parameter
            .description_key
            .as_deref()
            .map(|key| t(key, language))
            .unwrap_or_else(|| format!("{} ({})", parameter.name, parameter.kind));
        let mut schema = match parameter.kind.as_str() {
            "usize" | "i32" | "i64" => {
                integer_schema(&description, i32::MIN as i64, i32::MAX as i64)
            }
            "f32" | "f64" => serde_json::json!({"type": "number", "description": description}),
            "bool" => boolean_schema(&description),
            "array" if parameter.name == "required_features" => serde_json::json!({
                "type": "array",
                "description": description,
                "items": {"type": "string", "enum": ["floor", "enclosure", "entrance", "circulation", "roof", "primary_modules", "details"]},
                "maxItems": 7
            }),
            "array" if matches!(parameter.name.as_str(), "targets" | "expected_names") => {
                serde_json::json!({
                    "type": "array",
                    "description": description,
                    "items": {"type": "string"},
                    "maxItems": 128
                })
            }
            "array" if parameter.name == "color_rgba" => {
                serde_json::json!({
                    "type": "array",
                    "description": description,
                    "items": {"type": "integer", "minimum": 0, "maximum": 255},
                    "minItems": 3,
                    "maxItems": 4
                })
            }
            "array" if parameter.name == "tags" => serde_json::json!({
                "type": "array",
                "description": description,
                "items": {"type": "string", "maxLength": 64},
                "maxItems": 32
            }),
            "array" => serde_json::json!({"type": "array", "description": description}),
            "object" => serde_json::json!({"type": "object", "description": description}),
            "enum" if parameter.name == "primitive" => enum_schema(
                &description,
                &["empty", "cube", "sphere", "plane", "cylinder", "capsule"],
            ),
            "enum" if parameter.name == "design_profile" => enum_schema(
                &description,
                &[
                    "generic",
                    "real_world",
                    "building",
                    "supermarket",
                    "parking",
                    "outdoor",
                ],
            ),
            "enum" if command.name == "assets.catalog" && parameter.name == "kind" => enum_schema(
                &description,
                &["image", "model", "audio", "script", "data", "file"],
            ),
            "enum" if command.name == "assets.catalog" && parameter.name == "usage" => {
                enum_schema(&description, &["all", "used", "unused"])
            }
            _ => string_schema(&description),
        };
        if command.name == "lighting.set" && parameter.name == "value" {
            schema = serde_json::json!({"description":description,"anyOf":[
                {"type":"number"},{"type":"boolean"},{"type":"string","maxLength":32},
                {"type":"array","items":{"type":"integer","minimum":0,"maximum":255},"minItems":3,"maxItems":3}
            ]});
        }
        if command.name == "ui.document.apply" && parameter.name == "edits" {
            schema=serde_json::json!({"type":"array","description":description,"maxItems":128,
                "items":{"type":"object","properties":{
                    "op":{"type":"string","enum":["insert","add","patch","remove","rename","reparent","mount","bind"]},
                    "parent":{"type":"string"},"id":{"type":"string"},"node":{},"kind":{"type":"string","enum":["frame","label","image","button","input"]},
                    "text":{"type":"string"},"rect":{"type":"object","properties":{"x":{"type":"number"},"y":{"type":"number"},"width":{"type":"number"},"height":{"type":"number"}},"required":["x","y","width","height"]},
                    "patch":{"type":"object"},"mount":{"type":"object"},"binding":{"type":"object"}
                },"required":["op"]}});
        }
        if command.name == "lighting.set" && parameter.name == "patch" {
            schema =
                serde_json::json!({"type":"object","description":description,"maxProperties":20});
        }
        if let Some(default) = &parameter.default {
            schema["default"] = parse_default(default, &parameter.kind);
        }
        properties.insert(parameter.name.clone(), schema);
        if parameter.required {
            required.push(parameter.name.as_str());
        }
    }
    openai_tool(
        sanitized_name,
        &localized_tool_description(command, language),
        object_schema(properties, &required),
    )
}

fn localized_tool_description(command: &CommandDefinition, language: Language) -> String {
    let mut description = t(&command.description_key, language);
    if let Some(example) = command.examples.first() {
        description.push_str(" Example: ");
        description.push_str(example);
    }
    description
}

pub(super) fn localized_native_tool_description(name: &str, fallback: &str, language: Language) -> String {
    let key = match name {
        "asset_generate_local_png" => Some("commands.asset_generate_local_png.desc"),
        "asset_generate_image" => Some("commands.asset_generate_image_native.desc"),
        "external_studio_install" => Some("commands.external_studio_install.desc"),
        "external_studio_generate" => Some("commands.external_studio_generate.desc"),
        "project_summary" => Some("commands.project_summary.desc"),
        "scene_outline" => Some("commands.scene_outline.desc"),
        "scene_query" => Some("commands.scene_query.desc"),
        "scene_spatial_map" => Some("commands.scene_spatial_map.desc"),
        "scene_check_overlaps" => Some("commands.scene_check_overlaps.desc"),
        "scene_diff" => Some("commands.scene_diff.desc"),
        "scene_design_audit" => Some("commands.scene_design_audit.desc"),
        "scene_inspect" => Some("commands.scene_inspect.desc"),
        "selection_get" => Some("commands.selection_get.desc"),
        "viewport_capture" => Some("commands.viewport_capture.desc"),
        "assets_catalog" => Some("commands.assets_catalog.desc"),
        "assets_recommend" => Some("commands.assets_recommend.desc"),
        "asset_inspect" => Some("commands.asset_inspect.desc"),
        "scripts_catalog" => Some("commands.scripts_catalog.desc"),
        "project_health" => Some("commands.project_health.desc"),
        "scene_verify" | "game_validate_layout" => Some("commands.scene_verify.desc"),
        "scene_build" => Some("commands.game_build.desc"),
        "scene_create_group" => Some("commands.game_create_group.desc"),
        "scene_create" => Some("commands.game_add.desc"),
        "scene_update" => Some("commands.game_update.desc"),
        "scene_reparent" => Some("commands.game_reparent.desc"),
        "scene_snap" => Some("commands.game_snap.desc"),
        "scene_delete" => Some("commands.game_delete.desc"),
        "scene_duplicate" => Some("commands.game_duplicate.desc"),
        "scene_arrange" => Some("commands.game_arrange_grid.desc"),
        "scene_instantiate_prefab" | "scene_instantiate_template" => {
            Some("commands.game_generate_prefab.desc")
        }
        "scene_batch" => Some("commands.game_batch.desc"),
        "scene_reconcile" => Some("commands.game_reconcile.desc"),
        "scene_repair" => Some("commands.game_repair.desc"),
        _ => None,
    };
    key.map(|key| {
        let translated = t(key, language);
        if translated == key {
            fallback.to_string()
        } else {
            translated
        }
    })
    .unwrap_or_else(|| fallback.to_string())
}

fn parse_default(default: &str, kind: &str) -> Value {
    match kind {
        "usize" | "i32" | "i64" => default
            .parse::<i64>()
            .map(Value::from)
            .unwrap_or_else(|_| Value::String(default.to_string())),
        "f32" | "f64" => default
            .parse::<f64>()
            .ok()
            .and_then(serde_json::Number::from_f64)
            .map(Value::Number)
            .unwrap_or_else(|| Value::String(default.to_string())),
        "bool" => default
            .parse::<bool>()
            .map(Value::Bool)
            .unwrap_or_else(|_| Value::String(default.to_string())),
        _ => Value::String(default.to_string()),
    }
}

pub(super) fn openai_tool(name: &str, description: &str, parameters: Value) -> OpenAiTool {
    OpenAiTool {
        tool_type: "function".to_string(),
        function: OpenAiFunction {
            name: name.to_string(),
            description: Some(description.to_string()),
            parameters,
        },
    }
}

pub(super) fn object_schema(properties: Map<String, Value>, required: &[&str]) -> Value {
    serde_json::json!({
        "type": "object",
        "properties": properties,
        "required": required,
        "additionalProperties": false
    })
}

pub(super) fn paginated_schema(extra: &[(&str, Value)]) -> Value {
    let mut properties = Map::from_iter([
        (
            "cursor".to_string(),
            integer_schema("Zero-based result cursor.", 0, 100000),
        ),
        (
            "limit".to_string(),
            integer_schema("Maximum rows to return.", 1, 128),
        ),
    ]);
    for (name, schema) in extra {
        properties.insert((*name).to_string(), schema.clone());
    }
    object_schema(properties, &[])
}

pub(super) fn transform_schema() -> Value {
    object_schema(
        Map::from_iter([
            ("position".to_string(), vec3_schema("Local XYZ position.")),
            (
                "rotation_deg".to_string(),
                vec3_schema("Local XYZ Euler rotation in degrees."),
            ),
            ("scale".to_string(), vec3_schema("Local XYZ scale.")),
        ]),
        &[],
    )
}

pub(super) fn vec3_schema(description: &str) -> Value {
    serde_json::json!({
        "type": "array",
        "description": description,
        "items": {"type": "number"},
        "minItems": 3,
        "maxItems": 3
    })
}

fn scene_entity_schema() -> Value {
    object_schema(
        Map::from_iter([
            (
                "kind".to_string(),
                enum_schema(
                    "Primitive to create.",
                    &["empty", "cube", "sphere", "plane", "cylinder", "capsule"],
                ),
            ),
            ("name".to_string(), string_schema("Unique entity name.")),
            (
                "parent".to_string(),
                string_schema("Optional group ID, UUID, name, or path."),
            ),
            ("transform".to_string(), transform_schema()),
            (
                "color_rgba".to_string(),
                color_schema("RGBA channels from 0 to 255."),
            ),
            (
                "semantic_role".to_string(),
                string_schema("Semantic role such as store.shelf."),
            ),
            (
                "stable_key".to_string(),
                string_schema("Unique reconciliation key."),
            ),
            ("tags".to_string(), semantic_tags_schema()),
            (
                "agent_origin".to_string(),
                string_schema("Task or workflow origin."),
            ),
        ]),
        &["kind", "name"],
    )
}

fn scene_group_schema() -> Value {
    object_schema(
        Map::from_iter([
            ("name".to_string(), string_schema("Unique group name.")),
            (
                "parent".to_string(),
                string_schema("Optional group ID, UUID, name, or path."),
            ),
            ("transform".to_string(), transform_schema()),
            (
                "semantic_role".to_string(),
                string_schema("Semantic role for the group."),
            ),
            (
                "stable_key".to_string(),
                string_schema("Unique reconciliation key."),
            ),
            ("tags".to_string(), semantic_tags_schema()),
            (
                "agent_origin".to_string(),
                string_schema("Task or workflow origin."),
            ),
        ]),
        &["name"],
    )
}

pub(super) fn scene_build_schema() -> Value {
    serde_json::json!({
        "type": "object",
        "properties": {
            "parent": string_schema("Optional existing group that receives the generated structure."),
            "design_profile": {
                "type": "string",
                "description": "Optional design contract. Use supermarket, building, parking, outdoor, real_world, or generic for a real-world request.",
                "enum": ["generic", "real_world", "building", "supermarket", "parking", "outdoor"]
            },
            "groups": {
                "type": "array",
                "description": "Folders/groups to create before entities.",
                "items": scene_group_schema(),
                "maxItems": 32
            },
            "entities": {
                "type": "array",
                "description": "Primitive entities to create after groups.",
                "items": scene_entity_schema(),
                "maxItems": 128
            }
        },
        "additionalProperties": false
    })
}

pub(super) fn scene_reconcile_schema() -> Value {
    let mut schema = scene_build_schema();
    schema["description"] = Value::String(
        "Keyed desired scene state. Existing stable_key values are updated instead of duplicated."
            .to_string(),
    );
    schema["properties"]["groups"]["items"]["required"] = serde_json::json!(["name", "stable_key"]);
    schema["properties"]["entities"]["items"]["required"] =
        serde_json::json!(["kind", "name", "stable_key"]);
    schema
}

pub(super) fn batch_operation_params_schema() -> Value {
    object_schema(
        Map::from_iter([
            (
                "kind".to_string(),
                enum_schema(
                    "Primitive to create.",
                    &["empty", "cube", "sphere", "plane", "cylinder"],
                ),
            ),
            (
                "primitive".to_string(),
                enum_schema(
                    "Canonical primitive alias.",
                    &["empty", "cube", "sphere", "plane", "cylinder"],
                ),
            ),
            (
                "type".to_string(),
                string_schema("Primitive alias accepted for compatibility."),
            ),
            (
                "shape".to_string(),
                string_schema("Primitive alias accepted for compatibility."),
            ),
            ("name".to_string(), string_schema("Entity or group name.")),
            (
                "target".to_string(),
                string_schema("Entity or group target."),
            ),
            (
                "parent".to_string(),
                string_schema("Parent ID, UUID, name, path, or root."),
            ),
            (
                "preserve_world".to_string(),
                boolean_schema("Keep world transform during reparenting."),
            ),
            (
                "offset".to_string(),
                vec3_schema("Local duplication offset."),
            ),
            (
                "count".to_string(),
                integer_schema("Number of duplicates or template instances.", 1, 128),
            ),
            (
                "axis".to_string(),
                enum_schema("Array or snap axis.", &["x", "y", "z"]),
            ),
            (
                "mode".to_string(),
                enum_schema("Snap mode.", &["grid", "floor", "surface"]),
            ),
            (
                "snap_to".to_string(),
                string_schema("Surface entity target."),
            ),
            (
                "placement".to_string(),
                enum_schema("Surface placement.", &["before", "after", "center"]),
            ),
            (
                "gap".to_string(),
                number_schema("Surface separation.", 0.0, 1000.0),
            ),
            (
                "grid".to_string(),
                number_schema("Grid interval.", 0.001, 1000.0),
            ),
            ("transform".to_string(), transform_schema()),
            (
                "color_rgba".to_string(),
                color_schema("RGBA channels from 0 to 255."),
            ),
            (
                "color".to_string(),
                color_schema("RGBA channels from 0 to 255."),
            ),
            ("visible".to_string(), boolean_schema("Entity visibility.")),
            ("locked".to_string(), boolean_schema("Editor lock state.")),
            ("animation".to_string(), serde_json::json!({"type":["object","null"],"description":"Replace or remove the transform animation rig on scene_update."})),
            ("animation_clip".to_string(), serde_json::json!({"type":"object","description":"Append a GLB clip on scene_update using asset, clip and bindings."})),
            ("semantic_role".to_string(), string_schema("Semantic role.")),
            (
                "stable_key".to_string(),
                string_schema("Unique reconciliation key."),
            ),
            ("tags".to_string(), semantic_tags_schema()),
            (
                "agent_origin".to_string(),
                string_schema("Task or workflow origin."),
            ),
            (
                "spacing".to_string(),
                number_schema("Grid or array spacing.", -10000.0, 10000.0),
            ),
            (
                "factor".to_string(),
                number_schema("Scale factor.", -10000.0, 10000.0),
            ),
            (
                "x".to_string(),
                number_schema("X component.", -100000.0, 100000.0),
            ),
            (
                "y".to_string(),
                number_schema("Y component.", -100000.0, 100000.0),
            ),
            (
                "z".to_string(),
                number_schema("Z component.", -100000.0, 100000.0),
            ),
            (
                "rx".to_string(),
                number_schema("X rotation component.", -36000.0, 36000.0),
            ),
            (
                "ry".to_string(),
                number_schema("Y rotation component.", -36000.0, 36000.0),
            ),
            (
                "rz".to_string(),
                number_schema("Z rotation component.", -36000.0, 36000.0),
            ),
            (
                "sx".to_string(),
                number_schema("X scale component.", -10000.0, 10000.0),
            ),
            (
                "sy".to_string(),
                number_schema("Y scale component.", -10000.0, 10000.0),
            ),
            (
                "sz".to_string(),
                number_schema("Z scale component.", -10000.0, 10000.0),
            ),
        ]),
        &[],
    )
}

pub(super) fn color_schema(description: &str) -> Value {
    serde_json::json!({
        "type": "array",
        "description": description,
        "items": {"type": "integer", "minimum": 0, "maximum": 255},
        "minItems": 3,
        "maxItems": 4
    })
}

pub(super) fn semantic_tags_schema() -> Value {
    serde_json::json!({
        "type": "array",
        "description": "Searchable semantic tags.",
        "items": {"type": "string", "maxLength": 64},
        "maxItems": 32
    })
}

pub(super) fn expected_scene_state_schema() -> Value {
    object_schema(
        Map::from_iter([
            (
                "primitive".to_string(),
                string_schema("Expected primitive kind, such as cube or sphere."),
            ),
            ("transform".to_string(), transform_schema()),
            (
                "color_rgba".to_string(),
                color_schema("Expected RGBA channels from 0 to 255."),
            ),
            (
                "parent".to_string(),
                string_schema("Expected parent target, or root."),
            ),
        ]),
        &[],
    )
}

pub(super) fn string_schema(description: &str) -> Value {
    serde_json::json!({"type": "string", "description": description})
}

pub(super) fn boolean_schema(description: &str) -> Value {
    serde_json::json!({"type": "boolean", "description": description})
}

pub(super) fn number_schema(description: &str, minimum: f64, maximum: f64) -> Value {
    serde_json::json!({
        "type": "number",
        "description": description,
        "minimum": minimum,
        "maximum": maximum
    })
}

pub(super) fn integer_schema(description: &str, minimum: impl Into<i64>, maximum: impl Into<i64>) -> Value {
    serde_json::json!({
        "type": "integer",
        "description": description,
        "minimum": minimum.into(),
        "maximum": maximum.into()
    })
}

pub(super) fn enum_schema(description: &str, values: &[&str]) -> Value {
    serde_json::json!({
        "type": "string",
        "description": description,
        "enum": values
    })
}
