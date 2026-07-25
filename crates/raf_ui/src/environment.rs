use serde::{Deserialize, Serialize};

/// User preference used to resolve a surface theme. `System` is deliberately
/// data-only: the platform host decides which concrete palette to use.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum UiColorMode {
    System,
    Dark,
    Light,
}

/// Sampling policy for texture-backed retained primitives. Solid geometry is
/// handled by physical-pixel snapping instead of texture filtering.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum UiSamplingMode {
    Nearest,
    Linear,
}

/// Geometry policy used by RafUI Studio and both retained presentation paths.
/// Keeping these values together prevents one host from applying text density
/// to borders or icon density to layout geometry.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum UiGeometrySnap {
    PhysicalPixel,
    Fractional,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct UiDensityContract {
    pub geometry_scale: f32,
    pub text_scale: f32,
    pub icon_scale: f32,
    pub geometry_snap: UiGeometrySnap,
    pub text_sampling: UiSamplingMode,
    pub icon_sampling: UiSamplingMode,
}

impl Default for UiDensityContract {
    fn default() -> Self {
        UiEnvironment::default().density_contract()
    }
}

impl Default for UiColorMode {
    fn default() -> Self {
        Self::System
    }
}

/// Platform facts that influence layout without leaking a windowing API into
/// user-authored documents.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct UiEnvironment {
    pub viewport_size: [f32; 2],
    pub scale_factor: f32,
    pub color_mode: UiColorMode,
    pub prefers_reduced_motion: bool,
    pub high_contrast: bool,
}

impl UiEnvironment {
    pub fn new(width: u32, height: u32) -> Self {
        Self {
            viewport_size: [width.max(1) as f32, height.max(1) as f32],
            ..Self::default()
        }
    }

    pub fn width(self) -> f32 {
        self.viewport_size[0].max(1.0)
    }

    pub fn height(self) -> f32 {
        self.viewport_size[1].max(1.0)
    }

    pub fn is_compact(self, breakpoint: f32) -> bool {
        breakpoint > 0.0 && self.width() <= breakpoint
    }

    /// Returns the raster density that matches the physical presentation
    /// target while preserving logical layout coordinates. The compositor
    /// must not supersample the atlas into a 1x target: that creates a second
    /// filtering step when the retained surface is placed by the host.
    pub fn raster_scale(self) -> f32 {
        self.scale_factor.clamp(1.0, 4.0)
    }

    /// Returns the text-atlas density for a retained surface.
    ///
    /// Geometry must remain 1:1 with its physical target, but small vector
    /// glyphs benefit from a modest source-density floor before the GPU (or
    /// CPU recovery compositor) resolves them into that target. This keeps
    /// panel edges and icons sharp without making body text look stair-stepped
    /// at 100% display scale.
    pub fn text_raster_scale(self) -> f32 {
        self.raster_scale().max(1.25)
    }

    /// Resolves the three density policies independently. Text may use a
    /// modest source-density floor for legibility; borders and small icons do
    /// not inherit that floor and therefore avoid the soft, enlarged look
    /// that previously appeared on retained editor chrome.
    pub fn density_contract(self) -> UiDensityContract {
        let scale = self.scale_factor.clamp(1.0, 4.0);
        let fractional = (scale.fract()).abs() > f32::EPSILON;
        UiDensityContract {
            geometry_scale: scale,
            text_scale: self.text_raster_scale(),
            icon_scale: scale,
            geometry_snap: if fractional {
                UiGeometrySnap::Fractional
            } else {
                UiGeometrySnap::PhysicalPixel
            },
            text_sampling: if fractional {
                UiSamplingMode::Linear
            } else {
                UiSamplingMode::Nearest
            },
            icon_sampling: if fractional {
                UiSamplingMode::Linear
            } else {
                UiSamplingMode::Nearest
            },
        }
    }

    pub fn physical_size(self) -> [u32; 2] {
        [
            (self.width() * self.scale_factor.max(0.5)).round().max(1.0) as u32,
            (self.height() * self.scale_factor.max(0.5))
                .round()
                .max(1.0) as u32,
        ]
    }

    pub fn logical_size_from_physical(self, physical: [u32; 2]) -> [f32; 2] {
        let scale = self.scale_factor.max(0.5);
        [physical[0] as f32 / scale, physical[1] as f32 / scale]
    }
}

impl Default for UiEnvironment {
    fn default() -> Self {
        Self {
            viewport_size: [1.0, 1.0],
            scale_factor: 1.0,
            color_mode: UiColorMode::System,
            prefers_reduced_motion: false,
            high_contrast: false,
        }
    }
}
