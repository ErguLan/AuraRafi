//! MCP tool input and output schema catalog.

use serde_json::{json, Value};

pub(super) fn mcp_semantic_tools() -> Vec<Value> {
    vec![
        mcp_tool(
            "raf_project_snapshot",
            "Read the compact project snapshot: active session, revision, scene identity, workspace and asset counts. Use this before authoring.",
            schema(Vec::new(), &[], false),
        ),
        mcp_tool(
            "raf_scene_outline",
            "Read the bounded scene hierarchy with UUID-first references, paths, parent/child links and compact transforms.",
            schema(
                vec![
                    ("root", json!({"type":"string","description":"Optional root ref, UUID, name or path to limit the outline."})),
                    ("cursor", json!({"type":"integer","minimum":0})),
                    ("limit", json!({"type":"integer","minimum":1,"maximum":128})),
                ],
                &[],
                false,
            ),
        ),
        mcp_tool(
            "raf_scene_query",
            "Find scene entities by name, path, asset, primitive or current selection without scanning project files.",
            schema(
                vec![
                    ("query", json!({"type":"string"})),
                    ("root", json!({"type":"string","description":"Optional root ref, UUID, name or path to limit the query."})),
                    ("primitive", json!({"type":"string","enum":["group","empty","cube","sphere","plane","cylinder", "capsule"]})),
                    ("semantic_role", json!({"type":"string","description":"Exact role or parent role prefix."})),
                    ("tags", json!({"type":"array","items":{"type":"string"},"maxItems":32})),
                    ("match_all_tags", json!({"type":"boolean"})),
                    ("selected_only", json!({"type":"boolean"})),
                    ("include_hidden", json!({"type":"boolean"})),
                    ("cursor", json!({"type":"integer","minimum":0})),
                    ("limit", json!({"type":"integer","minimum":1,"maximum":128})),
                ],
                &[],
                false,
            ),
        ),
        mcp_tool(
            "raf_scene_digest",
            "Read one compact but complete scene snapshot: counts, hierarchy roots, bounds, cameras, component coverage, semantic roles, optional overlaps and a cursor for large scopes.",
            schema(
                vec![
                    ("root", json!({"type":"string"})),
                    ("check_collisions", json!({"type":"boolean"})),
                    ("cursor", json!({"type":"integer","minimum":0})),
                    ("limit", json!({"type":"integer","minimum":1,"maximum":128})),
                ],
                &[],
                false,
            ),
        ),
        mcp_tool(
            "raf_scene_program",
            "Compile a World Program into a persistent plan without mutating the scene. Supports kits, archetypes, grids, paths, scatter, stable keys, budget checks, design preview and overlap evidence. Commit with raf_scene_commit.",
            schema(
                vec![
                    ("mode", json!({"type":"string","enum":["build","reconcile"]})),
                    ("parent", json!({"type":"string"})),
                    ("design_profile", json!({"type":"string","enum":["generic","real_world","building","supermarket","parking","outdoor"]})),
                    ("archetypes", json!({"type":"object"})),
                    ("groups", json!({"type":"array","maxItems":512,"items":{"type":"object"}})),
                    ("entities", json!({"type":"array","maxItems":4096,"items":{"type":"object"}})),
                    ("operations", json!({"type":"array","maxItems":2048,"items":{"type":"object"}})),
                ],
                &[],
                false,
            ),
        ),
        mcp_tool(
            "raf_scene_commit",
            "Commit a compiled World Plan by id after checking the scene fingerprint. Returns a plan-level undo token and structured diff.",
            schema(vec![("plan_id", json!({"type":"string","minLength":1}))], &["plan_id"], true),
        ),
        mcp_tool(
            "raf_scene_patch",
            "Compile a keyed patch against the current world using reconcile semantics. Stable keys update in place, missing keys are created and unmentioned nodes are preserved.",
            schema(
                vec![
                    ("mode", json!({"type":"string","enum":["reconcile"]})),
                    ("parent", json!({"type":"string"})),
                    ("design_profile", json!({"type":"string","enum":["generic","real_world","building","supermarket","parking","outdoor"]})),
                    ("archetypes", json!({"type":"object"})),
                    ("groups", json!({"type":"array","maxItems":512,"items":{"type":"object"}})),
                    ("entities", json!({"type":"array","maxItems":4096,"items":{"type":"object"}})),
                    ("operations", json!({"type":"array","maxItems":2048,"items":{"type":"object"}})),
                ],
                &[],
                false,
            ),
        ),
        mcp_tool(
            "raf_scene_cancel",
            "Cancel an uncommitted World Plan so it cannot be applied later. Committed plans must be reverted with undo.",
            schema(vec![("plan_id", json!({"type":"string","minLength":1}))], &["plan_id"], true),
        ),
        mcp_tool(
            "raf_world_brief",
            "Read or write the persistent World Brief that keeps art direction, scale, gameplay loop, constraints and acceptance criteria coherent across turns.",
            schema(
                vec![
                    ("action", json!({"type":"string","enum":["read","write"]})),
                    ("brief", json!({"type":"object"})),
                    ("content", json!({"type":"string"})),
                ],
                &["action"],
                true,
            ),
        ),
        mcp_tool(
            "raf_world_recipe",
            "Capture a compiled plan as a reusable recipe, list and inspect recipes, or instantiate one as a fresh reviewable plan.",
            schema(
                vec![
                    ("action", json!({"type":"string","enum":["save","list","show","delete","apply"]})),
                    ("name", json!({"type":"string","minLength":1})),
                    ("plan_id", json!({"type":"string"})),
                    ("description", json!({"type":"string"})),
                    ("tags", json!({"type":"array","items":{"type":"string"},"maxItems":32})),
                ],
                &["action"],
                true,
            ),
        ),
        mcp_tool(
            "raf_world_script",
            "Run a sandboxed Rhai authoring script that can only emit a World Program, then compile it into a reviewable plan without mutating the scene.",
            schema(
                vec![
                    ("script", json!({"type":"string","description":"Rhai source using only the world:: authoring namespace."})),
                    ("file", json!({"type":"string","description":"Project-relative .rhai file."})),
                ],
                &[],
                true,
            ),
        ),
        mcp_tool(
            "raf_scene_visual_diff",
            "Compare before/after project images, report changed ratio and bounds, and return an annotated PNG diff artifact.",
            schema(
                vec![
                    ("before", json!({"type":"string"})),
                    ("after", json!({"type":"string"})),
                    ("output", json!({"type":"string"})),
                    ("threshold", json!({"type":"number","minimum":0,"maximum":255})),
                    ("save", json!({"type":"boolean"})),
                ],
                &["before", "after"],
                false,
            ),
        ),
        mcp_tool(
            "raf_scene_spatial_map",
            "Read bounded world-space bounds, scene extents, visible renderable entities and conservative overlaps before layout changes.",
            schema(
                vec![
                    ("root", json!({"type":"string","description":"Optional root ref, UUID, name or path to limit the map."})),
                    ("include_hidden", json!({"type":"boolean"})),
                    ("check_collisions", json!({"type":"boolean"})),
                    ("cursor", json!({"type":"integer","minimum":0})),
                    ("limit", json!({"type":"integer","minimum":1,"maximum":128})),
                ],
                &[],
                false,
            ),
        ),
        mcp_tool(
            "raf_scene_overlaps",
            "Check bounded world-space overlaps and return penetration plus deterministic scene_snap repair suggestions.",
            schema(
                vec![
                    ("root", json!({"type":"string"})),
                    ("include_hidden", json!({"type":"boolean"})),
                    ("margin", json!({"type":"number","minimum":0})),
                    ("max_pairs", json!({"type":"integer","minimum":1,"maximum":256})),
                ],
                &[],
                false,
            ),
        ),
        mcp_tool(
            "raf_scene_navigation_audit",
            "Static walkability and reachability audit on authored AABBs. Samples a bounded grid, expands obstacles by an agent radius and BFS-checks spawn-to-goal reachability without Play or Runtime.",
            schema(
                vec![
                    ("root", json!({"type":"string"})),
                    ("spawn", json!({"type":"string"})),
                    ("goals", json!({"type":"array","items":{"type":"string"},"maxItems":32})),
                    ("resolution", json!({"type":"number","minimum":0.1,"maximum":8})),
                    ("agent_radius", json!({"type":"number","minimum":0,"maximum":8})),
                ],
                &[],
                false,
            ),
        ),
        mcp_tool(
            "raf_scene_diff",
            "Read retained created, updated and deleted scene UUIDs since a known revision.",
            schema(
                vec![("from_revision", json!({"type":"integer","minimum":0}))],
                &[],
                false,
            ),
        ),
        mcp_tool(
            "raf_scene_design_audit",
            "Audit whether a bounded scene scope reads as an intentional real-world place. This is read-only evidence for the next repair step.",
            schema(
                vec![
                    ("root", json!({"type":"string"})),
                    ("design_profile", json!({"type":"string","enum":["generic","real_world","building","store","supermarket","parking","outdoor"]})),
                    ("required_features", json!({"type":"array","items":{"type":"string","enum":["floor","enclosure","entrance","circulation","roof","primary_modules","details"]},"maxItems":7})),
                ],
                &[],
                false,
            ),
        ),
        mcp_tool(
            "raf_scene_inspect",
            "Inspect one entity by ref, UUID, path or exact name; selection is the fallback when target is omitted.",
            schema(
                vec![
                    ("target", json!({"type":"string"})),
                    ("targets", json!({"type":"array","items":{"type":"string"},"maxItems":64})),
                ],
                &[],
                false,
            ),
        ),
        mcp_tool(
            "raf_selection_get",
            "Read the live scene selection with UUID-first references and compact entity details.",
            schema(Vec::new(), &[], false),
        ),
        mcp_tool(
            "raf_viewport_capture",
            "Capture the attached Game scene as a PNG image for visual inspection. Use view=current for the last editor frame or front/back/left/right/top/bottom/isometric/custom for a framed angle. Targets default to the selection; frame=scene fits the whole scene. The MCP result includes the PNG as image content when the project artifact is available. This never starts Play mode.",
            schema(
                vec![
                    ("refresh", json!({"type":"boolean"})),
                    ("preset", json!({"type":"string","enum":["single","contact_sheet","top_down_map"]})),
                    ("contact_sheet", json!({"type":"boolean"})),
                    ("top_down", json!({"type":"boolean"})),
                    ("overlays", json!({"type":"array","items":{"type":"string","enum":["all","bounds","colliders","grid","labels"]},"maxItems":5})),
                    ("grid", json!({"type":"number","exclusiveMinimum":0})),
                    ("view", json!({"type":"string","enum":["current","front","back","left","right","top","bottom","isometric","custom"]})),
                    ("frame", json!({"type":"string","enum":["selection","scene"]})),
                    ("target", json!({"type":"string"})),
                    ("targets", json!({"type":"array","items":{"type":"string"},"maxItems":64})),
                    ("yaw_deg", json!({"type":"number"})),
                    ("pitch_deg", json!({"type":"number"})),
                    ("distance", json!({"type":"number","exclusiveMinimum":0})),
                    ("padding", json!({"type":"number","minimum":1,"maximum":3})),
                    ("width", json!({"type":"integer","minimum":128,"maximum":1024})),
                    ("height", json!({"type":"integer","minimum":128,"maximum":1024})),
                    ("highlight_selection", json!({"type":"boolean"})),
                    ("label", json!({"type":"string"})),
                ],
                &[],
                false,
            ),
        ),
        mcp_tool(
            "raf_scene_top_down_map",
            "Render a metric top-down XZ schematic of the attached Game scene with bounds, colliders, grid and labels. Returns the PNG plus structured entity/collider/bounds metadata. This never starts Play or Runtime.",
            schema(
                vec![
                    ("root", json!({"type":"string"})),
                    ("width", json!({"type":"integer","minimum":256,"maximum":2048})),
                    ("height", json!({"type":"integer","minimum":256,"maximum":2048})),
                    ("overlays", json!({"type":"array","items":{"type":"string","enum":["all","bounds","colliders","grid","labels"]},"maxItems":5})),
                    ("grid", json!({"type":"number","exclusiveMinimum":0})),
                    ("label", json!({"type":"string"})),
                ],
                &[],
                false,
            ),
        ),
        mcp_tool(
            "raf_scene_visual_review",
            "Ask the configured vision-capable reviewer model to inspect one to four project captures and return structured scores, findings and fixes. Returns a structured vision_unavailable fallback when no vision model is configured; it never starts Play or Runtime.",
            schema(
                vec![
                    ("images", json!({"type":"array","items":{"type":"string"},"maxItems":4})),
                    ("image", json!({"type":"string"})),
                    ("focus", json!({"type":"string"})),
                    ("brief", json!({"type":"object"})),
                ],
                &[],
                false,
            ),
        ),
        mcp_tool(
            "raf_task_list",
            "List the current native Agent task in the attached editor.",
            schema(Vec::new(), &[], false),
        ),
        mcp_tool(
            "raf_task_get",
            "Read one native Agent task snapshot by id.",
            schema(vec![("id", json!({"type":"string","minLength":1}))], &["id"], false),
        ),
        mcp_tool(
            "raf_task_events",
            "Read retained native Agent task progress events after a sequence.",
            schema(vec![("since", json!({"type":"integer","minimum":0}))], &[], false),
        ),
        mcp_tool(
            "raf_task_cancel",
            "Request cooperative cancellation of the active native Agent task.",
            schema(vec![("id", json!({"type":"string","minLength":1}))], &["id"], false),
        ),
        mcp_tool(
            "raf_asset_inspect",
            "Inspect one imported asset by path and report kind, usage and scene references.",
            schema(
                vec![
                    ("target", json!({"type":"string","minLength":1})),
                ],
                &["target"],
                false,
            ),
        ),
        mcp_tool(
            "raf_assets_search",
            "Search the project asset catalog and report whether each asset is used by the live scene.",
            schema(
                vec![
                    ("query", json!({"type":"string"})),
                    ("usage", json!({"type":"string","enum":["all","used","unused"]})),
                    ("kind", json!({"type":"string","enum":["image","model","audio","script","data","file"]})),
                    ("cursor", json!({"type":"integer","minimum":0})),
                    ("limit", json!({"type":"integer","minimum":1,"maximum":128})),
                ],
                &[],
                false,
            ),
        ),
        mcp_tool(
            "raf_assets_recommend",
            "Rank imported project assets for an authoring intent and explain every match.",
            schema(
                vec![
                    ("intent", json!({"type":"string","minLength":1})),
                    ("kind", json!({"type":"string","enum":["image","model","audio","script","data","file"]})),
                    ("limit", json!({"type":"integer","minimum":1,"maximum":24})),
                ],
                &["intent"],
                false,
            ),
        ),
        mcp_tool(
            "raf_scripts_search",
            "List scripts attached to scene entities, bounded and searchable by path.",
            schema(
                vec![
                    ("query", json!({"type":"string"})),
                    ("cursor", json!({"type":"integer","minimum":0})),
                    ("limit", json!({"type":"integer","minimum":1,"maximum":128})),
                ],
                &[],
                false,
            ),
        ),
        mcp_tool(
            "raf_script_list",
            "List project scripts and attached scene references without starting Play.",
            schema(
                vec![
                    ("query", json!({"type":"string"})),
                    ("cursor", json!({"type":"integer","minimum":0})),
                    ("limit", json!({"type":"integer","minimum":1,"maximum":128})),
                ],
                &[],
                false,
            ),
        ),
        mcp_tool(
            "raf_script_create",
            "Create a script file from the project template. Use raf_script_write when you already have complete source.",
            schema(
                vec![
                    ("lang", json!({"type":"string","enum":["rhai","cpp"]})),
                    ("name", json!({"type":"string"})),
                ],
                &["name"],
                true,
            ),
        ),
        mcp_tool(
            "raf_script_write",
            "Validate and write complete Rhai gameplay source into the project scripts folder. The source is compiled before disk replacement; overwrite is explicit.",
            schema(
                vec![
                    ("file", json!({"type":"string","description":"Project-relative .rhai path, for example scripts/player.rhai."})),
                    ("content", json!({"type":"string","description":"Complete Rhai source."})),
                    ("overwrite", json!({"type":"boolean"})),
                ],
                &["file","content"],
                true,
            ),
        ),
        mcp_tool(
            "raf_script_attach",
            "Attach a project script to a scene entity.",
            schema(
                vec![
                    ("file", json!({"type":"string"})),
                    ("entity", json!({"type":"string"})),
                ],
                &["file","entity"],
                true,
            ),
        ),
        mcp_tool(
            "raf_script_validate",
            "Compile and validate a project script without starting Play.",
            schema(vec![("file", json!({"type":"string"}))], &["file"], false),
        ),
        mcp_tool(
            "raf_script_run",
            "Run one script on a cloned scene without starting Play. Optional entity keeps that entity as the behavior owner and preserves its typed parameters. Optional ticks and keys exercise update, fixed-update, bounded physics and late-update with synthetic input. The editor document is never mutated.",
            schema(
                vec![
                    ("file", json!({"type":"string","description":"Project-relative .rhai path."})),
                    ("entity", json!({"type":"string","description":"Optional owner reference; defaults to a temporary __script_run__ root."})),
                    ("ticks", json!({"type":"integer","minimum":0,"maximum":600})),
                    ("keys", json!({"type":"array","items":{"type":"string"},"maxItems":16,"description":"Held key names such as w, space, shift or arrow_left."})),
                ],
                &["file"],
                false,
            ),
        ),
        mcp_tool(
            "raf_behavior_attach",
            "Attach an authored behavior to an entity through the shared behavior kernel.",
            schema(
                vec![
                    ("target", json!({"type":"string"})),
                    ("source", json!({"type":"string"})),
                ],
                &["target","source"],
                true,
            ),
        ),
        mcp_tool(
            "raf_lighting_inspect",
            "Read the authored Sun, Sky, Shadow, Clock and technology lighting values.",
            schema(Vec::new(), &[], false),
        ),
        mcp_tool(
            "raf_lighting_set",
            "Set validated lighting properties atomically. Use raf_lighting_inspect first for supported paths and bounds.",
            schema(
                vec![
                    ("patch", json!({"type":"object","maxProperties":20})),
                    ("property", json!({"type":"string"})),
                    ("value", json!({})),
                ],
                &[],
                true,
            ),
        ),
        mcp_tool(
            "raf_lighting_reset",
            "Reset one authored lighting property, or all properties when omitted.",
            schema(vec![("property", json!({"type":"string"}))], &[], true),
        ),
        mcp_tool(
            "raf_game_completion_audit",
            "Audit whether the attached Game foundation is playable rather than geometry alone: renderable geometry, camera/startup camera, lighting, attached scripts, gameplay signals, enabled systems, missing pieces, and next actions.",
            schema(Vec::new(), &[], false),
        ),
        mcp_tool(
            "raf_project_health",
            "Check a bounded scene scope for identity, duplicate names, hierarchy reachability and missing asset references. Use root to avoid unrelated legacy scenes.",
            schema(
                vec![
                    ("root", json!({"type":"string"})),
                    ("strict", json!({"type":"boolean"})),
                ],
                &[],
                false,
            ),
        ),
        mcp_tool(
            "raf_diagnostics",
            "Run the compact project health diagnostics used before and after authoring.",
            schema(Vec::new(), &[], false),
        ),
        mcp_tool(
            "raf_scene_verify",
            "Verify targets, hierarchy, transforms, expected counts and optional AABB collisions after a mutation. Scope checks with root and never infer success from model text alone.",
            schema(
                vec![
                    ("root", json!({"type":"string"})),
                    ("target", json!({"type":"string"})),
                    ("targets", json!({"type":"array","items":{"type":"string"}})),
                    ("expected_count", json!({"type":"integer","minimum":0})),
                    ("expected_names", json!({"type":"array","items":{"type":"string"},"maxItems":128})),
                    ("expected", json!({
                        "type":"object",
                        "properties": {
                            "primitive": {"type":"string","enum":["empty","cube","sphere","plane","cylinder", "capsule","group"]},
                            "transform": transform_schema(),
                            "color_rgba": {"type":"array","minItems":3,"maxItems":4,"items":{"type":"integer","minimum":0,"maximum":255}},
                            "parent": {"type":"string"}
                        },
                        "additionalProperties": false
                    })),
                    ("check_collisions", json!({"type":"boolean"})),
                    ("strict", json!({"type":"boolean"})),
                ],
                &[],
                false,
            ),
        ),
        mcp_tool(
            "raf_transaction_undo",
            "Roll back the exact attached Game scene mutation represented by a scoped undo token.",
            schema(
                vec![("token", json!({"type":"string","minLength":1}))],
                &["token"],
                true,
            ),
        ),
        mcp_tool(
            "raf_scene_create",
            "Create one semantic scene entity through the shared game command kernel. Use a stable parent ref when possible.",
            schema(scene_create_properties(), &["name"], true),
        ),
        mcp_tool(
            "raf_scene_update",
            "Update one scene entity by stable ref, UUID, path or exact name. Unspecified transform fields remain unchanged.",
            schema(scene_update_properties(), &["target"], true),
        ),
        mcp_tool(
            "raf_scene_reparent",
            "Move an existing scene subtree to a new group while preserving its world transform by default.",
            schema(
                vec![
                    ("target", json!({"type":"string"})),
                    ("parent", json!({"type":"string"})),
                    ("preserve_world", json!({"type":"boolean"})),
                ],
                &["target"],
                true,
            ),
        ),
        mcp_tool(
            "raf_scene_duplicate",
            "Duplicate a scene subtree into an optional parent or repeated offset array.",
            schema(
                vec![
                    ("target", json!({"type":"string"})),
                    ("parent", json!({"type":"string"})),
                    ("name", json!({"type":"string"})),
                    ("offset", json!({"type":"array","minItems":3,"maxItems":3,"items":{"type":"number"}})),
                    ("count", json!({"type":"integer","minimum":1,"maximum":128})),
                    ("axis", json!({"type":"string","enum":["x","y","z"]})),
                    ("spacing", json!({"type":"number"})),
                    ("preserve_world", json!({"type":"boolean"})),
                ],
                &["target"],
                true,
            ),
        ),
        mcp_tool(
            "raf_scene_snap",
            "Snap an entity to the grid, world floor, or another entity's world bounds.",
            schema(
                vec![
                    ("target", json!({"type":"string"})),
                    ("mode", json!({"type":"string","enum":["grid","floor","surface"]})),
                    ("snap_to", json!({"type":"string"})),
                    ("axis", json!({"type":"string","enum":["x","y","z"]})),
                    ("placement", json!({"type":"string","enum":["before","after","center"]})),
                    ("gap", json!({"type":"number","minimum":0})),
                    ("grid", json!({"type":"number","exclusiveMinimum":0})),
                ],
                &["target", "mode"],
                true,
            ),
        ),
        mcp_tool(
            "raf_scene_instantiate_template",
            "Instantiate one or more registered native scene templates with parent and transform controls.",
            schema(
                vec![
                    ("kind", json!({"type":"string"})),
                    ("name", json!({"type":"string"})),
                    ("parent", json!({"type":"string"})),
                    ("transform", transform_schema()),
                    ("count", json!({"type":"integer","minimum":1,"maximum":64})),
                    ("axis", json!({"type":"string","enum":["x","y","z"]})),
                    ("spacing", json!({"type":"number"})),
                ],
                &["kind"],
                true,
            ),
        ),
        mcp_tool(
            "raf_scene_build",
            "Apply an ordered semantic scene build through the shared kernel. For real-world places, build the named envelope, floor, boundaries, entrances and circulation before repeated modules and details; use stable_key and semantic_role. Prefer this for multi-entity construction and use dry_run first.",
            schema(
                vec![
                    ("parent", json!({"type":"string","description":"Optional existing group that receives the generated structure."})),
                    ("design_profile", json!({"type":"string","enum":["generic","real_world","building","supermarket","parking","outdoor"],"description":"Optional contract that rejects incomplete real-world structure before mutation."})),
                    ("groups", json!({"type":"array","maxItems":32,"items":scene_group_input_schema()})),
                    ("entities", json!({"type":"array","maxItems":128,"items":scene_entity_input_schema()})),
                ],
                &[],
                true,
            ),
        ),
        mcp_tool(
            "raf_scene_reconcile",
            "Apply a keyed desired scene state. Existing stable_key values are updated instead of duplicated; use dry_run first.",
            scene_reconcile_input_schema(),
        ),
        mcp_tool(
            "raf_scene_repair",
            "Apply explicit audit-driven scene repairs atomically inside a bounded scope. The server never invents geometry from prose; use the audit and spatial map first.",
            schema(
                vec![
                    ("root", json!({"type":"string","description":"Optional audited root ref, UUID, name or path."})),
                    ("operations", json!({"type":"array","minItems":1,"maxItems":512,"items":batch_operation_input_schema()})),
                ],
                &["operations"],
                true,
            ),
        ),
        mcp_tool(
            "raf_scene_plan",
            "Preview a semantic scene build without mutating the editor. The server forces dry_run=true.",
            schema(
                vec![
                    ("parent", json!({"type":"string","description":"Optional existing group that receives the generated structure."})),
                    ("design_profile", json!({"type":"string","enum":["generic","real_world","building","supermarket","parking","outdoor"],"description":"Optional contract used by the preview validator."})),
                    ("groups", json!({"type":"array","maxItems":32,"items":scene_group_input_schema()})),
                    ("entities", json!({"type":"array","maxItems":128,"items":scene_entity_input_schema()})),
                ],
                &[],
                true,
            ),
        ),
        mcp_tool(
            "raf_scene_apply",
            "Apply a previously planned semantic scene build through the shared kernel after approval.",
            schema(
                vec![
                    ("parent", json!({"type":"string","description":"Optional existing group that receives the generated structure."})),
                    ("design_profile", json!({"type":"string","enum":["generic","real_world","building","supermarket","parking","outdoor"],"description":"Optional contract checked before applying the planned structure."})),
                    ("groups", json!({"type":"array","maxItems":32,"items":scene_group_input_schema()})),
                    ("entities", json!({"type":"array","maxItems":128,"items":scene_entity_input_schema()})),
                ],
                &[],
                true,
            ),
        ),
        mcp_tool(
            "raf_scene_batch",
            "Apply a bounded ordered batch of semantic scene operations atomically; use only for operations that belong together.",
            schema(
                vec![("operations", json!({"type":"array","minItems":1,"maxItems":512,"items":batch_operation_input_schema()}))],
                &["operations"],
                true,
            ),
        ),
    ]
}

fn scene_create_properties() -> Vec<(&'static str, Value)> {
    vec![
        ("name", json!({"type":"string","minLength":1})),
        (
            "kind",
            json!({"type":"string","enum":["empty","cube","sphere","plane","cylinder", "capsule"]}),
        ),
        (
            "primitive",
            json!({"type":"string","enum":["empty","cube","sphere","plane","cylinder","capsule"]}),
        ),
        ("parent", json!({"type":"string"})),
        ("transform", transform_schema()),
        ("position", vec3_schema()),
        ("rotation_deg", vec3_schema()),
        ("scale", vec3_schema()),
        ("size", vec3_schema()),
        (
            "color_rgba",
            json!({"type":"array","minItems":3,"maxItems":4,"items":{"type":"integer","minimum":0,"maximum":255}}),
        ),
        (
            "visible",
            json!({"type":"boolean","description":"Entity visibility."}),
        ),
        (
            "locked",
            json!({"type":"boolean","description":"Editor lock state."}),
        ),
        ("semantic_role", json!({"type":"string"})),
        ("stable_key", json!({"type":"string"})),
        (
            "tags",
            json!({"type":"array","maxItems":32,"items":{"type":"string","maxLength":64}}),
        ),
        ("agent_origin", json!({"type":"string"})),
    ]
}

fn scene_update_properties() -> Vec<(&'static str, Value)> {
    let mut properties = scene_create_properties();
    properties.retain(|(name, _)| {
        *name != "name" && *name != "kind" && *name != "primitive" && *name != "parent"
    });
    properties.insert(0, ("target", json!({"type":"string","minLength":1})));
    properties.push(("animation", json!({"type":["object","null"],"description":"Replace the transform rig with layers and tracks, or remove it with null."})));
    properties.push(("animation_clip", json!({"type":"object","description":"Append an embedded GLB clip. Specify asset, clip index and bindings from GLB node indices to child-name paths."})));
    properties.push(("animation_asset", json!({"type":["string","null"],"description":"Link a project-relative .animation.json asset, or unlink with null."})));
    properties
}

fn scene_group_input_schema() -> Value {
    schema(
        vec![
            ("name", json!({"type":"string","minLength":1})),
            ("parent", json!({"type":"string"})),
            ("transform", transform_schema()),
            ("semantic_role", json!({"type":"string"})),
            ("stable_key", json!({"type":"string"})),
            (
                "tags",
                json!({"type":"array","maxItems":32,"items":{"type":"string","maxLength":64}}),
            ),
            ("agent_origin", json!({"type":"string"})),
        ],
        &["name"],
        false,
    )
}

fn scene_entity_input_schema() -> Value {
    schema(scene_create_properties(), &["name", "kind"], false)
}

fn scene_reconcile_input_schema() -> Value {
    let mut schema = schema(
        vec![
            (
                "parent",
                json!({"type":"string","description":"Optional existing group that receives the keyed structure."}),
            ),
            (
                "design_profile",
                json!({"type":"string","enum":["generic","real_world","building","supermarket","parking","outdoor"],"description":"Optional contract that rejects incomplete real-world structure before reconciliation."}),
            ),
            (
                "groups",
                json!({"type":"array","maxItems":32,"items":scene_reconcile_group_input_schema()}),
            ),
            (
                "entities",
                json!({"type":"array","maxItems":128,"items":scene_reconcile_entity_input_schema()}),
            ),
        ],
        &[],
        true,
    );
    schema["description"] = Value::String(
        "Keyed desired scene state. Existing stable_key values are updated instead of duplicated."
            .to_string(),
    );
    schema["properties"]["groups"]["items"]["required"] = json!(["name", "stable_key"]);
    schema["properties"]["entities"]["items"]["required"] = json!(["kind", "name", "stable_key"]);
    schema
}

fn scene_reconcile_group_input_schema() -> Value {
    let mut value = scene_group_input_schema();
    value["required"] = json!(["name", "stable_key"]);
    value
}

fn scene_reconcile_entity_input_schema() -> Value {
    let mut value = scene_entity_input_schema();
    value["required"] = json!(["kind", "name", "stable_key"]);
    value
}

fn batch_operation_input_schema() -> Value {
    schema(
        vec![
            (
                "name",
                json!({
                    "type":"string",
                    "enum":["scene_create","scene_create_group","scene_update","scene_delete","scene_duplicate","scene_arrange","scene_reparent","scene_snap","scene_instantiate_template"]
                }),
            ),
            ("params", batch_operation_params_schema()),
        ],
        &["name", "params"],
        false,
    )
}

/// Keep nested batch operations useful to external models too. A generic
/// `params: object` tells a provider nothing about transforms, targets or
/// semantic identity, which is exactly where tool calls used to drift.
fn batch_operation_params_schema() -> Value {
    let mut properties = scene_create_properties();
    properties.extend([
        ("target", json!({"type":"string","description":"Entity or group target."})),
        ("preserve_world", json!({"type":"boolean","description":"Keep world transform during reparenting."})),
        ("parent", json!({"type":"string","description":"Optional destination parent."})),
        ("name", json!({"type":"string","description":"Optional generated name."})),
        ("offset", json!({"type":"array","minItems":3,"maxItems":3,"items":{"type":"number"},"description":"World/local offset for repeated operations."})),
        ("count", json!({"type":"integer","minimum":1,"maximum":128})),
        ("axis", json!({"type":"string","enum":["x","y","z"]})),
        ("spacing", json!({"type":"number","minimum":-10000.0,"maximum":10000.0,"description":"Grid or repetition spacing."})),
        ("mode", json!({"type":"string","enum":["grid","floor","surface"]})),
        ("snap_to", json!({"type":"string","description":"Surface target for scene_snap."})),
        ("placement", json!({"type":"string","enum":["before","after","center"]})),
        ("gap", json!({"type":"number","minimum":0})),
        ("grid", json!({"type":"number","exclusiveMinimum":0})),
        ("factor", json!({"type":"number","minimum":-10000.0,"maximum":10000.0,"description":"Scale factor."})),
        ("x", json!({"type":"number","minimum":-100000.0,"maximum":100000.0})),
        ("y", json!({"type":"number","minimum":-100000.0,"maximum":100000.0})),
        ("z", json!({"type":"number","minimum":-100000.0,"maximum":100000.0})),
        ("rx", json!({"type":"number","minimum":-36000.0,"maximum":36000.0})),
        ("ry", json!({"type":"number","minimum":-36000.0,"maximum":36000.0})),
        ("rz", json!({"type":"number","minimum":-36000.0,"maximum":36000.0})),
        ("sx", json!({"type":"number","minimum":-10000.0,"maximum":10000.0})),
        ("sy", json!({"type":"number","minimum":-10000.0,"maximum":10000.0})),
        ("sz", json!({"type":"number","minimum":-10000.0,"maximum":10000.0})),
    ]);
    schema(properties, &[], false)
}

fn transform_schema() -> Value {
    json!({
        "type":"object",
        "properties": {"position": vec3_schema(), "rotation_deg": vec3_schema(), "scale": vec3_schema()},
        "additionalProperties": false
    })
}

fn vec3_schema() -> Value {
    json!({"type":"array","minItems":3,"maxItems":3,"items":{"type":"number"}})
}

pub(super) fn schema(properties: Vec<(&str, Value)>, required: &[&str], controls: bool) -> Value {
    let mut all = serde_json::Map::new();
    for (name, property) in properties {
        all.insert(name.to_string(), property);
    }
    if controls {
        all.insert("confirm".to_string(), json!({"type":"boolean"}));
        all.insert("dry_run".to_string(), json!({"type":"boolean"}));
        all.insert(
            "expected_revision".to_string(),
            json!({"type":"integer","minimum":0}),
        );
        all.insert("idempotency_key".to_string(), json!({"type":"string"}));
        all.insert("session".to_string(), json!({"type":"string"}));
        all.insert("budget".to_string(), budget_schema());
    }
    let mut result = json!({"type":"object","properties":all,"additionalProperties":false});
    if !required.is_empty() {
        result["required"] = json!(required);
    }
    result
}

pub(super) fn budget_schema() -> Value {
    json!({
        "type": "object",
        "description": "Bound the agent operation and result before execution.",
        "properties": {
            "max_tool_calls": {"type":"integer","minimum":1,"maximum":10000},
            "max_milliseconds": {"type":"integer","minimum":1,"maximum":3_600_000},
            "max_scene_operations": {"type":"integer","minimum":1,"maximum":10000},
            "max_scene_entities": {"type":"integer","minimum":1,"maximum":1000000},
            "max_result_bytes": {"type":"integer","minimum":256,"maximum":1_048_576},
            "profile": {"type":"string","enum":["safe","balanced","large"]}
        },
        "additionalProperties": false
    })
}

pub(super) fn mcp_tool(name: &str, description: &str, input_schema: Value) -> Value {
    let read_only = [
        "_status", "_context", "_info", "_path", "_list", "_outline", "_digest", "_query",
        "_snapshot", "_spatial", "_overlaps", "_diff", "_audit", "_inspect", "_selection",
        "_capture", "_get", "_events", "_preview", "_plan", "_program", "_verify", "_health",
        "_review", "_map",
        "_capabilities", "_doctor", "_assets", "_scripts", "_task_get", "_task_events",
        "_lighting_inspect",
    ]
    .iter()
    .any(|suffix| name.contains(suffix));
    let destructive = ["_delete", "_undo", "_commit", "_repair", "_close", "_cancel"]
        .iter()
        .any(|suffix| name.contains(suffix));
    let idempotent = ["_plan", "_program", "_save", "_reconcile", "_commit", "_verify"]
        .iter()
        .any(|suffix| name.contains(suffix));
    json!({
        "name": name,
        "description": description,
        "inputSchema": input_schema,
        "outputSchema": {
            "type": "object",
            "properties": {
                "ok": {"type": "boolean"},
                "summary": {"type": "string"},
                "data": {"type": "object"},
                "references": {"type": "array", "items": {"type": "string"}},
                "changed": {"type": "boolean"},
                "revision": {"type": "integer"},
                "warnings": {"type": "array", "items": {"type": "string"}},
                "verification": {"type": "object"},
                "artifacts": {"type": "array", "items": {"type": "object"}}
            }
        },
        "annotations": {
            "readOnlyHint": read_only && !destructive,
            "destructiveHint": destructive,
            "idempotentHint": idempotent,
            "openWorldHint": false
        }
    })
}
