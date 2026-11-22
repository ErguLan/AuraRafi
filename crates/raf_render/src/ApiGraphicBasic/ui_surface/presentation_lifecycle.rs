//! Bounded recovery state machine for native surface presentation.
//!
//! This module lives inside the private ApiGraphicBasic presentation boundary.
//! It classifies surface faults, paces retries, and reports health without
//! touching a GPU, so the policy can be exercised by plain unit tests while
//! the native window host remains the only production caller.
//!
//! Recovery is intentionally small and observable: a transient fault is
//! retried with bounded exponential backoff, a fatal fault stops immediately,
//! and a successful present clears the recovery state without discarding the
//! application state that owns the window.

use std::time::Duration;

use crate::engine_time::Instant;

/// Failure class observed while acquiring or presenting a native surface.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PresentationFault {
    /// The swapchain was lost and must be recreated from its configuration.
    Lost,
    /// The underlying surface changed and the swapchain must be reconfigured.
    Outdated,
    /// Frame acquisition timed out before an image became available.
    Timeout,
    /// The surface could not allocate another presentation image.
    OutOfMemory,
    /// Any other presentation failure reported by the backend.
    Other,
}

impl PresentationFault {
    /// True when retrying frame acquisition cannot repair the fault.
    ///
    /// Out-of-memory is terminal for the current surface. Transient faults
    /// become terminal only after [`PresentationPolicy::max_attempts`] is
    /// exhausted.
    pub const fn is_fatal(self) -> bool {
        matches!(self, Self::OutOfMemory)
    }

    /// Stable short label for diagnostics and structured logs.
    pub const fn label(self) -> &'static str {
        match self {
            Self::Lost => "lost",
            Self::Outdated => "outdated",
            Self::Timeout => "timeout",
            Self::OutOfMemory => "out-of-memory",
            Self::Other => "other",
        }
    }
}

/// Current presentation health as seen by an application host.
#[derive(Debug, Clone, Copy)]
pub enum PresentationHealth {
    /// The last presentation succeeded and no recovery is pending.
    Healthy,
    /// A transient fault is being retried.
    Recovering { attempt: u8, next_retry: Instant },
    /// Recovery is exhausted; the surface cannot present another frame.
    Failed { fault: PresentationFault },
}

impl PartialEq for PresentationHealth {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::Healthy, Self::Healthy) => true,
            (
                Self::Recovering {
                    attempt: left_attempt,
                    next_retry: left_retry,
                },
                Self::Recovering {
                    attempt: right_attempt,
                    next_retry: right_retry,
                },
            ) => left_attempt == right_attempt && retry_deadline_eq(*left_retry, *right_retry),
            (Self::Failed { fault: left }, Self::Failed { fault: right }) => left == right,
            _ => false,
        }
    }
}

impl Eq for PresentationHealth {}

#[cfg(not(target_arch = "wasm32"))]
fn retry_deadline_eq(left: Instant, right: Instant) -> bool {
    left == right
}

// The shared web clock has no Instant equality primitive; health remains
// comparable there by variant, attempt, and fault while native builds compare
// the exact retry deadline.
#[cfg(target_arch = "wasm32")]
fn retry_deadline_eq(_left: Instant, _right: Instant) -> bool {
    true
}

/// Retry budget for transient surface faults.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PresentationPolicy {
    /// Number of transient faults that still receive a retry.
    pub max_attempts: u8,
    /// Delay before the first retry.
    pub base_backoff_ms: u64,
    /// Upper bound for any retry delay.
    pub max_backoff_ms: u64,
    /// Healthy time after a fault before a success resets the attempt count.
    pub reset_after_ms: u64,
}

impl Default for PresentationPolicy {
    fn default() -> Self {
        Self {
            max_attempts: 3,
            base_backoff_ms: 80,
            max_backoff_ms: 900,
            reset_after_ms: 10_000,
        }
    }
}

impl PresentationPolicy {
    fn reset_after(&self) -> Duration {
        Duration::from_millis(self.reset_after_ms)
    }

    /// Exponential backoff for a 1-based attempt number: base, base * 3,
    /// base * 9, and so on, capped by `max_backoff_ms`.
    fn backoff(&self, attempt: u8) -> Duration {
        const GROWTH: u64 = 3;
        let exponent = u32::from(attempt.saturating_sub(1));
        let factor = GROWTH.saturating_pow(exponent);
        let millis = self
            .base_backoff_ms
            .saturating_mul(factor)
            .min(self.max_backoff_ms);
        Duration::from_millis(millis)
    }
}

/// Action a host should take after recording a fault.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PresentationDecision {
    /// No wait is required before the next acquisition attempt.
    RetryNow,
    /// Wait for the duration before the next acquisition attempt.
    RetryAfter(Duration),
    /// Recovery is exhausted; surface presentation is no longer possible.
    Fatal,
}

/// Consecutive-fault tracker shared by the native surface host.
///
/// The type owns no GPU resources and performs no allocation on the healthy
/// path. [`Self::record_success`] is called for every presented frame, so the
/// attempt count only survives while the surface is still unstable.
#[derive(Debug, Clone)]
pub struct PresentationRecovery {
    policy: PresentationPolicy,
    fault: Option<PresentationFault>,
    attempts: u8,
    last_fault_at: Option<Instant>,
    last_success_at: Option<Instant>,
    recovering: bool,
}

impl Default for PresentationRecovery {
    fn default() -> Self {
        Self::new(PresentationPolicy::default())
    }
}

impl PresentationRecovery {
    pub fn new(policy: PresentationPolicy) -> Self {
        Self {
            policy,
            fault: None,
            attempts: 0,
            last_fault_at: None,
            last_success_at: None,
            recovering: false,
        }
    }

    pub fn policy(&self) -> PresentationPolicy {
        self.policy
    }

    /// Records a successfully presented frame.
    ///
    /// The recovery state clears immediately. The attempt count is only reset
    /// once the surface has stayed healthy for `reset_after_ms` since the last
    /// fault, so rapid fault/success flapping still escalates the backoff.
    pub fn record_success(&mut self, now: Instant) {
        let stable = self
            .last_fault_at
            .map_or(true, |fault_at| elapsed_since(now, fault_at) >= self.policy.reset_after());
        if stable {
            self.attempts = 0;
        }
        self.recovering = false;
        self.last_success_at = Some(now);
    }

    /// Records a failed acquisition and returns the retry decision.
    ///
    /// A fault that arrives after `reset_after_ms` of quiet time starts a new
    /// burst. A non-fatal fault becomes fatal once `max_attempts` retries have
    /// been consumed. A zero base backoff reports `RetryNow`.
    pub fn record_fault(
        &mut self,
        fault: PresentationFault,
        now: Instant,
    ) -> PresentationDecision {
        let stale = self
            .last_fault_at
            .map_or(true, |fault_at| elapsed_since(now, fault_at) >= self.policy.reset_after());
        if stale {
            self.attempts = 0;
        }
        self.fault = Some(fault);
        self.last_fault_at = Some(now);
        self.attempts = self.attempts.saturating_add(1);
        self.recovering = true;
        if fault.is_fatal() || self.attempts > self.policy.max_attempts {
            return PresentationDecision::Fatal;
        }
        let delay = self.policy.backoff(self.attempts);
        if delay.is_zero() {
            PresentationDecision::RetryNow
        } else {
            PresentationDecision::RetryAfter(delay)
        }
    }

    /// Current health, derived from the recovery state without side effects.
    pub fn health(&self) -> PresentationHealth {
        if !self.recovering {
            return PresentationHealth::Healthy;
        }
        let fault = self.fault.unwrap_or(PresentationFault::Other);
        if fault.is_fatal() || self.attempts > self.policy.max_attempts {
            return PresentationHealth::Failed { fault };
        }
        let attempt = self.attempts.max(1);
        let anchor = self.last_fault_at.unwrap_or_else(Instant::now);
        PresentationHealth::Recovering {
            attempt,
            next_retry: retry_deadline(anchor, self.policy.backoff(attempt)),
        }
    }

    pub fn attempts(&self) -> u8 {
        self.attempts
    }

    pub fn last_fault(&self) -> Option<PresentationFault> {
        self.fault
    }

    pub fn last_fault_at(&self) -> Option<Instant> {
        self.last_fault_at
    }

    pub fn last_success_at(&self) -> Option<Instant> {
        self.last_success_at
    }

    /// Clears the recovery episode and its attempt history.
    pub fn reset(&mut self) {
        self.fault = None;
        self.attempts = 0;
        self.last_fault_at = None;
        self.last_success_at = None;
        self.recovering = false;
    }
}

/// Maps a backend surface error onto the Rafi-owned fault vocabulary.
///
/// This stays crate-visible because it names a backend type; editor, document,
/// and surface-facing contracts only see [`PresentationFault`].
pub(crate) fn classify_surface_error(error: &raf_agb::Error) -> PresentationFault {
    match error {
        raf_agb::Error::SurfaceLost => PresentationFault::Lost,
        raf_agb::Error::SurfaceOutdated => PresentationFault::Outdated,
        raf_agb::Error::DeviceLost => PresentationFault::Lost,
        raf_agb::Error::Busy => PresentationFault::Timeout,
        raf_agb::Error::Budget { .. } => PresentationFault::OutOfMemory,
        _ => PresentationFault::Other,
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn elapsed_since(now: Instant, earlier: Instant) -> Duration {
    now.saturating_duration_since(earlier)
}

#[cfg(target_arch = "wasm32")]
fn elapsed_since(_now: Instant, earlier: Instant) -> Duration {
    earlier.elapsed()
}

#[cfg(not(target_arch = "wasm32"))]
fn retry_deadline(origin: Instant, delay: Duration) -> Instant {
    origin + delay
}

// The shared web clock exposes `now`/`elapsed` only. The native swapchain
// host is not the production web presentation path, so there the deadline is
// anchored at the fault instant and retry pacing falls back to the caller's
// frame cadence while the same attempt budget and fatal transitions apply.
#[cfg(target_arch = "wasm32")]
fn retry_deadline(origin: Instant, _delay: Duration) -> Instant {
    origin
}

#[cfg(test)]
mod tests {
    use super::*;

    const BASE: u64 = 80;

    #[test]
    fn transient_faults_retry_with_first_backoff() {
        let mut recovery = PresentationRecovery::new(PresentationPolicy::default());
        let now = Instant::now();
        for fault in [
            PresentationFault::Lost,
            PresentationFault::Outdated,
            PresentationFault::Timeout,
            PresentationFault::Other,
        ] {
            recovery.reset();
            assert_eq!(
                recovery.record_fault(fault, now),
                PresentationDecision::RetryAfter(Duration::from_millis(BASE)),
                "unexpected decision for {fault:?}"
            );
            assert!(matches!(
                recovery.health(),
                PresentationHealth::Recovering { attempt: 1, .. }
            ));
            assert_eq!(recovery.attempts(), 1);
        }
    }

    #[test]
    fn out_of_memory_is_fatal_immediately() {
        let mut recovery = PresentationRecovery::new(PresentationPolicy::default());
        assert_eq!(
            recovery.record_fault(PresentationFault::OutOfMemory, Instant::now()),
            PresentationDecision::Fatal
        );
        assert!(matches!(
            recovery.health(),
            PresentationHealth::Failed {
                fault: PresentationFault::OutOfMemory
            }
        ));
        assert!(PresentationFault::OutOfMemory.is_fatal());
        assert!(!PresentationFault::Lost.is_fatal());
    }

    #[test]
    fn backoff_grows_and_becomes_fatal_after_max_attempts() {
        let mut recovery = PresentationRecovery::new(PresentationPolicy::default());
        let start = Instant::now();
        assert_eq!(
            recovery.record_fault(PresentationFault::Lost, start),
            PresentationDecision::RetryAfter(Duration::from_millis(80))
        );
        assert_eq!(
            recovery.record_fault(PresentationFault::Lost, start + Duration::from_millis(10)),
            PresentationDecision::RetryAfter(Duration::from_millis(240))
        );
        assert_eq!(
            recovery.record_fault(PresentationFault::Lost, start + Duration::from_millis(20)),
            PresentationDecision::RetryAfter(Duration::from_millis(720))
        );
        assert_eq!(
            recovery.record_fault(PresentationFault::Lost, start + Duration::from_millis(30)),
            PresentationDecision::Fatal
        );
        assert!(matches!(
            recovery.health(),
            PresentationHealth::Failed {
                fault: PresentationFault::Lost
            }
        ));
        assert_eq!(recovery.attempts(), 4);
    }

    #[test]
    fn backoff_is_capped_by_policy() {
        let policy = PresentationPolicy {
            max_attempts: 5,
            base_backoff_ms: 80,
            max_backoff_ms: 900,
            reset_after_ms: 10_000,
        };
        let mut recovery = PresentationRecovery::new(policy);
        let start = Instant::now();
        assert_eq!(
            recovery.record_fault(PresentationFault::Timeout, start),
            PresentationDecision::RetryAfter(Duration::from_millis(80))
        );
        assert_eq!(
            recovery.record_fault(PresentationFault::Timeout, start + Duration::from_millis(5)),
            PresentationDecision::RetryAfter(Duration::from_millis(240))
        );
        assert_eq!(
            recovery.record_fault(PresentationFault::Timeout, start + Duration::from_millis(10)),
            PresentationDecision::RetryAfter(Duration::from_millis(720))
        );
        assert_eq!(
            recovery.record_fault(PresentationFault::Timeout, start + Duration::from_millis(15)),
            PresentationDecision::RetryAfter(Duration::from_millis(900))
        );
        assert_eq!(
            recovery.record_fault(PresentationFault::Timeout, start + Duration::from_millis(20)),
            PresentationDecision::RetryAfter(Duration::from_millis(900))
        );
        assert_eq!(
            recovery.record_fault(PresentationFault::Timeout, start + Duration::from_millis(25)),
            PresentationDecision::Fatal
        );
    }

    #[test]
    fn success_immediately_clears_recovering() {
        let mut recovery = PresentationRecovery::new(PresentationPolicy::default());
        let start = Instant::now();
        recovery.record_fault(PresentationFault::Lost, start);
        recovery.record_success(start + Duration::from_millis(5));
        assert!(matches!(recovery.health(), PresentationHealth::Healthy));
        // The surface was only healthy for a moment, so the attempt budget is
        // retained until a full healthy window passes.
        assert_eq!(recovery.attempts(), 1);
        recovery.record_success(start + Duration::from_millis(10_500));
        assert_eq!(recovery.attempts(), 0);
        assert_eq!(recovery.last_fault(), Some(PresentationFault::Lost));
    }

    #[test]
    fn reset_clears_history() {
        let mut recovery = PresentationRecovery::new(PresentationPolicy::default());
        recovery.record_fault(PresentationFault::Outdated, Instant::now());
        recovery.reset();
        assert!(matches!(recovery.health(), PresentationHealth::Healthy));
        assert_eq!(recovery.attempts(), 0);
        assert_eq!(recovery.last_fault(), None);
        assert_eq!(recovery.last_fault_at(), None);
        assert_eq!(recovery.last_success_at(), None);
    }

    #[test]
    fn zero_backoff_reports_retry_now() {
        let policy = PresentationPolicy {
            base_backoff_ms: 0,
            ..PresentationPolicy::default()
        };
        let mut recovery = PresentationRecovery::new(policy);
        assert_eq!(
            recovery.record_fault(PresentationFault::Other, Instant::now()),
            PresentationDecision::RetryNow
        );
    }

    #[test]
    fn stale_fault_burst_starts_a_fresh_budget() {
        let mut recovery = PresentationRecovery::new(PresentationPolicy::default());
        let start = Instant::now();
        for step in 0..5 {
            recovery.record_fault(
                PresentationFault::Lost,
                start + Duration::from_millis(step),
            );
        }
        assert!(matches!(recovery.health(), PresentationHealth::Failed { .. }));
        assert_eq!(
            recovery.record_fault(
                PresentationFault::Lost,
                start + Duration::from_millis(10_100)
            ),
            PresentationDecision::RetryAfter(Duration::from_millis(80))
        );
        assert_eq!(recovery.attempts(), 1);
    }

    #[test]
    fn classify_maps_every_surface_error() {
        assert_eq!(
            classify_surface_error(&raf_agb::Error::SurfaceLost),
            PresentationFault::Lost
        );
        assert_eq!(
            classify_surface_error(&raf_agb::Error::SurfaceOutdated),
            PresentationFault::Outdated
        );
        assert_eq!(
            classify_surface_error(&raf_agb::Error::DeviceLost),
            PresentationFault::Lost
        );
        assert_eq!(
            classify_surface_error(&raf_agb::Error::Busy),
            PresentationFault::Timeout
        );
        assert_eq!(
            classify_surface_error(&raf_agb::Error::Budget {
                requested: 1,
                available: 0
            }),
            PresentationFault::OutOfMemory
        );
        assert_eq!(
            classify_surface_error(&raf_agb::Error::Invalid("probe")),
            PresentationFault::Other
        );
    }
}
