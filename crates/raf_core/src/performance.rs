//! Shared performance telemetry, benchmark reports and diagnosis.
//!
//! The editor records per-frame samples into a bounded ring buffer. Every
//! surface (native Agent, CLI, MCP) reads the same windowed aggregation and
//! the same benchmark file format, so an agent sees identical numbers no
//! matter which entry point asked.
//!
//! The module is intentionally transport-free: it only depends on `serde` and
//! the standard library, which keeps it unit-testable and usable by the
//! headless CLI without a renderer.

use crate::project_store::write_atomic;
use serde::{Deserialize, Serialize};
use std::collections::VecDeque;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

/// Ring capacity. One sample every 250 ms keeps ten minutes of history.
pub const MAX_TELEMETRY_SAMPLES: usize = 2400;
pub const DEFAULT_WINDOW_SECONDS: f32 = 5.0;
pub const MAX_WINDOW_SECONDS: f32 = 600.0;
pub const BENCHMARK_DIRECTORY: &str = ".aura_rafi/benchmarks";

/// One bounded frame sample. Counters coming from the renderer are cumulative
/// where the renderer reports cumulative values (hitches, cache misses,
/// shadow rebuilds); the window aggregation converts them into deltas.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, Default)]
pub struct PerformanceSample {
    pub at_seconds: f64,
    pub fps: f32,
    pub frame_ms: f32,
    pub cpu_ms: f32,
    pub gpu_ms: f32,
    pub draw_calls: u32,
    pub mesh_draw_calls: u32,
    pub line_draw_calls: u32,
    pub overlay_draw_calls: u32,
    pub shadow_draw_calls: u32,
    pub upload_bytes: u64,
    pub upload_rejected: u64,
    pub resident_bytes: u64,
    pub hitches: u64,
    pub mesh_cache_misses: u32,
    pub shadow_map_rebuilds: u32,
    pub shadow_prepare_cpu_ms: f32,
}

/// Aggregate of one measured metric.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, Default)]
pub struct MetricStats {
    pub avg: f32,
    pub min: f32,
    pub max: f32,
    pub p50: f32,
    pub p95: f32,
}

impl MetricStats {
    pub fn is_empty(&self) -> bool {
        self.avg == 0.0 && self.min == 0.0 && self.max == 0.0
    }
}

/// Stable description of where a measurement came from. Two benchmarks are
/// only comparable when this environment matches.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PerformanceEnvironment {
    pub backend: String,
    pub gpu_hardware: bool,
    pub gpu_timing_supported: bool,
    pub target_fps: u16,
    pub requested_target_fps: u16,
    pub width: u32,
    pub height: u32,
    pub revision: u64,
    pub scene_fingerprint: u64,
}

impl Default for PerformanceEnvironment {
    fn default() -> Self {
        Self {
            backend: "unknown".to_string(),
            gpu_hardware: false,
            gpu_timing_supported: false,
            target_fps: 0,
            requested_target_fps: 0,
            width: 0,
            height: 0,
            revision: 0,
            scene_fingerprint: 0,
        }
    }
}

/// Automatic bottleneck classification derived from one window.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct PerformanceDiagnosis {
    /// One of: no_data, at_target, gpu_bound, cpu_bound, hitch_prone,
    /// upload_pressure, balanced.
    pub verdict: String,
    pub evidence: Vec<String>,
    pub suggestions: Vec<String>,
}

/// Windowed view of the telemetry ring.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PerformanceWindow {
    pub window_seconds: f32,
    pub sample_count: usize,
    pub environment: PerformanceEnvironment,
    pub fps: MetricStats,
    pub frame_ms: MetricStats,
    pub cpu_ms: MetricStats,
    pub gpu_ms: MetricStats,
    pub draw_calls: MetricStats,
    /// Total bytes uploaded during the window.
    pub upload_bytes: u64,
    /// Latest cumulative upload rejection count.
    pub upload_rejected: u64,
    pub resident_mb: f32,
    /// Hitches recorded during the window (delta of the cumulative counter).
    pub hitches: u64,
    pub mesh_draw_calls: u32,
    pub line_draw_calls: u32,
    pub overlay_draw_calls: u32,
    pub shadow_draw_calls: u32,
    pub mesh_cache_misses: u32,
    pub shadow_map_rebuilds: u32,
    pub shadow_prepare_cpu_ms: f32,
    pub diagnosis: PerformanceDiagnosis,
}

impl PerformanceWindow {
    pub fn empty(window_seconds: f32, environment: &PerformanceEnvironment) -> Self {
        Self {
            window_seconds,
            sample_count: 0,
            environment: environment.clone(),
            fps: MetricStats::default(),
            frame_ms: MetricStats::default(),
            cpu_ms: MetricStats::default(),
            gpu_ms: MetricStats::default(),
            draw_calls: MetricStats::default(),
            upload_bytes: 0,
            upload_rejected: 0,
            resident_mb: 0.0,
            hitches: 0,
            mesh_draw_calls: 0,
            line_draw_calls: 0,
            overlay_draw_calls: 0,
            shadow_draw_calls: 0,
            mesh_cache_misses: 0,
            shadow_map_rebuilds: 0,
            shadow_prepare_cpu_ms: 0.0,
            diagnosis: PerformanceDiagnosis {
                verdict: "no_data".to_string(),
                evidence: vec!["No frame samples were recorded in this window.".to_string()],
                suggestions: vec![
                    "Keep the editor viewport active while sampling performance.".to_string(),
                ],
            },
        }
    }

    pub fn from_samples(
        samples: &[PerformanceSample],
        window_seconds: f32,
        environment: &PerformanceEnvironment,
    ) -> Self {
        if samples.is_empty() {
            return Self::empty(window_seconds, environment);
        }
        let first = samples.first().copied().unwrap_or_default();
        let latest = samples.last().copied().unwrap_or_default();
        let fps = metric(samples.iter().map(|sample| sample.fps));
        let frame_ms = metric(samples.iter().map(|sample| sample.frame_ms));
        let cpu_ms = metric(samples.iter().map(|sample| sample.cpu_ms));
        let gpu_ms = metric(samples.iter().map(|sample| sample.gpu_ms));
        let draw_calls = metric(samples.iter().map(|sample| sample.draw_calls as f32));
        let hitches = latest.hitches.saturating_sub(first.hitches);
        let window = Self {
            window_seconds,
            sample_count: samples.len(),
            environment: environment.clone(),
            fps,
            frame_ms,
            cpu_ms,
            gpu_ms,
            draw_calls,
            upload_bytes: samples.iter().map(|sample| sample.upload_bytes).sum(),
            upload_rejected: latest.upload_rejected,
            resident_mb: latest.resident_bytes as f32 / (1024.0 * 1024.0),
            hitches,
            mesh_draw_calls: latest.mesh_draw_calls,
            line_draw_calls: latest.line_draw_calls,
            overlay_draw_calls: latest.overlay_draw_calls,
            shadow_draw_calls: latest.shadow_draw_calls,
            mesh_cache_misses: latest.mesh_cache_misses,
            shadow_map_rebuilds: latest.shadow_map_rebuilds,
            shadow_prepare_cpu_ms: latest.shadow_prepare_cpu_ms,
            diagnosis: PerformanceDiagnosis::default(),
        };
        let diagnosis = diagnose(&window);
        Self { diagnosis, ..window }
    }
}

/// Bounded ring buffer with an optional benchmark state machine.
#[derive(Debug, Default)]
pub struct PerformanceTelemetry {
    samples: VecDeque<PerformanceSample>,
    last_recorded_at: Option<f64>,
}

impl PerformanceTelemetry {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn record_if_due(&mut self, sample: PerformanceSample, min_interval_seconds: f32) -> bool {
        if let Some(last) = self.last_recorded_at {
            if sample.at_seconds - last < min_interval_seconds.max(0.0) as f64 {
                return false;
            }
        }
        self.last_recorded_at = Some(sample.at_seconds);
        self.samples.push_back(sample);
        while self.samples.len() > MAX_TELEMETRY_SAMPLES {
            self.samples.pop_front();
        }
        true
    }

    pub fn clear(&mut self) {
        self.samples.clear();
        self.last_recorded_at = None;
    }

    pub fn sample_count(&self) -> usize {
        self.samples.len()
    }

    pub fn latest(&self) -> Option<PerformanceSample> {
        self.samples.back().copied()
    }

    pub fn window(&self, seconds: f32, environment: &PerformanceEnvironment) -> PerformanceWindow {
        let seconds = seconds.clamp(0.5, MAX_WINDOW_SECONDS);
        let Some(latest) = self.samples.back() else {
            return PerformanceWindow::empty(seconds, environment);
        };
        let cutoff = latest.at_seconds - seconds as f64;
        let start = self
            .samples
            .iter()
            .position(|sample| sample.at_seconds >= cutoff)
            .unwrap_or(0);
        let slice: Vec<PerformanceSample> = self.samples.iter().skip(start).copied().collect();
        PerformanceWindow::from_samples(&slice, seconds, environment)
    }
}

/// Project identity captured in a benchmark report.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct BenchmarkProject {
    pub id: String,
    pub path: String,
    pub engine_version: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BenchmarkPhase {
    Warmup,
    Measuring,
    Completed,
}

impl BenchmarkPhase {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Warmup => "warmup",
            Self::Measuring => "measuring",
            Self::Completed => "completed",
        }
    }
}

/// Live benchmark state owned by the editor thread.
#[derive(Debug)]
pub struct BenchmarkRun {
    pub id: String,
    pub label: String,
    pub project: BenchmarkProject,
    pub environment: PerformanceEnvironment,
    pub warmup: Duration,
    pub duration: Duration,
    pub started_at: Instant,
    pub started_unix_ms: i64,
    pub phase: BenchmarkPhase,
    samples: Vec<PerformanceSample>,
}

/// Serializable progress snapshot returned while a benchmark is running.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BenchmarkProgress {
    pub id: String,
    pub label: String,
    pub phase: BenchmarkPhase,
    pub elapsed_seconds: f32,
    pub warmup_seconds: f32,
    pub measured_seconds: f32,
    pub remaining_seconds: f32,
    pub sample_count: usize,
    pub fps_avg: f32,
    pub frame_p95: f32,
    pub gpu_ms_avg: f32,
}

impl BenchmarkRun {
    pub fn new(
        id: String,
        label: String,
        project: BenchmarkProject,
        environment: PerformanceEnvironment,
        warmup_seconds: f32,
        measured_seconds: f32,
    ) -> Self {
        Self {
            id,
            label,
            project,
            environment,
            warmup: Duration::from_secs_f32(warmup_seconds.clamp(0.0, 60.0)),
            duration: Duration::from_secs_f32(measured_seconds.clamp(1.0, 600.0)),
            started_at: Instant::now(),
            started_unix_ms: unix_millis(),
            phase: BenchmarkPhase::Warmup,
            samples: Vec::new(),
        }
    }

    pub fn tick(&mut self, now: Instant, sample: PerformanceSample) {
        let elapsed = now.duration_since(self.started_at);
        if self.phase == BenchmarkPhase::Warmup && elapsed >= self.warmup {
            self.phase = BenchmarkPhase::Measuring;
            self.samples.clear();
        }
        if self.phase == BenchmarkPhase::Completed {
            return;
        }
        self.samples.push(sample);
        if self.samples.len() > 400_000 {
            self.samples.drain(..200_000);
        }
        if self.phase == BenchmarkPhase::Measuring && elapsed >= self.warmup + self.duration {
            self.phase = BenchmarkPhase::Completed;
        }
    }

    fn measured_elapsed(&self, now: Instant) -> Duration {
        now.duration_since(self.started_at)
            .saturating_sub(self.warmup)
            .min(self.duration)
    }

    pub fn progress(&self, now: Instant) -> BenchmarkProgress {
        let elapsed = now.duration_since(self.started_at);
        let measured = self.measured_elapsed(now).as_secs_f32();
        let window = PerformanceWindow::from_samples(
            &self.samples,
            measured.max(0.5),
            &self.environment,
        );
        BenchmarkProgress {
            id: self.id.clone(),
            label: self.label.clone(),
            phase: self.phase,
            elapsed_seconds: elapsed.as_secs_f32(),
            warmup_seconds: self.warmup.as_secs_f32(),
            measured_seconds: measured,
            remaining_seconds: (self.duration.as_secs_f32() - measured).max(0.0),
            sample_count: self.samples.len(),
            fps_avg: window.fps.avg,
            frame_p95: window.frame_ms.p95,
            gpu_ms_avg: window.gpu_ms.avg,
        }
    }

    pub fn finish(&mut self, now: Instant) -> Result<BenchmarkReport, String> {
        if self.phase == BenchmarkPhase::Warmup {
            return Err(
                "The benchmark has not finished its warmup and has no measured samples yet."
                    .to_string(),
            );
        }
        let measured = self.measured_elapsed(now).as_secs_f32();
        if self.samples.len() < 8 || measured < 1.0 {
            return Err(format!(
                "The benchmark only collected {} sample(s) over {:.2}s; keep the viewport active \
                 for at least one second before finishing.",
                self.samples.len(),
                measured
            ));
        }
        let window =
            PerformanceWindow::from_samples(&self.samples, measured, &self.environment);
        self.phase = BenchmarkPhase::Completed;
        Ok(BenchmarkReport {
            id: self.id.clone(),
            label: self.label.clone(),
            created_at: rfc3339_now(),
            warmup_seconds: self.warmup.as_secs_f32(),
            measured_seconds: measured,
            sample_count: self.samples.len(),
            project: self.project.clone(),
            window,
        })
    }

    pub fn cancel(&mut self) {
        self.phase = BenchmarkPhase::Completed;
        self.samples.clear();
    }
}

/// Persisted benchmark report.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BenchmarkReport {
    pub id: String,
    pub label: String,
    pub created_at: String,
    pub warmup_seconds: f32,
    pub measured_seconds: f32,
    pub sample_count: usize,
    pub project: BenchmarkProject,
    pub window: PerformanceWindow,
}

/// Compact row used by `benchmark list`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BenchmarkSummary {
    pub id: String,
    pub label: String,
    pub created_at: String,
    pub measured_seconds: f32,
    pub sample_count: usize,
    pub fps_avg: f32,
    pub frame_p95: f32,
    pub gpu_ms_avg: f32,
    pub draw_calls_avg: f32,
    pub verdict: String,
    pub scene_fingerprint: u64,
    pub width: u32,
    pub height: u32,
    pub target_fps: u16,
    pub backend: String,
}

impl From<&BenchmarkReport> for BenchmarkSummary {
    fn from(report: &BenchmarkReport) -> Self {
        Self {
            id: report.id.clone(),
            label: report.label.clone(),
            created_at: report.created_at.clone(),
            measured_seconds: report.measured_seconds,
            sample_count: report.sample_count,
            fps_avg: report.window.fps.avg,
            frame_p95: report.window.frame_ms.p95,
            gpu_ms_avg: report.window.gpu_ms.avg,
            draw_calls_avg: report.window.draw_calls.avg,
            verdict: report.window.diagnosis.verdict.clone(),
            scene_fingerprint: report.window.environment.scene_fingerprint,
            width: report.window.environment.width,
            height: report.window.environment.height,
            target_fps: report.window.environment.target_fps,
            backend: report.window.environment.backend.clone(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BenchmarkDelta {
    pub left: f32,
    pub right: f32,
    pub delta: f32,
    pub delta_percent: f32,
}

impl BenchmarkDelta {
    fn new(left: f32, right: f32) -> Self {
        let delta = right - left;
        let delta_percent = if left.abs() > f32::EPSILON {
            (delta / left.abs()) * 100.0
        } else {
            0.0
        };
        Self {
            left,
            right,
            delta,
            delta_percent,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BenchmarkComparison {
    pub left: BenchmarkSummary,
    pub right: BenchmarkSummary,
    pub comparable: bool,
    pub warnings: Vec<String>,
    /// improved, regressed, unchanged or mixed.
    pub verdict: String,
    pub fps: BenchmarkDelta,
    pub frame_p95: BenchmarkDelta,
    pub cpu_ms: BenchmarkDelta,
    pub gpu_ms: BenchmarkDelta,
    pub draw_calls: BenchmarkDelta,
    pub diagnosis_left: String,
    pub diagnosis_right: String,
}

/// Filesystem store rooted at a project directory.
#[derive(Debug, Clone)]
pub struct BenchmarkStore {
    root: PathBuf,
}

impl BenchmarkStore {
    pub fn for_project(root: &Path) -> Self {
        Self {
            root: root.to_path_buf(),
        }
    }

    pub fn directory(&self) -> PathBuf {
        self.root.join(BENCHMARK_DIRECTORY)
    }

    pub fn save(&self, report: &BenchmarkReport) -> Result<PathBuf, String> {
        validate_id(&report.id)?;
        let directory = self.directory();
        std::fs::create_dir_all(&directory)
            .map_err(|error| format!("Create benchmark directory: {error}"))?;
        let path = directory.join(format!("{}.json", report.id));
        let bytes = serde_json::to_vec_pretty(report)
            .map_err(|error| format!("Encode benchmark report: {error}"))?;
        write_atomic(&path, &bytes)?;
        Ok(path)
    }

    pub fn load(&self, id: &str) -> Result<BenchmarkReport, String> {
        validate_id(id)?;
        let path = self.directory().join(format!("{id}.json"));
        let bytes = std::fs::read(&path).map_err(|error| {
            format!(
                "Could not read benchmark '{id}' at {}: {error}",
                path.display()
            )
        })?;
        serde_json::from_slice(&bytes)
            .map_err(|error| format!("Benchmark '{id}' is not valid JSON: {error}"))
    }

    pub fn list(&self) -> Result<Vec<BenchmarkSummary>, String> {
        let directory = self.directory();
        let mut reports = Vec::new();
        let entries = match std::fs::read_dir(&directory) {
            Ok(entries) => entries,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(reports),
            Err(error) => return Err(format!("Read benchmark directory: {error}")),
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().and_then(|value| value.to_str()) != Some("json") {
                continue;
            }
            let Ok(bytes) = std::fs::read(&path) else {
                continue;
            };
            let Ok(report) = serde_json::from_slice::<BenchmarkReport>(&bytes) else {
                continue;
            };
            reports.push(BenchmarkSummary::from(&report));
        }
        reports.sort_by(|left, right| right.created_at.cmp(&left.created_at));
        reports.truncate(64);
        Ok(reports)
    }

    pub fn compare(&self, left: &str, right: &str) -> Result<BenchmarkComparison, String> {
        let left_report = self.load(left)?;
        let right_report = self.load(right)?;
        Ok(compare_reports(&left_report, &right_report))
    }
}

pub fn compare_reports(left: &BenchmarkReport, right: &BenchmarkReport) -> BenchmarkComparison {
    let mut warnings = Vec::new();
    let left_env = &left.window.environment;
    let right_env = &right.window.environment;
    if left.project.id != right.project.id {
        warnings.push("The reports belong to different projects.".to_string());
    }
    if left_env.scene_fingerprint != right_env.scene_fingerprint {
        warnings.push(
            "The scene changed between reports; compare only intentional variants.".to_string(),
        );
    }
    if (left_env.width, left_env.height) != (right_env.width, right_env.height) {
        warnings.push(format!(
            "Resolution differs ({}x{} vs {}x{}).",
            left_env.width, left_env.height, right_env.width, right_env.height
        ));
    }
    if left_env.target_fps != right_env.target_fps {
        warnings.push(format!(
            "Target FPS differs ({} vs {}).",
            left_env.target_fps, right_env.target_fps
        ));
    }
    if left_env.backend != right_env.backend {
        warnings.push(format!(
            "Backend differs ({} vs {}).",
            left_env.backend, right_env.backend
        ));
    }
    let comparable = warnings.is_empty();
    let fps = BenchmarkDelta::new(left.window.fps.avg, right.window.fps.avg);
    let frame_p95 =
        BenchmarkDelta::new(left.window.frame_ms.p95, right.window.frame_ms.p95);
    let cpu_ms = BenchmarkDelta::new(left.window.cpu_ms.avg, right.window.cpu_ms.avg);
    let gpu_ms = BenchmarkDelta::new(left.window.gpu_ms.avg, right.window.gpu_ms.avg);
    let draw_calls =
        BenchmarkDelta::new(left.window.draw_calls.avg, right.window.draw_calls.avg);
    let verdict = if !comparable {
        "incomparable".to_string()
    } else if fps.delta_percent >= 3.0 && frame_p95.delta_percent <= 5.0 {
        "improved".to_string()
    } else if fps.delta_percent <= -3.0 || frame_p95.delta_percent >= 10.0 {
        "regressed".to_string()
    } else {
        "unchanged".to_string()
    };
    BenchmarkComparison {
        left: BenchmarkSummary::from(left),
        right: BenchmarkSummary::from(right),
        comparable,
        warnings,
        verdict,
        fps,
        frame_p95,
        cpu_ms,
        gpu_ms,
        draw_calls,
        diagnosis_left: left.window.diagnosis.verdict.clone(),
        diagnosis_right: right.window.diagnosis.verdict.clone(),
    }
}

/// Classifies the dominant bottleneck of one window.
pub fn diagnose(window: &PerformanceWindow) -> PerformanceDiagnosis {
    if window.sample_count == 0 {
        return PerformanceWindow::empty(window.window_seconds, &window.environment).diagnosis;
    }
    let frame_ms = if window.frame_ms.avg > 0.0 {
        window.frame_ms.avg
    } else if window.fps.avg > 0.0 {
        1000.0 / window.fps.avg
    } else {
        window.cpu_ms.avg.max(window.gpu_ms.avg)
    };
    let mut evidence = Vec::new();
    let mut suggestions = Vec::new();
    let target = window.environment.target_fps;
    if target > 0 {
        evidence.push(format!(
            "Presented {:.1} FPS against a {} FPS target.",
            window.fps.avg, target
        ));
    } else {
        evidence.push(format!("Presented {:.1} FPS.", window.fps.avg));
    }
    evidence.push(format!(
        "Frame {:.2} ms (p95 {:.2} ms), CPU {:.2} ms, GPU {:.2} ms.",
        frame_ms, window.frame_ms.p95, window.cpu_ms.avg, window.gpu_ms.avg
    ));
    evidence.push(format!(
        "Draw calls {} (mesh {}, line {}, overlay {}, shadow {}).",
        window.draw_calls.avg.max(0.0) as u32,
        window.mesh_draw_calls,
        window.line_draw_calls,
        window.overlay_draw_calls,
        window.shadow_draw_calls
    ));
    if window.hitches > 0 {
        evidence.push(format!("{} hitch(es) inside the window.", window.hitches));
    }
    let gpu_known = window.environment.gpu_timing_supported && window.gpu_ms.avg > 0.0;
    let gpu_ratio = if frame_ms > 0.0 && gpu_known {
        window.gpu_ms.avg / frame_ms
    } else {
        0.0
    };
    let cpu_ratio = if frame_ms > 0.0 {
        window.cpu_ms.avg / frame_ms
    } else {
        0.0
    };
    let verdict = if target > 0 && window.fps.avg >= target as f32 * 0.97 {
        suggestions.push(
            "The engine is already at its presentation target; raise the FPS target to measure \
             headroom, or compare draw calls if the cost is still too high."
                .to_string(),
        );
        "at_target"
    } else if gpu_ratio >= 0.7 {
        suggestions.push(
            "Reduce GPU work first: shadow resolution, overdraw, mesh triangle count and draw \
             calls usually dominate."
                .to_string(),
        );
        if window.shadow_map_rebuilds > 0 {
            suggestions.push(format!(
                "{} shadow map rebuild(s) were recorded; static lights should not rebuild every \
                 frame.",
                window.shadow_map_rebuilds
            ));
        }
        "gpu_bound"
    } else if cpu_ratio >= 0.7 {
        suggestions.push(
            "Reduce CPU work first: scene updates, UI layout cost, script ticks and per-frame \
             allocations."
                .to_string(),
        );
        "cpu_bound"
    } else if window.hitches > 0 && window.frame_ms.p95 > window.frame_ms.avg * 1.8 {
        suggestions.push(
            "Frame spikes dominate: inspect shadow rebuilds, mesh cache misses and upload \
             rejections around the hitch frames."
                .to_string(),
        );
        "hitch_prone"
    } else if window.upload_rejected > 0 {
        suggestions.push(
            "Uploads are being rejected; reduce texture churn or raise the graphics memory \
             budget."
                .to_string(),
        );
        "upload_pressure"
    } else {
        suggestions.push(
            "Cost is spread across CPU and GPU; compare a benchmark before and after one focused \
             change."
                .to_string(),
        );
        "balanced"
    };
    if window.mesh_cache_misses > 0 {
        evidence.push(format!(
            "{} mesh cache miss(es).",
            window.mesh_cache_misses
        ));
    }
    if window.shadow_prepare_cpu_ms > 0.0 {
        evidence.push(format!(
            "Shadow prepare {:.2} ms.",
            window.shadow_prepare_cpu_ms
        ));
    }
    PerformanceDiagnosis {
        verdict: verdict.to_string(),
        evidence,
        suggestions,
    }
}

fn metric(values: impl Iterator<Item = f32>) -> MetricStats {
    let mut values: Vec<f32> = values.filter(|value| value.is_finite()).collect();
    if values.is_empty() {
        return MetricStats::default();
    }
    let sum: f32 = values.iter().sum();
    let avg = sum / values.len() as f32;
    values.sort_by(|left, right| {
        left.partial_cmp(right)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    MetricStats {
        avg,
        min: values[0],
        max: values[values.len() - 1],
        p50: percentile(&values, 0.50),
        p95: percentile(&values, 0.95),
    }
}

fn percentile(sorted: &[f32], fraction: f32) -> f32 {
    if sorted.is_empty() {
        return 0.0;
    }
    let rank = (fraction.clamp(0.0, 1.0) * (sorted.len() - 1) as f32).round() as usize;
    sorted[rank.min(sorted.len() - 1)]
}

fn validate_id(id: &str) -> Result<(), String> {
    if id.is_empty()
        || id.len() > 96
        || !id
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
    {
        return Err(format!("Invalid benchmark id '{id}'."));
    }
    Ok(())
}

fn unix_millis() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis() as i64)
        .unwrap_or(0)
}

fn rfc3339_now() -> String {
    chrono::Utc::now().to_rfc3339()
}

/// Builds a filesystem-safe benchmark id such as `20261125-143012-lava-arena`.
pub fn benchmark_id(label: &str) -> String {
    let stamp = chrono::Utc::now().format("%Y%m%d-%H%M%S");
    let mut slug = String::new();
    for character in label.chars() {
        if character.is_ascii_alphanumeric() {
            slug.push(character.to_ascii_lowercase());
        } else if !slug.ends_with('-') {
            slug.push('-');
        }
        if slug.len() >= 24 {
            break;
        }
    }
    let slug = slug.trim_matches('-');
    if slug.is_empty() {
        format!("{stamp}")
    } else {
        format!("{stamp}-{slug}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample(at: f64, fps: f32, cpu: f32, gpu: f32) -> PerformanceSample {
        PerformanceSample {
            at_seconds: at,
            fps,
            frame_ms: 1000.0 / fps,
            cpu_ms: cpu,
            gpu_ms: gpu,
            draw_calls: 100,
            hitches: 0,
            ..PerformanceSample::default()
        }
    }

    #[test]
    fn window_aggregates_percentiles_and_hitch_delta() {
        let samples = vec![
            PerformanceSample { hitches: 2, ..sample(0.0, 60.0, 4.0, 8.0) },
            PerformanceSample { hitches: 2, ..sample(0.25, 50.0, 5.0, 9.0) },
            PerformanceSample { hitches: 4, ..sample(0.5, 40.0, 6.0, 10.0) },
            PerformanceSample { hitches: 4, ..sample(0.75, 30.0, 7.0, 11.0) },
        ];
        let window = PerformanceWindow::from_samples(&samples, 1.0, &PerformanceEnvironment::default());
        assert_eq!(window.sample_count, 4);
        assert!((window.fps.avg - 45.0).abs() < 0.01);
        assert!((window.fps.p95 - 60.0).abs() < 0.01);
        assert_eq!(window.hitches, 2);
    }

    #[test]
    fn empty_window_reports_no_data() {
        let window = PerformanceWindow::from_samples(&[], 5.0, &PerformanceEnvironment::default());
        assert_eq!(window.sample_count, 0);
        assert_eq!(window.diagnosis.verdict, "no_data");
    }

    #[test]
    fn diagnosis_prefers_gpu_when_gpu_dominates_frame() {
        let environment = PerformanceEnvironment {
            gpu_timing_supported: true,
            target_fps: 120,
            ..PerformanceEnvironment::default()
        };
        let samples = vec![
            sample(0.0, 60.0, 2.0, 15.0),
            sample(0.25, 60.0, 2.0, 15.5),
        ];
        let window = PerformanceWindow::from_samples(&samples, 1.0, &environment);
        assert_eq!(window.diagnosis.verdict, "gpu_bound");
    }

    #[test]
    fn telemetry_deduplicates_samples_inside_interval() {
        let mut telemetry = PerformanceTelemetry::new();
        assert!(telemetry.record_if_due(sample(0.0, 60.0, 1.0, 2.0), 0.25));
        assert!(!telemetry.record_if_due(sample(0.1, 60.0, 1.0, 2.0), 0.25));
        assert!(telemetry.record_if_due(sample(0.3, 60.0, 1.0, 2.0), 0.25));
        assert_eq!(telemetry.sample_count(), 2);
    }

    #[test]
    fn benchmark_lifecycle_produces_report() {
        let mut run = BenchmarkRun::new(
            "test-id".to_string(),
            "test".to_string(),
            BenchmarkProject::default(),
            PerformanceEnvironment::default(),
            0.0,
            10.0,
        );
        let start = run.started_at;
        for index in 0..20 {
            run.tick(
                start + Duration::from_millis(100 * index),
                sample(index as f64 * 0.1, 55.0, 3.0, 10.0),
            );
        }
        let report = run.finish(start + Duration::from_millis(2000)).unwrap();
        assert_eq!(report.sample_count, 20);
        assert!((report.window.fps.avg - 55.0).abs() < 0.01);
    }

    #[test]
    fn benchmark_comparison_flags_incomparable_scenes() {
        let mut left = BenchmarkReport {
            id: "a".into(),
            label: "a".into(),
            created_at: "2026-01-01T00:00:00Z".into(),
            warmup_seconds: 1.0,
            measured_seconds: 5.0,
            sample_count: 10,
            project: BenchmarkProject::default(),
            window: PerformanceWindow::from_samples(
                &[sample(0.0, 60.0, 3.0, 8.0)],
                5.0,
                &PerformanceEnvironment::default(),
            ),
        };
        let mut right = left.clone();
        left.window.environment.scene_fingerprint = 1;
        right.window.environment.scene_fingerprint = 2;
        let comparison = compare_reports(&left, &right);
        assert!(!comparison.comparable);
        assert_eq!(comparison.verdict, "incomparable");
    }

    #[test]
    fn benchmark_ids_are_filesystem_safe() {
        let id = benchmark_id("Lava Arena / Night!!");
        assert!(id
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_')));
    }
}
