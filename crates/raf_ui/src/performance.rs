//! Bounded runtime cost observations and cooperative optional-work limits.
//! No renderer, filesystem, window, or background polling is owned here.

use serde::{Deserialize, Serialize};
use std::collections::VecDeque;
use std::time::{Duration, Instant};

const MAX_REGIONS: usize = 128;
const MAX_SAMPLES: usize = 256;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum UiWorkCause {
    Initial,
    Document,
    Geometry,
    Scroll,
    Text,
    Visual,
    Resources,
    Animation,
    Explicit,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum UiWorkPhase {
    Document,
    Input,
    Layout,
    IntrinsicLayout,
    Text,
    Paint,
    Submission,
    ResourceAdmission,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct UiWorkLimits {
    /// Advisory CPU threshold, not preemption or an FPS ceiling.
    pub warning_us: u64,
    pub optional_slice_us: u64,
    pub optional_items: usize,
    pub optional_bytes: u64,
}

impl Default for UiWorkLimits {
    fn default() -> Self {
        Self {
            warning_us: 4_000,
            optional_slice_us: 1_500,
            optional_items: 4,
            optional_bytes: 4 * 1024 * 1024,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct UiWorkSample {
    pub region: usize,
    pub phase: UiWorkPhase,
    pub cause: UiWorkCause,
    pub cpu_us: u64,
    pub nodes: usize,
    pub bytes: u64,
    pub reused: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct UiRegionCost {
    pub id: String,
    pub observations: u64,
    pub rebuilds: u64,
    pub reuses: u64,
    pub slow_observations: u64,
    pub last_cpu_us: u64,
    pub peak_cpu_us: u64,
    pub last_nodes: usize,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct UiPerformanceSnapshot {
    pub regions: Vec<UiRegionCost>,
    pub samples: Vec<UiWorkSample>,
    pub dropped_regions: u64,
}

impl UiPerformanceSnapshot {
    pub fn append(&mut self, other: Self) {
        let offset = self.regions.len();
        self.regions.extend(other.regions);
        self.samples
            .extend(other.samples.into_iter().map(|mut sample| {
                sample.region += offset;
                sample
            }));
        self.dropped_regions = self.dropped_regions.saturating_add(other.dropped_regions);
    }
}

#[derive(Debug, Clone)]
pub struct UiPerformanceMonitor {
    limits: UiWorkLimits,
    regions: Vec<UiRegionCost>,
    samples: VecDeque<UiWorkSample>,
    dropped_regions: u64,
    last_warning: Option<Instant>,
}

impl Default for UiPerformanceMonitor {
    fn default() -> Self {
        Self {
            limits: UiWorkLimits::default(),
            regions: Vec::new(),
            samples: VecDeque::with_capacity(MAX_SAMPLES),
            dropped_regions: 0,
            last_warning: None,
        }
    }
}

impl UiPerformanceMonitor {
    pub fn limits(&self) -> UiWorkLimits {
        self.limits
    }
    pub fn set_limits(&mut self, limits: UiWorkLimits) {
        self.limits = limits;
    }
    pub fn clear(&mut self) {
        self.regions.clear();
        self.samples.clear();
        self.dropped_regions = 0;
        self.last_warning = None;
    }
    pub fn snapshot(&self) -> UiPerformanceSnapshot {
        UiPerformanceSnapshot {
            regions: self.regions.clone(),
            samples: self.samples.iter().copied().collect(),
            dropped_regions: self.dropped_regions,
        }
    }
    /// Returns true for a slow observation at most once per second per host.
    pub fn record(
        &mut self,
        id: &str,
        phase: UiWorkPhase,
        cause: UiWorkCause,
        elapsed: Duration,
        nodes: usize,
        bytes: u64,
        reused: bool,
    ) -> bool {
        if id.len() > 256 {
            self.dropped_regions = self.dropped_regions.saturating_add(1);
            return false;
        }
        let index = match self.regions.iter().position(|region| region.id == id) {
            Some(index) => index,
            None if self.regions.len() < MAX_REGIONS => {
                self.regions.push(UiRegionCost {
                    id: id.to_owned(),
                    ..Default::default()
                });
                self.regions.len() - 1
            }
            None => {
                self.dropped_regions = self.dropped_regions.saturating_add(1);
                return false;
            }
        };
        let cpu_us = elapsed.as_micros().min(u128::from(u64::MAX)) as u64;
        let region = &mut self.regions[index];
        region.observations = region.observations.saturating_add(1);
        region.rebuilds = region.rebuilds.saturating_add(u64::from(!reused));
        region.reuses = region.reuses.saturating_add(u64::from(reused));
        region.slow_observations = region
            .slow_observations
            .saturating_add(u64::from(cpu_us > self.limits.warning_us));
        region.last_cpu_us = cpu_us;
        region.peak_cpu_us = region.peak_cpu_us.max(cpu_us);
        region.last_nodes = nodes;
        if self.samples.len() == MAX_SAMPLES {
            self.samples.pop_front();
        }
        self.samples.push_back(UiWorkSample {
            region: index,
            phase,
            cause,
            cpu_us,
            nodes,
            bytes,
            reused,
        });
        let warn = cpu_us > self.limits.warning_us
            && self
                .last_warning
                .is_none_or(|last| last.elapsed() >= Duration::from_secs(1));
        if warn {
            self.last_warning = Some(Instant::now());
        }
        warn
    }
}

/// Only callers handling explicitly optional work may use this allowance.
/// One oversized first item is admitted to avoid starving a bounded resource.
pub struct UiWorkAllowance {
    started: Instant,
    limits: UiWorkLimits,
    items: usize,
    bytes: u64,
}

impl UiWorkAllowance {
    pub fn new(limits: UiWorkLimits) -> Self {
        Self {
            started: Instant::now(),
            limits,
            items: 0,
            bytes: 0,
        }
    }
    pub fn can_poll(&self) -> bool {
        self.items < self.limits.optional_items.max(1)
            && (self.items == 0
                || (self.started.elapsed().as_micros() < u128::from(self.limits.optional_slice_us)
                    && self.bytes < self.limits.optional_bytes))
    }
    pub fn admit(&mut self, bytes: u64) -> bool {
        if !self.can_poll()
            || (self.items > 0 && self.bytes.saturating_add(bytes) > self.limits.optional_bytes)
        {
            return false;
        }
        self.items += 1;
        self.bytes = self.bytes.saturating_add(bytes);
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn ui_cost_guard_bounds_history_and_optional_work_without_starvation() {
        let mut monitor = UiPerformanceMonitor::default();
        for _ in 0..MAX_SAMPLES * 3 {
            monitor.record(
                "panel",
                UiWorkPhase::Layout,
                UiWorkCause::Scroll,
                Duration::ZERO,
                10,
                0,
                true,
            );
        }
        assert_eq!(monitor.snapshot().samples.len(), MAX_SAMPLES);
        for i in 0..MAX_REGIONS * 2 {
            monitor.record(
                &format!("panel-{i}"),
                UiWorkPhase::Paint,
                UiWorkCause::Visual,
                Duration::ZERO,
                0,
                0,
                false,
            );
        }
        assert_eq!(monitor.snapshot().regions.len(), MAX_REGIONS);
        assert!(monitor.snapshot().dropped_regions > 0);
        let mut work = UiWorkAllowance::new(UiWorkLimits {
            optional_bytes: 16,
            ..Default::default()
        });
        assert!(work.admit(32));
        assert!(!work.admit(1));
    }
}
