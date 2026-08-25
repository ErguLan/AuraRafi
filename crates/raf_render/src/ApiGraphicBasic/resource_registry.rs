//! Bounded generational residency arenas for backend resources.
//!
//! Backends store their native buffer, texture, mesh, material, or pipeline
//! objects inside a typed arena. Upper layers retain only Rafi handles. The
//! arena evicts least-recently-used unpinned resources only when admission
//! would cross its hard byte budget.

use std::marker::PhantomData;

use super::handles::{
    BufferHandle, GraphicsHandle, MaterialHandle, MeshHandle, PipelineHandle, SamplerHandle,
    TextureHandle,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ResourceAdmission {
    pub bytes: u64,
    pub frame_index: u64,
    pub pinned: bool,
}

impl ResourceAdmission {
    pub const fn cached(bytes: u64, frame_index: u64) -> Self {
        Self {
            bytes,
            frame_index,
            pinned: false,
        }
    }

    pub const fn pinned(bytes: u64, frame_index: u64) -> Self {
        Self {
            bytes,
            frame_index,
            pinned: true,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ResourceBudgetError {
    pub requested_bytes: u64,
    pub resident_bytes: u64,
    pub budget_bytes: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct ResourceArenaMetrics {
    pub resident_bytes: u64,
    pub resident_entries: u32,
    pub peak_resident_bytes: u64,
    pub admissions: u64,
    pub removals: u64,
    pub evictions: u64,
    pub rejected_admissions: u64,
    pub cache_hits: u64,
    pub stale_handle_misses: u64,
}

struct ResourceSlot<T> {
    generation: u32,
    value: Option<T>,
    bytes: u64,
    last_used_frame: u64,
    pinned: bool,
}

impl<T> Default for ResourceSlot<T> {
    fn default() -> Self {
        Self {
            generation: 1,
            value: None,
            bytes: 0,
            last_used_frame: 0,
            pinned: false,
        }
    }
}

pub struct ResourceArena<H, T>
where
    H: GraphicsHandle,
{
    slots: Vec<ResourceSlot<T>>,
    free: Vec<u32>,
    budget_bytes: u64,
    metrics: ResourceArenaMetrics,
    marker: PhantomData<H>,
}

impl<H, T> ResourceArena<H, T>
where
    H: GraphicsHandle,
{
    pub fn new(budget_bytes: u64) -> Self {
        Self {
            slots: Vec::new(),
            free: Vec::new(),
            budget_bytes,
            metrics: ResourceArenaMetrics::default(),
            marker: PhantomData,
        }
    }

    pub fn budget_bytes(&self) -> u64 {
        self.budget_bytes
    }

    pub fn metrics(&self) -> ResourceArenaMetrics {
        self.metrics
    }

    pub fn set_budget_bytes(&mut self, budget_bytes: u64) {
        self.budget_bytes = budget_bytes;
        self.evict_to_budget(0);
    }

    pub fn insert(
        &mut self,
        value: T,
        admission: ResourceAdmission,
    ) -> Result<H, ResourceBudgetError> {
        if admission.bytes > self.budget_bytes {
            self.metrics.rejected_admissions = self.metrics.rejected_admissions.saturating_add(1);
            return Err(self.budget_error(admission.bytes));
        }
        self.evict_to_budget(admission.bytes);
        if self.metrics.resident_bytes.saturating_add(admission.bytes) > self.budget_bytes {
            self.metrics.rejected_admissions = self.metrics.rejected_admissions.saturating_add(1);
            return Err(self.budget_error(admission.bytes));
        }

        let index = self.free.pop().unwrap_or_else(|| {
            let index = self.slots.len() as u32;
            self.slots.push(ResourceSlot::default());
            index
        });
        let slot = &mut self.slots[index as usize];
        debug_assert!(slot.value.is_none());
        slot.value = Some(value);
        slot.bytes = admission.bytes;
        slot.last_used_frame = admission.frame_index;
        slot.pinned = admission.pinned;

        self.metrics.resident_bytes = self.metrics.resident_bytes.saturating_add(admission.bytes);
        self.metrics.resident_entries = self.metrics.resident_entries.saturating_add(1);
        self.metrics.peak_resident_bytes = self
            .metrics
            .peak_resident_bytes
            .max(self.metrics.resident_bytes);
        self.metrics.admissions = self.metrics.admissions.saturating_add(1);
        Ok(H::from_parts(index, slot.generation))
    }

    pub fn contains(&self, handle: H) -> bool {
        self.slot(handle).is_some()
    }

    pub fn get(&mut self, handle: H, frame_index: u64) -> Option<&T> {
        let index = handle.index() as usize;
        let valid = self
            .slots
            .get(index)
            .is_some_and(|slot| slot.generation == handle.generation() && slot.value.is_some());
        if !valid {
            self.metrics.stale_handle_misses = self.metrics.stale_handle_misses.saturating_add(1);
            return None;
        }
        self.metrics.cache_hits = self.metrics.cache_hits.saturating_add(1);
        let slot = &mut self.slots[index];
        slot.last_used_frame = frame_index;
        slot.value.as_ref()
    }

    pub fn get_mut(&mut self, handle: H, frame_index: u64) -> Option<&mut T> {
        let index = handle.index() as usize;
        let valid = self
            .slots
            .get(index)
            .is_some_and(|slot| slot.generation == handle.generation() && slot.value.is_some());
        if !valid {
            self.metrics.stale_handle_misses = self.metrics.stale_handle_misses.saturating_add(1);
            return None;
        }
        self.metrics.cache_hits = self.metrics.cache_hits.saturating_add(1);
        let slot = &mut self.slots[index];
        slot.last_used_frame = frame_index;
        slot.value.as_mut()
    }

    pub fn pin(&mut self, handle: H, pinned: bool) -> bool {
        let Some(slot) = self.slot_mut(handle) else {
            return false;
        };
        slot.pinned = pinned;
        true
    }

    pub fn remove(&mut self, handle: H) -> Option<T> {
        let index = handle.index() as usize;
        let slot = self.slots.get_mut(index)?;
        if slot.generation != handle.generation() {
            return None;
        }
        let value = slot.value.take()?;
        self.metrics.resident_bytes = self.metrics.resident_bytes.saturating_sub(slot.bytes);
        self.metrics.resident_entries = self.metrics.resident_entries.saturating_sub(1);
        self.metrics.removals = self.metrics.removals.saturating_add(1);
        slot.bytes = 0;
        slot.last_used_frame = 0;
        slot.pinned = false;
        slot.generation = slot.generation.wrapping_add(1).max(1);
        self.free.push(index as u32);
        Some(value)
    }

    pub fn clear_unpinned(&mut self) {
        for index in 0..self.slots.len() {
            if self.slots[index].value.is_some() && !self.slots[index].pinned {
                self.evict_index(index);
            }
        }
    }

    fn slot(&self, handle: H) -> Option<&ResourceSlot<T>> {
        let slot = self.slots.get(handle.index() as usize)?;
        (slot.generation == handle.generation() && slot.value.is_some()).then_some(slot)
    }

    fn slot_mut(&mut self, handle: H) -> Option<&mut ResourceSlot<T>> {
        let slot = self.slots.get_mut(handle.index() as usize)?;
        (slot.generation == handle.generation() && slot.value.is_some()).then_some(slot)
    }

    fn evict_to_budget(&mut self, incoming_bytes: u64) {
        while self.metrics.resident_bytes.saturating_add(incoming_bytes) > self.budget_bytes {
            let candidate = self
                .slots
                .iter()
                .enumerate()
                .filter(|(_, slot)| slot.value.is_some() && !slot.pinned)
                .min_by_key(|(_, slot)| slot.last_used_frame)
                .map(|(index, _)| index);
            let Some(index) = candidate else {
                break;
            };
            self.evict_index(index);
        }
    }

    fn evict_index(&mut self, index: usize) {
        let slot = &mut self.slots[index];
        if slot.value.take().is_none() {
            return;
        }
        self.metrics.resident_bytes = self.metrics.resident_bytes.saturating_sub(slot.bytes);
        self.metrics.resident_entries = self.metrics.resident_entries.saturating_sub(1);
        self.metrics.evictions = self.metrics.evictions.saturating_add(1);
        slot.bytes = 0;
        slot.last_used_frame = 0;
        slot.pinned = false;
        slot.generation = slot.generation.wrapping_add(1).max(1);
        self.free.push(index as u32);
    }

    fn budget_error(&self, requested_bytes: u64) -> ResourceBudgetError {
        ResourceBudgetError {
            requested_bytes,
            resident_bytes: self.metrics.resident_bytes,
            budget_bytes: self.budget_bytes,
        }
    }
}

pub type BufferRegistry<T> = ResourceArena<BufferHandle, T>;
pub type TextureRegistry<T> = ResourceArena<TextureHandle, T>;
pub type SamplerRegistry<T> = ResourceArena<SamplerHandle, T>;
pub type PipelineRegistry<T> = ResourceArena<PipelineHandle, T>;
pub type MeshRegistry<T> = ResourceArena<MeshHandle, T>;
pub type MaterialRegistry<T> = ResourceArena<MaterialHandle, T>;
