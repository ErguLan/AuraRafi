//! Player executable preflight and per-instance runtime logs.
//!
//! Local Play spawns a separate player process. This module owns the two
//! boundaries that must stay testable without a window: checking that the
//! player build matches the editor before anything spawns, and capturing the
//! child stdout/stderr into bounded files that outlive the process long enough
//! for the exit report.

use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use raf_core::config::Language;
use raf_runtime::wire::RuntimeDiagnostic;
use raf_runtime::RuntimePhase;

/// The editor hard-stops a local runtime that is not ready after this window.
///
/// The player owns its own 15 second slow-preparation warning; this is the
/// editor's last-resort deadline so Play never leaves an invisible process.
pub const RUNTIME_READY_TIMEOUT: Duration = Duration::from_secs(20);

/// Maximum bytes read from both player logs when the child exits.
pub const LOG_TAIL_LIMIT: usize = 64 * 1024;

/// Maximum log lines forwarded to one console entry.
///
/// The retained UI rasterizes a text block as one atlas slot. A raw frame trace
/// can produce hundreds of long lines, which neither fits the atlas nor reads
/// well in a console row, so the UI receives a short tail while the complete
/// stdout/stderr pair remains on disk.
pub const LOG_TAIL_MAX_LINES: usize = 24;

/// Maximum characters kept from one forwarded log line.
pub const LOG_TAIL_MAX_LINE_CHARS: usize = 200;

const VERSION_PROBE_TIMEOUT: Duration = Duration::from_secs(2);
const VERSION_PROBE_LIMIT: usize = 8 * 1024;

/// A player stdout/stderr pair inside the runtime temporary directory.
///
/// Only paths are retained. The child owns the OS handles while it runs, so a
/// Windows directory removal cannot fail because the editor kept them open.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChildLogs {
    stdout: PathBuf,
    stderr: PathBuf,
}

impl ChildLogs {
    /// Creates both log files in `directory`.
    pub fn create(directory: &Path) -> std::io::Result<Self> {
        let stdout = directory.join("player.stdout.log");
        let stderr = directory.join("player.stderr.log");
        if let Err(error) = std::fs::File::create(&stdout) {
            return Err(error);
        }
        if let Err(error) = std::fs::File::create(&stderr) {
            let _ = std::fs::remove_file(&stdout);
            return Err(error);
        }
        Ok(Self { stdout, stderr })
    }

    pub fn stdout_path(&self) -> &Path {
        &self.stdout
    }

    pub fn stderr_path(&self) -> &Path {
        &self.stderr
    }

    /// Opens the files for the child. Each Play instance truncates its own copy.
    pub fn open_stdio(&self) -> std::io::Result<(Stdio, Stdio)> {
        let stdout = std::fs::File::create(&self.stdout)?;
        let stderr = std::fs::File::create(&self.stderr)?;
        Ok((Stdio::from(stdout), Stdio::from(stderr)))
    }

    /// Reads the newest bounded tail from both files.
    pub fn collect_tail(&self, limit: usize) -> RuntimeLogTail {
        let limit = limit.max(1);
        let stdout_len = std::fs::metadata(&self.stdout)
            .map(|metadata| metadata.len())
            .unwrap_or(0);
        let stderr_len = std::fs::metadata(&self.stderr)
            .map(|metadata| metadata.len())
            .unwrap_or(0);
        let stdout = read_bounded_tail(&self.stdout, limit);
        let stderr = read_bounded_tail(&self.stderr, limit);

        let mut text = String::new();
        if !stdout.trim().is_empty() {
            text.push_str("stdout:\n");
            text.push_str(stdout.trim_end());
            text.push('\n');
        }
        if !stderr.trim().is_empty() {
            text.push_str("stderr:\n");
            text.push_str(stderr.trim_end());
            text.push('\n');
        }
        let mut truncated = stdout_len > limit as u64 || stderr_len > limit as u64;
        if text.len() > limit {
            let start = text.len() - limit;
            let start = char_boundary_at(&text, start);
            text = text[start..].to_string();
            truncated = true;
        }
        let mut lines = text.lines().map(str::to_string).collect::<Vec<_>>();
        if lines.len() > LOG_TAIL_MAX_LINES {
            lines = lines.split_off(lines.len() - LOG_TAIL_MAX_LINES);
            truncated = true;
        }
        for line in &mut lines {
            if line.chars().count() > LOG_TAIL_MAX_LINE_CHARS {
                *line = truncate_line(line, LOG_TAIL_MAX_LINE_CHARS);
                truncated = true;
            }
        }
        RuntimeLogTail {
            text,
            lines,
            truncated,
        }
    }

    /// Removes both logs before the owned temporary directory goes away.
    pub fn cleanup(&self) {
        let _ = std::fs::remove_file(&self.stdout);
        let _ = std::fs::remove_file(&self.stderr);
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuntimeLogTail {
    pub text: String,
    pub lines: Vec<String>,
    pub truncated: bool,
}

impl RuntimeLogTail {
    /// Builds the bounded block shown by the editor console.
    ///
    /// The trailing localized marker tells the user that the on-disk log has
    /// more context than the row. Keeping the marker inside the block makes
    /// truncation explicit instead of silently dropping output.
    pub fn console_lines(self) -> Vec<String> {
        let mut lines = self.lines;
        if self.truncated {
            lines.push("console.runtime_log_truncated".to_string());
        }
        lines
    }
}

/// Keeps at most `max_chars` characters, ending with an ellipsis.
fn truncate_line(line: &str, max_chars: usize) -> String {
    let max_chars = max_chars.max(1);
    let mut kept = 0;
    let mut end = 0;
    for (index, character) in line.char_indices() {
        if kept + 1 >= max_chars {
            break;
        }
        kept += 1;
        end = index + character.len_utf8();
    }
    let mut truncated = line[..end].to_string();
    truncated.push('…');
    truncated
}

/// Reads at most `limit` bytes from the end of `path`.
///
/// Invalid UTF-8 caused by starting in the middle of a code point is replaced;
/// the returned text is still bounded by `limit` bytes.
pub fn read_bounded_tail(path: &Path, limit: usize) -> String {
    let limit = limit.max(1);
    let Ok(mut file) = std::fs::File::open(path) else {
        return String::new();
    };
    let length = file.metadata().map(|metadata| metadata.len()).unwrap_or(0);
    let start = length.saturating_sub(limit as u64);
    if file.seek(SeekFrom::Start(start)).is_err() {
        return String::new();
    }
    let mut bytes = Vec::new();
    if file
        .take(limit as u64 + 1)
        .read_to_end(&mut bytes)
        .is_err()
    {
        return String::new();
    }
    let text = String::from_utf8_lossy(&bytes).into_owned();
    if text.len() <= limit {
        return text;
    }
    let start = char_boundary_at(&text, text.len() - limit);
    text[start..].to_string()
}

fn char_boundary_at(text: &str, index: usize) -> usize {
    let mut index = index.min(text.len());
    while index > 0 && !text.is_char_boundary(index) {
        index -= 1;
    }
    index
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlayerExecutableKind {
    /// A sibling `aurarafi-player` binary that must expose `--version`.
    Dedicated,
    /// The editor desktop build hosts the player path in the same binary.
    EditorFallback,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlayerExecutable {
    pub path: PathBuf,
    pub kind: PlayerExecutableKind,
}

/// Resolves the sibling player binary, falling back to the editor executable.
///
/// The fallback is the desktop build where the editor entry point accepts
/// `--runtime`; that binary is built from the same workspace version, so
/// probing it with `--version` would launch the editor UI instead of printing.
pub fn resolve_player_executable(editor_executable: &Path) -> PlayerExecutable {
    let player_name = if cfg!(windows) {
        "aurarafi-player.exe"
    } else {
        "aurarafi-player"
    };
    let candidate = editor_executable
        .parent()
        .unwrap_or_else(|| Path::new("."))
        .join(player_name);
    if candidate.is_file() {
        PlayerExecutable {
            path: candidate,
            kind: PlayerExecutableKind::Dedicated,
        }
    } else {
        PlayerExecutable {
            path: editor_executable.to_path_buf(),
            kind: PlayerExecutableKind::EditorFallback,
        }
    }
}

/// The result of resolving and checking the player binary.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlayerPreflight {
    pub executable: PlayerExecutable,
    pub exists: bool,
    pub expected_version: String,
    pub reported_version: String,
}

impl PlayerPreflight {
    /// Resolves the player and verifies it before any runtime window can open.
    pub fn probe(executable: &PlayerExecutable, expected: &str) -> Result<Self, RuntimeDiagnostic> {
        if !executable.path.is_file() {
            return Err(launch_failed(format!(
                "player executable not found: {}",
                executable.path.display()
            )));
        }
        if executable.kind == PlayerExecutableKind::EditorFallback {
            return Self::verify(executable.clone(), expected, Some(expected));
        }
        let output = probe_version_output(&executable.path)?;
        let reported = parse_version_output(&output);
        Self::verify(executable.clone(), expected, reported.as_deref())
    }

    /// Pure verification used by tests and by [`Self::probe`].
    pub fn verify(
        executable: PlayerExecutable,
        expected: &str,
        reported: Option<&str>,
    ) -> Result<Self, RuntimeDiagnostic> {
        if !executable.path.is_file() {
            return Err(launch_failed(format!(
                "player executable not found: {}",
                executable.path.display()
            )));
        }
        let Some(reported) = reported.map(str::trim).filter(|value| !value.is_empty()) else {
            return Err(version_mismatch(expected, "unavailable"));
        };
        if !versions_match(expected, reported) {
            return Err(version_mismatch(expected, reported));
        }
        Ok(Self {
            executable,
            exists: true,
            expected_version: expected.to_string(),
            reported_version: reported.to_string(),
        })
    }
}

/// Extracts the first semantic version token from `--version` output.
///
/// Accepts `0.12.3`, `v0.12.3`, `aurarafi-player 0.12.3` and pre-release or
/// build suffixes. Returns `None` when no complete `major.minor.patch` token
/// exists, so an older player that prints usage text cannot pass preflight.
pub fn parse_version_output(raw: &str) -> Option<String> {
    raw.split_whitespace().find_map(|token| {
        let token = token.trim_matches(|character: char| {
            !character.is_ascii_alphanumeric()
                && character != '.'
                && character != '-'
                && character != '+'
        });
        let token = token
            .strip_prefix('v')
            .or_else(|| token.strip_prefix('V'))
            .unwrap_or(token);
        let core = token
            .split(|character| character == '-' || character == '+')
            .next()
            .unwrap_or(token);
        let mut parts = core.split('.');
        let major = parts.next()?;
        let minor = parts.next()?;
        let patch = parts.next()?;
        if parts.next().is_some() {
            return None;
        }
        if major.is_empty()
            || minor.is_empty()
            || patch.is_empty()
            || !major.chars().all(|character| character.is_ascii_digit())
            || !minor.chars().all(|character| character.is_ascii_digit())
            || !patch.chars().all(|character| character.is_ascii_digit())
        {
            return None;
        }
        Some(token.to_string())
    })
}

/// Exact match against the editor package version.
pub fn versions_match(expected: &str, reported: &str) -> bool {
    expected.trim() == reported.trim()
}

/// True when a local runtime has exhausted its preparation window.
///
/// An authenticated runtime leaves the timeout once it reports any phase other
/// than `Preparing`. An unauthenticated one is still waiting for its Hello
/// handshake, even if a cached status made it look ready.
pub fn preparation_timed_out(
    authenticated: bool,
    phase: RuntimePhase,
    now: Instant,
    deadline: Instant,
) -> bool {
    if authenticated && phase != RuntimePhase::Preparing {
        return false;
    }
    now >= deadline
}

/// Localizes runtime keys while leaving player-authored text untouched.
pub fn localized_runtime_text(text: &str, language: Language) -> String {
    if text.starts_with("console.") || text.starts_with("runtime.") || text.starts_with("ui.") {
        raf_core::i18n::t(text, language)
    } else {
        text.to_string()
    }
}

pub fn launch_failed(detail: impl Into<String>) -> RuntimeDiagnostic {
    RuntimeDiagnostic::error(
        "console.runtime_launch_failed",
        "editor",
        "console.runtime_launch_failed",
    )
    .detail(detail)
}

pub fn version_mismatch(expected: &str, reported: &str) -> RuntimeDiagnostic {
    RuntimeDiagnostic::error(
        "console.runtime_version_mismatch",
        "editor",
        "console.runtime_version_mismatch",
    )
    .detail(format!("player={reported} editor={expected}"))
}

fn probe_version_output(path: &Path) -> Result<String, RuntimeDiagnostic> {
    let mut command = Command::new(path);
    command
        .arg("--version")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    #[cfg(target_os = "windows")]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x08000000);
    }
    let mut child = command.spawn().map_err(|error| {
        launch_failed(format!("player --version could not start: {error}"))
    })?;
    let deadline = Instant::now() + VERSION_PROBE_TIMEOUT;
    loop {
        match child.try_wait() {
            Ok(Some(_status)) => {
                let stdout = child
                    .stdout
                    .take()
                    .map(read_child_pipe)
                    .unwrap_or_default();
                let stderr = child
                    .stderr
                    .take()
                    .map(read_child_pipe)
                    .unwrap_or_default();
                let mut output = stdout;
                if !stderr.is_empty() {
                    if !output.is_empty() && !output.ends_with('\n') {
                        output.push('\n');
                    }
                    output.push_str(&stderr);
                }
                return Ok(output);
            }
            Ok(None) => {
                if Instant::now() >= deadline {
                    let _ = child.kill();
                    let _ = child.wait();
                    return Err(launch_failed(format!(
                        "player --version did not answer within {}s",
                        VERSION_PROBE_TIMEOUT.as_secs()
                    )));
                }
                std::thread::sleep(Duration::from_millis(10));
            }
            Err(error) => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(launch_failed(format!(
                    "player --version could not be queried: {error}"
                )));
            }
        }
    }
}

fn read_child_pipe(pipe: impl Read) -> String {
    let mut bytes = Vec::new();
    let mut limited = pipe.take(VERSION_PROBE_LIMIT as u64);
    let _ = limited.read_to_end(&mut bytes);
    String::from_utf8_lossy(&bytes).trim().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use uuid::Uuid;

    fn temporary_directory(label: &str) -> PathBuf {
        let directory =
            std::env::temp_dir().join(format!("raf-editor-{label}-{}", Uuid::new_v4()));
        std::fs::create_dir_all(&directory).expect("temporary directory");
        directory
    }

    #[test]
    fn version_output_is_parsed_from_common_player_lines() {
        assert_eq!(
            parse_version_output("aurarafi-player 0.12.3\n"),
            Some("0.12.3".to_string())
        );
        assert_eq!(parse_version_output("v0.12.3"), Some("0.12.3".to_string()));
        assert_eq!(
            parse_version_output("aurarafi-player 1.0.0+local (build 7)"),
            Some("1.0.0+local".to_string())
        );
        assert_eq!(parse_version_output("0.12"), None);
        assert_eq!(parse_version_output("player ready"), None);
        assert_eq!(parse_version_output("version=0.12.3"), None);
    }

    #[test]
    fn version_mismatch_reports_structured_diagnostic() {
        let directory = temporary_directory("preflight");
        let path = directory.join(if cfg!(windows) {
            "aurarafi-player.exe"
        } else {
            "aurarafi-player"
        });
        std::fs::write(&path, b"not a real executable").unwrap();
        let executable = PlayerExecutable {
            path: path.clone(),
            kind: PlayerExecutableKind::Dedicated,
        };

        let error =
            PlayerPreflight::verify(executable.clone(), "0.2.0", Some("0.1.0")).unwrap_err();
        assert_eq!(error.code, "console.runtime_version_mismatch");
        assert_eq!(error.detail.as_deref(), Some("player=0.1.0 editor=0.2.0"));
        assert!(PlayerPreflight::verify(executable, "0.2.0", Some("0.2.0")).is_ok());

        let _ = std::fs::remove_file(path);
        let _ = std::fs::remove_dir(directory);
    }

    #[test]
    fn preflight_rejects_missing_player_before_spawn() {
        let missing = PlayerExecutable {
            path: std::env::temp_dir().join(format!("missing-player-{}", Uuid::new_v4())),
            kind: PlayerExecutableKind::Dedicated,
        };
        let error = PlayerPreflight::probe(&missing, "0.1.0").unwrap_err();
        assert_eq!(error.code, "console.runtime_launch_failed");
    }

    #[test]
    fn runtime_log_tail_is_bounded() {
        let directory = temporary_directory("runtime-log");
        let logs = ChildLogs::create(&directory).unwrap();
        std::fs::write(logs.stdout_path(), "a".repeat(LOG_TAIL_LIMIT * 2)).unwrap();
        std::fs::write(logs.stderr_path(), "b".repeat(1024)).unwrap();

        let tail = logs.collect_tail(1024);
        assert!(tail.text.len() <= 1024);
        assert!(tail.lines.len() <= LOG_TAIL_MAX_LINES);
        assert!(tail.truncated);
        assert!(
            tail.lines
                .iter()
                .all(|line| line.chars().count() <= LOG_TAIL_MAX_LINE_CHARS)
        );

        logs.cleanup();
        let _ = std::fs::remove_dir(directory);
    }

    #[test]
    fn console_lines_are_bounded_and_marked_when_truncated() {
        let directory = temporary_directory("runtime-log-console");
        let logs = ChildLogs::create(&directory).unwrap();
        let long_line = format!("trace {}", "x".repeat(LOG_TAIL_MAX_LINE_CHARS * 4));
        std::fs::write(logs.stdout_path(), format!("{long_line}\nnext line\n")).unwrap();

        let tail = logs.collect_tail(LOG_TAIL_LIMIT);
        assert!(tail.truncated);
        let lines = tail.console_lines();
        assert_eq!(
            lines.last().map(String::as_str),
            Some("console.runtime_log_truncated")
        );
        assert!(
            lines[..lines.len() - 1]
                .iter()
                .all(|line| line.chars().count() <= LOG_TAIL_MAX_LINE_CHARS)
        );
        assert!(lines.iter().any(|line| line.ends_with('…')));

        logs.cleanup();
        let _ = std::fs::remove_dir(directory);
    }

    #[test]
    fn readiness_timeout_only_applies_while_preparing() {
        let base = Instant::now();
        let deadline = base + RUNTIME_READY_TIMEOUT;
        assert!(!preparation_timed_out(
            false,
            RuntimePhase::Preparing,
            base + Duration::from_secs(19),
            deadline
        ));
        assert!(preparation_timed_out(
            false,
            RuntimePhase::Preparing,
            base + Duration::from_secs(21),
            deadline
        ));
        assert!(preparation_timed_out(
            true,
            RuntimePhase::Preparing,
            base + Duration::from_secs(21),
            deadline
        ));
        assert!(!preparation_timed_out(
            true,
            RuntimePhase::Running,
            base + Duration::from_secs(21),
            deadline
        ));
        assert!(!preparation_timed_out(
            true,
            RuntimePhase::Paused,
            base + Duration::from_secs(21),
            deadline
        ));
    }
}
