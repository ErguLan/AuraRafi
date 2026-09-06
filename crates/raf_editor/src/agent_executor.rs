//! UI-independent Agent tool layer for the native editor.
//!
//! Provider tool calls stay structured here. Mutations enter the same typed
//! command gateway used by RafUI, CLI and MCP; no tool is converted to a CLI
//! command line and no transport is spawned inside the editor.

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;

use raf_ai::agent_runtime::{AgentToolKind, AgentToolResult, ToolExecutionMode, ToolExecutor};
use raf_ai::openai_client::{OpenAiFunction, OpenAiTool};
use raf_ai::provider::AgentMode;
use raf_core::i18n::t;
use raf_core::project::{Project, ProjectType};
use raf_core::scene::SceneGraph;
use raf_core::{
    CommandEndpoint, CommandSource, EngineCommandRequest, EngineCommandResponse, ExecutionBudget,
    Language, TransactionLedger,
};
use serde_json::{Map, Value};

use crate::agent_artifacts::capture_viewport_artifact;
use crate::agent_context::AgentObservationContext;
use crate::commands::{
    catalog::{CommandCatalog, CommandDefinition},
    game::{
        self, GameCommandContext, GameViewportPort, HeadlessGameViewportPort, SceneSelectionState,
    },
    gateway::{CommandGateway, ParsedCommandExecutor},
    output::CommandOutput,
    parser::ParsedCommand,
    script::{self, ScriptCommandContext},
};
use crate::electronics_controller::NativeElectronicsEditor;
use crate::panels::viewport_controller::NativeGameViewportController;
use raf_render::bridge::RenderRuntime;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AgentEditorAction {
    Undo,
    Redo,
}

#[derive(Debug, Clone)]
pub struct AgentProjectContext {
    pub root: PathBuf,
    pub project_type: ProjectType,
}

impl AgentProjectContext {
    pub fn from_project(project: Option<&Project>) -> Option<Self> {
        project.map(|project| Self {
            root: project.path.clone(),
            project_type: project.project_type,
        })
    }
}

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
    fn push_native(
        &mut self,
        name: &str,
        description: &str,
        parameters: Value,
        kind: AgentToolKind,
        command_name: Option<&str>,
    ) {
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

    fn push_catalog(
        &mut self,
        command: &CommandDefinition,
        language: Language,
        kind: AgentToolKind,
    ) {
        let name = sanitize_tool_name(&command.name);
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

/// Keep the most useful perception and semantic authoring tools inside the
/// provider budget. Low-level convenience commands remain reachable through
/// `scene_batch`, but must not displace the tools that let the model see scale,
/// bounds and structure before it acts.
fn order_contextual_tools(tools: &mut Vec<OpenAiTool>) {
    const PRIORITY: &[&str] = &[
        "project_summary",
        "scene_outline",
        "scene_query",
        "scene_spatial_map",
        "scene_design_audit",
        "scene_inspect",
        "selection_get",
        "viewport_capture",
        "assets_catalog",
        "asset_inspect",
        "scripts_catalog",
        "project_health",
        "scene_verify",
        "game_validate_layout",
        "scene_build",
        "scene_reconcile",
        "scene_repair",
        "scene_batch",
        "scene_create_group",
        "scene_create",
        "scene_update",
        "scene_reparent",
        "scene_arrange",
        "scene_delete",
        "scene_duplicate",
        "scene_instantiate_prefab",
    ];
    tools.sort_by_key(|tool| {
        PRIORITY
            .iter()
            .position(|name| *name == tool.function.name)
            .unwrap_or(PRIORITY.len())
    });
}

#[derive(Debug, Clone, Copy, Default)]
struct PromptIntent {
    authoring: bool,
    scripting: bool,
    pcb: bool,
    capabilities: bool,
    layout: bool,
    prefab: bool,
    assets: bool,
}

pub struct AgentToolExecutor<'a> {
    pub scene: &'a mut SceneGraph,
    pub selection: &'a mut SceneSelectionState,
    pub viewport: &'a mut NativeGameViewportController,
    pub graphics: &'a mut RenderRuntime,
    pub electronics: Option<&'a mut NativeElectronicsEditor>,
    pub project: Option<AgentProjectContext>,
    pub project_info: Option<&'a Project>,
    pub project_assets: &'a [String],
    pub active_session: &'a str,
    pub catalog_pending: bool,
    pub catalog_error: Option<&'a str>,
    pub catalog: &'a CommandCatalog,
    pub language: Language,
    pub routes: HashMap<String, AgentToolRoute>,
    pub ledger: &'a mut TransactionLedger,
    pub editor_actions: &'a mut Vec<AgentEditorAction>,
    pub canvas_changed: &'a mut bool,
}

impl AgentToolExecutor<'_> {
    pub fn build_tool_pack(
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

        match project_type {
            Some(ProjectType::Game) => {
                if mode != AgentMode::Inspect && intent.authoring {
                    add_game_mutation_tools(&mut pack, intent);
                }
                if intent.scripting {
                    for name in [
                        "script.create",
                        "script.attach",
                        "script.detach",
                        "script.validate",
                    ] {
                        if let Some(command) = catalog.find(name) {
                            let kind = if name == "script.validate" {
                                AgentToolKind::Read
                            } else {
                                AgentToolKind::Mutation
                            };
                            if kind == AgentToolKind::Read || mode != AgentMode::Inspect {
                                pack.push_catalog(command, language, kind);
                            }
                        }
                    }
                }
            }
            Some(ProjectType::Electronics) => {
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
        const MAX_CONTEXTUAL_TOOLS: usize = 22;
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

    fn observation_context(&self) -> AgentObservationContext<'_> {
        AgentObservationContext {
            scene: self.scene,
            selection: self.selection,
            project: self.project_info,
            assets: self.project_assets,
            active_session: self.active_session,
            revision: self.ledger.revision(),
            catalog_pending: self.catalog_pending,
            catalog_error: self.catalog_error,
        }
    }

    fn execute_gateway(
        &mut self,
        call_id: &str,
        command_name: &str,
        params: Value,
        kind: AgentToolKind,
        mode: ToolExecutionMode,
    ) -> Result<AgentToolResult, String> {
        let before = raf_core::agent_context::scene_fingerprint(self.scene);
        let expected_params = params.clone();
        let domain = self
            .catalog
            .find(command_name)
            .map(|command| command.domain.as_str())
            .unwrap_or_else(|| {
                if command_name.starts_with("game.") {
                    "game"
                } else {
                    "shared"
                }
            });

        if mode == ToolExecutionMode::Preview && domain == "electronics" {
            let mut result = AgentToolResult::success(
                format!("Previewed {command_name}; no Electronics state was changed."),
                serde_json::json!({"command": command_name, "params": params}),
            );
            result.preview = true;
            result.diff = Some(serde_json::json!({"preview": true, "would_change": true}));
            result.revision = Some(self.ledger.revision());
            return Ok(result);
        }

        let executor = AgentParsedExecutor {
            scene: self.scene,
            selection: self.selection,
            viewport: self.viewport,
            electronics: self.electronics.as_deref_mut(),
            project: self.project.as_ref(),
            editor_actions: self.editor_actions,
        };
        let mut gateway = CommandGateway {
            executor,
            ledger: std::mem::take(self.ledger),
        };
        let mut request = EngineCommandRequest::new(command_name, params, CommandSource::Agent);
        request.session = Some(self.active_session.to_string());
        request.confirm = mode == ToolExecutionMode::Apply;
        request.dry_run = kind == AgentToolKind::Mutation && mode == ToolExecutionMode::Preview;
        request.expected_revision = Some(gateway.ledger.revision());
        request.budget = Some(native_agent_budget());
        if !request.dry_run {
            request.idempotency_key = Some(call_id.to_string());
        }
        let response = gateway.execute(request);
        *self.ledger = gateway.ledger;
        let after = raf_core::agent_context::scene_fingerprint(self.scene);
        let scene_diff = raf_core::agent_context::scene_diff(&before, &after);
        if let (Some(transaction_id), Some(diff)) = (response.transaction_id, scene_diff.as_ref()) {
            self.ledger.attach_diff(transaction_id, diff.clone());
        }
        let observed_scene_change = scene_diff.is_some();
        let mut result = response_to_tool_result(response, request_preview(mode, kind), scene_diff);
        if result.ok && kind == AgentToolKind::Mutation {
            let is_game_mutation = command_name.starts_with("game.");
            let scene_changed =
                observed_scene_change && is_game_mutation && mode == ToolExecutionMode::Apply;
            if is_game_mutation {
                let replayed = result
                    .data
                    .get("replayed")
                    .and_then(Value::as_bool)
                    .unwrap_or(false);
                if result.changed && !scene_changed && !replayed {
                    if !matches!(command_name, "game.reconcile" | "game.repair") {
                        result.warnings.push(
                            "The game command reported a change, but the scene state did not change."
                                .to_string(),
                        );
                    }
                }
                let no_change_failure = mode == ToolExecutionMode::Apply
                    && expects_observable_scene_change(command_name)
                    && !scene_changed
                    && !replayed
                    && !matches!(command_name, "game.reconcile" | "game.repair");
                if no_change_failure {
                    result.ok = false;
                    result.summary = format!(
                        "No scene change was observed after {command_name}; inspect the result before continuing."
                    );
                    result.warnings.push(
                        "The mutation was reported as successful, but no observable scene diff was found."
                            .to_string(),
                    );
                }
                result.changed = scene_changed;
                let postcondition_failures = if mode == ToolExecutionMode::Apply
                    && (scene_changed || matches!(command_name, "game.reconcile" | "game.repair"))
                    && !replayed
                {
                    verify_postconditions(command_name, &expected_params, &result.data, self.scene)
                } else {
                    Vec::new()
                };
                if !postcondition_failures.is_empty() {
                    result.ok = false;
                    result.summary = format!(
                        "{command_name} changed the scene, but its result did not match the requested state."
                    );
                    result.warnings.extend(
                        postcondition_failures
                            .iter()
                            .map(|failure| format!("Postcondition: {failure}")),
                    );
                }
                result.verification = Some(serde_json::json!({
                    "status": if request_preview(mode, kind) {
                        "preview"
                    } else if replayed {
                        "replayed"
                    } else if !postcondition_failures.is_empty() {
                        "failed"
                    } else if no_change_failure {
                        "failed"
                    } else if scene_changed {
                        "passed"
                    } else {
                        "no_change"
                    },
                    "scene_changed": scene_changed,
                    "revision": self.ledger.revision(),
                    "scene_entities": live_scene_entity_count(self.scene),
                    "postcondition_failures": postcondition_failures
                }));
            } else if mode == ToolExecutionMode::Apply {
                *self.canvas_changed = true;
                result.verification = Some(serde_json::json!({
                    "status": "passed",
                    "revision": self.ledger.revision(),
                    "scene_entities": live_scene_entity_count(self.scene)
                }));
            }
            if scene_changed {
                *self.canvas_changed = true;
            }
        }
        // Domain failures remain structured tool results so the model receives
        // the command title, diagnostics, revision, and any partial evidence.
        // `Err` is reserved for adapter/runtime failures such as an unknown
        // route or an unavailable executor.
        Ok(result)
    }
}

fn native_agent_budget() -> ExecutionBudget {
    ExecutionBudget {
        max_tool_calls: None,
        max_milliseconds: None,
        max_scene_operations: Some(512),
        max_scene_entities: Some(2_048),
        max_result_bytes: Some(65_536),
        profile: Some("native-agent-balanced".to_string()),
    }
}

impl ToolExecutor for AgentToolExecutor<'_> {
    fn kind(&self, name: &str) -> AgentToolKind {
        self.routes
            .get(name)
            .map(|route| route.kind)
            .unwrap_or(AgentToolKind::Mutation)
    }

    fn execute(
        &mut self,
        call_id: &str,
        name: &str,
        arguments: Value,
        mode: ToolExecutionMode,
    ) -> Result<AgentToolResult, String> {
        let route = self
            .routes
            .get(name)
            .cloned()
            .ok_or_else(|| format!("Unknown Agent tool: {name}"))?;
        if route.kind == AgentToolKind::Mutation && mode == ToolExecutionMode::Inspect {
            return Err(format!("Tool '{name}' is unavailable in Inspect mode."));
        }

        if name == "viewport_capture" {
            let refresh_requested = arguments
                .get("refresh")
                .and_then(Value::as_bool)
                .unwrap_or(true);
            if refresh_requested {
                self.graphics.request_frame(
                    raf_render::api_graphic_basic::FrameInvalidation::DOCUMENT
                        | raf_render::api_graphic_basic::FrameInvalidation::UI,
                );
            }
            let mut result =
                capture_viewport_artifact(self.graphics, self.project_info, self.language);
            result.revision = Some(self.ledger.revision());
            result.data["refresh_requested"] = Value::Bool(refresh_requested);
            result.data["capture_is_last_completed_frame"] = Value::Bool(refresh_requested);
            if refresh_requested {
                result.warnings.push(
                    "A fresh viewport frame was requested; this synchronous tool call returns the last completed frame.".to_string(),
                );
            }
            return Ok(result);
        }

        if let Some(mut result) = self.observation_context().execute(name, &arguments) {
            result.revision = Some(self.ledger.revision());
            return Ok(result);
        }
        if name == "capabilities_search" {
            let query = arguments
                .get("query")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_ascii_lowercase();
            let items = self
                .routes
                .iter()
                .filter(|(name, route)| {
                    query.is_empty()
                        || name.to_ascii_lowercase().contains(&query)
                        || route
                            .command_name
                            .as_deref()
                            .unwrap_or_default()
                            .to_ascii_lowercase()
                            .contains(&query)
                })
                .map(|(name, route)| {
                    serde_json::json!({
                        "tool": name,
                        "command": route.command_name,
                        "kind": match route.kind {
                            AgentToolKind::Read => "read",
                            AgentToolKind::Mutation => "mutation"
                        }
                    })
                })
                .collect::<Vec<_>>();
            return Ok(AgentToolResult::success(
                format!("Found {} available Agent capabilities.", items.len()),
                serde_json::json!({"items": items}),
            ));
        }

        let command_name = route
            .command_name
            .as_deref()
            .ok_or_else(|| format!("Agent tool '{name}' has no command route."))?;
        let params = translate_arguments(name, command_name, arguments)?;
        self.execute_gateway(call_id, command_name, params, route.kind, mode)
    }

    fn describe(&self, name: &str) -> String {
        self.routes
            .get(name)
            .and_then(|route| route.command_name.as_deref())
            .and_then(|command| self.catalog.find(command))
            .map(|definition| {
                format!(
                    "{} (domain: {}, category: {})",
                    definition.name, definition.domain, definition.category
                )
            })
            .unwrap_or_else(|| format!("Native Agent tool {name}"))
    }
}

struct AgentParsedExecutor<'a> {
    scene: &'a mut SceneGraph,
    selection: &'a mut SceneSelectionState,
    viewport: &'a mut NativeGameViewportController,
    electronics: Option<&'a mut NativeElectronicsEditor>,
    project: Option<&'a AgentProjectContext>,
    editor_actions: &'a mut Vec<AgentEditorAction>,
}

impl AgentParsedExecutor<'_> {
    fn execute_live(&mut self, command: &ParsedCommand) -> CommandOutput {
        if command.name.starts_with("game.") {
            let mut context = GameCommandContext {
                scene: self.scene,
                selection: self.selection,
                viewport: self.viewport as &mut dyn GameViewportPort,
            };
            game::execute(&command.name, command, &mut context)
        } else if command.name.starts_with("electronics.") || command.name.starts_with("pcb.") {
            self.electronics
                .as_deref_mut()
                .map(|editor| editor.execute_catalog_command(&command.name, command))
                .unwrap_or_else(|| {
                    CommandOutput::error(
                        "Agent command",
                        "The native Electronics document is not mounted for this session.",
                    )
                })
        } else if command.name.starts_with("script.") {
            let assets_root = self.project.map(|project| project.root.join("assets"));
            let mut context = ScriptCommandContext {
                scene: self.scene,
                assets_root: assets_root.as_deref(),
            };
            script::execute(&command.name, command, &mut context)
        } else {
            match command.name.as_str() {
                "undo" => {
                    self.editor_actions.push(AgentEditorAction::Undo);
                    CommandOutput::info(
                        "Undo",
                        vec!["Undo queued.".to_string()],
                        serde_json::json!({"ok": true, "queued": true}),
                    )
                }
                "redo" => {
                    self.editor_actions.push(AgentEditorAction::Redo);
                    CommandOutput::info(
                        "Redo",
                        vec!["Redo queued.".to_string()],
                        serde_json::json!({"ok": true, "queued": true}),
                    )
                }
                _ => CommandOutput::error(
                    "Agent command",
                    format!("Command '{}' is not mounted.", command.name),
                ),
            }
        }
    }
}

impl ParsedCommandExecutor for AgentParsedExecutor<'_> {
    fn scene_entity_count(&self) -> Option<usize> {
        Some(self.scene.all_live_ids().len())
    }

    fn execute_parsed(&mut self, command: &ParsedCommand) -> CommandOutput {
        self.execute_live(command)
    }

    fn preview_parsed(&mut self, command: &ParsedCommand) -> CommandOutput {
        if command.name.starts_with("game.") {
            let mut scene = self.scene.clone();
            let mut selection = self.selection.clone();
            let mut viewport = HeadlessGameViewportPort::default();
            viewport.set_selected_ids(selection.selected_nodes.clone());
            let mut context = GameCommandContext {
                scene: &mut scene,
                selection: &mut selection,
                viewport: &mut viewport,
            };
            game::execute(&command.name, command, &mut context)
        } else {
            CommandOutput::changed(
                format!("Preview {}", command.name),
                vec!["No project state was changed.".to_string()],
                serde_json::json!({
                    "command": command.name,
                    "params": command.args,
                    "preview": true
                }),
            )
        }
    }
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
            "scene_query",
            "Find scene entities by name, path, kind, source asset, or current selection without scanning files.",
            paginated_schema(&[
                ("query", string_schema("Name, path, or asset query.")),
                ("kind", string_schema("Optional primitive or folder kind.")),
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
            "Capture the Game viewport as a PNG artifact for visual inspection. Set refresh=true to request a fresh editor frame; this never starts Play mode or changes the scene.",
            object_schema(
                Map::from_iter([(
                    "refresh".to_string(),
                    boolean_schema("Request a fresh document frame before the capture."),
                )]),
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
    if intent.scripting || !intent.authoring {
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
    if intent.capabilities {
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

fn add_game_mutation_tools(pack: &mut AgentToolPack, intent: PromptIntent) {
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
                                "name": {"type":"string","enum":["scene_create","scene_create_group","scene_update","scene_reparent","scene_delete","scene_duplicate","scene_arrange","scene_instantiate_prefab"]},
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
        "Apply an ordered, atomic batch of scene_create, scene_create_group, scene_update, scene_reparent, scene_delete, scene_duplicate, scene_arrange, or scene_instantiate_prefab operations. The live scene is committed only when every operation succeeds. Use explicit nested transform and color values; never flatten them into text.",
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
                                "scene_reparent", "scene_delete", "scene_duplicate",
                                "scene_arrange", "scene_instantiate_prefab"
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
        "Create one native scene entity with an explicit primitive, local transform, color, and optional group parent. Use scene_build for modular multi-part structures.",
        object_schema(
            Map::from_iter([
                (
                    "kind".to_string(),
                    enum_schema(
                        "Entity primitive.",
                        &["empty", "cube", "sphere", "plane", "cylinder"],
                    ),
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
        "Update an existing entity by stable ID, UUID, name, or path. Only supplied fields change.",
        object_schema(
            Map::from_iter([
                ("target".to_string(), string_schema("Entity target.")),
                ("name".to_string(), string_schema("Optional new name.")),
                ("transform".to_string(), transform),
                (
                    "color_rgba".to_string(),
                    color_schema("Optional RGBA channels from 0 to 255."),
                ),
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
    for (tool, command, description) in [
        (
            "scene_delete",
            "game.delete",
            "Delete one scene entity and its descendants.",
        ),
        (
            "scene_duplicate",
            "game.duplicate",
            "Duplicate one scene entity and its descendants.",
        ),
    ] {
        pack.push_native(
            tool,
            description,
            object_schema(
                Map::from_iter([("target".to_string(), string_schema("Entity target."))]),
                &["target"],
            ),
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
            "scene_instantiate_prefab",
            "Instantiate one registered native prefab/manifest into the scene.",
            object_schema(
                Map::from_iter([
                    ("kind".to_string(), string_schema("Registered prefab kind.")),
                    ("name".to_string(), string_schema("Optional root name.")),
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

fn translate_arguments(
    name: &str,
    _command_name: &str,
    mut arguments: Value,
) -> Result<Value, String> {
    if !arguments.is_object() {
        return Err(format!("Tool '{name}' arguments must be a JSON object."));
    }
    match name {
        "scene_create" | "scene_update" => normalize_scene_arguments(&mut arguments)?,
        "scene_build" => translate_build(&mut arguments)?,
        "scene_reconcile" => translate_reconcile(&mut arguments)?,
        "scene_repair" => translate_repair(&mut arguments)?,
        "scene_batch" => translate_batch(&mut arguments)?,
        _ => {}
    }
    Ok(arguments)
}

/// Normalize only the semantic discriminator at the Agent boundary.
///
/// Structured transform and color values intentionally remain intact until
/// they reach the game command kernel. Flattening them into a lossy string
/// map here used to make malformed or provider-shaped values disappear
/// silently, which is how a successful-looking call could create defaults at
/// the origin. The kernel accepts both this semantic shape and legacy CLI
/// scalar fields.
fn normalize_scene_arguments(arguments: &mut Value) -> Result<(), String> {
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

fn translate_build(arguments: &mut Value) -> Result<(), String> {
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

fn translate_batch(arguments: &mut Value) -> Result<(), String> {
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
            "scene_instantiate_prefab" | "game.generate_prefab" => "game.generate_prefab",
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

fn response_to_tool_result(
    response: EngineCommandResponse,
    preview: bool,
    scene_diff: Option<Value>,
) -> AgentToolResult {
    let references = collect_references(&response.data);
    let details = compact_response_lines(&response.lines);
    let summary = if details.is_empty() {
        response.title.clone()
    } else {
        format!("{}: {}", response.title, details[0])
    };
    AgentToolResult {
        ok: response.ok,
        summary,
        data: compact_command_data(response.data),
        references,
        changed: response.changed,
        revision: Some(response.revision),
        diff: response.diff.or(scene_diff),
        verification: response
            .verification
            .and_then(|verification| serde_json::to_value(verification).ok()),
        warnings: response.warnings,
        details,
        preview,
    }
}

fn compact_response_lines(lines: &[String]) -> Vec<String> {
    raf_core::agent_context::compact_result_lines(lines)
}

fn compact_command_data(data: Value) -> Value {
    let Some(object) = data.as_object() else {
        return raf_core::agent_context::compact_result_data(&data);
    };
    if let Some(entity) = object.get("entity").and_then(Value::as_object) {
        return serde_json::json!({
            "entity": {
                "id": entity.get("id"),
                "uuid": entity.get("uuid"),
                "name": entity.get("name"),
                "path": entity.get("path"),
                "parent_id": entity.get("parent_id"),
                "parent_path": entity.get("parent_path"),
                "children": entity.get("children"),
                "child_names": entity.get("child_names"),
                "ref": entity.get("ref"),
                "kind": entity.get("kind"),
                "is_folder": entity.get("is_folder"),
                "primitive": entity.get("primitive"),
                "position": entity.get("position"),
                "rotation_deg": entity.get("rotation_deg"),
                "scale": entity.get("scale"),
                "world_position": entity.get("world_position"),
                "world_bounds": entity.get("world_bounds"),
                "color_rgba": entity.get("color_rgba"),
                "visible": entity.get("visible"),
                "locked": entity.get("locked"),
                "source_asset": entity.get("source_asset"),
                "scripts": entity.get("scripts"),
                "semantic_role": entity.get("semantic_role"),
                "stable_key": entity.get("stable_key"),
                "tags": entity.get("tags"),
                "agent_origin": entity.get("agent_origin")
            }
        });
    }
    if object.get("operations").and_then(Value::as_array).is_some() {
        let mut compact = serde_json::Map::new();
        for key in [
            "ok",
            "created_ids",
            "entity_count",
            "build",
            "reconcile",
            "repair",
        ] {
            if let Some(value) = object.get(key) {
                compact.insert(key.to_string(), value.clone());
            }
        }
        if let Some(operations) = object.get("operations").and_then(Value::as_array) {
            compact.insert(
                "operations".to_string(),
                Value::Array(operations.iter().map(compact_operation_result).collect()),
            );
        }
        if let Some(entities) = object.get("entities").and_then(Value::as_array) {
            compact.insert(
                "entities".to_string(),
                Value::Array(entities.iter().map(compact_entity_data).collect()),
            );
        }
        return Value::Object(compact);
    }
    raf_core::agent_context::compact_result_data(&data)
}

fn compact_operation_result(operation: &Value) -> Value {
    let Some(operation) = operation.as_object() else {
        return operation.clone();
    };
    let mut compact = serde_json::Map::new();
    for key in ["name", "ok", "summary", "changed"] {
        if let Some(value) = operation.get(key) {
            compact.insert(key.to_string(), value.clone());
        }
    }
    if let Some(entity) = operation.get("entity") {
        compact.insert("entity".to_string(), compact_entity_data(entity));
    }
    Value::Object(compact)
}

fn compact_entity_data(entity: &Value) -> Value {
    let Some(entity) = entity.as_object() else {
        return entity.clone();
    };
    let mut compact = serde_json::Map::new();
    for key in [
        "id",
        "uuid",
        "ref",
        "name",
        "path",
        "parent_id",
        "parent_path",
        "children",
        "child_names",
        "kind",
        "is_folder",
        "primitive",
        "position",
        "rotation_deg",
        "scale",
        "world_position",
        "color_rgba",
        "visible",
        "locked",
        "source_asset",
        "source_schema_version",
        "scripts",
        "semantic_role",
        "stable_key",
        "tags",
        "agent_origin",
    ] {
        if let Some(value) = entity.get(key) {
            compact.insert(key.to_string(), value.clone());
        }
    }
    if let Some(mesh) = entity.get("mesh").and_then(Value::as_object) {
        let mut mesh_summary = serde_json::Map::new();
        for key in ["vertex_count", "index_count"] {
            if let Some(value) = mesh.get(key) {
                mesh_summary.insert(key.to_string(), value.clone());
            }
        }
        if !mesh_summary.is_empty() {
            compact.insert("mesh".to_string(), Value::Object(mesh_summary));
        }
    }
    Value::Object(compact)
}

fn collect_references(value: &Value) -> Vec<String> {
    let mut references = Vec::new();
    collect_reference_field(value, "uuid", &mut references);
    collect_reference_field(value, "path", &mut references);
    references.sort();
    references.dedup();
    references.truncate(64);
    references
}

fn collect_reference_field(value: &Value, key: &str, output: &mut Vec<String>) {
    match value {
        Value::Array(values) => {
            for value in values {
                collect_reference_field(value, key, output);
            }
        }
        Value::Object(values) => {
            if let Some(value) = values.get(key).and_then(Value::as_str) {
                output.push(value.to_string());
            }
            for value in values.values() {
                collect_reference_field(value, key, output);
            }
        }
        _ => {}
    }
}

fn verify_postconditions(
    command_name: &str,
    params: &Value,
    data: &Value,
    scene: &SceneGraph,
) -> Vec<String> {
    match command_name {
        "game.add" | "game.create_group" | "game.update" | "game.reparent" => data
            .get("entity")
            .map(|entity| verify_entity_postcondition(params, entity, scene))
            .unwrap_or_else(|| vec!["The mutation result did not include an entity.".to_string()]),
        "game.batch" | "game.repair" => {
            let operations = params
                .get("operations")
                .and_then(Value::as_array)
                .cloned()
                .unwrap_or_default();
            verify_batch_postconditions(&operations, data, scene)
        }
        "game.build" => {
            let operations = build_postcondition_operations(params);
            verify_batch_postconditions(&operations, data, scene)
        }
        "game.reconcile" => verify_reconcile_postconditions(params, scene),
        _ => Vec::new(),
    }
}

fn verify_reconcile_postconditions(params: &Value, scene: &SceneGraph) -> Vec<String> {
    let mut failures = Vec::new();
    let Some(object) = params.as_object() else {
        return vec!["reconcile params must be an object.".to_string()];
    };
    let default_parent = object.get("parent");
    for key in ["groups", "entities"] {
        let Some(items) = object.get(key).and_then(Value::as_array) else {
            continue;
        };
        for (index, item) in items.iter().enumerate() {
            let Some(stable_key) = item.get("stable_key").and_then(Value::as_str) else {
                failures.push(format!("{key}[{index}] has no stable_key."));
                continue;
            };
            let Some(id) = resolve_scene_target(scene, &format!("key:{stable_key}")) else {
                failures.push(format!(
                    "stable_key '{stable_key}' was not found after reconcile."
                ));
                continue;
            };
            let Some(node) = scene.get(id) else {
                failures.push(format!(
                    "stable_key '{stable_key}' resolved to an invalid entity."
                ));
                continue;
            };
            if key == "groups" && !node.is_folder {
                failures.push(format!("stable_key '{stable_key}' is not a group."));
            }
            if key == "entities" && node.is_folder {
                failures.push(format!(
                    "stable_key '{stable_key}' is a group, not an entity."
                ));
            }
            let Some(mut expected) = item.as_object().cloned() else {
                failures.push(format!("{key}[{index}] is not an object."));
                continue;
            };
            let parent_value = item.get("parent").or(default_parent);
            if let Some(parent) = parent_value.and_then(Value::as_str) {
                let expected_parent = if parent.trim().is_empty()
                    || matches!(
                        parent.trim().to_ascii_lowercase().as_str(),
                        "root" | "scene" | "none" | "null"
                    ) {
                    None
                } else {
                    resolve_scene_target(scene, parent)
                };
                if parent.trim() != ""
                    && !matches!(
                        parent.trim().to_ascii_lowercase().as_str(),
                        "root" | "scene" | "none" | "null"
                    )
                    && expected_parent.is_none()
                {
                    failures.push(format!(
                        "parent '{parent}' was not found for '{stable_key}'."
                    ));
                } else if node.parent != expected_parent {
                    failures.push(format!("'{stable_key}' has an unexpected parent."));
                }
            }
            if !expected.contains_key("parent") {
                if let Some(parent) = default_parent {
                    expected.insert("parent".to_string(), parent.clone());
                }
            }
            let inspection = raf_core::agent_context::scene_inspect(
                scene,
                &serde_json::json!({"target": format!("entity:{}", node.uuid)}),
                &[],
            );
            if let Some(entity) = inspection.data.get("entity") {
                failures.extend(
                    verify_entity_postcondition(&Value::Object(expected), entity, scene)
                        .into_iter()
                        .map(|failure| format!("'{stable_key}': {failure}")),
                );
            } else {
                failures.push(format!(
                    "stable_key '{stable_key}' could not be inspected after reconcile."
                ));
            }
        }
    }
    failures
}

fn build_postcondition_operations(params: &Value) -> Vec<Value> {
    let Some(object) = params.as_object() else {
        return Vec::new();
    };
    let default_parent = object.get("parent").cloned();
    let mut operations = Vec::new();
    for key in ["groups", "entities"] {
        let operation_name = if key == "groups" {
            "game.create_group"
        } else {
            "game.add"
        };
        let Some(items) = object.get(key).and_then(Value::as_array) else {
            continue;
        };
        for item in items {
            let Some(mut item) = item.as_object().cloned() else {
                continue;
            };
            if !item.contains_key("parent") {
                if let Some(parent) = &default_parent {
                    item.insert("parent".to_string(), parent.clone());
                }
            }
            operations.push(serde_json::json!({
                "name": operation_name,
                "params": Value::Object(item)
            }));
        }
    }
    operations
}

fn verify_batch_postconditions(
    operations: &[Value],
    data: &Value,
    scene: &SceneGraph,
) -> Vec<String> {
    let entities = data
        .get("entities")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let result_operations = data
        .get("operations")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let mut used_results = HashSet::new();
    let mut failures = Vec::new();
    for (index, operation) in operations.iter().enumerate() {
        let Some(name) = operation.get("name").and_then(Value::as_str) else {
            failures.push(format!("Operation {} has no command name.", index + 1));
            continue;
        };
        let Some(params) = operation.get("params") else {
            failures.push(format!("Operation {} has no params.", index + 1));
            continue;
        };
        if !matches!(
            name,
            "game.add"
                | "scene_create"
                | "game.create_group"
                | "scene_create_group"
                | "game.update"
                | "scene_update"
                | "game.reparent"
                | "scene_reparent"
        ) {
            continue;
        }
        let expected_name = canonical_operation_name(name);
        let result_match = result_operations
            .iter()
            .enumerate()
            .filter(|(result_index, result)| {
                !used_results.contains(result_index)
                    && result
                        .get("name")
                        .and_then(Value::as_str)
                        .is_some_and(|result_name| {
                            canonical_operation_name(result_name) == expected_name
                        })
                    && result
                        .get("entity")
                        .filter(|entity| entity.is_object())
                        .is_some_and(|entity| {
                            entity_matches_operation(entity, params, scene)
                                || (params.get("name").is_none()
                                    && params.get("target").is_none()
                                    && matches!(
                                        expected_name.as_str(),
                                        "game.add" | "game.create_group"
                                    ))
                        })
            })
            .min_by_key(|(result_index, _)| (*result_index).abs_diff(index));
        let actual = if let Some((result_index, result)) = result_match {
            used_results.insert(result_index);
            result.get("entity")
        } else {
            entities
                .iter()
                .find(|entity| entity_matches_operation(entity, params, scene))
        };
        match actual {
            Some(entity) => failures.extend(
                verify_entity_postcondition(params, entity, scene)
                    .into_iter()
                    .map(|failure| format!("operation {}: {failure}", index + 1)),
            ),
            None => failures.push(format!(
                "operation {} ({name}) did not return an affected entity.",
                index + 1
            )),
        }
    }
    failures
}

fn canonical_operation_name(name: &str) -> String {
    match name.trim_start_matches('/').to_ascii_lowercase().as_str() {
        "scene_create" | "game.add" => "game.add".to_string(),
        "scene_create_group" | "game.create_group" => "game.create_group".to_string(),
        "scene_update" | "game.update" => "game.update".to_string(),
        "scene_reparent" | "game.reparent" => "game.reparent".to_string(),
        "scene_delete" | "game.delete" => "game.delete".to_string(),
        "scene_duplicate" | "game.duplicate" => "game.duplicate".to_string(),
        "scene_arrange" | "game.arrange_grid" => "game.arrange_grid".to_string(),
        "scene_instantiate_prefab" | "game.generate_prefab" => "game.generate_prefab".to_string(),
        _ => name.to_string(),
    }
}

fn entity_matches_operation(entity: &Value, params: &Value, scene: &SceneGraph) -> bool {
    let Some(entity_object) = entity.as_object() else {
        return false;
    };
    if let Some(name) = params.get("name").and_then(Value::as_str) {
        let expected_name = raf_core::agent_context::display_name(name);
        return entity_object
            .get("name")
            .and_then(Value::as_str)
            .is_some_and(|actual| actual == expected_name);
    }
    let Some(target) = params.get("target").and_then(Value::as_str) else {
        return false;
    };
    if let Some(id) = entity_object.get("id").and_then(Value::as_u64) {
        if target.parse::<usize>().ok() == Some(id as usize) {
            return true;
        }
    }
    if entity_object
        .get("uuid")
        .and_then(Value::as_str)
        .is_some_and(|uuid| uuid == target)
    {
        return true;
    }
    if entity_object
        .get("ref")
        .and_then(Value::as_str)
        .is_some_and(|entity_ref| entity_ref.eq_ignore_ascii_case(target))
    {
        return true;
    }
    if entity_object
        .get("stable_key")
        .and_then(Value::as_str)
        .is_some_and(|stable_key| {
            stable_key.eq_ignore_ascii_case(target)
                || format!("key:{stable_key}").eq_ignore_ascii_case(target)
        })
    {
        return true;
    }
    if entity_object
        .get("path")
        .and_then(Value::as_str)
        .is_some_and(|path| {
            path.trim_matches('/')
                .eq_ignore_ascii_case(target.trim_matches('/'))
        })
    {
        return true;
    }
    resolve_scene_target(scene, target)
        .and_then(|id| {
            entity_object
                .get("id")
                .and_then(Value::as_u64)
                .map(|actual| (id, actual))
        })
        .is_some_and(|(id, actual)| id.0 == actual as usize)
}

fn verify_entity_postcondition(params: &Value, entity: &Value, scene: &SceneGraph) -> Vec<String> {
    let Some(params) = params.as_object() else {
        return vec!["Mutation params must be an object.".to_string()];
    };
    let Some(entity) = entity.as_object() else {
        return vec!["Mutation result entity must be an object.".to_string()];
    };
    let mut failures = Vec::new();
    if let Some(expected_name) = params.get("name").and_then(Value::as_str) {
        let expected_name = raf_core::agent_context::display_name(expected_name);
        if entity.get("name").and_then(Value::as_str) != Some(expected_name.as_str()) {
            failures.push(format!(
                "name is {:?}, expected {:?}.",
                entity.get("name").and_then(Value::as_str),
                expected_name
            ));
        }
    }
    if let Some(expected) = params
        .get("primitive")
        .or_else(|| params.get("kind"))
        .or_else(|| params.get("shape"))
        .or_else(|| params.get("type"))
        .and_then(Value::as_str)
    {
        let expected = canonical_primitive(expected);
        let actual = entity
            .get("primitive")
            .and_then(Value::as_str)
            .map(canonical_primitive);
        if actual.as_deref() != Some(expected.as_str()) {
            failures.push(format!("primitive is {:?}, expected {}.", actual, expected));
        }
    }
    if let Some(transform) = params.get("transform") {
        let Some(transform) = transform.as_object() else {
            failures.push("transform must be an object.".to_string());
            return failures;
        };
        for (key, actual_key) in [
            ("position", "position"),
            ("rotation_deg", "rotation_deg"),
            ("rotation", "rotation_deg"),
            ("scale", "scale"),
        ] {
            if let Some(expected) = transform.get(key) {
                compare_postcondition_vec3(
                    &mut failures,
                    &format!("transform.{key}"),
                    expected,
                    entity.get(actual_key),
                );
            }
        }
    }
    for (key, actual_key) in [
        ("position", "position"),
        ("rotation_deg", "rotation_deg"),
        ("rotation", "rotation_deg"),
        ("scale", "scale"),
    ] {
        if let Some(expected) = params.get(key) {
            compare_postcondition_vec3(&mut failures, key, expected, entity.get(actual_key));
        }
    }
    if let Some(expected) = params.get("color").or_else(|| params.get("color_rgba")) {
        compare_postcondition_color(&mut failures, expected, entity.get("color_rgba"));
    }
    for key in ["visible", "locked"] {
        if let Some(expected) = params.get(key).and_then(Value::as_bool) {
            if entity.get(key).and_then(Value::as_bool) != Some(expected) {
                failures.push(format!(
                    "{key} is {:?}, expected {expected}.",
                    entity.get(key).and_then(Value::as_bool)
                ));
            }
        }
    }
    if let Some(expected_parent) = params.get("parent") {
        let expected_id = match expected_parent {
            Value::Null => None,
            Value::String(parent)
                if parent.trim().is_empty()
                    || matches!(
                        parent.trim().to_ascii_lowercase().as_str(),
                        "root" | "scene" | "none" | "null"
                    ) =>
            {
                None
            }
            Value::String(parent) => resolve_scene_target(scene, parent).map(|id| id.0),
            Value::Number(parent) => parent.as_u64().map(|id| id as usize),
            _ => {
                failures.push("parent must be a target string or null.".to_string());
                None
            }
        };
        let actual_id = entity
            .get("parent_id")
            .and_then(Value::as_u64)
            .map(|id| id as usize);
        if expected_id != actual_id {
            failures.push(format!(
                "parent_id is {:?}, expected {:?}.",
                actual_id, expected_id
            ));
        }
    }
    for key in ["semantic_role", "stable_key", "agent_origin"] {
        if let Some(expected) = params.get(key) {
            let expected = match expected {
                Value::Null => None,
                Value::String(value) => {
                    let value = raf_core::agent_context::display_name(value);
                    (!value.is_empty()).then_some(value)
                }
                _ => {
                    failures.push(format!("{key} must be a string or null."));
                    continue;
                }
            };
            let actual = entity.get(key).and_then(Value::as_str).map(str::to_string);
            if actual != expected {
                failures.push(format!("{key} is {:?}, expected {:?}.", actual, expected));
            }
        }
    }
    if let Some(expected) = params.get("tags") {
        let expected = normalized_tags(expected);
        let actual = entity.get("tags").and_then(normalized_tags);
        match (expected, actual) {
            (Some(expected), Some(actual)) if expected == actual => {}
            (Some(expected), Some(actual)) => {
                failures.push(format!("tags are {:?}, expected {:?}.", actual, expected))
            }
            _ => failures.push("tags must be an array of strings or null.".to_string()),
        }
    }
    failures
}

fn normalized_tags(value: &Value) -> Option<Vec<String>> {
    match value {
        Value::Null => Some(Vec::new()),
        Value::Array(values) => {
            let mut tags = values
                .iter()
                .map(|value| value.as_str().map(raf_core::agent_context::display_name))
                .collect::<Option<Vec<_>>>()?;
            tags.retain(|tag| !tag.is_empty());
            tags.sort_unstable();
            tags.dedup();
            Some(tags)
        }
        _ => None,
    }
}

fn compare_postcondition_vec3(
    failures: &mut Vec<String>,
    label: &str,
    expected: &Value,
    actual: Option<&Value>,
) {
    let Some(expected) = postcondition_vec3(expected) else {
        failures.push(format!("{label} must contain 3 finite numbers."));
        return;
    };
    let Some(actual) = actual.and_then(postcondition_vec3) else {
        failures.push(format!("{label} was not returned by the engine."));
        return;
    };
    if !expected
        .iter()
        .zip(actual.iter())
        .all(|(expected, actual)| (expected - actual).abs() <= 0.0001)
    {
        failures.push(format!(
            "{label} is [{:.4}, {:.4}, {:.4}], expected [{:.4}, {:.4}, {:.4}].",
            actual[0], actual[1], actual[2], expected[0], expected[1], expected[2]
        ));
    }
}

fn compare_postcondition_color(
    failures: &mut Vec<String>,
    expected: &Value,
    actual: Option<&Value>,
) {
    let Some(expected) = postcondition_color(expected) else {
        failures.push("color must contain 3 or 4 channels.".to_string());
        return;
    };
    let Some(actual) = actual.and_then(postcondition_color) else {
        failures.push("color_rgba was not returned by the engine.".to_string());
        return;
    };
    if expected != actual {
        failures.push(format!(
            "color_rgba is [{}, {}, {}, {}], expected [{}, {}, {}, {}].",
            actual[0],
            actual[1],
            actual[2],
            actual[3],
            expected[0],
            expected[1],
            expected[2],
            expected[3]
        ));
    }
}

fn postcondition_vec3(value: &Value) -> Option<[f64; 3]> {
    match value {
        Value::Array(values) if values.len() == 3 => Some([
            values[0].as_f64()?,
            values[1].as_f64()?,
            values[2].as_f64()?,
        ]),
        Value::Object(object) => Some([
            object.get("x").and_then(Value::as_f64)?,
            object.get("y").and_then(Value::as_f64)?,
            object.get("z").and_then(Value::as_f64)?,
        ]),
        Value::String(raw) => serde_json::from_str(raw)
            .ok()
            .and_then(|value| postcondition_vec3(&value)),
        _ => None,
    }
}

fn postcondition_color(value: &Value) -> Option<[u64; 4]> {
    match value {
        Value::Array(values) if values.len() == 3 || values.len() == 4 => Some([
            values[0].as_u64()?,
            values[1].as_u64()?,
            values[2].as_u64()?,
            values.get(3).and_then(Value::as_u64).unwrap_or(255),
        ]),
        Value::Object(object) => Some([
            object.get("r").and_then(Value::as_u64)?,
            object.get("g").and_then(Value::as_u64)?,
            object.get("b").and_then(Value::as_u64)?,
            object.get("a").and_then(Value::as_u64).unwrap_or(255),
        ]),
        Value::String(raw) if raw.trim_start().starts_with('#') => {
            let raw = raw.trim().trim_start_matches('#');
            if raw.len() != 6 && raw.len() != 8 {
                return None;
            }
            Some([
                u64::from_str_radix(&raw[0..2], 16).ok()?,
                u64::from_str_radix(&raw[2..4], 16).ok()?,
                u64::from_str_radix(&raw[4..6], 16).ok()?,
                if raw.len() == 8 {
                    u64::from_str_radix(&raw[6..8], 16).ok()?
                } else {
                    255
                },
            ])
        }
        Value::String(raw) => serde_json::from_str(raw)
            .ok()
            .and_then(|value| postcondition_color(&value)),
        _ => None,
    }
}

fn canonical_primitive(value: &str) -> String {
    match value.trim().to_ascii_lowercase().as_str() {
        "box" | "block" => "cube".to_string(),
        "ball" | "circle" | "uv_sphere" => "sphere".to_string(),
        "floor" | "sprite" | "sprite2d" => "plane".to_string(),
        "group" | "folder" => "empty".to_string(),
        value => value.to_string(),
    }
}

fn resolve_scene_target(
    scene: &SceneGraph,
    target: &str,
) -> Option<raf_core::scene::graph::SceneNodeId> {
    raf_core::agent_context::resolve_target(scene, target)
}

fn request_preview(mode: ToolExecutionMode, kind: AgentToolKind) -> bool {
    mode == ToolExecutionMode::Preview && kind == AgentToolKind::Mutation
}

fn expects_observable_scene_change(command_name: &str) -> bool {
    matches!(
        command_name,
        "game.add"
            | "game.create_group"
            | "game.delete"
            | "game.duplicate"
            | "game.generate_prefab"
            | "game.build"
            | "game.reconcile"
            | "game.repair"
            | "game.batch"
    )
}

fn live_scene_entity_count(scene: &SceneGraph) -> usize {
    scene.all_live_ids().len()
}

fn classify_prompt(prompt: &str) -> PromptIntent {
    let prompt = prompt.to_lowercase();
    let contains_any = |terms: &[&str]| terms.iter().any(|term| prompt.contains(term));
    PromptIntent {
        authoring: contains_any(&[
            "crea",
            "crear",
            "constru",
            "haz",
            "hacer",
            "genera",
            "agrega",
            "añade",
            "anade",
            "añade",
            "añadir",
            "pon ",
            "mueve",
            "rota",
            "escala",
            "elimina",
            "borra",
            "duplica",
            "organiza",
            "modifica",
            "cambia",
            "diseña",
            "disena",
            "diseña",
            "dibuja",
            "arma",
            "construye",
            "estructura",
            "create",
            "build",
            "make",
            "generate",
            "add ",
            "move",
            "rotate",
            "scale",
            "delete",
            "duplicate",
            "arrange",
            "update",
            "change",
            "design",
        ]),
        scripting: contains_any(&[
            "script",
            "rhai",
            "codigo",
            "código",
            "código",
            "code",
            "controller",
            "controlador",
        ]),
        pcb: contains_any(&["pcb", "placa", "board", "route", "airwire"]),
        capabilities: contains_any(&[
            "que puedes",
            "qué puedes",
            "qué puedes",
            "comandos",
            "capacidades",
            "commands",
            "capabilities",
        ]),
        layout: contains_any(&[
            "arrange", "organiza", "organize", "grid", "layout", "distribu", "acomoda",
        ]),
        prefab: contains_any(&["prefab", "manifest", "prefabricado", "instancia"]),
        assets: contains_any(&[
            "asset",
            "assets",
            "modelo",
            "modelos",
            "model",
            "models",
            "textura",
            "texturas",
            "texture",
            "textures",
            "material",
            "materiales",
            "materials",
            "importa",
            "importar",
            "recurso",
            "recursos",
        ]),
    }
}

fn catalog_tool_kind(command: &CommandDefinition) -> AgentToolKind {
    let name = command.name.as_str();
    if name.ends_with(".describe")
        || name.ends_with(".netlist")
        || name.ends_with(".bom")
        || name.ends_with(".validate")
        || name.ends_with(".list")
    {
        AgentToolKind::Read
    } else {
        AgentToolKind::Mutation
    }
}

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

fn command_to_openai_tool(
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
                &["empty", "cube", "sphere", "plane", "cylinder"],
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

fn localized_native_tool_description(name: &str, fallback: &str, language: Language) -> String {
    let key = match name {
        "project_summary" => Some("commands.project_summary.desc"),
        "scene_outline" => Some("commands.scene_outline.desc"),
        "scene_query" => Some("commands.scene_query.desc"),
        "scene_spatial_map" => Some("commands.scene_spatial_map.desc"),
        "scene_design_audit" => Some("commands.scene_design_audit.desc"),
        "scene_inspect" => Some("commands.scene_inspect.desc"),
        "selection_get" => Some("commands.selection_get.desc"),
        "viewport_capture" => Some("commands.viewport_capture.desc"),
        "assets_catalog" => Some("commands.assets_catalog.desc"),
        "asset_inspect" => Some("commands.asset_inspect.desc"),
        "scripts_catalog" => Some("commands.scripts_catalog.desc"),
        "project_health" => Some("commands.project_health.desc"),
        "scene_verify" | "game_validate_layout" => Some("commands.scene_verify.desc"),
        "scene_build" => Some("commands.game_build.desc"),
        "scene_create_group" => Some("commands.game_create_group.desc"),
        "scene_create" => Some("commands.game_add.desc"),
        "scene_update" => Some("commands.game_update.desc"),
        "scene_reparent" => Some("commands.game_reparent.desc"),
        "scene_delete" => Some("commands.game_delete.desc"),
        "scene_duplicate" => Some("commands.game_duplicate.desc"),
        "scene_arrange" => Some("commands.game_arrange_grid.desc"),
        "scene_instantiate_prefab" => Some("commands.game_generate_prefab.desc"),
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

fn openai_tool(name: &str, description: &str, parameters: Value) -> OpenAiTool {
    OpenAiTool {
        tool_type: "function".to_string(),
        function: OpenAiFunction {
            name: name.to_string(),
            description: Some(description.to_string()),
            parameters,
        },
    }
}

fn object_schema(properties: Map<String, Value>, required: &[&str]) -> Value {
    serde_json::json!({
        "type": "object",
        "properties": properties,
        "required": required,
        "additionalProperties": false
    })
}

fn paginated_schema(extra: &[(&str, Value)]) -> Value {
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

fn transform_schema() -> Value {
    let vector = |description: &str| {
        serde_json::json!({
            "type": "array",
            "description": description,
            "items": {"type": "number"},
            "minItems": 3,
            "maxItems": 3
        })
    };
    object_schema(
        Map::from_iter([
            ("position".to_string(), vector("Local XYZ position.")),
            (
                "rotation_deg".to_string(),
                vector("Local XYZ Euler rotation in degrees."),
            ),
            ("scale".to_string(), vector("Local XYZ scale.")),
        ]),
        &[],
    )
}

fn scene_entity_schema() -> Value {
    object_schema(
        Map::from_iter([
            (
                "kind".to_string(),
                enum_schema(
                    "Primitive to create.",
                    &["empty", "cube", "sphere", "plane", "cylinder"],
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

fn scene_build_schema() -> Value {
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

fn scene_reconcile_schema() -> Value {
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

fn batch_operation_params_schema() -> Value {
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
                number_schema("Grid spacing.", 0.1, 10000.0),
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

fn color_schema(description: &str) -> Value {
    serde_json::json!({
        "type": "array",
        "description": description,
        "items": {"type": "integer", "minimum": 0, "maximum": 255},
        "minItems": 3,
        "maxItems": 4
    })
}

fn semantic_tags_schema() -> Value {
    serde_json::json!({
        "type": "array",
        "description": "Searchable semantic tags.",
        "items": {"type": "string", "maxLength": 64},
        "maxItems": 32
    })
}

fn expected_scene_state_schema() -> Value {
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

fn string_schema(description: &str) -> Value {
    serde_json::json!({"type": "string", "description": description})
}

fn boolean_schema(description: &str) -> Value {
    serde_json::json!({"type": "boolean", "description": description})
}

fn number_schema(description: &str, minimum: f64, maximum: f64) -> Value {
    serde_json::json!({
        "type": "number",
        "description": description,
        "minimum": minimum,
        "maximum": maximum
    })
}

fn integer_schema(description: &str, minimum: impl Into<i64>, maximum: impl Into<i64>) -> Value {
    serde_json::json!({
        "type": "integer",
        "description": description,
        "minimum": minimum.into(),
        "maximum": maximum.into()
    })
}

fn enum_schema(description: &str, values: &[&str]) -> Value {
    serde_json::json!({
        "type": "string",
        "description": description,
        "enum": values
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn game_tool_pack_is_contextual_and_never_exposes_electronics() {
        let catalog = CommandCatalog::builtin();
        let pack = AgentToolExecutor::build_tool_pack(
            &catalog,
            Language::English,
            Some(ProjectType::Game),
            AgentMode::Active,
            "Create a detailed store shelf",
        );

        assert!(pack.tools.len() <= 22);
        assert!(pack
            .tools
            .iter()
            .any(|tool| tool.function.name == "scene_batch"));
        assert!(pack
            .tools
            .iter()
            .any(|tool| tool.function.name == "scene_build"));
        assert!(pack
            .tools
            .iter()
            .any(|tool| tool.function.name == "scene_create_group"));
        assert!(pack
            .tools
            .iter()
            .any(|tool| tool.function.name == "scene_reparent"));
        assert!(pack
            .tools
            .iter()
            .any(|tool| tool.function.name == "scene_repair"));
        assert!(pack
            .tools
            .iter()
            .any(|tool| tool.function.name == "scene_inspect"));
        assert!(pack
            .tools
            .iter()
            .any(|tool| tool.function.name == "project_health"));
        assert!(pack
            .tools
            .iter()
            .any(|tool| tool.function.name == "viewport_capture"));
        assert!(!pack
            .tools
            .iter()
            .any(|tool| tool.function.name.starts_with("electronics_")));
    }

    #[test]
    fn batch_schema_exposes_nested_scene_values() {
        let catalog = CommandCatalog::builtin();
        let pack = AgentToolExecutor::build_tool_pack(
            &catalog,
            Language::English,
            Some(ProjectType::Game),
            AgentMode::Active,
            "Create a detailed store shelf",
        );
        let batch = pack
            .tools
            .iter()
            .find(|tool| tool.function.name == "scene_batch")
            .expect("scene_batch");
        let params =
            &batch.function.parameters["properties"]["operations"]["items"]["properties"]["params"];
        assert_eq!(params["type"], "object");
        assert_eq!(params["properties"]["transform"]["type"], "object");
        assert_eq!(
            params["properties"]["transform"]["properties"]["position"]["type"],
            "array"
        );
        assert_eq!(params["properties"]["color_rgba"]["type"], "array");
    }

    #[test]
    fn inspect_mode_never_exposes_mutations() {
        let catalog = CommandCatalog::builtin();
        let pack = AgentToolExecutor::build_tool_pack(
            &catalog,
            Language::English,
            Some(ProjectType::Game),
            AgentMode::Inspect,
            "Create a detailed store shelf",
        );

        assert!(pack
            .routes
            .values()
            .all(|route| route.kind == AgentToolKind::Read));
    }

    #[test]
    fn semantic_scene_arguments_reach_the_game_kernel_without_lossy_flattening() {
        let mut arguments = serde_json::json!({
            "kind": "cube",
            "name": "Shelf",
            "transform": {
                "position": [1, 2, 3],
                "rotation_deg": [0, 90, 0],
                "scale": [2, 3, 4]
            },
            "color_rgba": [10, 20, 30, 255]
        });
        normalize_scene_arguments(&mut arguments).unwrap();

        assert_eq!(arguments["primitive"], "cube");
        assert_eq!(arguments["transform"]["position"][0], 1);
        assert_eq!(arguments["transform"]["rotation_deg"][1], 90);
        assert_eq!(arguments["transform"]["scale"][2], 4);
        assert_eq!(arguments["color_rgba"][2], 30);
        assert!(arguments.get("kind").is_none());
    }

    #[test]
    fn batch_translation_accepts_semantic_and_canonical_scene_operations() {
        let mut arguments = serde_json::json!({
            "operations": [
                {
                    "name": "scene_create",
                    "params": {
                        "kind": "sphere",
                        "name": "Display",
                        "transform": {"position": [1, 2, 3]}
                    }
                },
                {
                    "name": "/game.update",
                    "params": {"target": "Display", "color_rgba": [10, 20, 30, 255]}
                }
            ]
        });

        translate_batch(&mut arguments).unwrap();

        assert_eq!(arguments["operations"][0]["name"], "game.add");
        assert_eq!(arguments["operations"][0]["params"]["primitive"], "sphere");
        assert_eq!(arguments["operations"][1]["name"], "game.update");
        assert_eq!(arguments["operations"][1]["params"]["color_rgba"][2], 30);
    }

    #[test]
    fn batch_translation_rejects_missing_or_non_object_params() {
        let mut missing = serde_json::json!({
            "operations": [{"name": "scene_create"}]
        });
        assert!(translate_batch(&mut missing).is_err());

        let mut invalid = serde_json::json!({
            "operations": [{"name": "scene_create", "params": []}]
        });
        assert!(translate_batch(&mut invalid).is_err());
    }

    #[test]
    fn build_translation_keeps_groups_and_nested_entity_values() {
        let mut arguments = serde_json::json!({
            "groups": [{"name": "Structure"}],
            "entities": [{
                "kind": "cube",
                "name": "Floor",
                "transform": {"position": [0, 1, 2], "scale": [4, 1, 2]},
                "color_rgba": [60, 150, 90, 255]
            }]
        });

        translate_build(&mut arguments).unwrap();

        assert_eq!(arguments["groups"][0]["name"], "Structure");
        assert_eq!(arguments["entities"][0]["primitive"], "cube");
        assert_eq!(arguments["entities"][0]["transform"]["scale"][0], 4);
        assert_eq!(arguments["entities"][0]["color_rgba"][1], 150);
    }

    #[test]
    fn postconditions_detect_a_transform_or_color_mismatch() {
        let scene = SceneGraph::new();
        let failures = verify_entity_postcondition(
            &serde_json::json!({
                "primitive": "cube",
                "name": "Shelf",
                "transform": {"position": [1, 2, 3]},
                "color_rgba": [10, 20, 30, 255]
            }),
            &serde_json::json!({
                "id": 0,
                "name": "Shelf",
                "primitive": "Cube",
                "position": [0, 0, 0],
                "rotation_deg": [0, 0, 0],
                "scale": [1, 1, 1],
                "color_rgba": [224, 116, 24, 255],
                "parent_id": null
            }),
            &scene,
        );

        assert_eq!(failures.len(), 2);
        assert!(failures.iter().any(|failure| failure.contains("position")));
        assert!(failures
            .iter()
            .any(|failure| failure.contains("color_rgba")));
    }

    #[test]
    fn batch_result_compaction_keeps_effective_scene_values_without_mesh_noise() {
        let compact = compact_command_data(serde_json::json!({
            "ok": true,
            "entity_count": 1,
            "operations": [{
                "name": "game.add",
                "ok": true,
                "changed": true,
                "summary": "Created Shelf",
                "entity": {
                    "id": 4,
                    "name": "Shelf",
                    "primitive": "Cube",
                    "position": [1, 2, 3],
                    "scale": [2, 1, 1],
                    "color_rgba": [20, 30, 40, 255],
                    "mesh": {
                        "vertex_count": 24,
                        "index_count": 36,
                        "bounds_min": [-1, -1, -1]
                    }
                }
            }]
        }));

        assert_eq!(
            compact["operations"][0]["entity"]["position"],
            serde_json::json!([1, 2, 3])
        );
        assert_eq!(
            compact["operations"][0]["entity"]["color_rgba"],
            serde_json::json!([20, 30, 40, 255])
        );
        assert_eq!(
            compact["operations"][0]["entity"]["mesh"],
            serde_json::json!({"vertex_count": 24, "index_count": 36})
        );
        assert!(compact["operations"][0]["entity"]["mesh"]["bounds_min"].is_null());
    }
}
