use glam::{Mat4, Vec3};

use crate::{Error, Frame, Instance, Result, Texture, Vertex};

/// Mesh positions and normals are in object space. Texture UVs start at top left.
/// Vertex colors are linear. Material and texture indices belong to the packet.
#[derive(Default)]
pub struct Mesh {
    pub vertices: Vec<Vertex>,
    pub indices: Vec<u32>,
}

#[derive(Clone, Copy, Debug)]
pub struct Material {
    pub texture: Option<u32>,
    pub tint: [f32; 4],
}

impl Default for Material {
    fn default() -> Self {
        Self {
            texture: None,
            tint: [1.; 4],
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Topology {
    TriangleList,
    LineList,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Blend {
    Opaque,
    StraightAlpha,
    PremultipliedAlpha,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Depth {
    Disabled,
    ReadWrite,
    ReadOnly,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Cull {
    None,
    Front,
    Back,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Pipeline {
    pub topology: Topology,
    pub blend: Blend,
    pub depth: Depth,
    pub cull: Cull,
    pub lit: bool,
}

impl Pipeline {
    pub const SCENE: Self = Self {
        topology: Topology::TriangleList,
        blend: Blend::StraightAlpha,
        depth: Depth::ReadWrite,
        cull: Cull::None,
        lit: false,
    };
    pub const LIT: Self = Self {
        lit: true,
        ..Self::SCENE
    };
    pub const OVERLAY: Self = Self {
        depth: Depth::Disabled,
        ..Self::SCENE
    };
}

#[derive(Clone, Copy, Debug)]
pub struct Line {
    pub start: [f32; 3],
    pub end: [f32; 3],
    pub rgba: [f32; 4],
    pub width_px: f32,
    pub no_depth_test: bool,
    pub depth_bias: f32,
}

#[derive(Clone, Copy, Debug)]
pub struct Grid {
    pub y: f32,
    pub spacing: f32,
    pub no_depth_test: bool,
}

/// A top-left normalized rectangle [x, y, width, height] in the full target.
/// Clipping to another rectangle with the same coordinate system preserves UVs.
#[derive(Clone, Copy, Debug)]
pub struct PaintQuad {
    pub rect: [f32; 4],
    pub clip: [f32; 4],
    pub uv_rect: [f32; 4],
    pub color: [f32; 4],
}

#[derive(Clone, Copy, Debug)]
pub struct PaintStroke {
    pub start: [f32; 2],
    pub end: [f32; 2],
    /// Stroke width expressed separately in normalized horizontal and vertical units.
    pub width: [f32; 2],
    pub clip: [f32; 4],
    pub color: [f32; 4],
}

#[derive(Clone, Copy, Debug)]
pub enum Paint {
    Solid(PaintQuad),
    Stroke(PaintStroke),
    /// Tint a pre-rasterized RGBA coverage atlas using the quad color.
    Text {
        quad: PaintQuad,
        atlas: u32,
    },
    Image {
        quad: PaintQuad,
        texture: u32,
    },
    /// A visible effect that requires a capability beyond flat RGBA paint.
    Effect(crate::product::Feature),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CadLayer {
    Schematic,
    PcbTopCopper,
    PcbBottomCopper,
    PcbSilkscreen,
    Airwire,
    Overlay,
    Drc,
}

impl CadLayer {
    pub const ALL: [Self; 7] = [
        Self::Schematic,
        Self::PcbTopCopper,
        Self::PcbBottomCopper,
        Self::PcbSilkscreen,
        Self::Airwire,
        Self::Overlay,
        Self::Drc,
    ];
}

pub enum Command {
    Clear([f32; 4]),
    SetPipeline(Pipeline),
    SetShadowReceiver(bool),
    DrawMesh {
        mesh: u32,
        material: u32,
        instance: Instance,
    },
    DrawMeshBatch {
        mesh: u32,
        material: u32,
        instances: Vec<Instance>,
    },
    DrawShadowBatch {
        mesh: u32,
        material: u32,
        instances: Vec<Instance>,
    },
    DrawLine(Line),
    DrawLineBatch(Vec<Line>),
    /// Positions are clip-space, and consume z in [0, 1].
    DrawScreenTriangleBatch {
        vertices: Vec<Vertex>,
        material: u32,
    },
    DrawGrid(Grid),
    /// The camera-relative sky is drawn before the first scene primitive.
    DrawSky(SkyBackdrop),
    /// A fullscreen clip-space background that stays inside the scene viewport.
    /// It must precede retained UI paint.
    SceneBackground([f32; 4]),
    Paint(Paint),
    StyledPaint {
        paint: Paint,
        style: Box<super::PaintStyle>,
    },
    /// The domain supplies the paint order and keeps picking and editing.
    Cad {
        layer: CadLayer,
        paint: Paint,
    },
    CadStyled {
        layer: CadLayer,
        paint: Paint,
        style: Box<super::PaintStyle>,
    },
    CadTriangles {
        layer: CadLayer,
        vertices: Vec<Vertex>,
        material: u32,
    },
}

/// Owned inputs for a retained revision. No serialization or backend handles
/// are needed to prepare this data. The caller may drop it after submission.
#[derive(Default)]
pub struct ProductPacket {
    pub meshes: Vec<Mesh>,
    pub materials: Vec<Material>,
    pub textures: Vec<Texture>,
    pub commands: Vec<Command>,
}

#[derive(Clone, Copy, Debug)]
pub struct Camera {
    pub view_projection: Mat4,
    pub world_position: Vec3,
}

impl Camera {
    pub fn from_view_projection(view: Mat4, projection: Mat4, world_position: Vec3) -> Self {
        Self {
            view_projection: projection * view,
            world_position,
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct DirectionalLight {
    pub toward_light: Vec3,
    pub color: [f32; 3],
}

#[derive(Clone, Copy, Debug)]
pub struct ShadowUniforms {
    pub light_view_projection: Mat4,
    pub bias: f32,
    pub resolution: u32,
    pub depth_key: u64,
    pub distance: f32,
    pub strength: f32,
    pub filter_taps: u32,
    pub slope_bias_per_texel: f32,
}

#[derive(Clone, Copy, Debug)]
pub struct Environment {
    /// Neutral ambient lighting fraction in [0, 1].
    pub ambient: f32,
    pub ambient_tint: [f32; 3],
    pub exposure_ev: f32,
    pub authored: bool,
    pub improved: bool,
    pub toon_steps: f32,
    pub toon_softness: f32,
    /// Optional environment map in the prepared packet's texture table.
    pub radiance_map: Option<u32>,
}

#[derive(Clone, Copy, Debug)]
pub struct SkyBackdrop {
    pub zenith: [f32; 4],
    pub horizon: [f32; 4],
    pub ground: [f32; 4],
    pub haze: f32,
    /// Optional equirectangular texture, indexed within the packet.
    pub texture: Option<u32>,
    /// A cosine of one disables the solar disk.
    pub sun_disk_cos: f32,
    pub halo: f32,
    pub halo_cos: f32,
    pub stars_density: f32,
    pub stars_intensity: f32,
}

#[derive(Clone, Copy, Debug)]
pub struct FrameUniforms {
    pub camera: Camera,
    pub light: DirectionalLight,
    pub environment: Environment,
    pub shadows: Option<ShadowUniforms>,
    pub scene_filter: bool,
    /// Monotonic playback time for passes that consume time.
    pub time_seconds: f32,
    /// Top-left normalized `[x, y, width, height]` of the scene canvas inside
    /// the full target. RafUI paint and CAD overlays ignore it and always draw
    /// across the full target.
    pub viewport: [f32; 4],
}

impl Default for FrameUniforms {
    fn default() -> Self {
        Self {
            camera: Camera {
                view_projection: Mat4::IDENTITY,
                world_position: Vec3::ZERO,
            },
            light: DirectionalLight {
                toward_light: Vec3::Y,
                color: [1.; 3],
            },
            environment: Environment {
                ambient: 0.35,
                ambient_tint: [1.; 3],
                exposure_ev: 0.,
                authored: false,
                improved: false,
                toon_steps: 0.,
                toon_softness: 0.,
                radiance_map: None,
            },
            shadows: None,
            scene_filter: false,
            time_seconds: 0.,
            viewport: [0., 0., 1., 1.],
        }
    }
}

impl FrameUniforms {
    pub(crate) fn raster(&self, clear: [f32; 4]) -> Result<Frame> {
        if self.environment.radiance_map.is_some() {
            return Err(Error::Unsupported("environment radiance map"));
        }
        let ambient = self.environment.ambient;
        if !self.camera.view_projection.is_finite()
            || !self.camera.world_position.is_finite()
            || !self.time_seconds.is_finite()
            || !(0. ..=1.).contains(&ambient)
            || !(-16. ..=16.).contains(&self.environment.exposure_ev)
            || !(0. ..=6.).contains(&self.environment.toon_steps)
            || !(0. ..=1.).contains(&self.environment.toon_softness)
            || self
                .environment
                .ambient_tint
                .into_iter()
                .any(|value| !value.is_finite() || value < 0.)
            || self
                .light
                .color
                .into_iter()
                .any(|v| !v.is_finite() || v < 0.)
        {
            return Err(Error::Invalid("invalid frame uniforms"));
        }
        let light = self
            .light
            .toward_light
            .try_normalize()
            .ok_or(Error::Invalid("invalid directional light"))?;
        let [r, g, b] = self.light.color;
        let [ar, ag, ab] = self.environment.ambient_tint;
        let viewport = crate::device::validate_viewport(self.viewport)?;
        let shadow = self
            .shadows
            .map(|shadow| {
                if !shadow.light_view_projection.is_finite()
                    || !shadow.bias.is_finite()
                    || shadow.bias < 0.
                    || ![256, 512, 1024].contains(&shadow.resolution)
                    || !(2. ..=256.).contains(&shadow.distance)
                    || !(0. ..=1.).contains(&shadow.strength)
                    || ![1, 4, 9].contains(&shadow.filter_taps)
                    || !shadow.slope_bias_per_texel.is_finite()
                    || shadow.slope_bias_per_texel < 0.0
                {
                    return Err(Error::Invalid("invalid directional shadow parameters"));
                }
                Ok(crate::ShadowFrame {
                    light_view_projection: shadow.light_view_projection.to_cols_array_2d(),
                    parameters: [
                        shadow.bias,
                        shadow.distance,
                        shadow.strength,
                        shadow.filter_taps as f32,
                    ],
                    sampling: [shadow.slope_bias_per_texel, 0.0, 0.0, 0.0],
                })
            })
            .transpose()?;
        Ok(Frame {
            view_projection: self.camera.view_projection.to_cols_array_2d(),
            eye: self.camera.world_position.to_array(),
            clear,
            light: [light.x, light.y, light.z, ambient],
            light_color: [r, g, b, 1.],
            environment: [ar, ag, ab, self.environment.exposure_ev.exp2()],
            lighting_style: [
                f32::from(self.environment.authored),
                f32::from(self.environment.improved),
                self.environment.toon_steps,
                self.environment.toon_softness,
            ],
            viewport,
            shadow,
            shadow_map: self
                .shadows
                .map(|shadow| (shadow.resolution, shadow.depth_key)),
            scene_filter: self.scene_filter,
        })
    }
}
