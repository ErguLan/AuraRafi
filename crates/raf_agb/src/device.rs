use std::fmt;

use crate::capabilities::GraphicsMemoryBudget;
use crate::frame_graph::{
    CompiledFrameGraph, FrameGraph, GraphTextureDesc, GraphTextureFormat, PassDesc, PassKind,
    ResourceAccess, ResourceState, TextureUsage,
};
use crate::handles::MeshHandle;
use crate::memory_ledger::{
    GraphicsLedgerSnapshot, GraphicsResourceFamily, GraphicsResourceLedger, LedgerAdmission,
};
use crate::resource_registry::{MeshRegistry, ResourceAdmission};
use crate::{ResourcePatch, Workload};

mod viewport;

pub(crate) use viewport::{validate_viewport, ViewportRect};

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug)]
pub enum Error {
    Invalid(&'static str),
    Unsupported(&'static str),
    Budget { requested: u64, available: u64 },
    Backend(String),
    SurfaceOutdated,
    SurfaceLost,
    DeviceLost,
    Busy,
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Invalid(reason) => f.write_str(reason),
            Self::Unsupported(feature) => write!(f, "unsupported graphics capability: {feature}"),
            Self::Budget {
                requested,
                available,
            } => write!(
                f,
                "graphics budget: {requested} bytes requested, {available} available"
            ),
            Self::Backend(reason) => f.write_str(reason),
            Self::SurfaceOutdated => f.write_str("graphics surface must be reconfigured"),
            Self::SurfaceLost => f.write_str("graphics surface lost"),
            Self::DeviceLost => f.write_str("graphics device lost"),
            Self::Busy => f.write_str("graphics queue is busy"),
        }
    }
}

impl std::error::Error for Error {}

#[derive(Clone, Debug, serde::Serialize)]
pub struct AdapterInfo {
    pub backend: &'static str,
    pub name: String,
    pub driver: String,
    pub hardware: bool,
    pub max_texture: u32,
    pub max_buffer: u64,
    pub gpu_timestamps: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Output {
    Capture,
    Present,
}

pub struct FramePlan {
    pub extent: [u32; 2],
    pub output: Output,
    pub graph: CompiledFrameGraph,
    pub target_bytes: u64,
}

impl FramePlan {
    pub fn new(extent: [u32; 2], output: Output, budget: u64) -> Result<Self> {
        let [w, h] = extent;
        if w == 0 || h == 0 {
            return Err(Error::Invalid("zero-sized target"));
        }
        let color = GraphTextureDesc::new(w, h, GraphTextureFormat::Rgba8Unorm)
            .with_usage(TextureUsage::RENDER_ATTACHMENT.union(TextureUsage::COPY_SRC));
        let depth = GraphTextureDesc::new(w, h, GraphTextureFormat::Depth32Float)
            .with_usage(TextureUsage::RENDER_ATTACHMENT);
        let mut graph = FrameGraph::new(budget);
        let color_id = graph.import_texture(color).map_err(graph_error)?;
        let depth_id = graph.create_texture(depth).map_err(graph_error)?;
        graph
            .add_pass(
                PassDesc::new("scene", PassKind::Graphics)
                    .write(color_id, ResourceState::ColorAttachment)
                    .write(depth_id, ResourceState::DepthAttachment),
            )
            .map_err(graph_error)?;
        graph
            .add_pass(
                PassDesc::new("overlays", PassKind::Graphics)
                    .write(color_id, ResourceState::ColorAttachment),
            )
            .map_err(graph_error)?;
        let (name, kind, state) = match output {
            Output::Capture => ("capture", PassKind::Copy, ResourceState::CopySrc),
            Output::Present => ("present", PassKind::Present, ResourceState::Present),
        };
        graph
            .add_pass(PassDesc::new(name, kind).access(ResourceAccess {
                resource: color_id,
                state,
            }))
            .map_err(graph_error)?;
        let graph = graph.compile().map_err(graph_error)?;
        let target_bytes = graph.transient_bytes.saturating_add(graph.imported_bytes);
        if target_bytes > budget {
            return Err(Error::Budget {
                requested: target_bytes,
                available: budget,
            });
        }
        Ok(Self {
            extent,
            output,
            graph,
            target_bytes,
        })
    }
}

fn graph_error(error: crate::frame_graph::FrameGraphError) -> Error {
    Error::Backend(error.to_string())
}

#[derive(Clone, Copy, Debug)]
pub struct Frame {
    pub view_projection: [[f32; 4]; 4],
    pub eye: [f32; 3],
    pub clear: [f32; 4],
    /// Direction toward the light (xyz) and neutral ambient fraction (w).
    pub light: [f32; 4],
    pub light_color: [f32; 4],
    pub environment: [f32; 4],
    /// Authored environment, improved technology, toon steps, toon softness.
    pub lighting_style: [f32; 4],
    /// Top-left normalized scene viewport `[x, y, width, height]` inside the
    /// full target. Overlay batches always cover the whole target.
    pub viewport: [f32; 4],
    pub shadow: Option<ShadowFrame>,
    /// Effective map side and a key for its light projection and casters.
    pub shadow_map: Option<(u32, u64)>,
    pub scene_filter: bool,
}

pub(crate) const SHADOW_MAP_SIDE: u32 = 1024;
pub(crate) fn shadow_map_bytes(side: u32) -> u64 {
    u64::from(side) * u64::from(side) * 4
}

#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, bytemuck::Pod, bytemuck::Zeroable)]
pub struct ShadowFrame {
    pub light_view_projection: [[f32; 4]; 4],
    /// x stores the comparison bias in normalized light depth coordinates.
    pub parameters: [f32; 4],
    /// x is the normalized light depth spanned by the filter footprint.
    pub sampling: [f32; 4],
}

#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, bytemuck::Pod, bytemuck::Zeroable)]
pub struct RasterUniforms {
    pub view_projection: [[f32; 4]; 4],
    pub light: [f32; 4],
    pub light_color: [f32; 4],
    /// Pixel size of the scene viewport. World-space line widths use it so
    /// lines keep their authored pixel width inside the canvas rectangle.
    pub viewport: [f32; 4],
    pub eye: [f32; 4],
    pub environment: [f32; 4],
}

impl Frame {
    pub fn uniforms(&self, extent: [u32; 2]) -> RasterUniforms {
        let [x, y, z] = self.eye;
        let scene = self.viewport_rect(extent);
        RasterUniforms {
            view_projection: self.view_projection,
            light: self.light,
            light_color: [
                self.light_color[0],
                self.light_color[1],
                self.light_color[2],
                self.lighting_style[2],
            ],
            viewport: [
                scene.width,
                scene.height,
                self.lighting_style[0],
                self.lighting_style[1],
            ],
            eye: [x, y, z, self.lighting_style[3]],
            environment: self.environment,
        }
    }

    /// Scene rectangle in physical pixels for the given target extent.
    pub(crate) fn viewport_rect(&self, extent: [u32; 2]) -> ViewportRect {
        ViewportRect::from_normalized(self.viewport, extent)
    }
}

#[derive(Clone, Copy, Debug, Default, serde::Serialize)]
pub struct FrameMetrics {
    pub draw_calls: u32,
    pub submits: u32,
    pub upload_bytes: u64,
    pub gpu_ms: Option<f64>,
    pub shadow_draw_calls: u32,
    pub shadow_resolution: u32,
    pub shadow_resident_bytes: u64,
    pub shadows_unsupported: bool,
    pub shadow_budget_denied: bool,
}

/// Backend objects stay private to the selected executor. A prepared resource
/// owns its native allocation and must remain valid until its last submit ends.
pub trait Backend {
    type Resident;

    fn info(&self) -> &AdapterInfo;
    fn supports_shadows(&self) -> bool {
        false
    }
    fn shadow_bytes(&self) -> u64 {
        0
    }
    fn supports_scene_filter(&self) -> bool {
        false
    }
    fn filter_bytes(&self) -> u64 {
        0
    }
    fn extra_target_bytes(&self) -> u64 {
        self.shadow_bytes() + self.filter_bytes()
    }
    fn configure(&mut self, plan: &FramePlan) -> Result<()>;
    fn upload(&mut self, workload: &Workload) -> Result<Self::Resident>;
    fn patch_row_alignment(&self) -> u64 {
        1
    }
    /// # Safety
    /// The resident must belong to this executor. The patch's ranges and byte
    /// layout must have been validated against the original allocation.
    unsafe fn patch(
        &mut self,
        _resident: &mut Self::Resident,
        _patch: ResourcePatch<'_>,
    ) -> Result<()> {
        Err(Error::Unsupported(
            "partial resource uploads on this executor",
        ))
    }
    fn draw(
        &mut self,
        resident: &Self::Resident,
        plan: &FramePlan,
        frame: &Frame,
    ) -> Result<FrameMetrics>;
    fn wait(&mut self) -> Result<()>;
    fn capture(&mut self) -> Result<Vec<u8>>;
}

struct Resident<R> {
    native: R,
    charge: LedgerAdmission,
    vertices: usize,
    instances: usize,
    textures: Vec<[u32; 2]>,
}

impl<R> Resident<R> {
    fn check_patch(&self, patch: ResourcePatch<'_>) -> Result<()> {
        let range_fits = |first: u32, count: usize, limit: usize| {
            count != 0
                && (first as usize)
                    .checked_add(count)
                    .is_some_and(|end| end <= limit)
        };
        let valid = match patch {
            ResourcePatch::Vertices { first, data } => range_fits(first, data.len(), self.vertices),
            ResourcePatch::Instances { first, data } => {
                range_fits(first, data.len(), self.instances)
            }
            ResourcePatch::Texture {
                index,
                origin,
                size,
                rgba,
            } => self.textures.get(index as usize).is_some_and(|limit| {
                size.into_iter().enumerate().all(|(axis, dim)| {
                    dim != 0
                        && origin[axis]
                            .checked_add(dim)
                            .is_some_and(|end| end <= limit[axis])
                }) && rgba.len() % 4 == 0
                    && u64::from(size[0]) * u64::from(size[1]) == (rgba.len() / 4) as u64
            }),
        };
        if !valid {
            return Err(Error::Invalid("resource patch exceeds resident bounds"));
        }
        Ok(())
    }
}

pub struct Device<B: Backend> {
    resources: MeshRegistry<Resident<B::Resident>>,
    backend: B,
    ledger: GraphicsResourceLedger,
    budget: GraphicsMemoryBudget,
    plan: Option<FramePlan>,
    frame: u64,
    rendered_target: bool,
    patched_bytes: u64,
}

impl<B: Backend> Device<B> {
    pub fn new(backend: B, budget: GraphicsMemoryBudget) -> Result<Self> {
        budget
            .validate()
            .map_err(|error| Error::Backend(error.to_string()))?;
        let mut ledger = GraphicsResourceLedger::new(budget.gpu_bytes);
        ledger.set_family_budget(GraphicsResourceFamily::Mesh, budget.gpu_bytes);
        ledger.set_family_budget(GraphicsResourceFamily::Transient, budget.gpu_bytes);
        Ok(Self {
            backend,
            budget,
            resources: MeshRegistry::new(budget.gpu_bytes),
            ledger,
            plan: None,
            frame: 0,
            rendered_target: false,
            patched_bytes: 0,
        })
    }

    pub fn info(&self) -> &AdapterInfo {
        self.backend.info()
    }

    pub fn supports_shadows(&self) -> bool {
        self.backend.supports_shadows()
    }

    /// The target must be configured before querying its filter support.
    pub fn supports_scene_filter(&self) -> bool {
        self.backend.supports_scene_filter()
    }

    pub fn ledger(&self) -> GraphicsLedgerSnapshot {
        self.ledger.snapshot()
    }

    pub fn configure(&mut self, extent: [u32; 2], output: Output) -> Result<()> {
        if extent.into_iter().any(|dim| dim > self.info().max_texture) {
            return Err(Error::Invalid("target exceeds adapter limit"));
        }
        let available = self
            .budget
            .gpu_bytes
            .saturating_sub(self.backend.extra_target_bytes())
            .saturating_sub(self.resources.metrics().resident_bytes);
        let plan = FramePlan::new(extent, output, available)?;
        self.backend.configure(&plan)?;
        self.ledger.sync_resident(
            GraphicsResourceFamily::Transient,
            plan.target_bytes + self.backend.extra_target_bytes(),
        );
        self.plan = Some(plan);
        self.rendered_target = false;
        Ok(())
    }

    pub fn upload(&mut self, workload: &Workload) -> Result<MeshHandle> {
        workload.validate(self.info().max_texture, self.info().max_buffer)?;
        let bytes = workload.bytes();
        let target_bytes = self.plan.as_ref().map_or(0, |plan| plan.target_bytes);
        let available = self
            .budget
            .gpu_bytes
            .saturating_sub(self.backend.extra_target_bytes())
            .saturating_sub(target_bytes)
            .saturating_sub(self.resources.metrics().resident_bytes);
        if bytes > available || bytes > self.budget.staging_bytes {
            return Err(Error::Budget {
                requested: bytes,
                available: available.min(self.budget.staging_bytes),
            });
        }
        let charge = self
            .ledger
            .try_admit(GraphicsResourceFamily::Mesh, bytes, true)
            .map_err(|error| Error::Backend(error.to_string()))?;
        let native = match self.backend.upload(workload) {
            Ok(native) => native,
            Err(error) => {
                self.ledger.release(charge);
                return Err(error);
            }
        };
        self.resources
            .insert(
                Resident {
                    native,
                    charge,
                    vertices: workload.vertices.len(),
                    instances: workload.instances.len(),
                    textures: workload
                        .textures
                        .iter()
                        .map(|texture| texture.size)
                        .collect(),
                },
                ResourceAdmission::pinned(bytes, self.frame),
            )
            .map_err(|_| {
                self.ledger.release(charge);
                Error::Budget {
                    requested: bytes,
                    available,
                }
            })
    }

    pub(crate) fn workload_budget(&self) -> u64 {
        self.budget.staging_bytes.min(
            self.budget
                .gpu_bytes
                .saturating_sub(self.backend.extra_target_bytes())
                .saturating_sub(self.plan.as_ref().map_or(0, |plan| plan.target_bytes))
                .saturating_sub(self.resources.metrics().resident_bytes),
        )
    }

    pub fn release(&mut self, handle: MeshHandle) -> Result<()> {
        if !self.resources.contains(handle) {
            return Err(Error::Invalid("stale workload handle"));
        }
        self.backend.wait()?;
        self.retire(handle)
    }

    pub(crate) fn replace(
        &mut self,
        previous: MeshHandle,
        workload: &Workload,
    ) -> Result<MeshHandle> {
        if !self.resources.contains(previous) {
            return Err(Error::Invalid("stale workload handle"));
        }
        self.backend.wait()?;
        let replacement = self.upload(workload)?;
        self.retire(previous)?;
        Ok(replacement)
    }

    fn retire(&mut self, handle: MeshHandle) -> Result<()> {
        let resident = self
            .resources
            .remove(handle)
            .ok_or(Error::Invalid("stale workload handle"))?;
        self.ledger.release(resident.charge);
        Ok(())
    }

    pub(crate) fn patch(&mut self, handle: MeshHandle, patch: ResourcePatch<'_>) -> Result<()> {
        let resident = self
            .resources
            .get_mut(handle, self.frame)
            .ok_or(Error::Invalid("stale workload handle"))?;
        resident.check_patch(patch)?;
        let bytes = patch.bytes();
        let staging = patch.staging_bytes(self.backend.patch_row_alignment().max(1));
        if staging > self.budget.staging_bytes {
            return Err(Error::Budget {
                requested: staging,
                available: self.budget.staging_bytes,
            });
        }
        let available = self
            .budget
            .frame_upload_bytes
            .saturating_sub(self.patched_bytes);
        if bytes > available {
            return Err(Error::Budget {
                requested: bytes,
                available,
            });
        }
        // The registry owns the allocation and check_patch validated the ranges.
        unsafe { self.backend.patch(&mut resident.native, patch) }?;
        self.patched_bytes += bytes;
        Ok(())
    }

    pub fn draw(&mut self, handle: MeshHandle, frame: &Frame) -> Result<FrameMetrics> {
        let plan = self
            .plan
            .as_ref()
            .ok_or(Error::Invalid("configure an output before drawing"))?;
        let mut effect_bytes = 0;
        if frame.shadow.is_some() {
            if !self.backend.supports_shadows() {
                return Err(Error::Unsupported("shadow map on this executor"));
            }
            let side = frame.shadow_map.map_or(SHADOW_MAP_SIDE, |(side, _)| side);
            if ![256, 512, 1024].contains(&side) || side > self.backend.info().max_texture {
                return Err(Error::Invalid("invalid shadow map resolution"));
            }
            effect_bytes += shadow_map_bytes(side);
        }
        if frame.scene_filter {
            if !self.backend.supports_scene_filter() {
                return Err(Error::Unsupported("scene filter on this executor"));
            }
            effect_bytes += filter_bytes(plan.extent);
        }
        let available = self
            .budget
            .gpu_bytes
            .saturating_sub(plan.target_bytes)
            .saturating_sub(self.resources.metrics().resident_bytes);
        if effect_bytes > available {
            return Err(Error::Budget {
                requested: effect_bytes,
                available,
            });
        }
        let resident = self
            .resources
            .get(handle, self.frame)
            .ok_or(Error::Invalid("stale workload handle"))?;
        let before = self.backend.extra_target_bytes();
        let draw = self.backend.draw(&resident.native, plan, frame);
        let after = self.backend.extra_target_bytes();
        if before != after {
            self.ledger
                .sync_resident(GraphicsResourceFamily::Transient, plan.target_bytes + after);
        }
        let mut metrics = draw?;
        metrics.upload_bytes = metrics
            .upload_bytes
            .saturating_add(std::mem::take(&mut self.patched_bytes));
        self.frame = self.frame.saturating_add(1);
        self.ledger.begin_frame(self.frame);
        self.rendered_target = true;
        Ok(metrics)
    }

    /// A product frame may omit a quality effect while still presenting its scene.
    pub(crate) fn draw_product(
        &mut self,
        handle: MeshHandle,
        mut frame: Frame,
    ) -> Result<FrameMetrics> {
        let plan = self
            .plan
            .as_ref()
            .ok_or(Error::Invalid("configure an output before drawing"))?;
        let available = self
            .budget
            .gpu_bytes
            .saturating_sub(plan.target_bytes)
            .saturating_sub(self.resources.metrics().resident_bytes);
        let side = frame.shadow_map.map_or(SHADOW_MAP_SIDE, |(side, _)| side);
        let requested = frame.shadow.is_some();
        let unsupported = requested && (!self.supports_shadows() || side > self.info().max_texture);
        let denied = requested && !unsupported && shadow_map_bytes(side) > available;
        if unsupported || denied {
            frame.shadow = None;
        }
        let resident_shadow_bytes = if frame.shadow.is_some() {
            shadow_map_bytes(side)
        } else {
            0
        };
        if frame.scene_filter
            && (!self.supports_scene_filter()
                || filter_bytes(plan.extent) > available.saturating_sub(resident_shadow_bytes))
        {
            frame.scene_filter = false;
        }
        let mut metrics = self.draw(handle, &frame)?;
        metrics.shadow_resolution = if resident_shadow_bytes == 0 { 0 } else { side };
        metrics.shadow_resident_bytes = resident_shadow_bytes;
        metrics.shadows_unsupported = unsupported;
        metrics.shadow_budget_denied = denied;
        Ok(metrics)
    }

    pub fn wait(&mut self) -> Result<()> {
        self.backend.wait()
    }

    pub fn capture(&mut self) -> Result<Vec<u8>> {
        if !self
            .plan
            .as_ref()
            .is_some_and(|plan| plan.output == Output::Capture)
            || !self.rendered_target
        {
            return Err(Error::Invalid("draw to a capture target first"));
        }
        self.backend.capture()
    }
}

pub(crate) fn filter_bytes([width, height]: [u32; 2]) -> u64 {
    (u64::from(width) * u64::from(height)).saturating_mul(4)
}

impl<B: Backend> Drop for Device<B> {
    fn drop(&mut self) {
        let _ = self.backend.wait();
    }
}

#[cfg(test)]
mod tests;
