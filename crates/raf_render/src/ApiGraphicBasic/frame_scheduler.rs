//! Event-driven frame pacing shared by Scene, CAD, and RafUI composition.
//!
//! Rendering throughput and presentation rate are separate concerns. The
//! scheduler permits an uncapped benchmark lane while normal editor profiles
//! sleep when no visible state changed.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum FramePacingProfile {
    Eco,
    Balanced,
    Performance,
    Benchmark,
}

impl Default for FramePacingProfile {
    fn default() -> Self {
        Self::Eco
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct FramePacingBudget {
    pub foreground_fps: u16,
    pub background_fps: u16,
    pub passive_fps: u16,
    pub min_resolution_scale: f32,
    pub max_resolution_scale: f32,
    pub event_driven_idle: bool,
}

impl FramePacingBudget {
    pub const fn eco() -> Self {
        Self {
            foreground_fps: 60,
            background_fps: 5,
            passive_fps: 30,
            min_resolution_scale: 0.60,
            max_resolution_scale: 1.0,
            event_driven_idle: true,
        }
    }

    pub const fn balanced() -> Self {
        Self {
            foreground_fps: 120,
            background_fps: 10,
            passive_fps: 60,
            min_resolution_scale: 0.75,
            max_resolution_scale: 1.0,
            event_driven_idle: true,
        }
    }

    pub const fn performance() -> Self {
        Self {
            foreground_fps: 240,
            background_fps: 15,
            passive_fps: 120,
            min_resolution_scale: 0.85,
            max_resolution_scale: 1.0,
            event_driven_idle: true,
        }
    }

    pub const fn benchmark() -> Self {
        Self {
            foreground_fps: 0,
            background_fps: 0,
            passive_fps: 0,
            min_resolution_scale: 1.0,
            max_resolution_scale: 1.0,
            event_driven_idle: false,
        }
    }

    pub const fn for_profile(profile: FramePacingProfile) -> Self {
        match profile {
            FramePacingProfile::Eco => Self::eco(),
            FramePacingProfile::Balanced => Self::balanced(),
            FramePacingProfile::Performance => Self::performance(),
            FramePacingProfile::Benchmark => Self::benchmark(),
        }
    }
}

impl Default for FramePacingBudget {
    fn default() -> Self {
        Self::eco()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct FrameInvalidation(u32);

impl FrameInvalidation {
    pub const NONE: Self = Self(0);
    pub const WINDOW: Self = Self(1 << 0);
    pub const DOCUMENT: Self = Self(1 << 1);
    pub const CAMERA: Self = Self(1 << 2);
    pub const POINTER_CAPTURE: Self = Self(1 << 3);
    pub const ANIMATION: Self = Self(1 << 4);
    pub const SIMULATION: Self = Self(1 << 5);
    pub const ASSET_UPLOAD: Self = Self(1 << 6);
    pub const UI: Self = Self(1 << 7);
    pub const OVERLAY: Self = Self(1 << 8);
    pub const EXPLICIT: Self = Self(1 << 9);

    pub const fn contains(self, other: Self) -> bool {
        self.0 & other.0 == other.0
    }

    pub const fn intersects(self, other: Self) -> bool {
        self.0 & other.0 != 0
    }

    pub const fn is_empty(self) -> bool {
        self.0 == 0
    }

    pub fn insert(&mut self, other: Self) {
        self.0 |= other.0;
    }

    pub fn remove(&mut self, other: Self) {
        self.0 &= !other.0;
    }
}

impl std::ops::BitOr for FrameInvalidation {
    type Output = Self;

    fn bitor(self, rhs: Self) -> Self::Output {
        Self(self.0 | rhs.0)
    }
}

impl std::ops::BitOrAssign for FrameInvalidation {
    fn bitor_assign(&mut self, rhs: Self) {
        self.0 |= rhs.0;
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum FrameActivity {
    Idle,
    Passive,
    Interactive,
    Benchmark,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct FramePermit {
    pub frame_index: u64,
    pub reasons: FrameInvalidation,
    pub activity: FrameActivity,
    pub requested_at_seconds: f64,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, Default)]
pub struct FrameSchedulerMetrics {
    pub frames_permitted: u64,
    pub idle_frames_skipped: u64,
    pub paced_frames_deferred: u64,
    pub last_frame_cpu_ms: f32,
    pub last_frame_gpu_ms: f32,
    pub last_present_seconds: f64,
    /// Smoothed presentation rate measured from completed frames.
    ///
    /// This is deliberately separate from the configured frame budget. The
    /// budget is a ceiling; this value describes what the editor actually
    /// presented while it was active.
    pub presented_fps: f32,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct DynamicResolutionController {
    scale: f32,
    smoothed_frame_ms: f32,
    under_budget_frames: u16,
}

impl Default for DynamicResolutionController {
    fn default() -> Self {
        Self {
            scale: 1.0,
            smoothed_frame_ms: 0.0,
            under_budget_frames: 0,
        }
    }
}

impl DynamicResolutionController {
    pub fn scale(&self) -> f32 {
        self.scale
    }

    pub fn reset(&mut self, budget: FramePacingBudget) {
        self.scale = budget.max_resolution_scale;
        self.smoothed_frame_ms = 0.0;
        self.under_budget_frames = 0;
    }

    pub fn update(
        &mut self,
        budget: FramePacingBudget,
        activity: FrameActivity,
        cpu_ms: f32,
        gpu_ms: f32,
    ) -> f32 {
        if budget.foreground_fps == 0 {
            self.scale = budget.max_resolution_scale;
            return self.scale;
        }
        let measured = cpu_ms.max(gpu_ms).max(0.0);
        if measured <= f32::EPSILON {
            return self
                .scale
                .clamp(budget.min_resolution_scale, budget.max_resolution_scale);
        }
        self.smoothed_frame_ms = if self.smoothed_frame_ms <= f32::EPSILON {
            measured
        } else {
            self.smoothed_frame_ms * 0.86 + measured * 0.14
        };
        let fps = match activity {
            FrameActivity::Passive => budget.passive_fps,
            FrameActivity::Idle | FrameActivity::Interactive => budget.foreground_fps,
            FrameActivity::Benchmark => 0,
        };
        if fps == 0 {
            self.scale = budget.max_resolution_scale;
            return self.scale;
        }
        let target_ms = 1000.0 / f32::from(fps);
        if self.smoothed_frame_ms > target_ms * 1.08 {
            let pressure = (target_ms / self.smoothed_frame_ms)
                .sqrt()
                .clamp(0.82, 0.96);
            self.scale = (self.scale * pressure).max(budget.min_resolution_scale);
            self.under_budget_frames = 0;
        } else if self.smoothed_frame_ms < target_ms * 0.70 {
            self.under_budget_frames = self.under_budget_frames.saturating_add(1);
            let recovery_delay = if activity == FrameActivity::Interactive {
                45
            } else {
                24
            };
            if self.under_budget_frames >= recovery_delay {
                self.scale = (self.scale + 0.025).min(budget.max_resolution_scale);
                self.under_budget_frames = 0;
            }
        } else {
            self.under_budget_frames = 0;
        }
        self.scale = self
            .scale
            .clamp(budget.min_resolution_scale, budget.max_resolution_scale);
        self.scale
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FrameScheduler {
    profile: FramePacingProfile,
    budget: FramePacingBudget,
    /// Optional user ceiling from EngineSettings. `Some(0)` means uncapped
    /// while the window is focused; `None` keeps the profile default.
    #[serde(default)]
    frame_limit: Option<u16>,
    pending: FrameInvalidation,
    continuous: FrameInvalidation,
    window_focused: bool,
    suspended: bool,
    next_frame_index: u64,
    next_present_seconds: f64,
    metrics: FrameSchedulerMetrics,
}

impl Default for FrameScheduler {
    fn default() -> Self {
        Self::new(FramePacingProfile::Eco)
    }
}

impl FrameScheduler {
    pub fn new(profile: FramePacingProfile) -> Self {
        Self {
            profile,
            budget: FramePacingBudget::for_profile(profile),
            frame_limit: None,
            pending: FrameInvalidation::WINDOW | FrameInvalidation::EXPLICIT,
            continuous: FrameInvalidation::NONE,
            window_focused: true,
            suspended: false,
            next_frame_index: 1,
            next_present_seconds: 0.0,
            metrics: FrameSchedulerMetrics::default(),
        }
    }

    pub fn profile(&self) -> FramePacingProfile {
        self.profile
    }

    pub fn budget(&self) -> FramePacingBudget {
        self.budget
    }

    pub fn frame_limit(&self) -> Option<u16> {
        self.frame_limit
    }

    pub fn set_frame_limit(&mut self, fps_limit: u32) {
        let next = if fps_limit == 0 {
            Some(0)
        } else {
            Some(fps_limit.clamp(15, 240) as u16)
        };
        if self.frame_limit == next {
            return;
        }
        self.frame_limit = next;
        self.next_present_seconds = 0.0;
        self.request(FrameInvalidation::EXPLICIT);
    }

    pub fn metrics(&self) -> FrameSchedulerMetrics {
        self.metrics
    }

    pub fn pending(&self) -> FrameInvalidation {
        self.pending | self.continuous
    }

    pub fn set_profile(&mut self, profile: FramePacingProfile) {
        if self.profile == profile {
            return;
        }
        self.profile = profile;
        self.budget = FramePacingBudget::for_profile(profile);
        self.next_present_seconds = 0.0;
        self.request(FrameInvalidation::EXPLICIT);
    }

    pub fn set_window_focused(&mut self, focused: bool) {
        if self.window_focused != focused {
            self.window_focused = focused;
            self.request(FrameInvalidation::WINDOW);
        }
    }

    pub fn set_suspended(&mut self, suspended: bool) {
        if self.suspended == suspended {
            return;
        }
        self.suspended = suspended;
        if !suspended {
            self.next_present_seconds = 0.0;
            self.request(FrameInvalidation::WINDOW);
        }
    }

    pub fn request(&mut self, reason: FrameInvalidation) {
        self.pending.insert(reason);
    }

    pub fn set_continuous(&mut self, reason: FrameInvalidation, active: bool) {
        if active {
            self.continuous.insert(reason);
        } else {
            self.continuous.remove(reason);
        }
    }

    pub fn request_frame(&mut self, now_seconds: f64) -> Option<FramePermit> {
        if self.suspended {
            self.metrics.idle_frames_skipped = self.metrics.idle_frames_skipped.saturating_add(1);
            return None;
        }

        let reasons = self.pending | self.continuous;
        let benchmark = self.profile == FramePacingProfile::Benchmark;
        if reasons.is_empty() && self.budget.event_driven_idle && !benchmark {
            self.metrics.idle_frames_skipped = self.metrics.idle_frames_skipped.saturating_add(1);
            return None;
        }

        let activity = if benchmark {
            FrameActivity::Benchmark
        } else if reasons.intersects(FrameInvalidation::CAMERA | FrameInvalidation::POINTER_CAPTURE)
        {
            FrameActivity::Interactive
        } else if !self.continuous.is_empty() {
            FrameActivity::Passive
        } else {
            FrameActivity::Idle
        };
        let fps = self.target_fps(activity);
        if fps > 0 && now_seconds + f64::EPSILON < self.next_present_seconds {
            self.metrics.paced_frames_deferred =
                self.metrics.paced_frames_deferred.saturating_add(1);
            return None;
        }

        let permit = FramePermit {
            frame_index: self.next_frame_index,
            reasons,
            activity,
            requested_at_seconds: now_seconds.max(0.0),
        };
        self.next_frame_index = self.next_frame_index.wrapping_add(1).max(1);
        self.pending = FrameInvalidation::NONE;
        self.metrics.frames_permitted = self.metrics.frames_permitted.saturating_add(1);
        if fps > 0 {
            self.next_present_seconds = permit.requested_at_seconds + 1.0 / f64::from(fps);
        } else {
            self.next_present_seconds = permit.requested_at_seconds;
        }
        Some(permit)
    }

    pub fn finish_frame(
        &mut self,
        permit: FramePermit,
        presented_at_seconds: f64,
        cpu_ms: f32,
        gpu_ms: f32,
    ) {
        let presented_at_seconds = presented_at_seconds.max(permit.requested_at_seconds);
        let previous_present_seconds = self.metrics.last_present_seconds;
        let interval = presented_at_seconds - previous_present_seconds;

        // Event-driven idle time is not a rendered frame interval. Ignore
        // long gaps so opening a panel after being idle does not report a
        // bogus one-frame FPS collapse.
        if previous_present_seconds > 0.0
            && interval.is_finite()
            && interval > 0.0001
            && interval <= 0.5
        {
            let sample_fps = (1.0 / interval).clamp(0.0, 1000.0) as f32;
            self.metrics.presented_fps = if self.metrics.presented_fps <= f32::EPSILON {
                sample_fps
            } else {
                self.metrics.presented_fps * 0.85 + sample_fps * 0.15
            };
        }
        self.metrics.last_present_seconds = presented_at_seconds;
        self.metrics.last_frame_cpu_ms = cpu_ms.max(0.0);
        self.metrics.last_frame_gpu_ms = gpu_ms.max(0.0);
    }

    pub fn seconds_until_next_frame(&self, now_seconds: f64) -> Option<f64> {
        if self.suspended {
            return None;
        }
        let reasons = self.pending | self.continuous;
        if reasons.is_empty()
            && self.budget.event_driven_idle
            && self.profile != FramePacingProfile::Benchmark
        {
            return None;
        }
        Some((self.next_present_seconds - now_seconds).max(0.0))
    }

    fn target_fps(&self, activity: FrameActivity) -> u16 {
        if activity == FrameActivity::Benchmark {
            return 0;
        }
        if !self.window_focused {
            let background_limit = self
                .frame_limit
                .filter(|limit| *limit > 0)
                .unwrap_or(u16::MAX);
            return self.budget.background_fps.min(background_limit);
        }
        let profile_fps = match activity {
            FrameActivity::Idle | FrameActivity::Interactive => self.budget.foreground_fps,
            FrameActivity::Passive => self.budget.passive_fps,
            FrameActivity::Benchmark => 0,
        };
        match self.frame_limit {
            Some(0) => 0,
            Some(limit) if profile_fps == 0 => limit,
            Some(limit) => profile_fps.min(limit),
            None => profile_fps,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn permit(requested_at_seconds: f64) -> FramePermit {
        FramePermit {
            frame_index: 1,
            reasons: FrameInvalidation::EXPLICIT,
            activity: FrameActivity::Interactive,
            requested_at_seconds,
        }
    }

    #[test]
    fn measures_completed_presentation_rate() {
        let mut scheduler = FrameScheduler::default();
        scheduler.finish_frame(permit(0.016), 0.016, 0.0, 0.0);
        scheduler.finish_frame(permit(0.032), 0.032, 0.0, 0.0);

        assert!((scheduler.metrics().presented_fps - 62.5).abs() < 0.01);
    }

    #[test]
    fn ignores_event_driven_idle_gap() {
        let mut scheduler = FrameScheduler::default();
        scheduler.finish_frame(permit(0.016), 0.016, 0.0, 0.0);
        scheduler.finish_frame(permit(2.016), 2.016, 0.0, 0.0);

        assert_eq!(scheduler.metrics().presented_fps, 0.0);
    }
}
