//! Human, JSON and NDJSON output formatting for the `raf` CLI.

use crate::cli::args::{Options, OutputFormat};
use crate::cli::CliResult;
use crate::discovery::{self, DiscoveredEditor};
use raf_core::EngineCommandResponse;
use serde_json::json;
use std::env;
use std::path::{Path, PathBuf};

pub(super) fn print_response(response: &EngineCommandResponse, format: OutputFormat) -> CliResult<()> {
    match format {
        OutputFormat::Human => {
            println!("{}", response.title);
            for line in raf_core::agent_context::compact_result_lines(&response.lines) {
                println!("- {line}");
            }
            if !response.data.is_null() {
                println!(
                    "{}",
                    serde_json::to_string_pretty(&raf_core::agent_context::compact_result_data(
                        &response.data,
                    ))
                    .map_err(|error| error.to_string())?
                );
            }
            if !response.warnings.is_empty() {
                println!("Warnings:");
                for warning in &response.warnings {
                    println!("- {warning}");
                }
            }
            if let Some(verification) = &response.verification {
                println!("Verification: {}", verification.status);
                for failure in &verification.failures {
                    println!("- {failure}");
                }
            }
            println!("Revision: {}", response.revision);
        }
        OutputFormat::Json | OutputFormat::Ndjson => {
            println!(
                "{}",
                serde_json::to_string(response).map_err(|error| error.to_string())?
            );
        }
    }
    if response.ok {
        Ok(())
    } else {
        Err(if response.lines.iter().any(|line| !line.trim().is_empty()) {
            response.lines.join(" ")
        } else { response.title.clone() })
    }
}

pub(super) fn print_project_path(options: &Options, format: OutputFormat) -> CliResult<()> {
    let explicit = options
        .path("project")
        .or_else(|| options.path("path"))
        .or_else(|| options.positionals.get(2).map(PathBuf::from));
    let (path, source) = match explicit {
        Some(path) => {
            let canonical = std::fs::canonicalize(&path)
                .map_err(|error| format!("Resolve project path {}: {error}", path.display()))?;
            let root = if canonical.is_file() {
                canonical
                    .parent()
                    .map(Path::to_path_buf)
                    .ok_or_else(|| "Project file has no parent directory.".to_string())?
            } else {
                canonical
            };
            (root, "explicit")
        }
        None => (discovery::pick_single_live(None)?.project_path, "attached"),
    };
    let clean = clean_path_string(&path);
    match format {
        OutputFormat::Human => println!("{clean}"),
        OutputFormat::Json => println!(
            "{}",
            json!({
                "ok": true,
                "path": clean,
                "raw_path": path.display().to_string(),
                "source": source,
            })
        ),
        OutputFormat::Ndjson => println!(
            "{}",
            json!({"path": clean, "source": source})
        ),
    }
    Ok(())
}

/// Windows project descriptors frequently use the verbatim `\\?\` prefix.
/// Agents should see the ordinary filesystem path, while `raw_path` remains
/// available for diagnostics.
pub(super) fn clean_path_string(path: &Path) -> String {
    let raw = path.display().to_string();
    raw.strip_prefix(r"\\?\").unwrap_or(&raw).to_string()
}

/// Lists every known project that publishes an attach descriptor, marking
/// which editors are actually listening right now. Stale descriptors from
/// crashed sessions are reported as not alive instead of failing silently.
///
/// With `--wait[=SECONDS]` the scan retries every 400 ms until a live editor
/// appears or the budget expires, so automation can start before the engine.
/// The poll is bounded and only runs for this command's lifetime.
pub(super) fn print_editors(
    extra: Option<&Path>,
    format: OutputFormat,
    wait_seconds: Option<f32>,
) -> CliResult<()> {
    const WAIT_POLL: std::time::Duration = std::time::Duration::from_millis(400);
    let deadline = wait_seconds.map(|seconds| {
        std::time::Instant::now() + std::time::Duration::from_secs_f32(seconds.max(0.5))
    });
    let editors = loop {
        let editors = discovery::discover_editors(extra.or(env::current_dir().ok().as_deref()));
        if !discovery::live_editors(&editors).is_empty() || deadline.is_none() {
            break editors;
        }
        if std::time::Instant::now() >= deadline.unwrap() {
            break editors;
        }
        std::thread::sleep(WAIT_POLL);
    };
    let live = discovery::live_editors(&editors).len();
    match format {
        OutputFormat::Human => {
            println!(
                "{live} live editor(s) across {} known project(s):",
                editors.len()
            );
            for editor in &editors {
                let state = if editor.alive { "LIVE" } else { "stale" };
                let session = editor
                    .descriptor
                    .session_name
                    .clone()
                    .unwrap_or_else(|| "-".to_string());
                let kind = editor
                    .descriptor
                    .project_type
                    .clone()
                    .unwrap_or_else(|| "?".to_string());
                println!(
                    "- [{state}] {} | type: {kind} | session: {} | pid {} | rev {}",
                    editor.project_path.display(),
                    session,
                    editor.descriptor.process_id,
                    editor.descriptor.revision
                );
            }
            if wait_seconds.is_some() && live == 0 {
                return Err("Timed out waiting for a live AuraRafi editor.".to_string());
            }
            Ok(())
        }
        OutputFormat::Json | OutputFormat::Ndjson => {
            let payload = json!({
                "live_count": live,
                "scanned": editors.len(),
                "timed_out": wait_seconds.is_some() && live == 0,
                "editors": editors.iter().map(DiscoveredEditor::to_json).collect::<Vec<_>>(),
            });
            println!(
                "{}",
                serde_json::to_string(&payload).map_err(|error| error.to_string())?
            );
            if wait_seconds.is_some() && live == 0 {
                return Err("Timed out waiting for a live AuraRafi editor.".to_string());
            }
            Ok(())
        }
    }
}
