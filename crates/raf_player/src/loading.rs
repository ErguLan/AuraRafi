//! Preparation loading state shared by the native and deterministic hosts.
//!
//! The model owns only presentation facts: which stage is active, the last
//! stages seen, the compiled asset path and the wall-clock thresholds that
//! drive the slow and failed notices. It never starts work or touches a world.
use raf_core::{config::Language, i18n::t};
use raf_runtime::wire::RuntimeDiagnostic;
use std::time::{Duration, Instant};

/// Preparation that lasts longer than this raises the slow notice and exposes
/// an explicit cancel action.
pub(crate) const SLOW_PREPARATION_AFTER: Duration = Duration::from_secs(15);
/// The loading surface shows at most three recent stages.
pub(crate) const LOADING_HISTORY_LIMIT: usize = 3;

const DEFAULT_ORIGIN_KEY: &str = "runtime.loading_snapshot";

/// Presentation state for one preparation attempt.
#[derive(Debug)]
pub(crate) struct PreparationLoading {
    pub started: Instant,
    pub history: Vec<String>,
    pub slow: bool,
    pub failed: bool,
    pub error: Option<String>,
    /// Frozen when the attempt fails so the error surface does not rebuild
    /// every second and lose text selection.
    pub finished_at: Option<Instant>,
    pub origin_key: &'static str,
    pub stage_key: String,
    pub progress: f32,
    pub count: String,
    pub detail: String,
}

impl PreparationLoading {
    pub fn new(origin_key: &'static str, language: Language) -> Self {
        let origin_key = if origin_key.is_empty() {
            DEFAULT_ORIGIN_KEY
        } else {
            origin_key
        };
        Self {
            started: Instant::now(),
            history: Vec::with_capacity(LOADING_HISTORY_LIMIT),
            slow: false,
            failed: false,
            error: None,
            finished_at: None,
            origin_key,
            stage_key: origin_key.to_string(),
            progress: 0.0,
            count: String::new(),
            detail: t("runtime.cancel_hint", language),
        }
    }

    /// Starts a fresh attempt. Retry and transition preparation both begin here,
    /// so no stale failure, path or history can survive into the next run.
    pub fn begin(&mut self, origin_key: &'static str, language: Language) {
        *self = Self::new(origin_key, language);
    }

    pub fn stage(&mut self, key: &'static str, language: Language) {
        self.stage_key = key.to_string();
        self.progress = 0.0;
        self.count.clear();
        self.detail = t("runtime.cancel_hint", language);
        push_loading_history(&mut self.history, t(key, language));
    }

    pub fn compile(&mut self, path: &str, completed: usize, total: usize, language: Language) {
        self.stage_key = "runtime.compiling".to_string();
        self.progress = if total > 0 {
            (completed as f32 / total as f32).clamp(0.0, 1.0)
        } else {
            0.0
        };
        self.count = format!("{completed}/{total}");
        self.detail = path.to_string();
        push_loading_history(&mut self.history, t("runtime.compiling", language));
    }

    pub fn finish(&mut self, language: Language) {
        self.history.clear();
        self.slow = false;
        self.failed = false;
        self.error = None;
        self.finished_at = None;
        self.progress = 0.0;
        self.count.clear();
        self.detail = t("runtime.cancel_hint", language);
        self.started = Instant::now();
    }

    pub fn fail(&mut self, error: impl Into<String>) {
        self.slow = false;
        self.failed = true;
        self.error = Some(error.into());
        self.finished_at = Some(Instant::now());
    }

    /// Marks the attempt slow once and returns whether the visual state
    /// changed, so the host rebuilds the surface only on the transition.
    pub fn update_slow(&mut self, now: Instant) -> bool {
        if !self.failed
            && !self.slow
            && now.saturating_duration_since(self.started) >= SLOW_PREPARATION_AFTER
        {
            self.slow = true;
            return true;
        }
        false
    }

    pub fn elapsed_secs(&self, now: Instant) -> u64 {
        self.finished_at
            .unwrap_or(now)
            .saturating_duration_since(self.started)
            .as_secs()
    }
}

/// Records a stage label, skipping consecutive duplicates and keeping the list
/// bounded to the three entries the loading card can display.
pub(crate) fn push_loading_history(history: &mut Vec<String>, stage: String) {
    if stage.is_empty() || history.last() == Some(&stage) {
        return;
    }
    history.push(stage);
    if history.len() > LOADING_HISTORY_LIMIT {
        let excess = history.len() - LOADING_HISTORY_LIMIT;
        history.drain(..excess);
    }
}

pub(crate) fn stage_diagnostic(message: &str, stage: &str, at_ms: u64) -> RuntimeDiagnostic {
    let message = if message.is_empty() {
        stage.to_string()
    } else {
        message.to_string()
    };
    RuntimeDiagnostic::info("runtime.prepare.stage", "runtime", message)
        .detail(stage.to_string())
        .at_ms(at_ms)
}

/// Builds the structured failure reported to the editor console. The stage and
/// path are preserved in `detail` even when the human message is an OS error.
pub(crate) fn failure_diagnostic(
    message: &str,
    stage: &str,
    path: &str,
    at_ms: u64,
) -> RuntimeDiagnostic {
    let message = if message.is_empty() {
        "Runtime preparation failed".to_string()
    } else {
        message.to_string()
    };
    let detail = match (stage.is_empty(), path.is_empty()) {
        (false, false) => format!("{stage} | {path}"),
        (false, true) => stage.to_string(),
        (true, false) => path.to_string(),
        (true, true) => "runtime.prepare".to_string(),
    };
    RuntimeDiagnostic::error("runtime.prepare.failed", "runtime", message)
        .detail(detail)
        .at_ms(at_ms)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn history_is_bounded_and_skips_consecutive_duplicates() {
        let mut history = Vec::new();
        for stage in ["A", "A", "B", "C", "D"] {
            push_loading_history(&mut history, stage.to_string());
        }
        assert_eq!(
            history,
            vec!["B".to_string(), "C".to_string(), "D".to_string()]
        );
    }

    #[test]
    fn slow_notice_starts_at_fifteen_seconds() {
        let mut loading = PreparationLoading::new("runtime.loading_snapshot", Language::English);
        let started = loading.started;
        assert!(!loading.update_slow(started + Duration::from_secs(14)));
        assert!(!loading.slow);
        assert!(loading.update_slow(started + SLOW_PREPARATION_AFTER));
        assert!(loading.slow);
        assert!(!loading.update_slow(started + Duration::from_secs(30)));
    }

    #[test]
    fn failure_diagnostic_preserves_stage_and_path() {
        let diagnostic = failure_diagnostic(
            "Script compile error",
            "runtime.compiling",
            "assets/scripts/player.raf",
            4200,
        );
        assert_eq!(diagnostic.code, "runtime.prepare.failed");
        assert_eq!(
            diagnostic.severity,
            raf_runtime::wire::RuntimeDiagnosticSeverity::Error
        );
        assert_eq!(diagnostic.source, "runtime");
        assert_eq!(diagnostic.message, "Script compile error");
        assert_eq!(
            diagnostic.detail.as_deref(),
            Some("runtime.compiling | assets/scripts/player.raf")
        );
        assert_eq!(diagnostic.at_ms, 4200);
    }

    #[test]
    fn retry_reset_clears_failure_history_and_progress() {
        let mut loading = PreparationLoading::new("runtime.loading_snapshot", Language::English);
        loading.stage("runtime.compiling_nodes", Language::English);
        loading.compile("assets/scripts/player.raf", 2, 5, Language::English);
        loading.fail("compile failed");
        loading.slow = true;

        loading.begin("runtime.loading_session", Language::English);

        assert_eq!(loading.origin_key, "runtime.loading_session");
        assert_eq!(loading.stage_key, "runtime.loading_session");
        assert!(!loading.failed);
        assert!(!loading.slow);
        assert!(loading.error.is_none());
        assert!(loading.finished_at.is_none());
        assert!(loading.history.is_empty());
        assert_eq!(loading.progress, 0.0);
        assert!(loading.count.is_empty());
        assert_eq!(
            loading.detail,
            t("runtime.cancel_hint", Language::English)
        );
    }

    #[test]
    fn elapsed_seconds_follow_the_attempt_clock() {
        let loading = PreparationLoading::new("runtime.loading_snapshot", Language::English);
        assert_eq!(
            loading.elapsed_secs(loading.started + Duration::from_millis(1_500)),
            1
        );
    }
}
