//! The retained rendering contract that editor surfaces can emit directly.
//! Native code and the benchmark reference remain behind the executor boundary.

mod contract;
mod coverage;
mod encode;
mod paint_style;
mod paint_updates;
pub mod startup;

/// One resident in a layered composition. Re-exported here because `device` is
/// an implementation module; backends and hosts compose through `product`.
pub use crate::device::Layer;
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
    FrameMetrics, Instance, Output, ResourcePatch, Result,
};

use self::encode::InstanceStyle;

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
    instance_styles: HashMap<usize, InstanceStyle>,
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

    /// True when this device can draw the scene and UI packets in one pass.
    /// Hosts keep the single-packet composition when this returns false.
    pub fn supports_layers(&self) -> bool {
        self.device.supports_layers()
    }

    /// True when the executor accepts per-frame instance patches on a retained
    /// packet. Upper layers keep the full-packet fingerprint when false.
    pub fn supports_frame_instance_patches(&self) -> bool {
        self.device.supports_frame_instance_patches()
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
                instance_styles: encoded.instance_styles,
            },
        );
        Ok(self.handle(key, native))
    }

    /// Updates only per-frame instance transforms and tints on a retained
    /// packet. Packet topology, meshes, textures and the device revision stay
    /// untouched, so a moving object never re-uploads the static world.
    ///
    /// `patches` pairs the packet command ordinal with the new instance list.
    /// The list length must match the encoded instance range. The executor
    /// reapplies the stored pipeline/shadow flags and material tint.
    pub fn patch_frame_instances(
        &mut self,
        handle: PacketHandle,
        patches: &[(usize, &[Instance])],
    ) -> Result<()> {
        let mut prepared = Vec::with_capacity(patches.len());
        {
            let entry = self.entry(handle)?;
            for (command, instances) in patches {
                let range = entry
                    .instances
                    .get(command)
                    .cloned()
                    .ok_or(Error::Invalid("instance patch on an unknown command"))?;
                if range.len() != instances.len() {
                    return Err(Error::Invalid(
                        "instance patch length does not match the retained command",
                    ));
                }
                let style = entry
                    .instance_styles
                    .get(command)
                    .copied()
                    .ok_or(Error::Invalid("instance patch has no retained style"))?;
                let mut data = Vec::with_capacity(instances.len());
                for instance in instances.iter().copied() {
                    let mut instance = instance;
                    if !instance
                        .transform
                        .as_flattened()
                        .iter()
                        .chain(instance.tint.iter())
                        .all(|value| value.is_finite())
                    {
                        return Err(Error::Invalid("instance patch contains a non-finite value"));
                    }
                    instance.raster_kind = style.raster_kind;
                    encode::multiply(&mut instance.tint, style.tint);
                    data.push(instance);
                }
                prepared.push((range.start, data));
            }
        }
        // Commands usually append their instances in packet order, so several
        // moving objects sit in one contiguous range. Coalescing avoids one
        // staging upload and one executor wait per object on backends whose
        // patch path fences the previous submission.
        prepared.sort_unstable_by_key(|(first, _)| *first);
        let mut merged: Vec<(u32, Vec<Instance>)> = Vec::with_capacity(prepared.len());
        for (first, data) in prepared {
            if let Some((last_first, last_data)) = merged.last_mut() {
                let contiguous =
                    (*last_first as usize).saturating_add(last_data.len()) == first as usize;
                if contiguous {
                    last_data.extend_from_slice(&data);
                    continue;
                }
            }
            merged.push((first, data));
        }
        for (first, data) in &merged {
            self.device.patch(
                handle.native,
                ResourcePatch::Instances {
                    first: *first,
                    data: data.as_slice(),
                },
            )?;
        }
        Ok(())
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
        self.device
            .draw_product(handle.native, uniforms.raster(clear)?)
    }

    /// Draws ordered packets in one target acquisition and one submit.
    ///
    /// The first packet supplies the target clear, which is the scene packet
    /// when a scene layer exists and the UI packet otherwise. Every handle must
    /// belong to this device. Executors without layered support reject more
    /// than one handle.
    pub fn draw_layers(
        &mut self,
        handles: &[PacketHandle],
        uniforms: &FrameUniforms,
    ) -> Result<FrameMetrics> {
        let first = handles
            .first()
            .ok_or(Error::Invalid("layered draw needs at least one packet"))?;
        let clear = self.entry(*first)?.clear;
        let mut natives = Vec::with_capacity(handles.len());
        for handle in handles {
            natives.push(self.entry(*handle)?.native);
        }
        self.device
            .draw_product_layers(&natives, uniforms.raster(clear)?)
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
