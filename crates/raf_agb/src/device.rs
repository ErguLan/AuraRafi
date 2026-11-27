use std::fmt;
use std::rc::Rc;

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

/// Maximum number of local point/spot lights one frame may carry.
pub const MAX_LOCAL_LIGHTS: usize = 8;
/// Vec4 slots reserved for local lights. Each light owns four slots so every
/// authored field fits without compressing or reinterpreting channels.
pub const LOCAL_LIGHT_VECS: usize = 4 * MAX_LOCAL_LIGHTS;
/// Slot stride between one local light and the next.
pub const LOCAL_LIGHT_STRIDE: usize = 4;

#[derive(Clone, Copy, Debug)]
pub struct Frame {
    pub view_projection: [[f32; 4]; 4],
    pub eye: [f32; 3],
    pub clear: [f32; 4],
    /// Direction toward the light (xyz) and neutral ambient fraction (w).
    pub light: [f32; 4],
    /// RGBA light color. `w` carries color only; it is never a lighting flag.
    pub light_color: [f32; 4],
    pub environment: [f32; 4],
    /// Authored environment, improved technology, toon steps, toon softness.
    /// `Frame::uniforms` packs this verbatim into `RasterUniforms::lighting`.
    pub lighting_style: [f32; 4],
    /// Local lights packed in groups of four `vec4`s:
    /// `[position.xyz, range]`, `[color.rgb, intensity]`,
    /// `[direction.xyz, kind]` and
    /// `[inner_cos, outer_cos, falloff, 0]`.
    /// `kind` is 0 for point and 1 for spot. Unused slots stay zero.
    pub lights: [[f32; 4]; LOCAL_LIGHT_VECS],
    /// `x` is the number of active local lights in `0..=8`; `yzw` stay zero.
    pub light_counts: [f32; 4],
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
    /// Linear RGBA light color. No channel is reused for another control.
    pub light_color: [f32; 4],
    /// Pixel size of the scene viewport `[width, height, 0, 0]`. World-space
    /// line widths use `xy` so lines keep their authored pixel width inside the
    /// canvas rectangle.
    pub viewport: [f32; 4],
    /// World-space camera position `[x, y, z, 0]`.
    pub eye: [f32; 4],
    pub environment: [f32; 4],
    /// `[authored, improved, toon_steps, toon_softness]`. The two flags are 0 or
    /// 1, steps are in 0..=6 and softness in 0..=1. This field owns every
    /// lighting-style control so other vectors keep their original meaning.
    pub lighting: [f32; 4],
    /// Local lights, four `vec4`s per light in the layout documented on
    /// [`Frame::lights`]. Every slot is 16-byte aligned.
    pub lights: [[f32; 4]; LOCAL_LIGHT_VECS],
    /// `x` is the active local light count; the shader never reads an inactive
    /// slot, so a stopped light costs no shading work.
    pub light_counts: [f32; 4],
}

impl Frame {
    /// Packs the frame for every executor. `lighting` owns the style flags and
    /// toon controls; `viewport` and `eye` keep only their geometric meaning.
    pub fn uniforms(&self, extent: [u32; 2]) -> RasterUniforms {
        let [x, y, z] = self.eye;
        let scene = self.viewport_rect(extent);
        RasterUniforms {
            view_projection: self.view_projection,
            light: self.light,
            light_color: self.light_color,
            viewport: [scene.width, scene.height, 0., 0.],
            eye: [x, y, z, 0.],
            environment: self.environment,
            lighting: self.lighting_style,
            lights: self.lights,
            light_counts: self.light_counts,
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

/// One retained resident inside a layered composition.
///
/// Layers draw in slice order in a single target pass. The first layer owns the
/// target clear; the first scene layer owns the depth pass, the shadow casters
/// and the scene filter. Overlay layers draw after it and never clear. A backend
/// that does not support layers accepts exactly one layer.
pub struct Layer<'a, R> {
    pub resident: &'a R,
    pub scene: bool,
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
    /// True when the host should compose more than one resident through
    /// `draw_layers` for this device. Only executors with a real multi-layer
    /// target path return true; every other device keeps the single packet.
    fn supports_layers(&self) -> bool {
        false
    }
    /// True when the executor can patch existing instance/vertex/texture
    /// ranges without replacing the whole retained workload.
    fn supports_frame_instance_patches(&self) -> bool {
        false
    }
    /// Upper bound for GPU textures retained across packet replacement.
    /// Executors without a cross-packet texture cache ignore it.
    fn set_texture_cache_budget(&mut self, bytes: u64) {
        let _ = bytes;
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
    /// Draws ordered layers in one target acquisition.
    ///
    /// The default executor accepts a single layer and delegates to `draw`.
    /// Layered composition requires an executor that overrides this method and
    /// reports `supports_layers()`.
    fn draw_layers(
        &mut self,
        layers: &[Layer<'_, Self::Resident>],
        plan: &FramePlan,
        frame: &Frame,
    ) -> Result<FrameMetrics> {
        match layers {
            [layer] => self.draw(layer.resident, plan, frame),
            _ => Err(Error::Unsupported("layered composition")),
        }
    }
    fn wait(&mut self) -> Result<()>;
    fn capture(&mut self) -> Result<Vec<u8>>;
}

struct Resident<R> {
    native: Rc<R>,
    charge: LedgerAdmission,
    vertices: usize,
    instances: usize,
    textures: Vec<[u32; 2]>,
    /// True when the prepared streams contain a scene-group batch. A packet
    /// with only retained paint (for example a CPU canvas) is still a valid
    /// layer but never owns the scene depth pass.
    scene_batches: bool,
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

struct ProductEffects {
    side: u32,
    resident_shadow_bytes: u64,
    unsupported: bool,
    denied: bool,
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
    pub fn new(mut backend: B, budget: GraphicsMemoryBudget) -> Result<Self> {
        budget
            .validate()
            .map_err(|error| Error::Backend(error.to_string()))?;
        backend.set_texture_cache_budget(budget.gpu_bytes / 4);
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

    /// True when the active executor can draw ordered layers in one target
    /// acquisition. A false value keeps the single-packet composition path.
    pub fn supports_layers(&self) -> bool {
        self.backend.supports_layers()
    }

    /// True when the active executor accepts partial frame instance patches.
    pub fn supports_frame_instance_patches(&self) -> bool {
        self.backend.supports_frame_instance_patches()
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
        // Whole packets are retained after upload, so their one-shot staging
        // copy is bounded by the packet limit rather than by the much smaller
        // per-frame patch staging allowance.
        let packet_limit = self.budget.packet_upload_bytes();
        if bytes > available || bytes > packet_limit {
            return Err(Error::Budget {
                requested: bytes,
                available: available.min(packet_limit),
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
                    native: Rc::new(native),
                    charge,
                    vertices: workload.vertices.len(),
                    instances: workload.instances.len(),
                    textures: workload
                        .textures
                        .iter()
                        .map(|texture| texture.size)
                        .collect(),
                    scene_batches: workload.batches.iter().any(|batch| batch.mode.is_scene()),
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
        self.budget.packet_upload_bytes().min(
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
        // `upload` already submits and waits for the replacement to become
        // visible before returning. A second queue-idle wait here only adds a
        // full GPU stall to every retained packet revision.
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
        let native =
            Rc::get_mut(&mut resident.native).ok_or(Error::Invalid("resident packet is shared"))?;
        unsafe { self.backend.patch(native, patch) }?;
        self.patched_bytes += bytes;
        Ok(())
    }

    fn check_effects(&self, plan: &FramePlan, frame: &Frame) -> Result<()> {
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
        Ok(())
    }

    /// Commits the frame counters after the executor accepted the submit.
    fn commit_draw(&mut self, draw: Result<FrameMetrics>) -> Result<FrameMetrics> {
        let mut metrics = draw?;
        metrics.upload_bytes = metrics
            .upload_bytes
            .saturating_add(std::mem::take(&mut self.patched_bytes));
        self.frame = self.frame.saturating_add(1);
        self.ledger.begin_frame(self.frame);
        self.rendered_target = true;
        Ok(metrics)
    }

    pub fn draw(&mut self, handle: MeshHandle, frame: &Frame) -> Result<FrameMetrics> {
        let plan = self
            .plan
            .as_ref()
            .ok_or(Error::Invalid("configure an output before drawing"))?;
        self.check_effects(plan, frame)?;
        let resident = self
            .resources
            .get(handle, self.frame)
            .ok_or(Error::Invalid("stale workload handle"))?;
        let before = self.backend.extra_target_bytes();
        let draw = self.backend.draw(&*resident.native, plan, frame);
        let after = self.backend.extra_target_bytes();
        if before != after {
            self.ledger
                .sync_resident(GraphicsResourceFamily::Transient, plan.target_bytes + after);
        }
        self.commit_draw(draw)
    }

    /// Draws ordered retained packets in one target acquisition and one submit.
    ///
    /// The handles are scene first, then overlay layers. The first resident
    /// owns the target clear; the executor is responsible for drawing the rest
    /// without clearing. An executor that does not support layers rejects more
    /// than one handle with `Error::Unsupported`.
    pub fn draw_layers(&mut self, handles: &[MeshHandle], frame: &Frame) -> Result<FrameMetrics> {
        if handles.is_empty() {
            return Err(Error::Invalid("layered draw needs at least one packet"));
        }
        let plan = self
            .plan
            .as_ref()
            .ok_or(Error::Invalid("configure an output before drawing"))?;
        self.check_effects(plan, frame)?;
        // `MeshRegistry::get` takes `&mut self` so it can stamp the frame touch,
        // so several residents cannot be borrowed at once. Clone their `Rc`
        // handles first and lend stable references to the executor.
        let frame_index = self.frame;
        let mut residents = Vec::with_capacity(handles.len());
        for handle in handles {
            let resident = self
                .resources
                .get(*handle, frame_index)
                .ok_or(Error::Invalid("stale workload handle"))?;
            residents.push((Rc::clone(&resident.native), resident.scene_batches));
        }
        let layers: Vec<Layer<'_, B::Resident>> = residents
            .iter()
            .map(|(native, scene)| Layer {
                resident: &**native,
                scene: *scene,
            })
            .collect();
        let before = self.backend.extra_target_bytes();
        let draw = self.backend.draw_layers(&layers, plan, frame);
        let after = self.backend.extra_target_bytes();
        if before != after {
            self.ledger
                .sync_resident(GraphicsResourceFamily::Transient, plan.target_bytes + after);
        }
        self.commit_draw(draw)
    }

    /// Removes quality effects the product frame may omit. The metrics keep the
    /// reason so a frame never pretends the effect ran.
    fn apply_product_effects(&self, plan: &FramePlan, frame: &mut Frame) -> ProductEffects {
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
        ProductEffects {
            side,
            resident_shadow_bytes,
            unsupported,
            denied,
        }
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
        let effects = self.apply_product_effects(plan, &mut frame);
        let mut metrics = self.draw(handle, &frame)?;
        metrics.shadow_resolution = if effects.resident_shadow_bytes == 0 {
            0
        } else {
            effects.side
        };
        metrics.shadow_resident_bytes = effects.resident_shadow_bytes;
        metrics.shadows_unsupported = effects.unsupported;
        metrics.shadow_budget_denied = effects.denied;
        Ok(metrics)
    }

    /// Layered product draw. The first packet's clear becomes the single target
    /// clear; every other packet overlays it without clearing.
    pub(crate) fn draw_product_layers(
        &mut self,
        handles: &[MeshHandle],
        mut frame: Frame,
    ) -> Result<FrameMetrics> {
        let frame_index = self.frame;
        let has_scene = handles.iter().any(|handle| {
            self.resources
                .get(*handle, frame_index)
                .is_some_and(|resident| resident.scene_batches)
        });
        if !has_scene {
            // Nothing to depth-test, shadow or spatially filter: a UI-only
            // composition keeps the clear and paints the overlays.
            frame.shadow = None;
            frame.scene_filter = false;
        }
        let plan = self
            .plan
            .as_ref()
            .ok_or(Error::Invalid("configure an output before drawing"))?;
        let effects = self.apply_product_effects(plan, &mut frame);
        let mut metrics = self.draw_layers(handles, &frame)?;
        metrics.shadow_resolution = if effects.resident_shadow_bytes == 0 {
            0
        } else {
            effects.side
        };
        metrics.shadow_resident_bytes = effects.resident_shadow_bytes;
        metrics.shadows_unsupported = effects.unsupported;
        metrics.shadow_budget_denied = effects.denied;
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
