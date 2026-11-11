//! Logging stays below ApiGraphicBasic; the data contract has no logger.
use raf_ui::{UiPerformanceMonitor, UiWorkCause, UiWorkPhase};
use std::time::Duration;

pub(super) fn observe(
    monitor: &mut UiPerformanceMonitor,
    region: &str,
    phase: UiWorkPhase,
    cause: UiWorkCause,
    elapsed: Duration,
    nodes: usize,
    bytes: u64,
    reused: bool,
) {
    if monitor.record(region, phase, cause, elapsed, nodes, bytes, reused) {
        tracing::warn!(
            region,
            ?phase,
            ?cause,
            cpu_us = elapsed.as_micros() as u64,
            nodes,
            bytes,
            reused,
            "Retained UI cost exceeded its advisory budget"
        );
    }
}
