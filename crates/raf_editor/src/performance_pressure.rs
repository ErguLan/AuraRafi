//! Presentation projection for the graphics memory ledger and upload pressure.
//!
//! [`SceneFrameMetrics`] already carries the ledger residency, peak, rejected
//! reservations and pressure ratios. This module turns that per-frame data into
//! a small, copyable readout that the editor status bar and the on-demand UI
//! cost inspector can display without touching layout or document revisions.
//!
//! Values are quantized to the precision the UI actually shows, so two frames
//! whose difference is invisible produce the same [`PerformanceReadout`]. The
//! host compares readouts and only rebuilds the compact status string when one
//! of the visible values changes.
//!
//! The on-demand cost inspector is composed by the workbench surface, whose
//! builder does not receive the live renderer metrics. The workbench host
//! publishes the latest bounded projection through [`publish`] when it changes
//! and the inspector reads it with [`last_published`] at document-build time.
//! This is a handoff of a `Copy` observation, not a second state model:
//! publishing never allocates, polls, or invalidates a surface.

use std::cell::Cell;

use raf_render::api_graphic_basic::SceneFrameMetrics;

/// One displayed tier of memory or upload budget utilization.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum PressureLevel {
    /// Below 70% of the budget.
    #[default]
    Nominal,
    /// At or above 70% and below 90% of the budget.
    Elevated,
    /// At or above 90% of the budget.
    Critical,
}

const ELEVATED_PRESSURE: f32 = 0.70;
const CRITICAL_PRESSURE: f32 = 0.90;
const BYTES_PER_MEGABYTE: f32 = 1024.0 * 1024.0;
const DISPLAY_STEPS_PER_MEGABYTE: f32 = 10.0;

/// Classifies a `0.0..=1.0` budget ratio into a display tier.
///
/// NaN is treated as [`PressureLevel::Nominal`] because an unknown ratio must
/// never present an alarm. Other values are clamped, so an infinite ratio
/// becomes critical instead of propagating through the surface.
pub fn classify(pressure: f32) -> PressureLevel {
    let clamped = pressure.clamp(0.0, 1.0);
    if clamped.is_nan() || clamped < ELEVATED_PRESSURE {
        PressureLevel::Nominal
    } else if clamped < CRITICAL_PRESSURE {
        PressureLevel::Elevated
    } else {
        PressureLevel::Critical
    }
}

/// Compact, display-ready projection of the graphics memory ledger.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct PerformanceReadout {
    pub resident_mb: f32,
    pub peak_mb: f32,
    pub budget_mb: f32,
    pub pressure: PressureLevel,
    pub rejected_reservations: u64,
    pub upload_rejected: u64,
    pub upload_pressure: PressureLevel,
}

/// Builds the readout from one frame of renderer metrics.
///
/// `budget_bytes` is the total graphics ledger budget reported by the active
/// render policy. A zero budget is retained as "unknown": the status segment
/// then omits the budget instead of dividing by zero.
pub fn from_metrics(metrics: &SceneFrameMetrics, budget_bytes: u64) -> PerformanceReadout {
    PerformanceReadout {
        resident_mb: megabytes(metrics.ledger_resident_bytes),
        peak_mb: megabytes(metrics.ledger_peak_bytes),
        budget_mb: megabytes(budget_bytes),
        pressure: classify(metrics.ledger_pressure),
        rejected_reservations: metrics.ledger_rejected_reservations,
        upload_rejected: metrics.frame_upload_rejected,
        upload_pressure: classify(metrics.frame_upload_pressure),
    }
}

/// Compact ASCII status segment, for example `MEM 123.4/256 MB`.
///
/// The upload suffix appears only after a real rejection. The caller caches the
/// returned string and rebuilds it only when a displayed source value changes.
pub fn status_segment(readout: &PerformanceReadout) -> String {
    let mut segment = if readout.budget_mb > 0.0 {
        format!("MEM {:.1}/{:.0} MB", readout.resident_mb, readout.budget_mb)
    } else {
        format!("MEM {:.1} MB", readout.resident_mb)
    };
    if readout.upload_rejected > 0 {
        use std::fmt::Write as _;
        let _ = write!(segment, " | UP {}", readout.upload_rejected);
    }
    segment
}

thread_local! {
    static LAST_READOUT: Cell<Option<PerformanceReadout>> = const { Cell::new(None) };
}

/// Publishes the latest bounded readout for the on-demand cost inspector.
///
/// The editor UI and its retained surfaces share one thread, so the
/// observation stays in a per-thread cell. Publishing a `Copy` value cannot
/// allocate or trigger presentation work.
pub fn publish(readout: PerformanceReadout) {
    LAST_READOUT.with(|slot| slot.set(Some(readout)));
}

/// Returns the readout published by the active workbench, if any.
pub fn last_published() -> Option<PerformanceReadout> {
    LAST_READOUT.with(Cell::get)
}

/// Converts bytes to megabytes, rounded to the one decimal the UI displays.
fn megabytes(bytes: u64) -> f32 {
    let megabytes = bytes as f32 / BYTES_PER_MEGABYTE;
    let steps = (megabytes * DISPLAY_STEPS_PER_MEGABYTE).round();
    if steps.is_finite() {
        steps / DISPLAY_STEPS_PER_MEGABYTE
    } else {
        megabytes
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const MIB: u64 = 1024 * 1024;

    fn metrics(
        resident: u64,
        peak: u64,
        pressure: f32,
        upload_rejected: u64,
        upload_pressure: f32,
    ) -> SceneFrameMetrics {
        SceneFrameMetrics {
            ledger_resident_bytes: resident,
            ledger_peak_bytes: peak,
            ledger_rejected_reservations: 3,
            ledger_pressure: pressure,
            frame_upload_rejected: upload_rejected,
            frame_upload_pressure: upload_pressure,
            ..SceneFrameMetrics::default()
        }
    }

    #[test]
    fn pressure_thresholds_are_half_open_at_each_boundary() {
        assert_eq!(classify(0.0), PressureLevel::Nominal);
        assert_eq!(classify(0.699), PressureLevel::Nominal);
        assert_eq!(classify(0.70), PressureLevel::Elevated);
        assert_eq!(classify(0.899), PressureLevel::Elevated);
        assert_eq!(classify(0.90), PressureLevel::Critical);
        assert_eq!(classify(1.0), PressureLevel::Critical);
    }

    #[test]
    fn unknown_and_out_of_range_pressures_clamp_without_alarms() {
        assert_eq!(classify(f32::NAN), PressureLevel::Nominal);
        assert_eq!(classify(-1.0), PressureLevel::Nominal);
        assert_eq!(classify(2.0), PressureLevel::Critical);
        assert_eq!(classify(f32::INFINITY), PressureLevel::Critical);
        assert_eq!(classify(f32::NEG_INFINITY), PressureLevel::Nominal);
    }

    #[test]
    fn readout_uses_one_decimal_and_display_precision() {
        let readout = from_metrics(&metrics(12 * MIB, 20 * MIB, 0.75, 2, 0.95), 256 * MIB);

        assert_eq!(readout.resident_mb, 12.0);
        assert_eq!(readout.peak_mb, 20.0);
        assert_eq!(readout.budget_mb, 256.0);
        assert_eq!(readout.pressure, PressureLevel::Elevated);
        assert_eq!(readout.rejected_reservations, 3);
        assert_eq!(readout.upload_rejected, 2);
        assert_eq!(readout.upload_pressure, PressureLevel::Critical);
        assert_eq!(status_segment(&readout), "MEM 12.0/256 MB | UP 2");
    }

    #[test]
    fn sub_display_changes_keep_the_cached_readout_equal() {
        let base = metrics(12 * MIB, 20 * MIB, 0.70, 0, 0.10);
        let first = from_metrics(&base, 256 * MIB);

        let mut jittered = base;
        jittered.ledger_resident_bytes = jittered.ledger_resident_bytes.saturating_add(512);
        let second = from_metrics(&jittered, 256 * MIB);

        assert_eq!(
            first, second,
            "a sub-visible byte change must not invalidate the status text"
        );
    }

    #[test]
    fn zero_budget_omits_the_ratio_instead_of_dividing() {
        let readout = from_metrics(&metrics(MIB, MIB, 0.0, 0, f32::NAN), 0);

        assert_eq!(readout.budget_mb, 0.0);
        assert_eq!(status_segment(&readout), "MEM 1.0 MB");
    }

    #[test]
    fn saturated_counters_stay_finite_and_keep_the_upload_warning() {
        let readout = from_metrics(
            &metrics(u64::MAX, u64::MAX, f32::INFINITY, u64::MAX, f32::NAN),
            u64::MAX,
        );

        assert!(readout.resident_mb.is_finite());
        assert!(readout.peak_mb.is_finite());
        assert!(readout.budget_mb.is_finite());
        assert_eq!(readout.pressure, PressureLevel::Critical);
        assert_eq!(readout.upload_pressure, PressureLevel::Nominal);
        let segment = status_segment(&readout);
        assert!(segment.starts_with("MEM "));
        assert!(segment.ends_with(&format!("| UP {}", u64::MAX)));
    }

    #[test]
    fn upload_suffix_appears_only_after_a_rejection() {
        let without = from_metrics(&metrics(MIB, MIB, 0.5, 0, 0.5), 256 * MIB);
        let with = from_metrics(&metrics(MIB, MIB, 0.5, 1, 0.5), 256 * MIB);

        assert_eq!(status_segment(&without), "MEM 1.0/256 MB");
        assert_eq!(status_segment(&with), "MEM 1.0/256 MB | UP 1");
    }

    #[test]
    fn published_readout_is_available_on_the_same_thread() {
        let readout = from_metrics(&metrics(MIB, MIB, 0.0, 0, 0.0), 256 * MIB);
        publish(readout);

        assert_eq!(last_published(), Some(readout));
    }
}
