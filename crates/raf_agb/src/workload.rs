use std::ops::Range;

use bytemuck::{Pod, Zeroable};
use serde::{Deserialize, Serialize};

use crate::{Error, Result};

#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable, Serialize, Deserialize)]
pub struct Vertex {
    pub position: [f32; 3],
    pub uv: [f32; 2],
    pub rgba: [f32; 4],
    /// Zero keeps legacy captures and unlit geometry unchanged.
    #[serde(default)]
    pub normal: [f32; 3],
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable, Serialize, Deserialize)]
pub struct Instance {
    pub transform: [[f32; 4]; 4],
    pub tint: [f32; 4],
    /// Vertex stream interpretation for AGB's internal raster commands.
    #[serde(default)]
    pub(crate) raster_kind: u32,
}

impl Instance {
    pub fn new(transform: glam::Mat4, tint: [f32; 4]) -> Self {
        Self {
            transform: transform.to_cols_array_2d(),
            tint,
            raster_kind: 0,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Texture {
    pub size: [u32; 2],
    pub rgba: Vec<u8>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum DrawMode {
    /// Opaque, one-sided scene geometry. It writes depth and culls backfaces.
    Scene,
    Overlay,
    DepthReadOnly,
    SceneNoDepth,
    /// Opaque scene geometry accepted for both windings.
    ///
    /// Appended after the original variants so serde formats that encode the
    /// variant index keep reading legacy captures.
    SceneTwoSided,
    /// Alpha-blended scene geometry. It tests depth but never writes it.
    SceneTransparent,
    /// Alpha-blended geometry accepted for both windings.
    SceneTransparentTwoSided,
}

impl DrawMode {
    /// Every mode except `Overlay` is clipped to the scene viewport. Only
    /// `Overlay` reaches the full target for editor chrome and RafUI paint.
    pub fn is_scene(self) -> bool {
        self != Self::Overlay
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Batch {
    pub mode: DrawMode,
    pub vertices: Range<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub indices: Option<Range<u32>>,
    pub instances: Range<u32>,
    pub texture: u32,
}

/// Immutable GPU inputs. Positions are world-space for Scene and clip-space
/// for Overlay. Texture coordinates have their origin at the top left.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Workload {
    pub vertices: Vec<Vertex>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub indices: Vec<u32>,
    pub instances: Vec<Instance>,
    pub textures: Vec<Texture>,
    pub batches: Vec<Batch>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub shadow_batches: Vec<Batch>,
}

/// Patch existing data. RGBA rows are tightly packed, with a top-left origin.
#[derive(Clone, Copy)]
pub enum ResourcePatch<'a> {
    Vertices {
        first: u32,
        data: &'a [Vertex],
    },
    Instances {
        first: u32,
        data: &'a [Instance],
    },
    Texture {
        index: u32,
        origin: [u32; 2],
        size: [u32; 2],
        rgba: &'a [u8],
    },
}

impl ResourcePatch<'_> {
    pub(crate) fn bytes(&self) -> u64 {
        match self {
            Self::Vertices { data, .. } => std::mem::size_of_val(*data) as u64,
            Self::Instances { data, .. } => std::mem::size_of_val(*data) as u64,
            Self::Texture { rgba, .. } => rgba.len() as u64,
        }
    }

    pub(crate) fn staging_bytes(&self, row_alignment: u64) -> u64 {
        match self {
            Self::Texture {
                size: [width, height],
                ..
            } => (u64::from(*width) * 4)
                .div_ceil(row_alignment)
                .saturating_mul(row_alignment)
                .saturating_mul(u64::from(*height)),
            _ => self.bytes(),
        }
    }
}

impl Workload {
    pub fn bytes(&self) -> u64 {
        (std::mem::size_of_val(self.vertices.as_slice()) as u64)
            .saturating_add(std::mem::size_of_val(self.indices.as_slice()) as u64)
            .saturating_add(std::mem::size_of_val(self.instances.as_slice()) as u64)
            .saturating_add(
                self.textures
                    .iter()
                    .map(|t| t.rgba.len() as u64)
                    .sum::<u64>(),
            )
    }

    pub(crate) fn validate(&self, max_texture: u32, max_buffer: u64) -> Result<()> {
        if self.vertices.is_empty() || self.instances.is_empty() || self.textures.is_empty() {
            return Err(Error::Invalid("workload lacks GPU bindings"));
        }
        if [
            std::mem::size_of_val(self.vertices.as_slice()),
            std::mem::size_of_val(self.instances.as_slice()),
            std::mem::size_of_val(self.indices.as_slice()),
        ]
        .into_iter()
        .fold(0u64, |total, bytes| total.saturating_add(bytes as u64))
            > max_buffer
        {
            return Err(Error::Invalid("buffer exceeds adapter limit"));
        }
        for texture in &self.textures {
            let [w, h] = texture.size;
            if w == 0
                || h == 0
                || w > max_texture
                || h > max_texture
                || u64::from(w) * u64::from(h) != texture.rgba.len() as u64 / 4
                || texture.rgba.len() % 4 != 0
            {
                return Err(Error::Invalid("invalid RGBA texture"));
            }
        }
        let mut overlay = false;
        for batch in &self.shadow_batches {
            if batch.mode != DrawMode::Scene {
                return Err(Error::Invalid("invalid shadow batch"));
            }
        }
        for batch in self.shadow_batches.iter().chain(&self.batches) {
            let count = match &batch.indices {
                Some(range) => {
                    let indices = self
                        .indices
                        .get(range.start as usize..range.end as usize)
                        .ok_or(Error::Invalid("invalid index range"))?;
                    if indices.iter().any(|index| !batch.vertices.contains(index)) {
                        return Err(Error::Invalid("index outside batch vertex range"));
                    }
                    indices.len()
                }
                None => batch.vertices.len(),
            };
            if batch.vertices.is_empty()
                || batch.instances.is_empty()
                || batch.vertices.end as usize > self.vertices.len()
                || batch.instances.end as usize > self.instances.len()
                || count == 0
                || count % 3 != 0
                || batch.texture as usize >= self.textures.len()
                || overlay && batch.mode.is_scene()
            {
                return Err(Error::Invalid("invalid batch ranges or paint order"));
            }
            overlay |= batch.mode == DrawMode::Overlay;
        }
        Ok(())
    }
}
