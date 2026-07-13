//! Stable editor frame telemetry.
//!
//! `egui::InputState::unstable_dt` is a prediction for widget animation, not a
//! trustworthy measurement of the editor loop. Keep the user-facing FPS meter
//! and the editor pacing decisions on a small monotonic clock instead.

use std::time::Instant;

#[derive(Debug)]
pub struct FrameTiming {
    last_tick: Option<Instant>,
    smoothed_frame_s: f32,
}

impl Default for FrameTiming {
    fn default() -> Self {
        Self {
            last_tick: None,
            smoothed_frame_s: 1.0 / 60.0,
        }
    }
}

impl FrameTiming {
    /// Records one editor frame and returns the smoothed elapsed duration.
    pub fn tick(&mut self) -> f32 {
        let now = Instant::now();
        let Some(previous) = self.last_tick.replace(now) else {
            return self.smoothed_frame_s;
        };

        // Ignore impossible sub-millisecond spikes and debugger pauses. The
        // latter must not make the visible meter claim a near-zero frame rate.
        let sample = now
            .saturating_duration_since(previous)
            .as_secs_f32()
            .clamp(1.0 / 1_000.0, 0.25);
        self.smoothed_frame_s += (sample - self.smoothed_frame_s) * 0.15;
        self.smoothed_frame_s
    }

    pub fn frame_time_s(&self) -> f32 {
        self.smoothed_frame_s
    }

    pub fn fps(&self) -> u32 {
        (1.0 / self.smoothed_frame_s.max(1.0 / 1_000.0))
            .round()
            .clamp(1.0, 1_000.0) as u32
    }
}

#[cfg(test)]
mod tests {
    use super::FrameTiming;

    #[test]
    fn default_telemetry_is_finite_before_the_second_tick() {
        let timing = FrameTiming::default();
        assert_eq!(timing.fps(), 60);
        assert!(timing.frame_time_s().is_finite());
    }
}
