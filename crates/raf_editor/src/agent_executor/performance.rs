//! Native Agent tools for performance telemetry and benchmarks.
//!
//! They read the same thread-local ring buffer that feeds the attached
//! CLI/MCP surface, so every entry point reports identical numbers.

use raf_ai::agent_runtime::AgentToolResult;
use raf_core::performance::{
    BenchmarkPhase, BenchmarkProject, BenchmarkStore, PerformanceEnvironment,
    DEFAULT_WINDOW_SECONDS,
};
use raf_render::api_graphic_basic::FrameInvalidation;
use serde_json::{json, Value};

use super::AgentToolExecutor;

impl AgentToolExecutor<'_> {
    pub(super) fn execute_performance(
        &mut self,
        name: &str,
        args: &Value,
    ) -> Result<AgentToolResult, String> {
        match name {
            "performance_stats" => {
                let seconds = args
                    .get("window_seconds")
                    .and_then(Value::as_f64)
                    .unwrap_or(DEFAULT_WINDOW_SECONDS as f64) as f32;
                let environment = self.performance_environment()?;
                let window = crate::performance_telemetry::window(seconds, &environment);
                let mut result = AgentToolResult::success(
                    format!(
                        "{:.1} FPS avg (p95 frame {:.2} ms) | CPU {:.2} ms | GPU {:.2} ms | {}",
                        window.fps.avg,
                        window.frame_ms.p95,
                        window.cpu_ms.avg,
                        window.gpu_ms.avg,
                        window.diagnosis.verdict
                    ),
                    json!({"window": window}),
                );
                result.revision = Some(self.ledger.revision());
                Ok(result)
            }
            "performance_benchmark_start" => {
                let Some(project) = self.project_info else {
                    return Err("Benchmarks require an open project.".to_string());
                };
                let measured_seconds = args
                    .get("seconds")
                    .and_then(Value::as_f64)
                    .unwrap_or(10.0)
                    .clamp(1.0, 600.0) as f32;
                let warmup_seconds = args
                    .get("warmup_seconds")
                    .and_then(Value::as_f64)
                    .unwrap_or(2.0)
                    .clamp(0.0, 60.0) as f32;
                let label = args
                    .get("label")
                    .and_then(Value::as_str)
                    .map(str::trim)
                    .filter(|label| !label.is_empty())
                    .unwrap_or("benchmark");
                let environment = self.performance_environment()?;
                let identity = BenchmarkProject {
                    id: project.id.to_string(),
                    path: project.path.display().to_string(),
                    engine_version: project.engine_version.clone(),
                };
                let id = crate::performance_telemetry::start_benchmark(
                    label,
                    warmup_seconds,
                    measured_seconds,
                    identity,
                    environment.clone(),
                )?;
                self.set_performance_sampling(true);
                let mut result = AgentToolResult::success(
                    format!(
                        "Benchmark {id}: {warmup_seconds:.1}s warmup + {measured_seconds:.1}s \
                         measured. Keep the viewport active, then query \
                         performance_benchmark_status."
                    ),
                    json!({
                        "id": id,
                        "phase": "warmup",
                        "warmup_seconds": warmup_seconds,
                        "measured_seconds": measured_seconds,
                        "environment": environment,
                    }),
                );
                result.revision = Some(self.ledger.revision());
                Ok(result)
            }
            "performance_benchmark_status" => {
                let requested = args.get("id").and_then(Value::as_str);
                match crate::performance_telemetry::benchmark_progress() {
                    Some(progress) => {
                        if let Some(id) = requested {
                            if !id.eq_ignore_ascii_case(&progress.id) {
                                return Err(format!(
                                    "Benchmark '{id}' is not active; '{}' is the current run.",
                                    progress.id
                                ));
                            }
                        }
                        if progress.phase == BenchmarkPhase::Completed {
                            return self.finalize_performance_benchmark();
                        }
                        let mut result = AgentToolResult::success(
                            format!(
                                "{}: {} — {:.1}s measured, {:.1}s remaining, {:.1} FPS avg.",
                                progress.id,
                                progress.phase.as_str(),
                                progress.measured_seconds,
                                progress.remaining_seconds,
                                progress.fps_avg
                            ),
                            json!({"progress": progress}),
                        );
                        result.revision = Some(self.ledger.revision());
                        Ok(result)
                    }
                    None => {
                        if let (Some(id), Some(project)) = (requested, self.project_info) {
                            let store = BenchmarkStore::for_project(&project.path);
                            if let Ok(report) = store.load(id) {
                                return Ok(self.saved_performance_report(report));
                            }
                        }
                        Err(
                            "No benchmark is active. Start one with performance_benchmark_start."
                                .to_string(),
                        )
                    }
                }
            }
            "performance_benchmark_stop" => self.finalize_performance_benchmark(),
            "performance_benchmark_cancel" => {
                self.set_performance_sampling(false);
                match crate::performance_telemetry::cancel_benchmark() {
                    Some(id) => {
                        let mut result = AgentToolResult::success(
                            format!("Cancelled benchmark {id}."),
                            json!({"cancelled": id}),
                        );
                        result.revision = Some(self.ledger.revision());
                        Ok(result)
                    }
                    None => Err("No benchmark is active.".to_string()),
                }
            }
            "performance_benchmark_list" => {
                let Some(project) = self.project_info else {
                    return Err("Benchmarks require an open project.".to_string());
                };
                let reports = BenchmarkStore::for_project(&project.path).list()?;
                let mut result = AgentToolResult::success(
                    format!("{} saved benchmark(s).", reports.len()),
                    json!({"count": reports.len(), "benchmarks": reports}),
                );
                result.revision = Some(self.ledger.revision());
                Ok(result)
            }
            "performance_benchmark_compare" => {
                let (Some(left), Some(right)) = (
                    args.get("a").and_then(Value::as_str),
                    args.get("b").and_then(Value::as_str),
                ) else {
                    return Err(
                        "performance_benchmark_compare requires 'a' and 'b' benchmark ids."
                            .to_string(),
                    );
                };
                let Some(project) = self.project_info else {
                    return Err("Benchmarks require an open project.".to_string());
                };
                let comparison =
                    BenchmarkStore::for_project(&project.path).compare(left, right)?;
                let mut result = AgentToolResult::success(
                    format!(
                        "{} → {}: {} | FPS {:+.1}% | p95 frame {:+.1}%",
                        comparison.left.id,
                        comparison.right.id,
                        comparison.verdict,
                        comparison.fps.delta_percent,
                        comparison.frame_p95.delta_percent
                    ),
                    json!({"comparison": comparison}),
                );
                result.revision = Some(self.ledger.revision());
                Ok(result)
            }
            other => Err(format!("Unknown performance tool '{other}'.")),
        }
    }

    fn finalize_performance_benchmark(&mut self) -> Result<AgentToolResult, String> {
        let report = crate::performance_telemetry::finish_benchmark()?;
        self.set_performance_sampling(false);
        let Some(project) = self.project_info else {
            return Err("Benchmarks require an open project.".to_string());
        };
        let path = BenchmarkStore::for_project(&project.path).save(&report)?;
        let mut result = AgentToolResult::success(
            format!(
                "{:.1} FPS avg | p95 frame {:.2} ms | CPU {:.2} ms | GPU {:.2} ms | {} (saved {})",
                report.window.fps.avg,
                report.window.frame_ms.p95,
                report.window.cpu_ms.avg,
                report.window.gpu_ms.avg,
                report.window.diagnosis.verdict,
                path.display()
            ),
            json!({
                "report": report,
                "saved_path": path.display().to_string(),
            }),
        );
        result.revision = Some(self.ledger.revision());
        Ok(result)
    }

    fn saved_performance_report(
        &self,
        report: raf_core::performance::BenchmarkReport,
    ) -> AgentToolResult {
        let mut result = AgentToolResult::success(
            format!(
                "{}: {:.1} FPS avg | p95 frame {:.2} ms | {}",
                report.id,
                report.window.fps.avg,
                report.window.frame_ms.p95,
                report.window.diagnosis.verdict
            ),
            json!({"report": report}),
        );
        result.revision = Some(self.ledger.revision());
        result
    }

    fn set_performance_sampling(&mut self, active: bool) {
        if let Some(graphics) = self.graphics.as_deref_mut() {
            let reasons =
                FrameInvalidation::DOCUMENT | FrameInvalidation::WINDOW | FrameInvalidation::UI;
            graphics.set_continuous_frame_reason(reasons, active);
            if active {
                graphics.request_frame(reasons);
            }
        }
    }

    fn performance_environment(&self) -> Result<PerformanceEnvironment, String> {
        let Some(graphics) = self.graphics.as_deref() else {
            return Err(
                "Performance telemetry requires a live editor viewport.".to_string(),
            );
        };
        let snapshot = graphics.snapshot();
        let scheduler = snapshot.scheduler_metrics;
        let metrics = snapshot.last_frame_metrics;
        let backend = snapshot
            .capabilities
            .map(|capabilities| format!("{:?}", capabilities.backend))
            .unwrap_or_else(|| snapshot.status_badge());
        let gpu_hardware = snapshot
            .capabilities
            .is_some_and(|capabilities| capabilities.gpu_hardware);
        let gpu_timing_supported = snapshot
            .capabilities
            .is_some_and(|capabilities| capabilities.timestamp_queries)
            || metrics.gpu_timing_supported;
        let [width, height] = crate::performance_telemetry::last_target_size();
        Ok(PerformanceEnvironment {
            backend,
            gpu_hardware,
            gpu_timing_supported,
            target_fps: scheduler.target_fps,
            requested_target_fps: scheduler.requested_target_fps,
            width,
            height,
            revision: self.ledger.revision(),
            scene_fingerprint: crate::scene_history::scene_fingerprint(self.scene),
        })
    }
}
