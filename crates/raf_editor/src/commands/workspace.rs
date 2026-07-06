use std::fs;
use std::path::{Path, PathBuf};

use crate::commands::output::CommandOutput;
use crate::commands::parser::ParsedCommand;

const DEFAULT_READ_LIMIT: usize = 64 * 1024;
const DEFAULT_SEARCH_LIMIT: usize = 80;
const MAX_SCAN_FILES: usize = 4000;

pub fn read_file(command: &ParsedCommand, project_root: &Path) -> CommandOutput {
    let Some(path) = command.arg("path").or_else(|| command.first_positional()) else {
        return CommandOutput::error("Workspace read", "Missing path=<project-relative file>.");
    };
    let max_bytes = usize_arg(command, "max_bytes", DEFAULT_READ_LIMIT).min(256 * 1024);
    let Ok(path) = resolve_project_path(project_root, path) else {
        return CommandOutput::error("Workspace read", "Path is outside the active project.");
    };
    if !path.is_file() {
        return CommandOutput::error("Workspace read", "Target is not a file.");
    }
    let Ok(bytes) = fs::read(&path) else {
        return CommandOutput::error("Workspace read", "Could not read file.");
    };
    let truncated = bytes.len() > max_bytes;
    let slice = if truncated {
        &bytes[..max_bytes]
    } else {
        bytes.as_slice()
    };
    let content = String::from_utf8_lossy(slice).to_string();
    let mut lines = vec![
        format!("path: {}", display_relative(project_root, &path)),
        format!("bytes_read: {}", slice.len()),
        format!("truncated: {truncated}"),
    ];
    lines.extend(content.lines().take(80).map(|line| format!("| {line}")));
    CommandOutput::info(
        "Workspace read",
        lines,
        serde_json::json!({
            "ok": true,
            "path": display_relative(project_root, &path),
            "bytes_read": slice.len(),
            "truncated": truncated,
            "content": content
        }),
    )
}

pub fn search(command: &ParsedCommand, project_root: &Path) -> CommandOutput {
    let Some(query) = command.arg("query").or_else(|| command.first_positional()) else {
        return CommandOutput::error("Workspace search", "Missing query=<text>.");
    };
    if query.trim().is_empty() {
        return CommandOutput::error("Workspace search", "Search query is empty.");
    }
    let max_results = usize_arg(command, "max_results", DEFAULT_SEARCH_LIMIT).min(250);
    let root = match project_root.canonicalize() {
        Ok(path) => path,
        Err(_) => return CommandOutput::error("Workspace search", "Project root is invalid."),
    };

    let mut visited = 0usize;
    let mut results = Vec::new();
    search_dir(&root, query, max_results, &mut visited, &mut results);

    let mut lines = vec![
        format!("project_root: {}", root.display()),
        format!("query: {query}"),
        format!("files_scanned: {visited}"),
        format!("matches: {}", results.len()),
    ];
    for result in &results {
        lines.push(format!(
            "{}:{} | {}",
            display_relative(&root, &result.path),
            result.line,
            result.preview
        ));
    }

    CommandOutput::info(
        "Workspace search",
        lines,
        serde_json::json!({
            "ok": true,
            "query": query,
            "files_scanned": visited,
            "matches": results.iter().map(|result| {
                serde_json::json!({
                    "path": display_relative(&root, &result.path),
                    "line": result.line,
                    "preview": result.preview
                })
            }).collect::<Vec<_>>()
        }),
    )
}

fn search_dir(
    dir: &Path,
    query: &str,
    max_results: usize,
    visited: &mut usize,
    results: &mut Vec<SearchResult>,
) {
    if *visited >= MAX_SCAN_FILES || results.len() >= max_results {
        return;
    }
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        if *visited >= MAX_SCAN_FILES || results.len() >= max_results {
            break;
        }
        let path = entry.path();
        let name = path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("");
        if should_skip(name) {
            continue;
        }
        if path.is_dir() {
            search_dir(&path, query, max_results, visited, results);
        } else if path.is_file() {
            *visited += 1;
            if is_likely_binary(&path) {
                continue;
            }
            let Ok(content) = fs::read_to_string(&path) else {
                continue;
            };
            for (line_index, line) in content.lines().enumerate() {
                if line
                    .to_ascii_lowercase()
                    .contains(&query.to_ascii_lowercase())
                {
                    results.push(SearchResult {
                        path: path.clone(),
                        line: line_index + 1,
                        preview: line.trim().chars().take(180).collect(),
                    });
                    if results.len() >= max_results {
                        break;
                    }
                }
            }
        }
    }
}

fn resolve_project_path(project_root: &Path, requested: &str) -> Result<PathBuf, ()> {
    let root = project_root.canonicalize().map_err(|_| ())?;
    let joined = root.join(requested);
    let candidate = if joined.exists() {
        joined.canonicalize().map_err(|_| ())?
    } else {
        normalize_missing_path(&joined)
    };
    if candidate.starts_with(&root) {
        Ok(candidate)
    } else {
        Err(())
    }
}

fn normalize_missing_path(path: &Path) -> PathBuf {
    let mut clean = PathBuf::new();
    for component in path.components() {
        match component {
            std::path::Component::ParentDir => {
                clean.pop();
            }
            std::path::Component::CurDir => {}
            other => clean.push(other.as_os_str()),
        }
    }
    clean
}

fn display_relative(root: &Path, path: &Path) -> String {
    path.strip_prefix(root)
        .unwrap_or(path)
        .display()
        .to_string()
}

fn should_skip(name: &str) -> bool {
    matches!(
        name,
        ".git" | "target" | "target_gnu" | "node_modules" | ".cache"
    )
}

fn is_likely_binary(path: &Path) -> bool {
    matches!(
        path.extension()
            .and_then(|ext| ext.to_str())
            .unwrap_or("")
            .to_ascii_lowercase()
            .as_str(),
        "png" | "jpg" | "jpeg" | "gif" | "webp" | "ico" | "exe" | "dll" | "pdb" | "bin"
    )
}

fn usize_arg(command: &ParsedCommand, name: &str, default: usize) -> usize {
    command
        .arg(name)
        .and_then(|value| value.parse::<usize>().ok())
        .unwrap_or(default)
}

struct SearchResult {
    path: PathBuf,
    line: usize,
    preview: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_path_guard_keeps_project_root() {
        let root = Path::new("C:/tmp/example_project");
        let bad = resolve_project_path(root, "../outside.txt");
        assert!(bad.is_err());
    }
}
