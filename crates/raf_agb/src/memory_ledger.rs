//! Cross-family graphics memory ledger.
//!
//! Owners report residency into this aggregate so potato/desktop budgets can be
//! compared and enforced through one vocabulary: buffers, textures, samplers,
//! pipelines, meshes, materials, staging, and transient resources.
//!
//! Caches that already own their residency (the bounded resource arenas) keep
//! their eviction semantics and publish their meters through
//! `ResourceArena::sync_ledger`. Families without an arena use
//! [`GraphicsResourceLedger::try_admit`] and [`GraphicsResourceLedger::release`]
//! directly. A family must not mix both styles, or its resident counter will
//! double count.
//!
//! [`ProcessMemoryLedger`] remains the process-wide reporting struct for
//! observability. It is not an enforcement point.

use std::fmt;

/// One budgeted family of graphics resources.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum GraphicsResourceFamily {
    #[default]
    Buffer,
    Texture,
    Sampler,
    Pipeline,
    Mesh,
    Material,
    Staging,
    Transient,
}

impl GraphicsResourceFamily {
    /// Every family in index order. `ALL[index] == family` for that index.
    pub const ALL: [Self; 8] = [
        Self::Buffer,
        Self::Texture,
        Self::Sampler,
        Self::Pipeline,
        Self::Mesh,
        Self::Material,
        Self::Staging,
        Self::Transient,
    ];

    pub const fn label(self) -> &'static str {
        match self {
            Self::Buffer => "buffer",
            Self::Texture => "texture",
            Self::Sampler => "sampler",
            Self::Pipeline => "pipeline",
            Self::Mesh => "mesh",
            Self::Material => "material",
            Self::Staging => "staging",
            Self::Transient => "transient",
        }
    }

    pub const fn index(self) -> usize {
        match self {
            Self::Buffer => 0,
            Self::Texture => 1,
            Self::Sampler => 2,
            Self::Pipeline => 3,
            Self::Mesh => 4,
            Self::Material => 5,
            Self::Staging => 6,
            Self::Transient => 7,
        }
    }
}

/// Resident and lifetime counters for one resource family.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct GraphicsFamilyLedger {
    pub budget_bytes: u64,
    pub resident_bytes: u64,
    pub peak_bytes: u64,
    pub admissions: u64,
    pub evictions: u64,
    pub rejected: u64,
    pub last_touch_frame: u64,
}

impl GraphicsFamilyLedger {
    pub const EMPTY: Self = Self {
        budget_bytes: 0,
        resident_bytes: 0,
        peak_bytes: 0,
        admissions: 0,
        evictions: 0,
        rejected: 0,
        last_touch_frame: 0,
    };

    /// Budget utilization clamped to `0.0..=1.0`.
    ///
    /// A zero-budget family with anything resident is fully saturated; this is
    /// a configuration signal rather than a division by zero.
    pub fn pressure(&self) -> f32 {
        if self.budget_bytes == 0 {
            if self.resident_bytes == 0 {
                0.0
            } else {
                1.0
            }
        } else {
            (self.resident_bytes as f32 / self.budget_bytes as f32).clamp(0.0, 1.0)
        }
    }
}

/// Successful reservation returned by [`GraphicsResourceLedger::try_admit`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LedgerAdmission {
    pub family: GraphicsResourceFamily,
    pub bytes: u64,
    /// Pinned admissions cannot be evicted by a cache; the flag is retained so
    /// owners can report the distinction without re-deriving it.
    pub pinned: bool,
}

/// Rejected admission with the state that produced the decision.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LedgerError {
    pub requested: u64,
    pub resident: u64,
    pub budget: u64,
    pub family: GraphicsResourceFamily,
}

impl fmt::Display for LedgerError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "graphics ledger rejected {} bytes for {} ({} of {} bytes resident)",
            self.requested,
            self.family.label(),
            self.resident,
            self.budget
        )
    }
}

impl std::error::Error for LedgerError {}

/// Copyable per-family view used by diagnostics and frame metrics.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GraphicsFamilySnapshot {
    pub family: GraphicsResourceFamily,
    pub budget_bytes: u64,
    pub resident_bytes: u64,
    pub peak_bytes: u64,
    pub admissions: u64,
    pub evictions: u64,
    pub rejected: u64,
    pub pressure: f32,
    pub last_touch_frame: u64,
}

impl Default for GraphicsFamilySnapshot {
    fn default() -> Self {
        Self {
            family: GraphicsResourceFamily::Buffer,
            budget_bytes: 0,
            resident_bytes: 0,
            peak_bytes: 0,
            admissions: 0,
            evictions: 0,
            rejected: 0,
            pressure: 0.0,
            last_touch_frame: 0,
        }
    }
}

/// Fixed-size, heap-free snapshot of the whole ledger.
///
/// The snapshot is stable between frames unless an owner changes residency or
/// counters. Only `frame_index` advances with [`GraphicsResourceLedger::begin_frame`].
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GraphicsLedgerSnapshot {
    pub total_budget_bytes: u64,
    pub resident_bytes: u64,
    pub peak_bytes: u64,
    pub reservations: u64,
    pub rejected_total: u64,
    pub frame_index: u64,
    pub pressure: f32,
    pub families: [GraphicsFamilySnapshot; 8],
}

impl GraphicsLedgerSnapshot {
    pub fn family(&self, family: GraphicsResourceFamily) -> &GraphicsFamilySnapshot {
        &self.families[family.index()]
    }
}

/// One enforced budget for every graphics resource family.
///
/// Arena-backed families publish authoritative resident/peak/counter values
/// once per frame. Direct owners use `try_admit`/`release`. All arithmetic is
/// saturating so a corrupted counter can never wrap into a false low value.
#[derive(Debug, Clone, PartialEq)]
pub struct GraphicsResourceLedger {
    pub total_budget_bytes: u64,
    pub families: [GraphicsFamilyLedger; 8],
    pub frame_index: u64,
    /// Live reservations created through [`Self::try_admit`].
    pub reservations: u64,
    /// Historical count of rejected admissions, including arena rejections.
    pub rejected_total: u64,
    /// Peak of the summed family residency, not the sum of family peaks.
    pub peak_resident_bytes: u64,
}

impl GraphicsResourceLedger {
    pub const fn new(gpu_budget: u64) -> Self {
        Self {
            total_budget_bytes: gpu_budget,
            families: [GraphicsFamilyLedger::EMPTY; 8],
            frame_index: 0,
            reservations: 0,
            rejected_total: 0,
            peak_resident_bytes: 0,
        }
    }

    pub fn set_family_budget(&mut self, family: GraphicsResourceFamily, budget_bytes: u64) {
        self.families[family.index()].budget_bytes = budget_bytes;
    }

    pub fn family(&self, family: GraphicsResourceFamily) -> &GraphicsFamilyLedger {
        &self.families[family.index()]
    }

    /// Reserve `bytes` in `family` if its budget still has room.
    ///
    /// Do not call this for families that publish through
    /// `ResourceArena::sync_ledger`; the arena is the authoritative owner for
    /// those bytes.
    pub fn try_admit(
        &mut self,
        family: GraphicsResourceFamily,
        bytes: u64,
        pinned: bool,
    ) -> Result<LedgerAdmission, LedgerError> {
        let entry = &mut self.families[family.index()];
        if entry.resident_bytes.saturating_add(bytes) > entry.budget_bytes {
            let resident = entry.resident_bytes;
            let budget = entry.budget_bytes;
            entry.rejected = entry.rejected.saturating_add(1);
            self.rejected_total = self.rejected_total.saturating_add(1);
            return Err(LedgerError {
                requested: bytes,
                resident,
                budget,
                family,
            });
        }

        entry.resident_bytes = entry.resident_bytes.saturating_add(bytes);
        entry.peak_bytes = entry.peak_bytes.max(entry.resident_bytes);
        entry.admissions = entry.admissions.saturating_add(1);
        self.reservations = self.reservations.saturating_add(1);
        self.refresh_peak_resident();
        Ok(LedgerAdmission {
            family,
            bytes,
            pinned,
        })
    }

    /// Release a reservation created by [`Self::try_admit`].
    pub fn release(&mut self, admission: LedgerAdmission) {
        let entry = &mut self.families[admission.family.index()];
        entry.resident_bytes = entry.resident_bytes.saturating_sub(admission.bytes);
        entry.evictions = entry.evictions.saturating_add(1);
        self.reservations = self.reservations.saturating_sub(1);
    }

    /// Mark a family as used in the current frame for future aging policies.
    pub fn touch(&mut self, family: GraphicsResourceFamily) {
        self.families[family.index()].last_touch_frame = self.frame_index;
    }

    /// Replace the resident byte count of a family that owns its own residency.
    pub fn sync_resident(&mut self, family: GraphicsResourceFamily, bytes: u64) {
        let entry = &mut self.families[family.index()];
        entry.resident_bytes = bytes;
        entry.peak_bytes = entry.peak_bytes.max(bytes);
        self.refresh_peak_resident();
    }

    /// Publish lifetime counters from an arena or another authoritative owner.
    ///
    /// Rejected admissions are folded into the historical total by delta so a
    /// replacement arena with reset counters cannot lose prior rejections.
    pub fn sync_family_stats(
        &mut self,
        family: GraphicsResourceFamily,
        peak_bytes: u64,
        admissions: u64,
        evictions: u64,
        rejected: u64,
    ) {
        let entry = &mut self.families[family.index()];
        entry.peak_bytes = entry.peak_bytes.max(peak_bytes).max(entry.resident_bytes);
        entry.admissions = admissions;
        entry.evictions = evictions;
        let previous_rejected = entry.rejected;
        entry.rejected = rejected;
        if rejected > previous_rejected {
            self.rejected_total = self
                .rejected_total
                .saturating_add(rejected - previous_rejected);
        }
    }

    /// Highest family utilization. A single exhausted family matters more than
    /// aggregate headroom because that family is the one rejecting admissions.
    pub fn pressure(&self) -> f32 {
        self.families
            .iter()
            .map(GraphicsFamilyLedger::pressure)
            .fold(0.0_f32, f32::max)
    }

    pub fn snapshot(&self) -> GraphicsLedgerSnapshot {
        let mut resident_bytes = 0_u64;
        let mut families = [GraphicsFamilySnapshot::default(); 8];
        for (slot, family) in families.iter_mut().zip(GraphicsResourceFamily::ALL) {
            let entry = &self.families[family.index()];
            resident_bytes = resident_bytes.saturating_add(entry.resident_bytes);
            *slot = GraphicsFamilySnapshot {
                family,
                budget_bytes: entry.budget_bytes,
                resident_bytes: entry.resident_bytes,
                peak_bytes: entry.peak_bytes,
                admissions: entry.admissions,
                evictions: entry.evictions,
                rejected: entry.rejected,
                pressure: entry.pressure(),
                last_touch_frame: entry.last_touch_frame,
            };
        }

        GraphicsLedgerSnapshot {
            total_budget_bytes: self.total_budget_bytes,
            resident_bytes,
            peak_bytes: self.peak_resident_bytes.max(resident_bytes),
            reservations: self.reservations,
            rejected_total: self.rejected_total,
            frame_index: self.frame_index,
            pressure: self.pressure(),
            families,
        }
    }

    pub fn begin_frame(&mut self, frame_index: u64) {
        self.frame_index = frame_index;
    }

    fn refresh_peak_resident(&mut self) {
        let resident_bytes = self.families.iter().fold(0_u64, |total, family| {
            total.saturating_add(family.resident_bytes)
        });
        self.peak_resident_bytes = self.peak_resident_bytes.max(resident_bytes);
    }
}

/// Process-wide residency reported by every graphics owner.
///
/// This is an observability aggregate, not a budget enforcer. New families
/// added to [`GraphicsResourceFamily`] report here as their owners migrate.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ProcessMemoryLedger {
    pub mesh_resident_bytes: u64,
    pub ui_gpu_image_bytes: u64,
    pub ui_cpu_image_bytes: u64,
    pub text_atlas_bytes: u64,
    pub staging_bytes: u64,
    pub buffer_resident_bytes: u64,
    pub sampler_resident_bytes: u64,
    pub pipeline_resident_bytes: u64,
    pub material_resident_bytes: u64,
    pub transient_bytes: u64,
}

impl ProcessMemoryLedger {
    pub const fn total_resident_bytes(&self) -> u64 {
        self.mesh_resident_bytes
            .saturating_add(self.ui_gpu_image_bytes)
            .saturating_add(self.ui_cpu_image_bytes)
            .saturating_add(self.text_atlas_bytes)
            .saturating_add(self.staging_bytes)
            .saturating_add(self.buffer_resident_bytes)
            .saturating_add(self.sampler_resident_bytes)
            .saturating_add(self.pipeline_resident_bytes)
            .saturating_add(self.material_resident_bytes)
            .saturating_add(self.transient_bytes)
    }

    pub const fn with_mesh_resident_bytes(mut self, bytes: u64) -> Self {
        self.mesh_resident_bytes = bytes;
        self
    }

    pub const fn with_ui_gpu_image_bytes(mut self, bytes: u64) -> Self {
        self.ui_gpu_image_bytes = bytes;
        self
    }

    pub const fn with_ui_cpu_image_bytes(mut self, bytes: u64) -> Self {
        self.ui_cpu_image_bytes = bytes;
        self
    }

    pub const fn with_text_atlas_bytes(mut self, bytes: u64) -> Self {
        self.text_atlas_bytes = bytes;
        self
    }

    pub const fn with_staging_bytes(mut self, bytes: u64) -> Self {
        self.staging_bytes = bytes;
        self
    }

    pub const fn with_buffer_resident_bytes(mut self, bytes: u64) -> Self {
        self.buffer_resident_bytes = bytes;
        self
    }

    pub const fn with_sampler_resident_bytes(mut self, bytes: u64) -> Self {
        self.sampler_resident_bytes = bytes;
        self
    }

    pub const fn with_pipeline_resident_bytes(mut self, bytes: u64) -> Self {
        self.pipeline_resident_bytes = bytes;
        self
    }

    pub const fn with_material_resident_bytes(mut self, bytes: u64) -> Self {
        self.material_resident_bytes = bytes;
        self
    }

    pub const fn with_transient_bytes(mut self, bytes: u64) -> Self {
        self.transient_bytes = bytes;
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ledger_with_family(
        family: GraphicsResourceFamily,
        budget_bytes: u64,
    ) -> GraphicsResourceLedger {
        let mut ledger = GraphicsResourceLedger::new(budget_bytes);
        ledger.set_family_budget(family, budget_bytes);
        ledger
    }

    #[test]
    fn family_admission_rejects_when_full() {
        let mut ledger = ledger_with_family(GraphicsResourceFamily::Texture, 100);

        ledger
            .try_admit(GraphicsResourceFamily::Texture, 60, false)
            .expect("first texture fits");
        let error = ledger
            .try_admit(GraphicsResourceFamily::Texture, 60, false)
            .expect_err("second texture crosses the family budget");

        assert_eq!(
            error,
            LedgerError {
                requested: 60,
                resident: 60,
                budget: 100,
                family: GraphicsResourceFamily::Texture,
            }
        );
        assert_eq!(
            ledger
                .family(GraphicsResourceFamily::Texture)
                .resident_bytes,
            60
        );
        assert_eq!(ledger.family(GraphicsResourceFamily::Texture).rejected, 1);
        assert_eq!(ledger.rejected_total, 1);
        assert_eq!(ledger.reservations, 1);
    }

    #[test]
    fn eviction_accounting_saturates() {
        let mut ledger = ledger_with_family(GraphicsResourceFamily::Mesh, 128);
        let admission = ledger
            .try_admit(GraphicsResourceFamily::Mesh, 128, false)
            .expect("mesh fits exactly");

        ledger.release(admission);
        ledger.release(admission);
        ledger.release(admission);

        let family = ledger.family(GraphicsResourceFamily::Mesh);
        assert_eq!(family.resident_bytes, 0);
        assert_eq!(family.evictions, 3);
        assert_eq!(ledger.reservations, 0);
        assert!(ledger.snapshot().peak_bytes >= 128);
    }

    #[test]
    fn rejected_counters_saturate_instead_of_wrapping() {
        let mut ledger = ledger_with_family(GraphicsResourceFamily::Pipeline, 0);
        ledger.families[GraphicsResourceFamily::Pipeline.index()].rejected = u64::MAX;

        let error = ledger
            .try_admit(GraphicsResourceFamily::Pipeline, 1, false)
            .expect_err("zero-budget family rejects every admission");

        assert_eq!(error.budget, 0);
        assert_eq!(
            ledger.family(GraphicsResourceFamily::Pipeline).rejected,
            u64::MAX
        );
        assert_eq!(ledger.rejected_total, 1);
    }

    #[test]
    fn snapshot_is_stable_across_frames() {
        let mut ledger = ledger_with_family(GraphicsResourceFamily::Staging, 4_096);
        ledger.begin_frame(7);
        ledger
            .try_admit(GraphicsResourceFamily::Staging, 2_048, true)
            .expect("staging fits");
        let first = ledger.snapshot();

        ledger.begin_frame(8);
        let second = ledger.snapshot();

        assert_eq!(
            first.families, second.families,
            "resident data must not drift on a frame boundary"
        );
        assert_eq!(first.resident_bytes, second.resident_bytes);
        assert_eq!(first.peak_bytes, second.peak_bytes);
        assert_eq!(first.rejected_total, second.rejected_total);
        assert_eq!(first.frame_index, 7);
        assert_eq!(second.frame_index, 8);
    }

    #[test]
    fn snapshot_family_lookup_matches_index_order() {
        let ledger = GraphicsResourceLedger::new(1_024);
        let snapshot = ledger.snapshot();

        for family in GraphicsResourceFamily::ALL {
            assert_eq!(snapshot.family(family).family, family);
            assert_eq!(snapshot.families[family.index()].family, family);
        }
    }

    #[test]
    fn process_totals_include_every_family_and_saturate() {
        let ledger = ProcessMemoryLedger::default()
            .with_mesh_resident_bytes(1_024)
            .with_ui_gpu_image_bytes(2_048)
            .with_ui_cpu_image_bytes(4_096)
            .with_text_atlas_bytes(512)
            .with_staging_bytes(128)
            .with_buffer_resident_bytes(256)
            .with_sampler_resident_bytes(32)
            .with_pipeline_resident_bytes(64)
            .with_material_resident_bytes(16)
            .with_transient_bytes(8);
        assert_eq!(
            ledger.total_resident_bytes(),
            1_024 + 2_048 + 4_096 + 512 + 128 + 256 + 32 + 64 + 16 + 8
        );

        let saturated = ProcessMemoryLedger::default()
            .with_mesh_resident_bytes(u64::MAX)
            .with_transient_bytes(u64::MAX);
        assert_eq!(saturated.total_resident_bytes(), u64::MAX);
    }
}
