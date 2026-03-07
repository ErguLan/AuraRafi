//! Node compiler stub - future: compiles node graphs to executable logic.

use crate::graph::NodeGraph;

/// Compilation result (placeholder for future implementation).
pub struct CompilationResult {
    pub success: bool,
    pub errors: Vec<String>,
}

/// Compile a node graph to executable logic.
/// Currently a stub that validates basic connectivity.
pub fn compile(graph: &NodeGraph) -> CompilationResult {
    let mut errors = Vec::new();

    // Basic validation: check for disconnected nodes.
    for node in &graph.nodes {
        let conns = graph.connections_for(node.id);
        if conns.is_empty() && graph.nodes.len() > 1 {
            errors.push(format!(
                "Node '{}' has no connections",
                node.name
            ));
        }
    }

    CompilationResult {
        success: errors.is_empty(),
        errors,
    }
}
