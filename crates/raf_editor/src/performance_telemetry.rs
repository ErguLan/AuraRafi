//! Editor-side performance telemetry.
//!
//! The native frame loop records one bounded sample every 250 ms. Benchmarks
//! collect every frame while they run. State is thread-local because the
//! editor is single-threaded and every consumer (attached CLI/MCP executor,
//! native Agent tools, status surfaces) runs on that same thread.

use raf_core::performance::{
    benchmark_id, BenchmarkPhase, BenchmarkProgress, BenchmarkProject, BenchmarkReport,
    BenchmarkRun, PerformanceEnvironment, PerformanceSample, PerformanceTelemetry,
    PerformanceWindow,
};
use raf_render::bridge::RenderRuntimeSnapshot;
use std::cell::RefCell;
use std::time::Instant;

const SAMPLE_INTERVAL_SECONDS: f32 = 0.25;

struct TelemetryState {
    started_at: Instant,
    telemetry: PerformanceTelemetry,
    benchmark: Option<BenchmarkRun>,
    last_target_size: [u32; 2],
    last_runtime_present: Option<Instant>,
}

impl TelemetryState {
    fn new() -> Self {
        Self {
            started_at: Instant::now(),
            telemetry: PerformanceTelemetry::new(),
            benchmark: None,
            last_target_size: [0, 0],
            last_runtime_present: None,
        }
    }

    fn now_seconds(&self) -> f64 {
        self.started_at.elapsed().as_secs_f64()
    }
}

thread_local! {
    static STATE: RefCell<TelemetryState> = RefCell::new(TelemetryState::new());
}

/// Records one active editor frame. Called from `NativeEditorRuntime::finish_frame`.
pub fn record_frame(
    frame_cpu_ms: f32,
    frame_gpu_ms: f32,
    _total_cpu_ms: f32,
    target_size: [u32; 2],
    snapshot: &RenderRuntimeSnapshot,
) {
    let scheduler = snapshot.scheduler_metrics;
    let metrics = snapshot.last_frame_metrics;
    let frame_ms = if scheduler.frame_time_ms.is_finite() && scheduler.frame_time_ms > 0.0 {
        scheduler.frame_time_ms
    } else if scheduler.presented_fps.is_finite() && scheduler.presented_fps > 0.0 {
        1000.0 / scheduler.presented_fps
    } else {
        (frame_cpu_ms.max(0.0) + frame_gpu_ms.max(0.0)).max(0.0)
    };
    let fps = if frame_ms > 0.0 { 1000.0 / frame_ms } else { 0.0 };
    if fps <= 0.0 && frame_cpu_ms <= 0.0 && frame_gpu_ms <= 0.0 {
        return;
    }
    let gpu_ms = if frame_gpu_ms > 0.0 {
        frame_gpu_ms
    } else {
        metrics.gpu_frame_ms
    };
    STATE.with(|cell| {
        let mut state = cell.borrow_mut();
        state.last_target_size = target_size;
        let sample = PerformanceSample {
            at_seconds: state.now_seconds(),
            fps,
            frame_ms,
            cpu_ms: frame_cpu_ms.max(0.0),
            gpu_ms: gpu_ms.max(0.0),
            draw_calls: metrics.total_draw_calls(),
            mesh_draw_calls: metrics.mesh_draw_calls,
            line_draw_calls: metrics.line_draw_calls,
            overlay_draw_calls: metrics.overlay_draw_calls,
            shadow_draw_calls: metrics.shadow_draw_calls,
            upload_bytes: metrics.total_upload_bytes(),
            upload_rejected: metrics.frame_upload_rejected,
            resident_bytes: metrics.ledger_resident_bytes,
            hitches: scheduler.hitch_count,
            mesh_cache_misses: metrics.mesh_cache_misses,
            shadow_map_rebuilds: metrics.shadow_map_rebuilds,
            shadow_prepare_cpu_ms: metrics.shadow_prepare_cpu_ms,
        };
        state
            .telemetry
            .record_if_due(sample, SAMPLE_INTERVAL_SECONDS);
        if let Some(run) = state.benchmark.as_mut() {
            run.tick(Instant::now(), sample);
        }
    });
}

/// Records one same-window Play frame. The editor frame loop does not run
/// `NativeEditorRuntime::finish_frame` while the player owns presentation, so
/// without this hook `raf perf` reported zero draws and a single stale sample.
pub fn record_runtime_frame(
    cpu_ms: f32,
    render_ms: f32,
    target_size: [u32; 2],
    draw_calls: u32,
    upload_bytes: u64,
) {
    STATE.with(|cell| {
        let mut state = cell.borrow_mut();
        let now = Instant::now();
        let frame_ms = state
            .last_runtime_present
            .map(|previous| now.duration_since(previous).as_secs_f32() * 1000.0)
            .filter(|value| value.is_finite() && *value > 0.0)
            .unwrap_or_else(|| cpu_ms.max(render_ms).max(0.1));
        state.last_runtime_present = Some(now);
        state.last_target_size = target_size;
        let fps = if frame_ms > 0.0 { 1000.0 / frame_ms } else { 0.0 };
        let sample = PerformanceSample {
            at_seconds: state.now_seconds(),
            fps,
            frame_ms,
            cpu_ms: cpu_ms.max(0.0),
            gpu_ms: 0.0,
            draw_calls,
            mesh_draw_calls: 0,
            line_draw_calls: 0,
            overlay_draw_calls: 0,
            shadow_draw_calls: 0,
            upload_bytes,
            upload_rejected: 0,
            resident_bytes: 0,
            hitches: 0,
            mesh_cache_misses: 0,
            shadow_map_rebuilds: 0,
            shadow_prepare_cpu_ms: render_ms.max(0.0),
        };
        state
            .telemetry
            .record_if_due(sample, SAMPLE_INTERVAL_SECONDS);
        if let Some(run) = state.benchmark.as_mut() {
            run.tick(now, sample);
        }
    });
}

/// Windowed statistics for the last `seconds` of recorded frames.
pub fn window(seconds: f32, environment: &PerformanceEnvironment) -> PerformanceWindow {
    STATE.with(|cell| cell.borrow().telemetry.window(seconds, environment))
}

pub fn sample_count() -> usize {
    STATE.with(|cell| cell.borrow().telemetry.sample_count())
}

/// Render target size observed in the most recent recorded frame.
pub fn last_target_size() -> [u32; 2] {
    STATE.with(|cell| cell.borrow().last_target_size)
}

pub fn latest() -> Option<PerformanceSample> {
    STATE.with(|cell| cell.borrow().telemetry.latest())
}

/// Starts a benchmark. Fails when another benchmark is still open.
pub fn start_benchmark(
    label: &str,
    warmup_seconds: f32,
    measured_seconds: f32,
    project: BenchmarkProject,
    environment: PerformanceEnvironment,
) -> Result<String, String> {
    STATE.with(|cell| {
        let mut state = cell.borrow_mut();
        if let Some(run) = state.benchmark.as_ref() {
            if run.phase != BenchmarkPhase::Completed {
                return Err(format!(
                    "Benchmark '{}' is still {} with {:.1}s remaining; finish or cancel it first.",
                    run.id,
                    run.phase.as_str(),
                    run.progress(Instant::now()).remaining_seconds
                ));
            }
            state.benchmark = None;
        }
        let id = benchmark_id(label);
        let run = BenchmarkRun::new(
            id.clone(),
            label.to_string(),
            project,
            environment,
            warmup_seconds,
            measured_seconds,
        );
        state.benchmark = Some(run);
        Ok(id)
    })
}

pub fn benchmark_progress() -> Option<BenchmarkProgress> {
    STATE.with(|cell| {
        cell.borrow()
            .benchmark
            .as_ref()
            .map(|run| run.progress(Instant::now()))
    })
}

pub fn benchmark_phase() -> Option<BenchmarkPhase> {
    STATE.with(|cell| cell.borrow().benchmark.as_ref().map(|run| run.phase))
}

/// Finalizes the current benchmark. On success the run is consumed; on error
/// it stays available so the caller can keep sampling or cancel it.
pub fn finish_benchmark() -> Result<BenchmarkReport, String> {
    STATE.with(|cell| {
        let mut state = cell.borrow_mut();
        let run = state
            .benchmark
            .as_mut()
            .ok_or_else(|| "No benchmark is active. Start one with performance.benchmark.start.".to_string())?;
        let report = run.finish(Instant::now())?;
        state.benchmark = None;
        Ok(report)
    })
}

pub fn cancel_benchmark() -> Option<String> {
    STATE.with(|cell| {
        let mut state = cell.borrow_mut();
        state.benchmark.take().map(|mut run| {
            run.cancel();
            run.id
        })
    })
}

/// Test-only reset so unit tests do not leak state between cases.
#[cfg(test)]
pub(crate) fn reset_for_tests() {
    STATE.with(|cell| {
        *cell.borrow_mut() = TelemetryState::new();
    });
}
