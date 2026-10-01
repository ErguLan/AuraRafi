//! Node compiler stub - future: compiles node graphs to executable logic.

use crate::graph::{GraphDiagnostic, GraphDiagnosticSeverity, NodeGraph};

/// Compilation result (placeholder for future implementation).
pub struct CompilationResult {
    pub success: bool,
    /// Compatibility summaries for callers that only need plain messages.
    pub errors: Vec<String>,
    pub warnings: Vec<String>,
    /// Structured diagnostics used by the editor to localize and target issues.
    pub diagnostics: Vec<GraphDiagnostic>,
}

/// Compile a node graph to executable logic.
/// Validates the graph without enabling or invoking runtime execution.
pub fn compile(graph: &NodeGraph) -> CompilationResult {
    let diagnostics = graph.diagnostics();
    let errors = diagnostics
        .iter()
        .filter(|diagnostic| diagnostic.severity == GraphDiagnosticSeverity::Error)
        .map(|diagnostic| diagnostic.message.clone())
        .collect::<Vec<_>>();
    let warnings = diagnostics
        .iter()
        .filter(|diagnostic| diagnostic.severity == GraphDiagnosticSeverity::Warning)
        .map(|diagnostic| diagnostic.message.clone())
        .collect::<Vec<_>>();

    CompilationResult {
        success: errors.is_empty(),
        errors,
        warnings,
        diagnostics,
    }
}
