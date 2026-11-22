//! Backend-neutral frame graph for ApiGraphicBasic.
//!
//! The frame graph is the scheduling contract between scene, shadow, post and
//! presentation work. It owns resource lifetimes, pass ordering, barriers and
//! transient aliasing without importing a backend API. A backend consumes a
//! [`CompiledFrameGraph`] and maps the neutral resource states to its own
//! barriers and attachments.
//!
//! Design rules:
//! - Imported resources live outside the graph and are never aliased.
//! - Transient resources are declared once per frame and are aliased only when
//!   format, usage and dimensions are compatible.
//! - Every read must have a producer in the same graph or be imported.
//! - Cycles are rejected; the compiler never guesses an order.
//! - All limits are explicit and bounded so a malformed graph cannot exhaust
//!   memory or wedge the renderer.

use std::collections::{BTreeMap, VecDeque};

pub type ResourceId = u32;
pub type PassId = u32;

/// Neutral texture formats used by engine-owned render targets.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum GraphTextureFormat {
    Rgba8Unorm,
    Rgba8UnormSrgb,
    Bgra8Unorm,
    Bgra8UnormSrgb,
    Rgba16Float,
    R16Float,
    Depth32Float,
    Depth24PlusStencil8,
}

impl GraphTextureFormat {
    /// Bytes per pixel used for budgeting and alias compatibility.
    pub const fn bytes_per_pixel(self) -> u32 {
        match self {
            Self::Rgba8Unorm | Self::Rgba8UnormSrgb | Self::Bgra8Unorm | Self::Bgra8UnormSrgb => 4,
            Self::Rgba16Float => 8,
            Self::R16Float => 2,
            Self::Depth32Float => 4,
            Self::Depth24PlusStencil8 => 4,
        }
    }

    pub const fn is_depth(self) -> bool {
        matches!(self, Self::Depth32Float | Self::Depth24PlusStencil8)
    }
}

/// Neutral resource state. The backend maps it to its own barrier vocabulary.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResourceState {
    Undefined,
    ColorAttachment,
    DepthAttachment,
    ShaderRead,
    ShaderWrite,
    CopySrc,
    CopyDst,
    Present,
}

/// Texture usage bits declared by the graph.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct TextureUsage(u8);

impl TextureUsage {
    pub const RENDER_ATTACHMENT: Self = Self(1 << 0);
    pub const TEXTURE_BINDING: Self = Self(1 << 1);
    pub const COPY_SRC: Self = Self(1 << 2);
    pub const COPY_DST: Self = Self(1 << 3);
    pub const STORAGE: Self = Self(1 << 4);

    pub const fn empty() -> Self {
        Self(0)
    }

    pub const fn union(self, other: Self) -> Self {
        Self(self.0 | other.0)
    }

    pub const fn contains(self, other: Self) -> bool {
        self.0 & other.0 == other.0
    }
}

/// Texture description owned by the graph.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GraphTextureDesc {
    pub width: u32,
    pub height: u32,
    pub depth_or_layers: u32,
    pub mip_levels: u32,
    pub sample_count: u32,
    pub format: GraphTextureFormat,
    pub usage: TextureUsage,
}

impl GraphTextureDesc {
    pub const fn new(width: u32, height: u32, format: GraphTextureFormat) -> Self {
        Self {
            width,
            height,
            depth_or_layers: 1,
            mip_levels: 1,
            sample_count: 1,
            format,
            usage: TextureUsage::empty(),
        }
    }

    pub const fn with_usage(mut self, usage: TextureUsage) -> Self {
        self.usage = usage;
        self
    }

    pub const fn with_layers(mut self, layers: u32) -> Self {
        self.depth_or_layers = layers;
        self
    }

    pub const fn with_samples(mut self, samples: u32) -> Self {
        self.sample_count = samples;
        self
    }

    /// Resident bytes for the base level plus mip chain. Conservative: every
    /// mip level keeps the full layer count even when the backend may optimize.
    pub fn estimated_bytes(self) -> u64 {
        let mut width = self.width.max(1) as u64;
        let mut height = self.height.max(1) as u64;
        let layers = self.depth_or_layers.max(1) as u64;
        let mut total = 0u64;
        for _ in 0..self.mip_levels.max(1) {
            total = total.saturating_add(
                width
                    .saturating_mul(height)
                    .saturating_mul(layers)
                    .saturating_mul(self.format.bytes_per_pixel() as u64),
            );
            width = (width / 2).max(1);
            height = (height / 2).max(1);
        }
        total
    }

    fn alias_compatible(self, other: Self) -> bool {
        self.format == other.format
            && self.sample_count == other.sample_count
            && self.width >= other.width
            && self.height >= other.height
            && self.depth_or_layers >= other.depth_or_layers
            && self.mip_levels >= other.mip_levels
            && self.usage.contains(other.usage)
    }

    fn validate(&self) -> Result<(), FrameGraphError> {
        if self.width == 0 || self.height == 0 || self.depth_or_layers == 0 {
            return Err(FrameGraphError::InvalidTexture(
                "texture dimensions must be non-zero",
            ));
        }
        if self.width > 16_384 || self.height > 16_384 || self.depth_or_layers > 2_048 {
            return Err(FrameGraphError::InvalidTexture(
                "texture dimensions exceed the engine limit",
            ));
        }
        if self.mip_levels == 0 || self.mip_levels > 16 {
            return Err(FrameGraphError::InvalidTexture(
                "mip level count must be within 1..=16",
            ));
        }
        if !matches!(self.sample_count, 1 | 2 | 4 | 8) {
            return Err(FrameGraphError::InvalidTexture(
                "sample count must be 1, 2, 4 or 8",
            ));
        }
        if self.usage == TextureUsage::empty() {
            return Err(FrameGraphError::InvalidTexture(
                "texture usage must not be empty",
            ));
        }
        Ok(())
    }
}

/// One resource access inside a pass.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ResourceAccess {
    pub resource: ResourceId,
    pub state: ResourceState,
}

impl ResourceAccess {
    pub const fn read(resource: ResourceId) -> Self {
        Self {
            resource,
            state: ResourceState::ShaderRead,
        }
    }

    pub const fn write(resource: ResourceId, state: ResourceState) -> Self {
        Self { resource, state }
    }
}

/// Pass category. It only affects validation and diagnostics.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PassKind {
    Graphics,
    Compute,
    Copy,
    Present,
}

#[derive(Debug, Clone)]
pub struct PassDesc {
    pub name: &'static str,
    pub kind: PassKind,
    pub accesses: Vec<ResourceAccess>,
}

impl PassDesc {
    pub fn new(name: &'static str, kind: PassKind) -> Self {
        Self {
            name,
            kind,
            accesses: Vec::new(),
        }
    }

    pub fn read(mut self, resource: ResourceId) -> Self {
        self.accesses.push(ResourceAccess::read(resource));
        self
    }

    pub fn write(mut self, resource: ResourceId, state: ResourceState) -> Self {
        self.accesses.push(ResourceAccess::write(resource, state));
        self
    }

    pub fn access(mut self, access: ResourceAccess) -> Self {
        self.accesses.push(access);
        self
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ResourceOrigin {
    Imported,
    Transient,
}

#[derive(Debug, Clone)]
struct GraphResource {
    desc: GraphTextureDesc,
    origin: ResourceOrigin,
}

/// A compiled barrier request. `before_pass` is the pass that needs the new
/// state; the backend inserts the barrier immediately before it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GraphBarrier {
    pub before_pass: PassId,
    pub resource: ResourceId,
    pub from: ResourceState,
    pub to: ResourceState,
}

/// A transient allocation slot. Resources in the same slot may share memory
/// because their live ranges do not overlap.
#[derive(Debug, Clone)]
pub struct AliasSlot {
    pub slot: u32,
    pub desc: GraphTextureDesc,
    pub bytes: u64,
    pub resources: Vec<ResourceId>,
}

/// Result of a successful compile.
#[derive(Debug, Clone, Default)]
pub struct CompiledFrameGraph {
    pub order: Vec<PassId>,
    pub barriers: Vec<GraphBarrier>,
    pub slots: Vec<AliasSlot>,
    pub transient_bytes: u64,
    pub imported_bytes: u64,
    pub peak_transient_bytes: u64,
}

impl CompiledFrameGraph {
    pub fn clear(&mut self) {
        self.order.clear();
        self.barriers.clear();
        self.slots.clear();
        self.transient_bytes = 0;
        self.imported_bytes = 0;
        self.peak_transient_bytes = 0;
    }

    pub fn pass_count(&self) -> usize {
        self.order.len()
    }

    pub fn barrier_count(&self) -> usize {
        self.barriers.len()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FrameGraphError {
    TooManyResources,
    TooManyPasses,
    TooManyAccesses(PassId),
    InvalidTexture(&'static str),
    UnknownResource(ResourceId),
    DuplicateWrite { pass: PassId, resource: ResourceId },
    MissingProducer { pass: PassId, resource: ResourceId },
    Cycle,
    TransientWithoutWriter(ResourceId),
    UnusedTransient(ResourceId),
    BudgetExceeded { requested: u64, budget: u64 },
}

impl std::fmt::Display for FrameGraphError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::TooManyResources => write!(f, "frame graph resource limit exceeded"),
            Self::TooManyPasses => write!(f, "frame graph pass limit exceeded"),
            Self::TooManyAccesses(pass) => {
                write!(f, "pass {pass} exceeds the per-pass access limit")
            }
            Self::InvalidTexture(reason) => write!(f, "invalid texture: {reason}"),
            Self::UnknownResource(id) => write!(f, "unknown resource {id}"),
            Self::DuplicateWrite { pass, resource } => {
                write!(f, "pass {pass} writes resource {resource} more than once")
            }
            Self::MissingProducer { pass, resource } => write!(
                f,
                "pass {pass} reads resource {resource} before any pass writes it"
            ),
            Self::Cycle => write!(f, "frame graph contains a dependency cycle"),
            Self::TransientWithoutWriter(id) => {
                write!(f, "transient resource {id} has no writer")
            }
            Self::UnusedTransient(id) => write!(f, "transient resource {id} is never used"),
            Self::BudgetExceeded { requested, budget } => write!(
                f,
                "transient memory {requested} bytes exceeds the {budget} byte budget"
            ),
        }
    }
}

impl std::error::Error for FrameGraphError {}

/// Builder used once per frame. It is intentionally not `Clone` and not
/// thread-safe: one graph belongs to one render thread.
pub struct FrameGraph {
    resources: Vec<GraphResource>,
    passes: Vec<PassDesc>,
    budget_bytes: u64,
}

const MAX_RESOURCES: usize = 4_096;
const MAX_PASSES: usize = 1_024;
const MAX_ACCESSES_PER_PASS: usize = 128;
const DEFAULT_BUDGET_BYTES: u64 = 256 * 1024 * 1024;

impl Default for FrameGraph {
    fn default() -> Self {
        Self::new(DEFAULT_BUDGET_BYTES)
    }
}

impl FrameGraph {
    pub fn new(budget_bytes: u64) -> Self {
        Self {
            resources: Vec::new(),
            passes: Vec::new(),
            budget_bytes: budget_bytes.max(1),
        }
    }

    pub fn budget_bytes(&self) -> u64 {
        self.budget_bytes
    }

    pub fn set_budget_bytes(&mut self, budget_bytes: u64) {
        self.budget_bytes = budget_bytes.max(1);
    }

    pub fn reset(&mut self) {
        self.resources.clear();
        self.passes.clear();
    }

    /// Import a texture owned outside the graph (swapchain, persistent scene
    /// target, editor canvas). Imported resources are never aliased.
    pub fn import_texture(
        &mut self,
        desc: GraphTextureDesc,
    ) -> Result<ResourceId, FrameGraphError> {
        self.push_resource(desc, ResourceOrigin::Imported)
    }

    /// Declare a texture that lives only for this frame.
    pub fn create_texture(
        &mut self,
        desc: GraphTextureDesc,
    ) -> Result<ResourceId, FrameGraphError> {
        self.push_resource(desc, ResourceOrigin::Transient)
    }

    pub fn add_pass(&mut self, pass: PassDesc) -> Result<PassId, FrameGraphError> {
        if self.passes.len() >= MAX_PASSES {
            return Err(FrameGraphError::TooManyPasses);
        }
        let id = self.passes.len() as PassId;
        if pass.accesses.len() > MAX_ACCESSES_PER_PASS {
            return Err(FrameGraphError::TooManyAccesses(id));
        }
        self.passes.push(pass);
        Ok(id)
    }

    fn push_resource(
        &mut self,
        desc: GraphTextureDesc,
        origin: ResourceOrigin,
    ) -> Result<ResourceId, FrameGraphError> {
        if self.resources.len() >= MAX_RESOURCES {
            return Err(FrameGraphError::TooManyResources);
        }
        desc.validate()?;
        let id = self.resources.len() as ResourceId;
        self.resources.push(GraphResource { desc, origin });
        Ok(id)
    }

    /// Validate, order and alias the graph. Allocation is reused when a
    /// scratch buffer is provided, which keeps steady-state frames allocation
    /// free.
    pub fn compile(&self) -> Result<CompiledFrameGraph, FrameGraphError> {
        let mut out = CompiledFrameGraph::default();
        self.compile_into(&mut out)?;
        Ok(out)
    }

    pub fn compile_into(&self, out: &mut CompiledFrameGraph) -> Result<(), FrameGraphError> {
        out.clear();
        let result = self.compile_inner(out);
        if result.is_err() {
            // A failed compile must never leave a half-valid graph for the
            // backend. Capacity is preserved for the next frame.
            out.clear();
        }
        result
    }

    fn compile_inner(&self, out: &mut CompiledFrameGraph) -> Result<(), FrameGraphError> {
        // 1. Validate accesses and collect producer/consumer edges.
        let mut writers: Vec<u32> = vec![0; self.resources.len()];
        let mut edges: Vec<Vec<PassId>> = vec![Vec::new(); self.passes.len()];
        let mut indegree: Vec<u32> = vec![0; self.passes.len()];
        let mut seen_writes: Vec<ResourceId> = Vec::new();

        for (pass_index, pass) in self.passes.iter().enumerate() {
            let pass_id = pass_index as PassId;
            seen_writes.clear();
            for access in &pass.accesses {
                let index = access.resource as usize;
                let Some(resource) = self.resources.get(index) else {
                    return Err(FrameGraphError::UnknownResource(access.resource));
                };
                if is_write_state(access.state) {
                    if seen_writes.contains(&access.resource) {
                        return Err(FrameGraphError::DuplicateWrite {
                            pass: pass_id,
                            resource: access.resource,
                        });
                    }
                    seen_writes.push(access.resource);
                    writers[index] = writers[index].saturating_add(1);
                    if resource.origin == ResourceOrigin::Transient
                        && resource.desc.format.is_depth()
                        && access.state != ResourceState::DepthAttachment
                        && access.state != ResourceState::ShaderRead
                    {
                        // Depth textures may be written as attachments or read
                        // as shader input; other write states are rejected.
                        return Err(FrameGraphError::InvalidTexture(
                            "depth texture only supports depth-attachment or shader-read",
                        ));
                    }
                } else {
                    if writers[index] == 0 && resource.origin == ResourceOrigin::Transient {
                        return Err(FrameGraphError::MissingProducer {
                            pass: pass_id,
                            resource: access.resource,
                        });
                    }
                }
            }
        }

        // 2. Build edges from the last writer of each resource to every
        //    subsequent reader, and from earlier passes to later writers.
        let mut last_writer: Vec<Option<PassId>> = vec![None; self.resources.len()];
        for (pass_index, pass) in self.passes.iter().enumerate() {
            let pass_id = pass_index as PassId;
            for access in &pass.accesses {
                let index = access.resource as usize;
                if let Some(writer) = last_writer[index] {
                    if writer != pass_id {
                        edges[writer as usize].push(pass_id);
                        indegree[pass_index] = indegree[pass_index].saturating_add(1);
                    }
                }
                if is_write_state(access.state) {
                    last_writer[index] = Some(pass_id);
                }
            }
        }

        // 3. Stable topological order.
        let mut queue: VecDeque<PassId> = VecDeque::new();
        for (index, degree) in indegree.iter().enumerate() {
            if *degree == 0 {
                queue.push_back(index as PassId);
            }
        }
        while let Some(pass_id) = queue.pop_front() {
            out.order.push(pass_id);
            for next in &edges[pass_id as usize] {
                let degree = &mut indegree[*next as usize];
                *degree = degree.saturating_sub(1);
                if *degree == 0 {
                    queue.push_back(*next);
                }
            }
        }
        if out.order.len() != self.passes.len() {
            return Err(FrameGraphError::Cycle);
        }

        // 4. Barriers. Track the last state per resource in execution order.
        let mut states: Vec<ResourceState> = vec![ResourceState::Undefined; self.resources.len()];
        for pass_id in &out.order {
            for access in &self.passes[*pass_id as usize].accesses {
                let index = access.resource as usize;
                if states[index] != access.state {
                    out.barriers.push(GraphBarrier {
                        before_pass: *pass_id,
                        resource: access.resource,
                        from: states[index],
                        to: access.state,
                    });
                    states[index] = access.state;
                }
            }
        }

        // 5. Transient liveness and aliasing.
        let resource_count = self.resources.len();
        let mut used = vec![false; resource_count];
        let mut first_pass: Vec<Option<PassId>> = vec![None; resource_count];
        let mut last_pass: Vec<Option<PassId>> = vec![None; resource_count];
        for (pass_index, pass) in self.passes.iter().enumerate() {
            let pass_id = pass_index as PassId;
            for access in &pass.accesses {
                let index = access.resource as usize;
                used[index] = true;
                first_pass[index].get_or_insert(pass_id);
                last_pass[index] = Some(pass_id);
            }
        }

        let mut transient_order: Vec<ResourceId> = Vec::new();
        for (index, resource) in self.resources.iter().enumerate() {
            if resource.origin != ResourceOrigin::Transient {
                out.imported_bytes = out
                    .imported_bytes
                    .saturating_add(resource.desc.estimated_bytes());
                continue;
            }
            if !used[index] {
                return Err(FrameGraphError::UnusedTransient(index as ResourceId));
            }
            if writers[index] == 0 {
                return Err(FrameGraphError::TransientWithoutWriter(index as ResourceId));
            }
            transient_order.push(index as ResourceId);
        }
        transient_order.sort_by_key(|id| first_pass[*id as usize].unwrap_or(PassId::MAX));

        let mut slots: Vec<AliasSlot> = Vec::new();
        for resource_id in transient_order {
            let resource = &self.resources[resource_id as usize];
            let first = first_pass[resource_id as usize].unwrap_or(0);
            let bytes = resource.desc.estimated_bytes();
            let mut placed = false;
            for slot in &mut slots {
                if slot
                    .resources
                    .iter()
                    .all(|existing| last_pass[*existing as usize].unwrap_or(0) < first)
                    && slot.desc.alias_compatible(resource.desc)
                {
                    slot.resources.push(resource_id);
                    // Keep the largest descriptor: the slot must satisfy every
                    // resource that reuses it.
                    if resource.desc.estimated_bytes() > slot.bytes {
                        slot.desc = resource.desc;
                        slot.bytes = bytes;
                    }
                    placed = true;
                    break;
                }
            }
            if !placed {
                slots.push(AliasSlot {
                    slot: slots.len() as u32,
                    desc: resource.desc,
                    bytes,
                    resources: vec![resource_id],
                });
            }
        }

        out.transient_bytes = slots
            .iter()
            .map(|slot| slot.bytes)
            .fold(0u64, |total, bytes| total.saturating_add(bytes));
        out.peak_transient_bytes = out.peak_transient_bytes.max(out.transient_bytes);
        out.slots = slots;
        if out.transient_bytes > self.budget_bytes {
            return Err(FrameGraphError::BudgetExceeded {
                requested: out.transient_bytes,
                budget: self.budget_bytes,
            });
        }

        Ok(())
    }

    /// Debug projection used by diagnostics and tests. Bounded to the graph's
    /// own resource count.
    pub fn describe(&self) -> Vec<(ResourceId, &'static str, u64, bool)> {
        let mut writers = vec![false; self.resources.len()];
        for pass in &self.passes {
            for access in &pass.accesses {
                if is_write_state(access.state) {
                    if let Some(slot) = writers.get_mut(access.resource as usize) {
                        *slot = true;
                    }
                }
            }
        }
        self.resources
            .iter()
            .enumerate()
            .map(|(index, resource)| {
                (
                    index as ResourceId,
                    match resource.origin {
                        ResourceOrigin::Imported => "imported",
                        ResourceOrigin::Transient => "transient",
                    },
                    resource.desc.estimated_bytes(),
                    writers[index],
                )
            })
            .collect()
    }

    pub fn pass_names(&self) -> Vec<&'static str> {
        self.passes.iter().map(|pass| pass.name).collect()
    }

    pub fn resource_bytes_by_family(&self) -> BTreeMap<&'static str, u64> {
        let mut map = BTreeMap::new();
        for resource in &self.resources {
            let key = match resource.origin {
                ResourceOrigin::Imported => "imported",
                ResourceOrigin::Transient => "transient",
            };
            let entry = map.entry(key).or_insert(0u64);
            *entry = entry.saturating_add(resource.desc.estimated_bytes());
        }
        map
    }
}

const fn is_write_state(state: ResourceState) -> bool {
    matches!(
        state,
        ResourceState::ColorAttachment
            | ResourceState::DepthAttachment
            | ResourceState::ShaderWrite
            | ResourceState::CopyDst
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn color(width: u32, height: u32) -> GraphTextureDesc {
        GraphTextureDesc::new(width, height, GraphTextureFormat::Rgba8UnormSrgb)
            .with_usage(TextureUsage::RENDER_ATTACHMENT.union(TextureUsage::TEXTURE_BINDING))
    }

    #[test]
    fn orders_a_diamond_and_emits_minimal_barriers() {
        let mut graph = FrameGraph::new(1 << 20);
        let target = graph.import_texture(color(64, 64)).unwrap();
        let shadow = graph.create_texture(color(32, 32)).unwrap();
        let lit = graph.create_texture(color(64, 64)).unwrap();
        let import = graph
            .add_pass(PassDesc::new("import", PassKind::Copy).write(target, ResourceState::CopyDst))
            .unwrap();
        let shadow_pass = graph
            .add_pass(
                PassDesc::new("shadow", PassKind::Graphics)
                    .write(shadow, ResourceState::ColorAttachment),
            )
            .unwrap();
        let lit_pass = graph
            .add_pass(
                PassDesc::new("lit", PassKind::Graphics)
                    .read(shadow)
                    .write(lit, ResourceState::ColorAttachment),
            )
            .unwrap();
        let present = graph
            .add_pass(
                PassDesc::new("present", PassKind::Present)
                    .read(lit)
                    .write(target, ResourceState::Present),
            )
            .unwrap();

        let compiled = graph.compile().unwrap();
        assert_eq!(compiled.order, vec![import, shadow_pass, lit_pass, present]);
        assert_eq!(compiled.barrier_count(), 6);
        assert_eq!(compiled.transient_bytes, 32 * 32 * 4 + 64 * 64 * 4);
    }

    #[test]
    fn rejects_a_read_before_the_producer() {
        let mut graph = FrameGraph::new(1 << 20);
        let texture = graph.create_texture(color(8, 8)).unwrap();
        graph
            .add_pass(PassDesc::new("read-first", PassKind::Graphics).read(texture))
            .unwrap();
        assert_eq!(
            graph.compile().unwrap_err(),
            FrameGraphError::MissingProducer {
                pass: 0,
                resource: 0
            }
        );
    }

    #[test]
    fn rejects_a_read_before_a_writer_that_appears_later() {
        let mut graph = FrameGraph::new(1 << 20);
        let a = graph.create_texture(color(8, 8)).unwrap();
        let b = graph.create_texture(color(8, 8)).unwrap();
        graph
            .add_pass(
                PassDesc::new("a", PassKind::Compute)
                    .read(b)
                    .write(a, ResourceState::ShaderWrite),
            )
            .unwrap();
        graph
            .add_pass(
                PassDesc::new("b", PassKind::Compute)
                    .read(a)
                    .write(b, ResourceState::ShaderWrite),
            )
            .unwrap();
        assert_eq!(
            graph.compile().unwrap_err(),
            FrameGraphError::MissingProducer {
                pass: 0,
                resource: 1
            }
        );
    }

    #[test]
    fn aliases_transients_with_disjoint_lifetimes() {
        let mut graph = FrameGraph::new(1 << 20);
        let first = graph.create_texture(color(64, 64)).unwrap();
        let second = graph.create_texture(color(64, 64)).unwrap();
        graph
            .add_pass(
                PassDesc::new("produce", PassKind::Graphics)
                    .write(first, ResourceState::ColorAttachment),
            )
            .unwrap();
        graph
            .add_pass(PassDesc::new("consume", PassKind::Graphics).read(first))
            .unwrap();
        graph
            .add_pass(
                PassDesc::new("produce-later", PassKind::Graphics)
                    .write(second, ResourceState::ColorAttachment),
            )
            .unwrap();

        let compiled = graph.compile().unwrap();
        assert_eq!(compiled.slots.len(), 1);
        assert_eq!(compiled.transient_bytes, 64 * 64 * 4);
    }

    #[test]
    fn does_not_alias_overlapping_lifetimes() {
        let mut graph = FrameGraph::new(1 << 20);
        let a = graph.create_texture(color(64, 64)).unwrap();
        let b = graph.create_texture(color(64, 64)).unwrap();
        graph
            .add_pass(
                PassDesc::new("both", PassKind::Graphics)
                    .write(a, ResourceState::ColorAttachment)
                    .write(b, ResourceState::ColorAttachment),
            )
            .unwrap();

        let compiled = graph.compile().unwrap();
        assert_eq!(compiled.slots.len(), 2);
        assert_eq!(compiled.transient_bytes, 2 * 64 * 64 * 4);
    }

    #[test]
    fn rejects_an_over_budget_frame() {
        let mut graph = FrameGraph::new(1024);
        let texture = graph.create_texture(color(64, 64)).unwrap();
        graph
            .add_pass(
                PassDesc::new("draw", PassKind::Graphics)
                    .write(texture, ResourceState::ColorAttachment),
            )
            .unwrap();
        assert!(matches!(
            graph.compile().unwrap_err(),
            FrameGraphError::BudgetExceeded { .. }
        ));
    }

    #[test]
    fn depth_textures_only_accept_attachment_or_read() {
        let mut graph = FrameGraph::new(1 << 20);
        let depth = graph
            .create_texture(
                GraphTextureDesc::new(64, 64, GraphTextureFormat::Depth32Float)
                    .with_usage(TextureUsage::RENDER_ATTACHMENT),
            )
            .unwrap();
        graph
            .add_pass(PassDesc::new("copy", PassKind::Copy).write(depth, ResourceState::CopyDst))
            .unwrap();
        assert!(matches!(
            graph.compile().unwrap_err(),
            FrameGraphError::InvalidTexture(_)
        ));
    }
}
