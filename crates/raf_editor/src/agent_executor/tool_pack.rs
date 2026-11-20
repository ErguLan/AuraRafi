//! Contextual Agent tool pack assembly, allowlists, and capability routing.

use std::collections::HashMap;

use raf_ai::agent_runtime::{AgentToolKind, AgentToolResult, ToolExecutionMode};
use raf_ai::openai_client::OpenAiTool;
use raf_ai::provider::AgentMode;
use raf_core::project::ProjectType;
use raf_core::Language;
use serde_json::{Map, Value};

use crate::commands::catalog::{CommandCatalog, CommandDefinition};

use super::intent::{catalog_tool_kind, classify_prompt, order_contextual_tools, PromptIntent};
use super::schemas::{
    boolean_schema, command_to_openai_tool, enum_schema, expected_scene_state_schema,
    integer_schema, localized_native_tool_description, number_schema, object_schema, openai_tool,
    paginated_schema, sanitize_tool_name, semantic_tags_schema, string_schema,
};
use super::scripting::add_game_scripting_tools;
use super::world_build::{
    add_game_mutation_tools, add_game_program_tools, normalize_scene_arguments,
};
use super::AgentToolExecutor;
#[derive(Debug, Clone)]
pub struct AgentToolRoute {
    pub command_name: Option<String>,
    pub kind: AgentToolKind,
}

#[derive(Debug, Clone, Default)]
pub struct AgentToolPack {
    pub tools: Vec<OpenAiTool>,
    pub routes: HashMap<String, AgentToolRoute>,
    language: Language,
}

impl AgentToolPack {
    fn has_tool(&self, name: &str) -> bool {
        self.routes.contains_key(name)
    }

    pub(super) fn push_native(
        &mut self,
        name: &str,
        description: &str,
        parameters: Value,
        kind: AgentToolKind,
        command_name: Option<&str>,
    ) {
        if self.has_tool(name) {
            return;
        }
        let description = localized_native_tool_description(name, description, self.language);
        self.tools.push(openai_tool(name, &description, parameters));
        self.routes.insert(
            name.to_string(),
            AgentToolRoute {
                command_name: command_name.map(str::to_string),
                kind,
            },
        );
    }

    pub(super) fn push_catalog(
        &mut self,
        command: &CommandDefinition,
        language: Language,
        kind: AgentToolKind,
    ) {
        let name = sanitize_tool_name(&command.name);
        if self.has_tool(&name) {
            return;
        }
        self.tools
            .push(command_to_openai_tool(&name, command, language));
        self.routes.insert(
            name,
            AgentToolRoute {
                command_name: Some(command.name.clone()),
                kind,
            },
        );
    }
}

pub(super) fn build_tool_pack(
        catalog: &CommandCatalog,
        language: Language,
        project_type: Option<ProjectType>,
        mode: AgentMode,
        prompt: &str,
    ) -> AgentToolPack {
        let intent = classify_prompt(prompt);
        let mut pack = AgentToolPack {
            language,
            ..AgentToolPack::default()
        };
        add_project_read_tools(&mut pack, intent, project_type);
        if project_type.is_some() && mode != AgentMode::Inspect && intent.assets {
            add_asset_generation_tools(&mut pack);
        }

        match project_type {
            Some(ProjectType::Game) => {
                if intent.game_ui {
                    for name in ["ui.document.read","ui.document.create","ui.document.apply","ui.document.undo"] {
                        let kind=if name=="ui.document.read"{AgentToolKind::Read}else{AgentToolKind::Mutation};
                        if kind==AgentToolKind::Read || mode!=AgentMode::Inspect {if let Some(command)=catalog.find(name){pack.push_catalog(command,language,kind);}}
                    }
                }
                for &name in &["lighting.inspect", "lighting.set", "lighting.reset"][..] {
                    let kind = if name == "lighting.inspect" {
                        AgentToolKind::Read
                    } else {
                        AgentToolKind::Mutation
                    };
                    if kind == AgentToolKind::Read || mode != AgentMode::Inspect {
                        if let Some(command) = catalog.find(name) {
                            pack.push_catalog(command, language, kind);
                        }
                    }
                }
                // A prompt such as "un supermercado" has no explicit authoring
                // verb, but it still needs the construction surface. The core
                // pack is therefore mode-gated, not keyword-gated.
                if mode != AgentMode::Inspect {
                    add_game_mutation_tools(
                        &mut pack,
                        PromptIntent {
                            authoring: true,
                            ..intent
                        },
                    );
                    add_game_program_tools(&mut pack);
                }
                add_game_scripting_tools(&mut pack, catalog, language, mode);
            }
            Some(ProjectType::Electronics) => {
                pack.push_native("electronics_lab_simulate",
                    "Run cancellable DC analysis of the current schematic. Returns currents and pin voltages with convergence evidence.",
                    serde_json::json!({"type":"object","properties":{},"additionalProperties":false}), AgentToolKind::Read, None);
                pack.push_native("electronics_lab_probe",
                    "Read a page of DC measurements. Optionally filter by component reference or UUID. Check stale before comparing.",
                    serde_json::json!({"type":"object","properties":{"component":{"type":"string"},"offset":{"type":"integer","minimum":0}},"additionalProperties":false}), AgentToolKind::Read, None);
                pack.push_native("electronics_lab_signals",
                    "Inspect serial status and latest measured channels for the active Electronics document.",
                    serde_json::json!({"type":"object","properties":{},"additionalProperties":false}), AgentToolKind::Read, None);
                let names: &[&str] = if mode != AgentMode::Inspect && intent.authoring {
                    if intent.pcb {
                        &[
                            "pcb.describe",
                            "pcb.sync",
                            "pcb.route_airwire",
                            "pcb.set_board",
                            "pcb.move",
                            "pcb.rotate",
                        ]
                    } else {
                        &[
                            "electronics.describe",
                            "electronics.add_part",
                            "electronics.wire",
                            "electronics.set_value",
                            "electronics.rotate",
                            "electronics.delete",
                            "electronics.generate_circuit",
                            "electronics.autolayout",
                        ]
                    }
                } else {
                    &[
                        "electronics.describe",
                        "electronics.netlist",
                        "electronics.bom",
                        "pcb.describe",
                    ]
                };
                for name in names {
                    if let Some(command) = catalog.find(name) {
                        pack.push_catalog(command, language, catalog_tool_kind(command));
                    }
                }
            }
            None => {}
        }

        // Provider quality degrades sharply when every command is advertised.
        // Keep the contextual pack bounded and deterministic.
        order_contextual_tools(&mut pack.tools);
        // Intent-critical scripting must survive the contextual tool budget.
        // Stable sorting retains the perception/scene order within each group.
        if intent.scripting || intent.gameplay {
            pack.tools.sort_by_key(|tool| {
                !(matches!(
                    tool.function.name.as_str(),
                    "script_write"
                        | "script_create"
                        | "script_attach"
                        | "script_detach"
                        | "script_validate"
                        | "script_run"
                        | "behavior_attach"
                        | "behavior_detach"
                        | "behavior_parameter"
                        | "prefab_capture"
                        | "prefab_instantiate"
                ) || (intent.authoring
                    && matches!(
                        tool.function.name.as_str(),
                        "scene_build" | "scene_reconcile" | "scene_batch" | "scene_verify"
                    )))
            });
        }
        // Perception, gameplay, lighting, and capture must coexist for a
        // complete game request. The pack stays bounded, but 26 was too small
        // to keep scripting and the visual loop in the same turn.
        // The pack now carries perception, a complete authoring core, World
        // Program tools, scripting and visual verification in one run. Fifty-two
        // compact schemas stay below the former context blow-up while avoiding
        // the old failure mode where a gameplay prompt dropped scene_build or
        // game_completion_audit from the same turn.
        const MAX_CONTEXTUAL_TOOLS: usize = 60;
        if pack.tools.len() > MAX_CONTEXTUAL_TOOLS {
            pack.tools.truncate(MAX_CONTEXTUAL_TOOLS);
            let visible = pack
                .tools
                .iter()
                .map(|tool| tool.function.name.as_str())
                .collect::<std::collections::HashSet<_>>();
            pack.routes
                .retain(|name, _| visible.contains(name.as_str()));
        }
        pack
    }

fn add_asset_generation_tools(pack: &mut AgentToolPack) {
    let image_schema = object_schema(Map::from_iter([
        ("prompt", string_schema("Describe the image to generate.")),
        ("name", string_schema("Project image name; never a host path.")),
        ("size", enum_schema("Image proportions.", &["square", "landscape", "portrait"])),
        ("transparent", serde_json::json!({"type": "boolean"})),
        ("model", string_schema("Optional explicitly chosen native image generator ID. Omit to use the current conversation model; never invent or silently replace it.")),
    ].map(|(key, value)| (key.to_string(), value))), &["prompt", "name"]);
    pack.push_native("asset_generate_image", "Generate an image with the current conversation provider/model. Output capabilities are discovered, not guessed. Waits for real PNG output in assets/generated. If unsupported, choose local PNG or a configured external studio.", image_schema.clone(), AgentToolKind::Mutation, Some("asset.generate_image"));
    pack.push_native("external_studio_generate", "Generate a complex image in the separately installed, user-configured studio using its CLI. Waits for the result in project Assets. Does not bundle an application or change model settings.", image_schema, AgentToolKind::Mutation, None);
    pack.push_native("asset_generate_local_png", "Generate a usable local procedural PNG with the existing icon/badge/sprite/texture presets. No provider request or cost.",
        object_schema(Map::from_iter([
            ("prompt", string_schema("Local image description.")),
            ("name", string_schema("Project image name.")),
            ("size", enum_schema("Image proportions.", &["square", "landscape", "portrait"])),
            ("style", enum_schema("Existing local preset.", &["icon", "badge", "sprite", "texture"])),
            ("transparent", serde_json::json!({"type": "boolean"})),
        ].map(|(key, value)| (key.to_string(), value))), &["prompt", "name"]), AgentToolKind::Mutation, Some("asset.generate_local_png"));
    pack.push_native("external_studio_install", "Download the public Git repository configured by the user in AI Providers into its separate installation directory. Never overwrites a directory or installs dependencies/runs package hooks. Use before external generation when not installed.", object_schema(Map::new(), &[]), AgentToolKind::Mutation, None);
}

fn add_project_read_tools(
    pack: &mut AgentToolPack,
    intent: PromptIntent,
    project_type: Option<ProjectType>,
) {
    pack.push_native(
        "project_summary",
        "Read a compact snapshot of the open project, active session, selection, scene, assets, scripts, revision, and warnings.",
        object_schema(Map::new(), &[]),
        AgentToolKind::Read,
        None,
    );
    if project_type == Some(ProjectType::Game) {
        pack.push_native(
            "scene_outline",
            "Read the native scene hierarchy with stable IDs, UUIDs, names, paths, parent/child counts, local transforms, world positions, assets, and scripts. Paginated.",
            paginated_schema(&[
                ("root", string_schema("Optional root ID, UUID, name, or path.")),
                ("depth", integer_schema("Maximum hierarchy depth.", 1, 16)),
            ]),
            AgentToolKind::Read,
            None,
        );
        pack.push_native(
            "scene_digest",
            "Read one compact but complete scene snapshot: counts, hierarchy roots, world bounds, cameras, component coverage, top semantic roles/tags, optional overlaps and a cursor for large scopes. Use this before planning a world instead of chaining many read calls.",
            paginated_schema(&[
                ("root", string_schema("Optional root ID, UUID, name, or path.")),
                (
                    "check_collisions",
                    boolean_schema("Include a bounded overlap summary; defaults to false."),
                ),
            ]),
            AgentToolKind::Read,
            None,
        );
        pack.push_native(
            "scene_query",
            "Find scene entities by name, path, kind, semantic role, exact tags, source asset, or current selection without scanning files.",
            paginated_schema(&[
                ("query", string_schema("Name, path, or asset query.")),
                ("kind", string_schema("Optional primitive or folder kind.")),
                ("semantic_role", string_schema("Exact semantic role or parent role prefix.")),
                ("tags", semantic_tags_schema()),
                ("match_all_tags", boolean_schema("Require every tag; defaults to true.")),
                ("selected", boolean_schema("Only search the current selection.")),
            ]),
            AgentToolKind::Read,
            None,
        );
        pack.push_native(
            "scene_spatial_map",
            "Read world-space bounds, overall extents, visible renderable entities, and conservative overlaps for a bounded scene scope. Use it before arranging real spaces.",
            paginated_schema(&[
                (
                    "root",
                    string_schema("Optional root ID, UUID, name, or path to scope the map."),
                ),
                (
                    "include_hidden",
                    boolean_schema("Include hidden renderable entities; defaults to false."),
                ),
                (
                    "check_collisions",
                    boolean_schema("Check conservative world-space AABB overlaps; defaults to true."),
                ),
            ]),
            AgentToolKind::Read,
            None,
        );
        pack.push_native(
            "scene_check_overlaps",
            "Check a bounded scene scope for conservative world-space overlaps. Returns penetration depth and a suggested scene_snap repair for every pair.",
            object_schema(
                Map::from_iter([
                    ("root".to_string(), string_schema("Optional root ID, UUID, name, or path.")),
                    ("include_hidden".to_string(), boolean_schema("Include hidden renderable entities.")),
                    ("margin".to_string(), number_schema("Extra separation after repair.", 0.0, 100.0)),
                    ("max_pairs".to_string(), integer_schema("Maximum overlap pairs returned.", 1, 256)),
                ]),
                &[],
            ),
            AgentToolKind::Read,
            None,
        );
        pack.push_native(
            "scene_navigation_audit",
            "Static walkability and reachability audit. Samples authored AABBs into a bounded grid, expands obstacles by an agent radius and BFS-checks spawn-to-goal reachability without starting Play or Runtime.",
            object_schema(
                Map::from_iter([
                    ("root".to_string(), string_schema("Optional root ID, UUID, name, or path.")),
                    ("spawn".to_string(), string_schema("Spawn entity target; inferred from spawn-tagged nodes when omitted.")),
                    ("goals".to_string(), serde_json::json!({"type":"array","items":{"type":"string"},"maxItems":32,"description":"Goal entities; inferred from goal/checkpoint/exit tags when omitted."})),
                    ("resolution".to_string(), number_schema("Grid resolution in metres.", 0.1, 8.0)),
                    ("agent_radius".to_string(), number_schema("Agent clearance radius in metres.", 0.0, 8.0)),
                ]),
                &[],
            ),
            AgentToolKind::Read,
            None,
        );
        pack.push_native(
            "scene_diff",
            "Read retained semantic scene changes since an Agent revision, including created, updated and deleted UUIDs.",
            object_schema(
                Map::from_iter([(
                    "from_revision".to_string(),
                    integer_schema("Known revision to compare from.", 0, i32::MAX as i64),
                )]),
                &[],
            ),
            AgentToolKind::Read,
            None,
        );
        pack.push_native(
            "scene_design_audit",
            "Audit whether a bounded scene scope reads as an intentional real-world place: floor, enclosure, entrance, circulation, primary modules, details, and meaningful 3D extent. This is read-only evidence for the next repair step.",
            object_schema(
                Map::from_iter([
                    (
                        "root".to_string(),
                        string_schema("Optional root ID, UUID, name, or path to scope the audit."),
                    ),
                    (
                        "required_features".to_string(),
                        serde_json::json!({
                            "type": "array",
                            "items": {"type": "string", "enum": ["floor", "enclosure", "entrance", "circulation", "roof", "primary_modules", "details"]},
                            "maxItems": 7
                        }),
                    ),
                    (
                        "design_profile".to_string(),
                        serde_json::json!({
                            "type": "string",
                            "description": "Optional place contract used to choose the required design envelope.",
                            "enum": ["generic", "real_world", "building", "store", "supermarket", "parking", "outdoor"]
                        }),
                    ),
                ]),
                &[],
            ),
            AgentToolKind::Read,
            None,
        );
        pack.push_native(
            "scene_inspect",
            "Inspect exact entities using stable IDs, UUIDs, names, or paths. Returns local/world transforms, hierarchy, primitive, color, asset, script, visibility, and variables.",
            object_schema(
                Map::from_iter([
                    ("target".to_string(), string_schema("One entity target.")),
                    (
                        "targets".to_string(),
                        serde_json::json!({
                            "type": "array",
                            "items": {"type": "string"},
                            "maxItems": 32
                        }),
                    ),
                ]),
                &[],
            ),
            AgentToolKind::Read,
            None,
        );
        pack.push_native(
            "selection_get",
            "Read the current scene selection with stable IDs, UUIDs, names, paths, and compact entity details.",
            object_schema(Map::new(), &[]),
            AgentToolKind::Read,
            None,
        );
        pack.push_native(
            "viewport_capture",
            "Capture the Game scene as a PNG artifact for visual inspection. Use view=current for the last editor frame or front/back/left/right/top/bottom/isometric/custom to render a framed angle. preset=contact_sheet renders four angles into one bounded image. preset=top_down_map renders a metric XZ schematic with world bounds, collider outlines, grid and labels, and returns structured map metadata. Targets default to the current selection; frame=scene fits the whole scene. Images are automatically attached to your next model turn when the model accepts images. This never starts Play mode or changes the scene.",
            object_schema(
                Map::from_iter([
                    (
                        "refresh".to_string(),
                        boolean_schema("Request a fresh live frame before the capture."),
                    ),
                    (
                        "preset".to_string(),
                        enum_schema(
                            "Capture preset. contact_sheet renders four angles into one bounded image; top_down_map renders a metric XZ schematic with bounds, colliders, grid and labels.",
                            &["single", "contact_sheet", "top_down_map"],
                        ),
                    ),
                    (
                        "contact_sheet".to_string(),
                        boolean_schema("Shortcut for preset=contact_sheet."),
                    ),
                    (
                        "top_down".to_string(),
                        boolean_schema("Shortcut for preset=top_down_map."),
                    ),
                    (
                        "overlays".to_string(),
                        serde_json::json!({
                            "type":"array",
                            "items":{"type":"string","enum":["all","bounds","colliders","grid","labels"]},
                            "maxItems":5,
                            "description":"Top-down overlays. Defaults to all."
                        }),
                    ),
                    (
                        "grid".to_string(),
                        number_schema("Top-down world grid spacing in metres; omitted uses an automatic metric step.", 0.05, 10_000.0),
                    ),
                    (
                        "view".to_string(),
                        enum_schema(
                            "Capture angle. current preserves the last editor frame; other values frame the target or selection offscreen.",
                            &[
                                "current", "front", "back", "left", "right", "top", "bottom",
                                "isometric", "custom",
                            ],
                        ),
                    ),
                    (
                        "frame".to_string(),
                        enum_schema("Capture scope.", &["selection", "scene"]),
                    ),
                    (
                        "target".to_string(),
                        string_schema("Entity, group, UUID, path, or stable key to frame."),
                    ),
                    (
                        "targets".to_string(),
                        serde_json::json!({
                            "type": "array",
                            "items": {"type": "string"},
                            "maxItems": 64,
                            "description": "Entities or groups to frame together. Omit to use the selection."
                        }),
                    ),
                    (
                        "yaw_deg".to_string(),
                        number_schema("Custom horizontal angle in degrees.", -360.0, 360.0),
                    ),
                    (
                        "pitch_deg".to_string(),
                        number_schema("Custom vertical angle in degrees.", -89.0, 89.0),
                    ),
                    (
                        "distance".to_string(),
                        number_schema("Optional explicit camera distance in metres.", 0.1, 100_000.0),
                    ),
                    (
                        "padding".to_string(),
                        number_schema("Framing padding multiplier.", 1.0, 3.0),
                    ),
                    (
                        "width".to_string(),
                        integer_schema("Image width in pixels.", 128, 1_024),
                    ),
                    (
                        "height".to_string(),
                        integer_schema("Image height in pixels.", 128, 1_024),
                    ),
                    (
                        "highlight_selection".to_string(),
                        boolean_schema("Outline the framed selection in the capture."),
                    ),
                    (
                        "label".to_string(),
                        string_schema("Optional human-readable evidence label."),
                    ),
                ]),
                &[],
            ),
            AgentToolKind::Read,
            None,
        );
        pack.push_native(
            "scene_visual_review",
            "Ask the configured vision-capable reviewer model to inspect one to four project capture artifacts and return structured scores, findings and fixes. Use this when the active Agent model is text-only or when a second visual opinion is useful. If no vision model is available it returns a structured `vision_unavailable` fallback and never blocks the task.",
            object_schema(
                Map::from_iter([
                    (
                        "images".to_string(),
                        serde_json::json!({
                            "type":"array",
                            "items":{"type":"string"},
                            "maxItems":4,
                            "description":"Project-relative PNG capture paths, for example .aura_rafi/agent_artifacts/viewport-....png."
                        }),
                    ),
                    (
                        "image".to_string(),
                        string_schema("Optional single project-relative PNG capture path."),
                    ),
                    (
                        "focus".to_string(),
                        string_schema("Optional review focus such as entrance readability, aisle width or lighting."),
                    ),
                    (
                        "brief".to_string(),
                        serde_json::json!({"type":"object","description":"Optional world brief; the active project brief is used when omitted."}),
                    ),
                ]),
                &[],
            ),
            AgentToolKind::Read,
            None,
        );
    }
    pack.push_native(
        "assets_catalog",
        "Read the worker-backed imported asset catalog, including type, usage, and entities using each asset. Does not scan internal Agent files.",
        paginated_schema(&[
            ("query", string_schema("Optional asset name or path query.")),
            (
                "usage",
                enum_schema("Usage filter.", &["all", "used", "unused"]),
            ),
            (
                "kind",
                enum_schema(
                    "Asset type filter.",
                    &["image", "model", "audio", "script", "data", "file"],
                ),
            ),
        ]),
        AgentToolKind::Read,
        None,
    );
    if project_type == Some(ProjectType::Game) && (intent.authoring || intent.assets) {
        pack.push_native(
            "assets_recommend",
            "Rank imported assets for an authoring intent using transparent name, semantic-token, type, and usage evidence.",
            object_schema(
                Map::from_iter([
                    ("intent".to_string(), string_schema("What is needed, such as supermarket shelf or checkout counter.")),
                    ("kind".to_string(), enum_schema("Optional preferred asset kind.", &["image", "model", "audio", "script", "data", "file"])),
                    ("limit".to_string(), integer_schema("Maximum recommendations.", 1, 24)),
                ]),
                &["intent"],
            ),
            AgentToolKind::Read,
            None,
        );
    }
    if !intent.authoring || intent.prefab || intent.assets {
        pack.push_native(
            "asset_inspect",
            "Inspect one imported asset by path and report its type, usage, and scene references.",
            object_schema(
                Map::from_iter([("target".to_string(), string_schema("Imported asset path."))]),
                &["target"],
            ),
            AgentToolKind::Read,
            None,
        );
    }
    if intent.scripting || intent.gameplay || !intent.authoring {
        pack.push_native(
            "scripts_catalog",
            "List project scripts and the scene entities that reference each script.",
            paginated_schema(&[("query", string_schema("Optional script query."))]),
            AgentToolKind::Read,
            None,
        );
    }
    if project_type == Some(ProjectType::Game) {
        pack.push_native(
            "project_health",
            "Check the requested scene scope for duplicate names, broken hierarchy links, invalid display names, missing imported assets, and missing attached scripts. Use root for a focused audit.",
            object_schema(
                Map::from_iter([
                    ("root".to_string(), string_schema("Optional root ID, UUID, name, or path to scope the audit.")),
                    ("strict".to_string(), boolean_schema("Also enforce unique names within the scope.")),
                ]),
                &[],
            ),
            AgentToolKind::Read,
            None,
        );
        pack.push_native(
            "game_completion_audit",
            "Audit whether the Game foundation is actually playable rather than geometry alone. Reports renderable geometry, camera/startup camera, authored lighting, attached scripts, gameplay signals, enabled systems, missing pieces, and the next required actions. Call this after a gameplay build and before claiming completion.",
            object_schema(Map::new(), &[]),
            AgentToolKind::Read,
            None,
        );
    }
    if project_type == Some(ProjectType::Game) {
        pack.push_native(
            "scene_verify",
            "Verify exact scene targets, scope counts, hierarchy links, finite transforms, primitive types, transforms, colors, and optional AABB collisions after an operation.",
            object_schema(
                Map::from_iter([
                    (
                        "root".to_string(),
                        string_schema("Optional root ID, UUID, name, or path to scope the verification."),
                    ),
                    (
                        "targets".to_string(),
                        serde_json::json!({"type": "array", "items": {"type": "string"}, "maxItems": 64}),
                    ),
                    (
                        "expected_count".to_string(),
                        integer_schema("Expected number of matching targets.", 0, 10000),
                    ),
                    (
                        "expected".to_string(),
                        expected_scene_state_schema(),
                    ),
                    (
                        "expected_names".to_string(),
                        serde_json::json!({"type": "array", "items": {"type": "string"}, "maxItems": 128}),
                    ),
                    (
                        "check_collisions".to_string(),
                        boolean_schema("Check conservative world-space AABB overlaps."),
                    ),
                    (
                        "strict".to_string(),
                        boolean_schema("Require unique names within the requested scope."),
                    ),
                ]),
                &[],
            ),
            AgentToolKind::Read,
            None,
        );
    }
    if intent.capabilities || project_type.is_some() {
        pack.push_native(
            "capabilities_search",
            "Search the Agent tools currently available for this project, intent, and permission mode.",
            object_schema(
                Map::from_iter([(
                    "query".to_string(),
                    string_schema("Capability or command query."),
                )]),
                &[],
            ),
            AgentToolKind::Read,
            None,
        );
    }
}
impl AgentToolExecutor<'_> {
    pub(super) fn execute_capabilities_call(
        &mut self,
        call_id: &str,
        arguments: &Value,
        mode: ToolExecutionMode,
    ) -> Result<AgentToolResult, String> {
        let command_name = arguments
            .get("name")
            .or_else(|| arguments.get("command"))
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|name| !name.is_empty())
            .ok_or_else(|| "capabilities_call requires a catalog command name.".to_string())?
            .trim_start_matches('/')
            .to_ascii_lowercase();
        if command_name.starts_with("runtime.") {
            return Err(
                "Runtime commands are not available through the authoring harness.".to_string(),
            );
        }
        let command = self
            .catalog
            .find(&command_name)
            .ok_or_else(|| {
                format!(
                    "Unknown capability '{command_name}'. Call capabilities_search first; no command was executed."
                )
            })?
            .clone();
        let kind = catalog_tool_kind(&command);
        if kind == AgentToolKind::Mutation && mode == ToolExecutionMode::Inspect {
            return Err(format!(
                "Capability '{command_name}' is unavailable in Inspect mode."
            ));
        }
        let mut params = arguments
            .get("params")
            .cloned()
            .unwrap_or_else(|| serde_json::json!({}));
        if !params.is_object() {
            return Err("capabilities_call.params must be a JSON object.".to_string());
        }
        if matches!(command.name.as_str(), "game.add" | "game.update") {
            normalize_scene_arguments(&mut params)?;
        }
        self.execute_gateway(call_id, &command.name, params, kind, mode)
    }

    pub(super) fn execute_capabilities_search(
        &self,
        arguments: &Value,
        mode: ToolExecutionMode,
    ) -> AgentToolResult {
        let query = arguments
            .get("query")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .trim()
            .to_ascii_lowercase();
        let domain = self
            .project
            .as_ref()
            .map(|project| match project.project_type {
                ProjectType::Game => "game",
                ProjectType::Electronics => "electronics",
            });
        let mut items = Vec::new();
        let mut matches = self
            .catalog
            .commands
            .iter()
            .filter(|command| {
                command.domain == "shared" || Some(command.domain.as_str()) == domain
            })
            .filter(|command| {
                query.is_empty()
                    || command.name.to_ascii_lowercase().contains(&query)
                    || command
                        .aliases
                        .iter()
                        .any(|alias| alias.to_ascii_lowercase().contains(&query))
                    || command.category.to_ascii_lowercase().contains(&query)
                    || command.domain.to_ascii_lowercase().contains(&query)
                    || command.description_key.to_ascii_lowercase().contains(&query)
                    || command
                        .examples
                        .iter()
                        .any(|example| example.to_ascii_lowercase().contains(&query))
            })
            .collect::<Vec<_>>();
        matches.sort_by(|left, right| left.name.cmp(&right.name));
        for command in matches {
            if command.name.starts_with("runtime.") {
                continue;
            }
            if command.domain != "shared" && Some(command.domain.as_str()) != domain {
                continue;
            }
            let enabled = self.routes.iter().find_map(|(tool, route)| {
                (route.command_name.as_deref() == Some(command.name.as_str())).then_some(tool)
            });
            let kind = catalog_tool_kind(command);
            items.push(serde_json::json!({
                "tool": enabled,
                "command": command.name,
                "domain": command.domain,
                "category": command.category,
                "kind": match kind {
                    AgentToolKind::Read => "read",
                    AgentToolKind::Mutation => "mutation"
                },
                "enabled_in_pack": enabled.is_some(),
                "execution": if enabled.is_some() {
                    "direct_tool"
                } else if kind == AgentToolKind::Read || mode != ToolExecutionMode::Inspect {
                    "capabilities_call"
                } else {
                    "unavailable_in_inspect"
                },
            }));
            if items.len() >= 96 {
                break;
            }
        }
        let enabled_count = items
            .iter()
            .filter(|item| item.get("enabled_in_pack").and_then(Value::as_bool) == Some(true))
            .count();
        AgentToolResult::success(
            format!(
                "Found {} capability(ies); {} are already exposed as typed tools and the rest can be called through capabilities_call.",
                items.len(),
                enabled_count
            ),
            serde_json::json!({
                "query": query,
                "items": items,
                "enabled_count": enabled_count,
                "next": "Use capabilities_call with an exact command name and params for a capability not exposed as a typed tool.",
            }),
        )
    }
}
