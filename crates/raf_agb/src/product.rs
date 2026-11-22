//! The retained rendering contract that editor surfaces can emit directly.
//! Native code and the benchmark reference remain behind the executor boundary.

mod contract;
mod coverage;
mod encode;
mod paint_style;
mod paint_updates;
pub mod startup;

pub use contract::*;
pub use coverage::{Coverage, Feature, CAD_COVERAGE, COVERAGE};
pub use paint_style::{
    ImageFit, ImageLayout, LinearGradient, NineSlice, PaintBorder, PaintStyle, RoundedClip, Side,
    MAX_DECLARED_CLIPS, MAX_ROUNDED_CLIPS,
};
pub use paint_updates::TexturePatch;

use std::{
    collections::HashMap,
    ops::Range,
    sync::atomic::{AtomicU64, Ordering},
};

use crate::{
    capabilities::GraphicsMemoryBudget, handles::MeshHandle, AdapterInfo, Backend, Device, Error,
    FrameMetrics, Output, ResourcePatch, Result,
};

static NEXT_OWNER: AtomicU64 = AtomicU64::new(1);

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct PacketKey(pub u64);

/// Valid only on the device and for the resident generation that created it.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PacketHandle {
    owner: u64,
    native: MeshHandle,
    key: PacketKey,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MeshSpan {
    pub vertices: Range<u32>,
    pub indices: Option<Range<u32>>,
}

struct Entry {
    native: MeshHandle,
    revision: u64,
    clear: [f32; 4],
    meshes: HashMap<(u32, bool), MeshSpan>,
    instances: HashMap<usize, Range<u32>>,
}

/// Device-scoped cache. Callers retain the source of truth and can replay it
/// after device loss. A draw updates only uniforms and references GPU residency.
pub struct ProductDevice<B: Backend> {
    device: Device<B>,
    owner: u64,
    entries: HashMap<PacketKey, Entry>,
    budget: GraphicsMemoryBudget,
}

impl<B: Backend> ProductDevice<B> {
    pub fn new(backend: B, budget: GraphicsMemoryBudget) -> Result<Self> {
        let owner = NEXT_OWNER
            .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |next| {
                next.checked_add(1)
            })
            .map_err(|_| Error::Invalid("graphics device identity exhausted"))?;
        Ok(Self {
            device: Device::new(backend, budget)?,
            owner,
            entries: HashMap::new(),
            budget,
        })
    }

    pub fn configure(&mut self, extent: [u32; 2], output: Output) -> Result<()> {
        self.device.configure(extent, output)
    }

    pub fn info(&self) -> &AdapterInfo {
        self.device.info()
    }

    pub fn supports_shadows(&self) -> bool {
        self.device.supports_shadows()
    }

    pub fn supports_scene_filter(&self) -> bool {
        self.device.supports_scene_filter()
    }

    pub fn ledger(&self) -> crate::memory_ledger::GraphicsLedgerSnapshot {
        self.device.ledger()
    }

    /// Builds the packet only for a new revision. A replacement is admitted
    /// while the previous generation remains resident, so failure preserves it.
    pub fn prepare(
        &mut self,
        key: PacketKey,
        revision: u64,
        build: impl FnOnce() -> ProductPacket,
    ) -> Result<PacketHandle> {
        let previous = self.entries.get(&key);
        if let Some(previous) = previous {
            if revision < previous.revision {
                return Err(Error::Invalid("obsolete graphics packet revision"));
            }
            if revision == previous.revision {
                return Ok(self.handle(key, previous.native));
            }
        } else if self.entries.len() >= self.budget.mesh_cache_entries as usize {
            return Err(Error::Budget {
                requested: 1,
                available: 0,
            });
        }
        let encoded = encode::encode(build(), self.device.workload_budget())?;
        let native = match previous {
            Some(previous) => self.device.replace(previous.native, &encoded.workload)?,
            None => self.device.upload(&encoded.workload)?,
        };
        self.entries.insert(
            key,
            Entry {
                native,
                revision,
                clear: encoded.clear,
                meshes: encoded.meshes,
                instances: encoded.instances,
            },
        );
        Ok(self.handle(key, native))
    }

    /// Locates a source mesh in the prepared native streams. A mesh used by
    /// both pipelines has a separate unlit and lit span.
    pub fn mesh_span(
        &self,
        handle: PacketHandle,
        mesh: u32,
        lit: bool,
    ) -> Result<Option<MeshSpan>> {
        Ok(self.entry(handle)?.meshes.get(&(mesh, lit)).cloned())
    }

    /// Instance upload range for a DrawMesh or DrawMeshBatch command by ordinal.
    /// Instance tints already include their material tint in the native stream.
    pub fn instance_span(
        &self,
        handle: PacketHandle,
        command: usize,
    ) -> Result<Option<Range<u32>>> {
        Ok(self.entry(handle)?.instances.get(&command).cloned())
    }

    /// Advances the source revision only after the patch has been accepted.
    /// Offsets address the prepared packet's streams, not source mesh indices.
    pub fn patch(
        &mut self,
        handle: PacketHandle,
        expected_revision: u64,
        revision: u64,
        patch: ResourcePatch<'_>,
    ) -> Result<()> {
        let entry = self.entry(handle)?;
        if entry.revision != expected_revision || revision <= expected_revision {
            return Err(Error::Invalid("obsolete graphics patch revision"));
        }
        self.device.patch(handle.native, patch)?;
        self.entries.get_mut(&handle.key).unwrap().revision = revision;
        Ok(())
    }

    pub fn draw(&mut self, handle: PacketHandle, uniforms: &FrameUniforms) -> Result<FrameMetrics> {
        let clear = self.entry(handle)?.clear;
        self.device.draw(handle.native, &uniforms.raster(clear)?)
    }

    pub fn release(&mut self, handle: PacketHandle) -> Result<()> {
        self.entry(handle)?;
        self.device.release(handle.native)?;
        self.entries.remove(&handle.key);
        Ok(())
    }

    pub fn wait(&mut self) -> Result<()> {
        self.device.wait()
    }

    /// Blocking readback for tooling. Call away from the interactive event loop.
    pub fn capture(&mut self) -> Result<Vec<u8>> {
        self.device.capture()
    }

    fn entry(&self, handle: PacketHandle) -> Result<&Entry> {
        self.entries
            .get(&handle.key)
            .filter(|entry| handle.owner == self.owner && handle.native == entry.native)
            .ok_or(Error::Invalid("stale or foreign graphics packet"))
    }

    fn handle(&self, key: PacketKey, native: MeshHandle) -> PacketHandle {
        PacketHandle {
            owner: self.owner,
            key,
            native,
        }
    }
}

#[cfg(test)]
mod tests;
#[cfg(test)]
mod tests_rafui_paint;
