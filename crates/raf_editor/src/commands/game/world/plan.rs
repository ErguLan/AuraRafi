//! World plan artifacts, cache, staging and pipeline gates.

use super::commit::apply_plan_to_scene;
use super::compile::compile_world_program;
use super::*;
use std::io::Write as _;

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

pub(super) fn plan_cache() -> &'static Mutex<HashMap<String, PlannedWorld>> {
    static CACHE: OnceLock<Mutex<HashMap<String, PlannedWorld>>> = OnceLock::new();
    CACHE.get_or_init(|| Mutex::new(HashMap::new()))
}

pub(super) fn world_directory(root: &Path) -> PathBuf {
    root.join(".aura_rafi").join("world")
}

fn plan_path(root: &Path, id: &str) -> PathBuf {
    world_directory(root)
        .join("plans")
        .join(format!("{id}.json"))
}

pub(super) fn brief_path(root: &Path) -> PathBuf {
    world_directory(root).join("brief.json")
}

fn journal_path(root: &Path) -> PathBuf {
    world_directory(root).join("journal.jsonl")
}

pub(super) fn sha256_hex(bytes: &[u8]) -> String {
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

pub(super) fn update_persisted_plan(plan: &PlannedWorld) {
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

pub(super) fn append_journal(root: &Path, entry: Value) {
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

pub(super) fn load_planned_world(id: &str) -> Result<PlannedWorld, String> {
    if let Ok(cache) = plan_cache().lock() {
        if let Some(plan) = cache.get(id) {
            return Ok(plan.clone());
        }
    }
    let Some(root) = active_project_root() else {
        return Err(format!(
            "Unknown world plan '{id}'. Create it with game.plan first."
        ));
    };
    let path = plan_path(&root, id);
    let bytes = std::fs::read(&path).map_err(|error| {
        format!(
            "World plan '{id}' was not found ({}): {error}",
            path.display()
        )
    })?;
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
        parent: value
            .get("parent")
            .cloned()
            .filter(|value| !value.is_null()),
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

fn stage_plan(
    scene: &SceneGraph,
    plan: &PlannedWorld,
) -> Result<(CommandOutput, SceneGraph), String> {
    let mut staged = scene.clone();
    let mut selection = SceneSelectionState::default();
    let mut viewport = HeadlessGameViewportPort::default();
    let output = apply_plan_to_scene(plan, &mut staged, &mut selection, &mut viewport)?;
    Ok((output, staged))
}

fn plan_command_output(
    request_id: u64,
    plan: &PlannedWorld,
    stage: &CommandOutput,
) -> CommandOutput {
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

pub(crate) fn execute_plan(
    request: &ParsedCommand,
    ctx: &mut GameCommandContext<'_>,
) -> CommandOutput {
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
                request.structured_args.clone().filter(|value| {
                    value.get("operations").is_some()
                        || value.get("entities").is_some()
                        || value.get("groups").is_some()
                })
            })
            .ok_or_else(|| {
                "game.plan requires a program object with archetypes/groups/entities/operations."
                    .to_string()
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
            project_root: active_project_root()
                .map(|root| root.to_string_lossy().replace('\\', "/")),
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
        let design_audit = plan.design_profile.as_deref().map(|profile| {
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
            if let Some(min_reachable) =
                operation.get("min_reachable_goals").and_then(Value::as_u64)
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
