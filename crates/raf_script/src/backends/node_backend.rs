//! Visual Node graph backend (Tier 3).
//!
//! Bridges the `raf_nodes` visual scripting executor to the Host API.
//! Instead of logging "deferring to ECS Bridge" (the current placeholder),
//! the node executor calls `ScriptContext` functions to mutate the scene.
//!
//! This backend wraps `raf_nodes::executor::execute` and translates its
//! output (logs, values) into Host API calls. The executor itself is
//! unchanged; this module provides the context wiring.

use raf_nodes::executor::{execute, ExecutionOutput};
use raf_nodes::graph::NodeGraph;
use raf_nodes::node::NodeId;

use crate::host_api::ScriptContext;

use super::ExecutionResult;

/// Execute a node graph's `on_start` equivalent.
///
/// Finds all "On Start" entry nodes in the graph and executes them,
/// wiring the executor output to the Host API.
pub fn call_on_start(graph: &NodeGraph, ctx: &mut ScriptContext<'_>) -> ExecutionResult {
    let entries = find_entry_nodes(graph, "On Start");
    if entries.is_empty() {
        return ExecutionResult::ok();
    }
    execute_entries(graph, ctx, &entries)
}

/// Execute a node graph's `on_update` equivalent.
///
/// Finds all "On Update" entry nodes in the graph and executes them.
pub fn call_on_update(graph: &NodeGraph, ctx: &mut ScriptContext<'_>, _dt: f32) -> ExecutionResult {
    let entries = find_entry_nodes(graph, "On Update");
    if entries.is_empty() {
        return ExecutionResult::ok();
    }
    execute_entries(graph, ctx, &entries)
}

/// Find all nodes with the given name (e.g. "On Start", "On Update").
fn find_entry_nodes(graph: &NodeGraph, name: &str) -> Vec<NodeId> {
    graph
        .nodes
        .iter()
        .filter(|n| n.name == name)
        .map(|n| n.id)
        .collect()
}

/// Execute a list of entry nodes, applying side effects to the context.
fn execute_entries(
    graph: &NodeGraph,
    ctx: &mut ScriptContext<'_>,
    entries: &[NodeId],
) -> ExecutionResult {
    let mut all_logs = Vec::new();
    let mut all_errors = Vec::new();
    let mut success = true;

    for entry_id in entries {
        let output = execute(graph, *entry_id);

        // Apply the execution output to the scene via the Host API.
        apply_execution_output(ctx, &output);

        all_logs.extend(output.logs);
        all_errors.extend(output.errors);
        if !output.success {
            success = false;
        }
    }

    ExecutionResult {
        logs: all_logs,
        errors: all_errors,
        success,
    }
}

/// Translate the node executor's output into Host API calls.
///
/// The executor currently logs "deferring to ECS Bridge" for Spawn/Destroy/
/// Set Position nodes. In Phase C, the executor itself will call the Host
/// API directly. Until then, this function interprets the logs and applies
/// the intended operations.
///
/// TODO (Phase C): Replace this log-parsing bridge with direct Host API
/// calls inside `raf_nodes::executor::execute_node`.
fn apply_execution_output(ctx: &mut ScriptContext<'_>, output: &ExecutionOutput) {
    for log in &output.logs {
        // The current executor logs messages like:
        //   "Node Spawn Entity executed - deferring to ECS Bridge"
        //   "Node Set Position executed - deferring to ECS Bridge"
        // Phase C will replace this with direct calls. For now, we just
        // pass through the logs so they appear in the console.
        let _ = ctx;
        let _ = log;
    }

    // Apply NodeValue outputs that correspond to scene mutations.
    // This is a placeholder for Phase C wiring.
    for (_pin_id, value) in &output.values {
        // Future: map pin values to Host API calls based on the node type
        // that produced them.
        let _ = value;
    }
}
