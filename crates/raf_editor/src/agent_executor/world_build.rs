//! World, plan, and scene build request handling.

use raf_ai::agent_runtime::{AgentToolKind, AgentToolResult, ToolExecutionMode};
use serde_json::{Map, Value};

use crate::commands::game::{self, GameCommandContext, GameViewportPort};
use crate::commands::parser::ParsedCommand;

use super::evidence::command_output_to_tool_result;
use super::intent::PromptIntent;
use super::schemas::{
    batch_operation_params_schema, boolean_schema, color_schema, enum_schema, integer_schema,
    light_color_schema, light_schema, mesh_asset_schema, number_schema, object_schema,
    pivot_schema, scene_build_schema, scene_reconcile_schema, semantic_tags_schema, string_schema,
    surface_schema, transform_schema, vec3_schema, PRIMITIVE_KINDS,
};
use super::{AgentToolExecutor, AgentToolPack};

impl AgentToolExecutor<'_> {
    pub(super) fn execute_world_brief(
        &mut self,
        arguments: &Value,
        mode: ToolExecutionMode,
    ) -> Result<AgentToolResult, String> {
        let action = arguments
            .get("action")
            .and_then(Value::as_str)
            .unwrap_or("read")
            .to_ascii_lowercase();
        if action == "write" && mode != ToolExecutionMode::Apply {
            let mut result = AgentToolResult::success(
                "World brief preview; no project file was written.",
                serde_json::json!({
                    "ok": true,
                    "preview": true,
                    "action": "write",
                    "brief": arguments.get("brief").cloned().or_else(|| arguments.get("content").cloned()),
                }),
            );
            result.preview = true;
            return Ok(result);
        }
        let _root_guard = crate::commands::game::bind_project_root(
            self.project.as_ref().map(|project| project.root.clone()),
        );
        let parsed = ParsedCommand {
            raw: "/world.brief".to_string(),
            name: "world.brief".to_string(),
            args: std::collections::BTreeMap::new(),
            positional: Vec::new(),
            structured_args: Some(arguments.to_owned()),
        };
        let mut context = GameCommandContext {
            scene: self.scene,
            selection: self.selection,
            viewport: self.viewport as &mut dyn GameViewportPort,
        };
        let output = game::execute("world.brief", &parsed, &mut context);
        let file_changed = output.changed;
        let mut result = command_output_to_tool_result(output);
        // The brief is a project file, not a scene mutation. Do not make
        // the durable executor save the scene document for this write.
        result.changed = false;
        result.data["file_changed"] = Value::Bool(file_changed);
        result.revision = Some(self.ledger.revision());
        Ok(result)
    }
}
pub(super) fn add_game_program_tools(pack: &mut AgentToolPack) {
    pack.push_native(
        "scene_camera",
        "Create, configure, assign, activate or remove a Game camera component. Use action=create for a new camera, action=add/set for an existing entity, action=activate or look_through to make it the active view, and action=remove to detach the component. This is the tool that completes game_completion_audit camera checks.",
        object_schema(
            Map::from_iter([
                (
                    "action".to_string(),
                    enum_schema(
                        "Camera action.",
                        &["create", "add", "set", "remove", "activate", "look_through"],
                    ),
                ),
                ("target".to_string(), string_schema("Entity target for add/set/remove/activate.")),
                ("name".to_string(), string_schema("Name for a new camera entity.")),
                ("parent".to_string(), string_schema("Optional parent for a new camera.")),
                ("fov".to_string(), number_schema("Vertical field of view in degrees.", 1.0, 179.0)),
                ("near".to_string(), number_schema("Near plane.", 0.0001, 100_000.0)),
                ("far".to_string(), number_schema("Far plane.", 0.0001, 100_000.0)),
                ("ortho_scale".to_string(), number_schema("Orthographic scale.", 0.0001, 100_000.0)),
                ("orthographic".to_string(), boolean_schema("Use an orthographic projection.")),
            ]),
            &["action"],
        ),
        AgentToolKind::Mutation,
        Some("game.camera"),
    );
    pack.push_native(
        "camera_activate",
        "Make an existing camera entity the active Game camera and use it as the look-through viewpoint. The camera must already exist or be created with scene_camera action=create. This does not start Play.",
        object_schema(
            Map::from_iter([(
                "target".to_string(),
                string_schema("Camera entity target by stable ID, UUID, name, or path."),
            )]),
            &["target"],
        ),
        AgentToolKind::Mutation,
        Some("game.camera"),
    );
    let component = object_schema(
        Map::from_iter([
            ("target".to_string(), string_schema("Entity target.")),
            (
                "texture".to_string(),
                string_schema("Project-relative base-color texture path, for example assets/generated/wall.png."),
            ),
            (
                "collider".to_string(),
                serde_json::json!({
                    "type": ["string", "object"],
                    "description": "none, aabb, convex_hull or mesh; or an object with type, offset and visible_in_editor."
                }),
            ),
            (
                "rigid_body".to_string(),
                serde_json::json!({
                    "type": "object",
                    "description": "enabled, body_type (static/dynamic/kinematic), use_gravity, velocity, mass, damping, is_trigger."
                }),
            ),
            (
                "audio".to_string(),
                serde_json::json!({
                    "type": "object",
                    "description": "enabled, clip (project audio path), autoplay, looping, volume, bus, spatial, min_distance, max_distance and rolloff. Spatial audio follows the entity world transform."
                }),
            ),
            (
                "camera".to_string(),
                serde_json::json!({
                    "type": "object",
                    "description": "orthographic, fov_degrees, near, far and ortho_scale."
                }),
            ),
            ("light".to_string(), light_schema()),
            ("mesh_asset".to_string(), mesh_asset_schema()),
            ("surface".to_string(), surface_schema()),
            ("pivot".to_string(), pivot_schema()),
            (
                "scripts".to_string(),
                serde_json::json!({"type":"array","items":{"type":"string"},"maxItems":32}),
            ),
            (
                "variables".to_string(),
                serde_json::json!({"type":"array","maxItems":64}),
            ),
        ]),
        &["target"],
    );
    pack.push_native(
        "scene_component",
        "Set Inspector-level components on an entity: base-color texture or surface, 3D mesh asset, point/spot light, collider, rigid body, audio source, camera lens, attached scripts or variables. This is required for gameplay-ready objects, not just visible primitives.",
        component,
        AgentToolKind::Mutation,
        Some("game.update"),
    );
    let program_schema = object_schema(
        Map::from_iter([
            (
                "mode".to_string(),
                enum_schema(
                    "Plan mode. Reconcile updates stable keys in place.",
                    &["build", "reconcile"],
                ),
            ),
            (
                "parent".to_string(),
                string_schema("Optional existing group that receives the program."),
            ),
            (
                "design_profile".to_string(),
                enum_schema(
                    "Optional design contract.",
                    &[
                        "generic",
                        "real_world",
                        "building",
                        "supermarket",
                        "parking",
                        "outdoor",
                    ],
                ),
            ),
            (
                "archetypes".to_string(),
                serde_json::json!({"type":"object","description":"Named kits. Each kit has a parts array of primitive entities."}),
            ),
            (
                "groups".to_string(),
                serde_json::json!({"type":"array","maxItems":512,"items":{"type":"object"}}),
            ),
            (
                "entities".to_string(),
                serde_json::json!({"type":"array","maxItems":4096,"items":{"type":"object"}}),
            ),
            (
                "operations".to_string(),
                serde_json::json!({
                    "type":"array","maxItems":2048,"items":{"type":"object"},
                    "description":"group, place, grid, along/path, scatter, camera, script, mirror, variant, update, delete and assert operations. Supports built-in kits such as wall, door, shelf, checkout, street_lamp, tree, bench, crate, platform, stairs, checkpoint, car, aisle, spawn and goal."
                }),
            ),
        ]),
        &[],
    );
    pack.push_native(
        "scene_program",
        "Compile a World Program into a plan without mutating the scene. Use it for a whole level or district instead of hundreds of tool calls. Supports built-in kits, archetype parts, grids, paths, deterministic scatter, cameras, script attachment, mirror, variants, keyed updates, deletions, stable keys, design profiles, overlap preview and a persistent plan id. Then call scene_commit.",
        program_schema.clone(),
        AgentToolKind::Mutation,
        Some("game.plan"),
    );
    pack.push_native(
        "scene_patch",
        "Compile a keyed patch against an existing world. It uses reconcile semantics: stable keys update entities in place, missing keys are created and unmentioned nodes are preserved.",
        program_schema,
        AgentToolKind::Mutation,
        Some("game.plan"),
    );
    pack.push_native(
        "scene_commit",
        "Commit a previously compiled World Plan by plan_id. The scene fingerprint and the real editor revision are checked before mutation; a plan-level undo token and diff are returned.",
        object_schema(
            Map::from_iter([
                ("plan_id".to_string(), string_schema("Plan id returned by scene_program.")),
                (
                    "regions".to_string(),
                    serde_json::json!({"type":"array","items":{"type":"string"},"maxItems":64,"description":"Optional region filter reserved for chunked commits."}),
                ),
            ]),
            &["plan_id"],
        ),
        AgentToolKind::Mutation,
        Some("game.commit"),
    );
    pack.push_native(
        "scene_cancel",
        "Cancel an uncommitted World Plan by plan_id so it cannot be applied later. Committed plans must be reverted with Undo instead.",
        object_schema(
            Map::from_iter([(
                "plan_id".to_string(),
                string_schema("Plan id returned by scene_program."),
            )]),
            &["plan_id"],
        ),
        AgentToolKind::Mutation,
        Some("game.cancel"),
    );
    pack.push_native(
        "world_brief",
        "Read or write the persistent World Brief: genre, camera, core loop, palette, lighting, scale metrics, constraints and acceptance criteria. Read it at task start and update it when the user changes direction. Writing in Plan mode returns a preview and does not touch the project.",
        object_schema(
            Map::from_iter([
                ("action".to_string(), enum_schema("Brief action.", &["read", "write"])),
                ("brief".to_string(), serde_json::json!({"type":"object"})),
                ("content".to_string(), string_schema("Optional raw brief content.")),
            ]),
            &["action"],
        ),
        AgentToolKind::Read,
        Some("world.brief"),
    );
    pack.push_native(
        "scene_recipe",
        "Save a compiled plan as a reusable recipe, list and inspect recipes, and instantiate one as a fresh reviewable plan. Recipes keep the stable keys and compiled groups/entities from a successful plan.",
        object_schema(
            Map::from_iter([
                (
                    "action".to_string(),
                    enum_schema("Recipe action.", &["save", "list", "show", "delete", "apply"]),
                ),
                ("name".to_string(), string_schema("Stable recipe name.")),
                (
                    "plan_id".to_string(),
                    string_schema("Plan id to capture when action=save."),
                ),
                (
                    "description".to_string(),
                    string_schema("Optional human-readable recipe description."),
                ),
                (
                    "tags".to_string(),
                    serde_json::json!({"type":"array","items":{"type":"string"},"maxItems":32}),
                ),
            ]),
            &["action"],
        ),
        AgentToolKind::Mutation,
        Some("world.recipe"),
    );
    pack.push_native(
        "scene_authoring_script",
        "Run a sandboxed Rhai authoring script that can only emit a World Program, then compile it into a reviewable plan. It cannot read files, run commands or mutate the scene. Use world::group, world::place, world::grid, world::line, world::scatter, world::camera, world::script, world::parent, world::count (read-only scene tag/role count) and world::assert_max_entities.",
        object_schema(
            Map::from_iter([
                (
                    "script".to_string(),
                    string_schema(
                        "Rhai source. Only the world:: authoring namespace is available; eval and host APIs are disabled.",
                    ),
                ),
                (
                    "file".to_string(),
                    string_schema("Project-relative .rhai file with the authoring script."),
                ),
            ]),
            &[],
        ),
        AgentToolKind::Mutation,
        Some("world.script"),
    );
    pack.push_native(
        "scene_visual_diff",
        "Compare before/after project images pixel by pixel. Reports changed ratio, mean and max delta, changed bounds and writes an annotated PNG artifact for the evidence gallery.",
        object_schema(
            Map::from_iter([
                ("before".to_string(), string_schema("Project-relative before image.")),
                ("after".to_string(), string_schema("Project-relative after image.")),
                (
                    "output".to_string(),
                    string_schema("Optional project-relative diff PNG path."),
                ),
                (
                    "threshold".to_string(),
                    serde_json::json!({"type":"number","minimum":0,"maximum":255,"description":"Per-channel delta threshold."}),
                ),
                (
                    "save".to_string(),
                    serde_json::json!({"type":"boolean","description":"Write the annotated diff PNG (default true)."}),
                ),
            ]),
            &["before", "after"],
        ),
        AgentToolKind::Read,
        Some("scene.visual_diff"),
    );
    pack.push_native(
        "capabilities_call",
        "Execute one catalog command by name through the same revision, permission, idempotency and undo gateway. Use capabilities_search first. This is the escape hatch for an Inspector or engine command that is not in the current typed pack; runtime.* commands are always rejected.",
        object_schema(
            Map::from_iter([
                ("name".to_string(), string_schema("Exact catalog command name, for example game.camera.")),
                ("params".to_string(), serde_json::json!({"type":"object"})),
            ]),
            &["name"],
        ),
        AgentToolKind::Mutation,
        None,
    );
}

pub(super) fn add_game_mutation_tools(pack: &mut AgentToolPack, intent: PromptIntent) {
    let transform = transform_schema();
    pack.push_native(
        "scene_build",
        "Build a modular scene atomically from explicit groups and entities. Set design_profile to supermarket, building, parking, outdoor, or real_world when the request describes a real place; the engine will reject an incomplete envelope before changing the scene. For a real-world place, create the named envelope group, floor, walls/roof or other boundaries, entrances and circulation before repeated modules and details. Groups are created first (child groups may be listed before their parent), then entities are parented by stable name/path. Give meaningful semantic_role, stable_key and tags to generated parts; every entity must declare its kind, name, transform, and color when those details matter.",
        scene_build_schema(),
        AgentToolKind::Mutation,
        Some("game.build"),
    );
    pack.push_native(
        "scene_reconcile",
        "Reconcile a keyed desired scene state without duplicating existing entities. Existing stable keys are updated in place, missing keys are created, and unmentioned nodes are preserved.",
        scene_reconcile_schema(),
        AgentToolKind::Mutation,
        Some("game.reconcile"),
    );
    pack.push_native(
        "scene_repair",
        "Apply explicit audit-driven repair operations atomically inside an existing scene scope. Inspect the audit and spatial map first; the engine never invents geometry from prose and does not apply a partial repair.",
        object_schema(
            Map::from_iter([
                (
                    "root".to_string(),
                    string_schema("Optional audited root ID, UUID, name, or path."),
                ),
                (
                    "operations".to_string(),
                    serde_json::json!({
                        "type": "array",
                        "minItems": 1,
                        "maxItems": 512,
                        "items": {
                            "type": "object",
                            "properties": {
                                "name": {"type":"string","enum":["scene_create","scene_create_group","scene_update","scene_reparent","scene_snap","scene_delete","scene_duplicate","scene_arrange","scene_instantiate_template"]},
                                "params": batch_operation_params_schema()
                            },
                            "required": ["name", "params"],
                            "additionalProperties": false
                        }
                    }),
                ),
            ]),
            &["operations"],
        ),
        AgentToolKind::Mutation,
        Some("game.repair"),
    );
    pack.push_native(
        "scene_batch",
        "Apply an ordered, atomic batch of scene_create, scene_create_group, scene_update, scene_reparent, scene_snap, scene_delete, scene_duplicate, scene_arrange, or scene_instantiate_template operations. The live scene is committed only when every operation succeeds. Use explicit nested transform and color values; never flatten them into text.",
        object_schema(
            Map::from_iter([(
                "operations".to_string(),
                serde_json::json!({
                    "type": "array",
                    "minItems": 1,
                    "maxItems": 512,
                    "items": {
                        "type": "object",
                        "properties": {
                            "name": {"type": "string", "enum": [
                                "scene_create", "scene_create_group", "scene_update",
                                "scene_reparent", "scene_snap", "scene_delete", "scene_duplicate",
                                "scene_arrange", "scene_instantiate_template"
                            ]},
                            "params": batch_operation_params_schema()
                        },
                        "required": ["name", "params"],
                        "additionalProperties": false
                    }
                }),
            )]),
            &["operations"],
        ),
        AgentToolKind::Mutation,
        Some("game.batch"),
    );
    pack.push_native(
        "scene_create_group",
        "Create an empty organizational group/folder. Use it to keep a generated structure modular instead of putting every part at the scene root.",
        object_schema(
            Map::from_iter([
                ("name".to_string(), string_schema("Unique group name.")),
                (
                    "parent".to_string(),
                    string_schema("Optional group ID, UUID, name, or path; omit for scene root."),
                ),
                ("transform".to_string(), transform.clone()),
                ("size".to_string(), vec3_schema("Local dimensions X/Y/Z in metres; do not combine with scale.")),
                ("semantic_role".to_string(), string_schema("Optional semantic role such as store.shelf.")),
                ("stable_key".to_string(), string_schema("Optional unique reconciliation key.")),
                ("tags".to_string(), semantic_tags_schema()),
                ("agent_origin".to_string(), string_schema("Optional task or workflow origin.")),
            ]),
            &["name"],
        ),
        AgentToolKind::Mutation,
        Some("game.create_group"),
    );
    pack.push_native(
        "scene_create",
        "Create one native scene entity with an explicit primitive, local transform, color, optional light, mesh asset, surface and pivot. Use scene_build for modular multi-part structures.",
        object_schema(
            Map::from_iter([
                (
                    "kind".to_string(),
                    enum_schema("Entity primitive.", PRIMITIVE_KINDS),
                ),
                (
                    "name".to_string(),
                    string_schema("Unique descriptive entity name."),
                ),
                (
                    "parent".to_string(),
                    string_schema("Optional parent ID, UUID, name, or path."),
                ),
                ("transform".to_string(), transform.clone()),
                (
                    "color_rgba".to_string(),
                    color_schema("Optional RGBA channels from 0 to 255."),
                ),
                ("light".to_string(), light_schema()),
                ("mesh_asset".to_string(), mesh_asset_schema()),
                ("surface".to_string(), surface_schema()),
                ("pivot".to_string(), pivot_schema()),
                ("semantic_role".to_string(), string_schema("Optional semantic role such as store.shelf.")),
                ("stable_key".to_string(), string_schema("Optional unique reconciliation key.")),
                ("tags".to_string(), semantic_tags_schema()),
                ("agent_origin".to_string(), string_schema("Optional task or workflow origin.")),
            ]),
            &["kind", "name"],
        ),
        AgentToolKind::Mutation,
        Some("game.add"),
    );
    pack.push_native(
        "scene_update",
        "Update an existing entity by stable ID, UUID, name, or path. Only supplied fields change; light, mesh_asset, surface and pivot replace the corresponding component.",
        object_schema(
            Map::from_iter([
                ("target".to_string(), string_schema("Entity target.")),
                ("name".to_string(), string_schema("Optional new name.")),
                ("transform".to_string(), transform),
                (
                    "size".to_string(),
                    vec3_schema("Local dimensions X/Y/Z in metres; do not combine with scale."),
                ),
                (
                    "color_rgba".to_string(),
                    color_schema("Optional RGBA channels from 0 to 255."),
                ),
                ("light".to_string(), light_schema()),
                ("mesh_asset".to_string(), mesh_asset_schema()),
                ("surface".to_string(), surface_schema()),
                ("pivot".to_string(), pivot_schema()),
                (
                    "visible".to_string(),
                    boolean_schema("Optional visibility."),
                ),
                (
                    "locked".to_string(),
                    boolean_schema("Optional editor lock."),
                ),
                (
                    "semantic_role".to_string(),
                    string_schema("Optional semantic role; use an empty string to clear it."),
                ),
                (
                    "stable_key".to_string(),
                    string_schema(
                        "Optional unique reconciliation key; use an empty string to clear it.",
                    ),
                ),
                ("tags".to_string(), semantic_tags_schema()),
                (
                    "agent_origin".to_string(),
                    string_schema("Optional task or workflow origin."),
                ),
            ]),
            &["target"],
        ),
        AgentToolKind::Mutation,
        Some("game.update"),
    );
    pack.push_native(
        "scene_reparent",
        "Move an existing entity or group to another parent while preserving its world-space transform by default. Use parent=root to move it to the scene root.",
        object_schema(
            Map::from_iter([
                ("target".to_string(), string_schema("Entity or group target.")),
                (
                    "parent".to_string(),
                    string_schema("New parent ID, UUID, name, path, or root."),
                ),
                (
                    "preserve_world".to_string(),
                    boolean_schema("Keep the rendered world transform; defaults to true."),
                ),
            ]),
            &["target"],
        ),
        AgentToolKind::Mutation,
        Some("game.reparent"),
    );
    pack.push_native(
        "scene_snap",
        "Snap one entity to the local grid, world floor, or another entity's world bounds. Use snap_to plus axis and placement for an explicit surface relation.",
        object_schema(
            Map::from_iter([
                ("target".to_string(), string_schema("Entity target.")),
                ("mode".to_string(), enum_schema("Snap mode.", &["grid", "floor", "surface"])),
                ("snap_to".to_string(), string_schema("Optional surface entity target.")),
                ("axis".to_string(), enum_schema("Alignment axis.", &["x", "y", "z"])),
                ("placement".to_string(), enum_schema("Place before, after, or centered on the surface.", &["before", "after", "center"])),
                ("gap".to_string(), number_schema("Separation from the surface.", 0.0, 1000.0)),
                ("grid".to_string(), number_schema("Grid interval for grid mode.", 0.001, 1000.0)),
            ]),
            &["target", "mode"],
        ),
        AgentToolKind::Mutation,
        Some("game.snap"),
    );
    if intent.lighting || intent.gameplay {
        pack.push_native(
            "light_create",
            "Create a point or spot light entity using the shared entity-light component. Point lights emit in every direction; spot lights follow the entity frame with inner/outer cones. Entity lights do not cast shadows. For sun, sky or ambient light use the world environment instead.",
        object_schema(
            Map::from_iter([
                (
                    "kind".to_string(),
                    enum_schema("Light shape.", &["point", "spot"]),
                ),
                ("name".to_string(), string_schema("Unique descriptive light name.")),
                (
                    "parent".to_string(),
                    string_schema("Optional parent ID, UUID, name, or path."),
                ),
                ("transform".to_string(), transform_schema()),
                (
                    "color".to_string(),
                    light_color_schema(),
                ),
                ("intensity".to_string(), number_schema("Light multiplier.", 0.0, 64.0)),
                ("range".to_string(), number_schema("Reach in metres.", 0.05, 10000.0)),
                (
                    "inner_cone_degrees".to_string(),
                    number_schema("Spot inner cone in degrees.", 0.0, 179.0),
                ),
                (
                    "outer_cone_degrees".to_string(),
                    number_schema("Spot outer cone in degrees.", 0.0, 179.0),
                ),
                ("enabled".to_string(), boolean_schema("Light enabled state.")),
                ("semantic_role".to_string(), string_schema("Optional semantic role.")),
                ("tags".to_string(), semantic_tags_schema()),
            ]),
            &["kind", "name"],
        ),
        AgentToolKind::Mutation,
        Some("game.add"),
    );
        pack.push_native(
        "light_update",
        "Replace the light component on an existing entity. game.update deserializes the supplied light object with SceneLight defaults, so include kind, color, intensity, range and cones when they must be preserved. Setting enabled=false keeps the component but stops its contribution.",
        object_schema(
            Map::from_iter([
                ("target".to_string(), string_schema("Entity target.")),
                (
                    "kind".to_string(),
                    enum_schema("New light shape.", &["point", "spot"]),
                ),
                (
                    "color".to_string(),
                    light_color_schema(),
                ),
                ("intensity".to_string(), number_schema("Light multiplier.", 0.0, 64.0)),
                ("range".to_string(), number_schema("Reach in metres.", 0.05, 10000.0)),
                (
                    "inner_cone_degrees".to_string(),
                    number_schema("Spot inner cone in degrees.", 0.0, 179.0),
                ),
                (
                    "outer_cone_degrees".to_string(),
                    number_schema("Spot outer cone in degrees.", 0.0, 179.0),
                ),
                ("enabled".to_string(), boolean_schema("Light enabled state.")),
            ]),
            &["target"],
        ),
        AgentToolKind::Mutation,
        Some("game.update"),
    );
    }
    if intent.gameplay || intent.authoring {
        pack.push_native(
            "audio_edit",
            "Attach or edit an entity-scoped audio source. The clip is project-relative and spatial audio follows the entity's world transform, so a sound stays linked to its block. Use bus=music|sfx|voice|ambient, spatial=true for 3D attenuation, and looping/autoplay for ambient cues.",
            object_schema(
                Map::from_iter([
                    (
                        "action".to_string(),
                        enum_schema("Audio action.", &["attach", "update", "remove"]),
                    ),
                    (
                        "target".to_string(),
                        string_schema("Entity ID, UUID, name, or path. Defaults to selection for update."),
                    ),
                    (
                        "clip".to_string(),
                        string_schema("Project-relative .wav/.mp3/.ogg/.flac path."),
                    ),
                    (
                        "bus".to_string(),
                        enum_schema("Mixer bus.", &["master", "music", "sfx", "voice", "ambient"]),
                    ),
                    ("volume".to_string(), number_schema("Source gain.", 0.0, 1.0)),
                    ("looping".to_string(), boolean_schema("Loop playback.")),
                    ("autoplay".to_string(), boolean_schema("Play automatically on runtime start.")),
                    ("spatial".to_string(), boolean_schema("Attenuate from the entity world position.")),
                    ("min_distance".to_string(), number_schema("Full-volume distance in metres.", 0.001, 10000.0)),
                    ("max_distance".to_string(), number_schema("Silent distance in metres.", 0.002, 10000.0)),
                    ("rolloff".to_string(), number_schema("Distance rolloff exponent.", 0.1, 8.0)),
                    ("enabled".to_string(), boolean_schema("Audio source enabled state.")),
                ]),
                &["action", "target"],
            ),
            AgentToolKind::Mutation,
            Some("game.audio"),
        );
        pack.push_native(
            "mesh_edit",
            "Edit persistent per-entity mesh topology with indexed vertices and faces. Use action=enter first to convert a primitive into editable topology, then inspect with topology=true before changing indices. set replaces the topology, move/scale edit vertex indices, and extrude/inset/subdivide/delete operate on face indices. action=revert restores the parametric primitive; action=apply validates the current mesh without changing it. Imported GLB/GLTF meshes are not editable yet; do not enter them, rebuild from primitives instead. Examples: mesh_edit action=enter target=Cube; mesh_edit action=extrude target=Cube faces=[0] distance=0.25.",
            object_schema(
                Map::from_iter([
                    (
                        "action".to_string(),
                        enum_schema(
                            "Mesh action.",
                            &[
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
                            ],
                        ),
                    ),
                    (
                        "target".to_string(),
                        string_schema(
                            "Entity ID, UUID, name, path, stable key, or selected. Defaults to the current selection.",
                        ),
                    ),
                    (
                        "topology".to_string(),
                        boolean_schema(
                            "Include vertex positions and triangle indices when action=inspect.",
                        ),
                    ),
                    (
                        "vertices".to_string(),
                        serde_json::json!({
                            "type": "array",
                            "description": "For action=set, [x,y,z] vertex positions. For move/scale, zero-based vertex indices; omit to affect every vertex.",
                            "maxItems": 32768,
                            "items": {
                                "oneOf": [
                                    {"type": "array", "items": {"type": "number"}, "minItems": 3, "maxItems": 3},
                                    {"type": "integer", "minimum": 0}
                                ]
                            }
                        }),
                    ),
                    (
                        "indices".to_string(),
                        serde_json::json!({
                            "type": "array",
                            "items": {"type": "integer", "minimum": 0},
                            "maxItems": 196608,
                            "description": "Flat triangle indices for action=set; every three entries form one face."
                        }),
                    ),
                    (
                        "faces".to_string(),
                        serde_json::json!({
                            "type": "array",
                            "items": {"type": "integer", "minimum": 0},
                            "maxItems": 65536,
                            "description": "Zero-based face indices for extrude, inset, subdivide and delete."
                        }),
                    ),
                    (
                        "face".to_string(),
                        integer_schema("Single zero-based face index; shortcut for faces=[n].", 0, 65535),
                    ),
                    (
                        "delta".to_string(),
                        vec3_schema("Local vertex delta in metres for action=move."),
                    ),
                    (
                        "factor".to_string(),
                        vec3_schema("Per-axis scale factors for action=scale."),
                    ),
                    (
                        "center".to_string(),
                        vec3_schema(
                            "Local scale center for action=scale; defaults to the selection center.",
                        ),
                    ),
                    (
                        "distance".to_string(),
                        number_schema(
                            "Extrude distance in metres along the average face normal.",
                            -10000.0,
                            10000.0,
                        ),
                    ),
                    (
                        "amount".to_string(),
                        number_schema("Inset distance fraction toward the face center.", 0.0, 0.999),
                    ),
                ]),
                &["action"],
            ),
            AgentToolKind::Mutation,
            Some("game.mesh"),
        );
    }
    if intent.assets || intent.prefab || intent.gameplay {
        pack.push_native(
            "mesh_asset_place",
        "Place a static .glb or .gltf mesh asset as a scene entity. The path is relative to the project assets directory. Optional node selects one source node and import_scale normalises units. Use mesh_asset_inspect first when the file has several nodes or unknown units.",
        object_schema(
            Map::from_iter([
                (
                    "path".to_string(),
                    string_schema("Mesh path under assets, for example models/shelf.glb."),
                ),
                ("name".to_string(), string_schema("Optional entity name.")),
                (
                    "parent".to_string(),
                    string_schema("Optional parent ID, UUID, name, or path."),
                ),
                ("transform".to_string(), transform_schema()),
                (
                    "node".to_string(),
                    integer_schema("Optional source node index inside the glTF document.", 0, 65535),
                ),
                (
                    "import_scale".to_string(),
                    number_schema("Uniform import scale applied after the node transform.", 0.0001, 10000.0),
                ),
            ]),
            &["path"],
        ),
        AgentToolKind::Mutation,
        Some("game.mesh_asset"),
    );
    }
    for (tool, command, description) in [
        (
            "scene_delete",
            "game.delete",
            "Delete one scene entity and its descendants.",
        ),
        (
            "scene_duplicate",
            "game.duplicate",
            "Duplicate one scene entity and its descendants, optionally into a new parent or as a repeated offset array.",
        ),
    ] {
        let properties = if tool == "scene_duplicate" {
            Map::from_iter([
                ("target".to_string(), string_schema("Entity target.")),
                ("parent".to_string(), string_schema("Optional new parent or root.")),
                ("name".to_string(), string_schema("Optional base name for copies.")),
                ("offset".to_string(), vec3_schema("Base local offset from the source.")),
                ("count".to_string(), integer_schema("Number of copies.", 1, 128)),
                ("axis".to_string(), enum_schema("Array axis.", &["x", "y", "z"])),
                ("spacing".to_string(), number_schema("Additional spacing between copies.", -10000.0, 10000.0)),
                ("preserve_world".to_string(), boolean_schema("Preserve world transform when changing parent.")),
            ])
        } else {
            Map::from_iter([("target".to_string(), string_schema("Entity target."))])
        };
        pack.push_native(
            tool,
            description,
            object_schema(properties, &["target"]),
            AgentToolKind::Mutation,
            Some(command),
        );
    }
    if intent.layout {
        pack.push_native(
            "scene_arrange",
            "Arrange selected entities, or all visible entities when selection is empty, in a grid.",
            object_schema(
                Map::from_iter([(
                    "spacing".to_string(),
                    number_schema("Grid spacing.", 0.1, 10000.0),
                )]),
                &[],
            ),
            AgentToolKind::Mutation,
            Some("game.arrange_grid"),
        );
    }
    if intent.prefab {
        pack.push_native(
            "scene_instantiate_template",
            "Instantiate a registered native template into an optional parent, with an explicit transform or repeated count, axis and spacing.",
            object_schema(
                Map::from_iter([
                    ("kind".to_string(), string_schema("Registered prefab kind.")),
                    ("name".to_string(), string_schema("Optional root name.")),
                    ("parent".to_string(), string_schema("Optional parent ID, UUID, name, path, or root.")),
                    ("transform".to_string(), transform_schema()),
                    ("count".to_string(), integer_schema("Number of instances.", 1, 64)),
                    ("axis".to_string(), enum_schema("Array axis.", &["x", "y", "z"])),
                    ("spacing".to_string(), number_schema("Spacing between instances.", -10000.0, 10000.0)),
                ]),
                &["kind"],
            ),
            AgentToolKind::Mutation,
            Some("game.generate_prefab"),
        );
    }
    pack.push_native(
        "game_validate_layout",
        "Validate scene transforms after a build or layout change.",
        object_schema(Map::new(), &[]),
        AgentToolKind::Read,
        None,
    );
}

pub(super) fn translate_arguments(
    name: &str,
    _command_name: &str,
    mut arguments: Value,
) -> Result<Value, String> {
    if !arguments.is_object() {
        return Err(format!("Tool '{name}' arguments must be a JSON object."));
    }
    match name {
        "scene_create" | "scene_update" => normalize_scene_arguments(&mut arguments)?,
        "light_create" => translate_light_create(&mut arguments)?,
        "light_update" => translate_light_update(&mut arguments)?,
        "mesh_asset_inspect" => translate_mesh_asset_inspect(&mut arguments)?,
        "mesh_asset_place" => translate_mesh_asset_place(&mut arguments)?,
        "camera_activate" => {
            if let Some(object) = arguments.as_object_mut() {
                object.insert("action".to_string(), Value::String("activate".to_string()));
            }
        }
        "audio_edit" => {
            if let Some(object) = arguments.as_object_mut() {
                if !object.contains_key("action") {
                    object.insert("action".to_string(), Value::String("attach".to_string()));
                }
            }
        }
        "mesh_edit" => {
            if let Some(object) = arguments.as_object_mut() {
                if !object.contains_key("action") {
                    object.insert("action".to_string(), Value::String("inspect".to_string()));
                }
            }
        }
        "scene_build" => translate_build(&mut arguments)?,
        "scene_reconcile" => translate_reconcile(&mut arguments)?,
        "scene_repair" => translate_repair(&mut arguments)?,
        "scene_batch" => translate_batch(&mut arguments)?,
        "scene_patch" => {
            if let Some(object) = arguments.as_object_mut() {
                object.insert("mode".to_string(), Value::String("reconcile".to_string()));
            }
        }
        _ => {}
    }
    Ok(arguments)
}

/// Move flat light fields into the `light` component object expected by
/// `game.add`. The entity itself stays an Empty node with a light component.
fn translate_light_create(arguments: &mut Value) -> Result<(), String> {
    let object = arguments
        .as_object_mut()
        .ok_or_else(|| "light_create arguments must be an object.".to_string())?;
    let light = take_light_fields(object);
    if !light.contains_key("kind") {
        return Err("light_create requires kind=point or kind=spot.".to_string());
    }
    object.insert("primitive".to_string(), Value::String("empty".to_string()));
    object.insert("light".to_string(), Value::Object(light));
    Ok(())
}

fn translate_light_update(arguments: &mut Value) -> Result<(), String> {
    let object = arguments
        .as_object_mut()
        .ok_or_else(|| "light_update arguments must be an object.".to_string())?;
    let light = take_light_fields(object);
    if light.is_empty() {
        return Err("light_update requires at least one light field besides target.".to_string());
    }
    object.insert("light".to_string(), Value::Object(light));
    Ok(())
}

fn take_light_fields(object: &mut Map<String, Value>) -> Map<String, Value> {
    const LIGHT_FIELDS: [&str; 7] = [
        "kind",
        "color",
        "intensity",
        "range",
        "inner_cone_degrees",
        "outer_cone_degrees",
        "enabled",
    ];
    let mut light = Map::new();
    for field in LIGHT_FIELDS {
        if let Some(value) = object.remove(field) {
            light.insert(field.to_string(), value);
        }
    }
    light
}

fn translate_mesh_asset_inspect(arguments: &mut Value) -> Result<(), String> {
    let object = arguments
        .as_object_mut()
        .ok_or_else(|| "mesh_asset_inspect arguments must be an object.".to_string())?;
    object.insert("action".to_string(), Value::String("inspect".to_string()));
    Ok(())
}

fn translate_mesh_asset_place(arguments: &mut Value) -> Result<(), String> {
    let object = arguments
        .as_object_mut()
        .ok_or_else(|| "mesh_asset_place arguments must be an object.".to_string())?;
    let path = object
        .remove("path")
        .and_then(|value| value.as_str().map(str::to_string))
        .ok_or_else(|| "mesh_asset_place requires path.".to_string())?;
    let mut reference = Map::from_iter([("path".to_string(), Value::String(path))]);
    for key in ["node", "import_scale"] {
        if let Some(value) = object.remove(key) {
            reference.insert(key.to_string(), value);
        }
    }
    object.insert("action".to_string(), Value::String("place".to_string()));
    object.insert("mesh_asset".to_string(), Value::Object(reference));
    Ok(())
}

/// Normalize only the semantic discriminator at the Agent boundary.
///
/// Structured transform and color values intentionally remain intact until
/// they reach the game command kernel. Flattening them into a lossy string
/// map here used to make malformed or provider-shaped values disappear
/// silently, which is how a successful-looking call could create defaults at
/// the origin. The kernel accepts both this semantic shape and legacy CLI
/// scalar fields.
pub(super) fn normalize_scene_arguments(arguments: &mut Value) -> Result<(), String> {
    let Some(object) = arguments.as_object_mut() else {
        return Err("Scene tool arguments must be a JSON object.".to_string());
    };
    if !object.contains_key("primitive") {
        if let Some(kind) = object.remove("kind") {
            object.insert("primitive".to_string(), kind);
        } else if let Some(shape) = object.remove("shape") {
            object.insert("primitive".to_string(), shape);
        }
    }
    Ok(())
}

pub(super) fn translate_build(arguments: &mut Value) -> Result<(), String> {
    let object = arguments
        .as_object_mut()
        .ok_or_else(|| "scene_build arguments must be an object.".to_string())?;
    let mut found = false;
    for key in ["groups", "entities"] {
        let Some(value) = object.get_mut(key) else {
            continue;
        };
        let items = value
            .as_array_mut()
            .ok_or_else(|| format!("scene_build.{key} must be an array."))?;
        found |= !items.is_empty();
        for (index, item) in items.iter_mut().enumerate() {
            if key == "entities" {
                normalize_scene_arguments(item)?;
            }
            let item_object = item
                .as_object_mut()
                .ok_or_else(|| format!("scene_build.{key}[{index}] must be an object."))?;
            if item_object
                .get("name")
                .and_then(Value::as_str)
                .map(|name| name.trim().is_empty())
                .unwrap_or(true)
            {
                return Err(format!("scene_build.{key}[{index}].name is required."));
            }
        }
    }
    if !found {
        return Err("scene_build requires at least one group or entity.".to_string());
    }
    Ok(())
}

fn translate_reconcile(arguments: &mut Value) -> Result<(), String> {
    translate_build(arguments)?;
    let object = arguments
        .as_object()
        .ok_or_else(|| "scene_reconcile arguments must be an object.".to_string())?;
    for key in ["groups", "entities"] {
        let Some(items) = object.get(key).and_then(Value::as_array) else {
            continue;
        };
        for (index, item) in items.iter().enumerate() {
            let stable_key = item
                .get("stable_key")
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|value| !value.is_empty());
            if stable_key.is_none() {
                return Err(format!(
                    "scene_reconcile.{key}[{index}].stable_key is required."
                ));
            }
        }
    }
    Ok(())
}

pub(super) fn translate_batch(arguments: &mut Value) -> Result<(), String> {
    let operations = arguments
        .get_mut("operations")
        .and_then(Value::as_array_mut)
        .ok_or_else(|| "scene_batch.operations must be an array.".to_string())?;
    for operation in operations {
        let object = operation
            .as_object_mut()
            .ok_or_else(|| "Each batch operation must be an object.".to_string())?;
        let semantic_name = object
            .get("name")
            .and_then(Value::as_str)
            .ok_or_else(|| "Each batch operation requires name.".to_string())?
            .trim_start_matches('/')
            .to_ascii_lowercase();
        let canonical = match semantic_name.as_str() {
            "scene_create" | "game.add" => "game.add",
            "scene_update" | "game.update" => "game.update",
            "scene_delete" | "game.delete" => "game.delete",
            "scene_duplicate" | "game.duplicate" => "game.duplicate",
            "scene_arrange" | "game.arrange_grid" => "game.arrange_grid",
            "scene_snap" | "game.snap" => "game.snap",
            "scene_instantiate_prefab" | "scene_instantiate_template" | "game.generate_prefab" => {
                "game.generate_prefab"
            }
            "scene_create_group" | "game.create_group" => "game.create_group",
            "scene_reparent" | "game.reparent" => "game.reparent",
            "scene_build" | "game.build" => {
                return Err(
                    "scene_build must be the outer tool; nested scene_build is not supported."
                        .to_string(),
                )
            }
            other => return Err(format!("Unsupported batch operation: {other}")),
        };
        object.insert("name".to_string(), Value::String(canonical.to_string()));
        let params = object
            .get_mut("params")
            .ok_or_else(|| "Each batch operation requires an object params value.".to_string())?;
        if !params.is_object() {
            return Err("Each batch operation params value must be a JSON object.".to_string());
        }
        if matches!(canonical, "game.add" | "game.update") {
            normalize_scene_arguments(params)?;
        }
    }
    Ok(())
}

fn translate_repair(arguments: &mut Value) -> Result<(), String> {
    let operations = arguments
        .as_object_mut()
        .ok_or_else(|| "scene_repair arguments must be an object.".to_string())?
        .remove("operations")
        .and_then(|value| value.as_array().cloned())
        .ok_or_else(|| "scene_repair.operations must be an array.".to_string())?;
    if operations.is_empty() {
        return Err("scene_repair.operations must contain at least one operation.".to_string());
    }
    let mut batch_arguments = serde_json::json!({"operations": operations});
    translate_batch(&mut batch_arguments)?;
    if let Some(object) = arguments.as_object_mut() {
        if let Some(operations) = batch_arguments.get("operations").cloned() {
            object.insert("operations".to_string(), operations);
        }
    }
    Ok(())
}
