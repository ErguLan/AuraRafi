//! Small time-based motion primitives shared by RafUI hosts.
//!
//! Motion is state, not a renderer concern. Hosts provide elapsed seconds and
//! decide whether reduced motion is active; the same tween then drives GPU and
//! CPU presentation consistently.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum UiEasing {
    Linear,
    EaseOut,
    EaseInOut,
}

impl UiEasing {
    pub fn sample(self, progress: f32) -> f32 {
        let progress = progress.clamp(0.0, 1.0);
        match self {
            Self::Linear => progress,
            Self::EaseOut => 1.0 - (1.0 - progress).powi(3),
            Self::EaseInOut => {
                if progress < 0.5 {
                    4.0 * progress.powi(3)
                } else {
                    1.0 - (-2.0 * progress + 2.0).powi(3) * 0.5
                }
            }
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct UiMotionSpec {
    pub duration_seconds: f32,
    pub easing: UiEasing,
}

impl UiMotionSpec {
    pub const fn tooltip() -> Self {
        Self {
            duration_seconds: 0.12,
            easing: UiEasing::EaseOut,
        }
    }

    pub const fn dock() -> Self {
        Self {
            duration_seconds: 0.22,
            easing: UiEasing::EaseOut,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct UiTween {
    value: f32,
    start: f32,
    target: f32,
    elapsed_seconds: f32,
    spec: UiMotionSpec,
}

impl Default for UiTween {
    fn default() -> Self {
        Self::new(0.0, UiMotionSpec::tooltip())
    }
}

impl UiTween {
    pub const fn new(value: f32, spec: UiMotionSpec) -> Self {
        Self {
            value,
            start: value,
            target: value,
            elapsed_seconds: spec.duration_seconds,
            spec,
        }
    }

    pub fn value(&self) -> f32 {
        self.value
    }

    pub fn set_immediate(&mut self, value: f32) {
        self.value = value.clamp(0.0, 1.0);
        self.start = self.value;
        self.target = self.value;
        self.elapsed_seconds = self.spec.duration_seconds;
    }

    pub fn set_target(&mut self, target: f32) {
        let target = target.clamp(0.0, 1.0);
        if (target - self.target).abs() <= f32::EPSILON {
            return;
        }
        self.start = self.value;
        self.target = target;
        self.elapsed_seconds = 0.0;
    }

    pub fn advance(&mut self, delta_seconds: f32, reduced_motion: bool) -> f32 {
        if reduced_motion || self.spec.duration_seconds <= f32::EPSILON {
            self.set_immediate(self.target);
            return self.value;
        }
        self.elapsed_seconds =
            (self.elapsed_seconds + delta_seconds.max(0.0)).min(self.spec.duration_seconds);
        let progress = (self.elapsed_seconds / self.spec.duration_seconds).clamp(0.0, 1.0);
        let eased = self.spec.easing.sample(progress);
        self.value = self.start + (self.target - self.start) * eased;
        self.value
    }

    pub fn is_settled(&self) -> bool {
        (self.value - self.target).abs() <= f32::EPSILON
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tween_reaches_target_using_elapsed_time() {
        let mut tween = UiTween::new(0.0, UiMotionSpec::tooltip());
        tween.set_target(1.0);
        assert!(tween.advance(0.06, false) > 0.0);
        assert_eq!(tween.advance(0.10, false), 1.0);
        assert!(tween.is_settled());
    }

    #[test]
    fn reduced_motion_jumps_to_target() {
        let mut tween = UiTween::new(0.0, UiMotionSpec::tooltip());
        tween.set_target(1.0);
        assert_eq!(tween.advance(0.001, true), 1.0);
    }
}
