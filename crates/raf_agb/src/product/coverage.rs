use super::CadLayer;

/// Capabilities of the current product encoder on native raster executors.
/// Place the coverage table next to the encoder so commands and diagnostics
/// can refer to the same operation names.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Feature {
    Clear,
    Pipeline,
    ShadowReceiver,
    DrawMesh,
    DrawMeshBatch,
    DrawLine,
    DrawLineBatch,
    DrawScreenTriangleBatch,
    DrawGrid,
    Sky,
    Light,
    ShadowMap,
    SceneFilter,
    Solid,
    Stroke,
    Text,
    Image,
    RoundedClips,
    Gradient,
    ImageFit,
    Feather,
    TextAtlasPatch,
    CustomPass,
    DepthBias,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Coverage {
    Supported,
    Restricted,
    Unsupported,
}

impl Feature {
    pub const fn name(self) -> &'static str {
        match self {
            Self::Clear => "Clear",
            Self::Pipeline => "SetPipeline",
            Self::ShadowReceiver => "SetShadowReceiver",
            Self::DrawMesh => "DrawMesh",
            Self::DrawMeshBatch => "DrawMeshBatch",
            Self::DrawLine => "DrawLine",
            Self::DrawLineBatch => "DrawLineBatch",
            Self::DrawScreenTriangleBatch => "DrawScreenTriangleBatch",
            Self::DrawGrid => "DrawGrid",
            Self::Sky => "camera-relative sky",
            Self::Light => "directional/ambient lighting",
            Self::ShadowMap => "shadow map pass",
            Self::SceneFilter => "scene-only spatial filter",
            Self::Solid => "RafUI Solid",
            Self::Stroke => "RafUI Stroke",
            Self::Text => "RafUI Text",
            Self::Image => "RafUI Image",
            Self::RoundedClips => "RafUI rounded clips and borders",
            Self::Gradient => "RafUI two-color linear gradients",
            Self::ImageFit => "RafUI image fit and nine-slice",
            Self::Feather => "RafUI bounded edge feather",
            Self::TextAtlasPatch => "RafUI text atlas rectangle patches",
            Self::CustomPass => "custom pass execution",
            Self::DepthBias => "line depth bias",
        }
    }
}

use Coverage::*;
use Feature::*;

/// RafUI rows are produced as CPU geometry and consumed by every executor, so
/// a supported row means identical triangles on Vulkan, DX12, Metal and WebGPU.
///
/// Remaining explicit restrictions:
/// - `Stroke` accepts clip stacks, gradients and feather; corner radii and
///   per-side borders describe a box and are rejected.
/// - `Image` accepts border, gradient and feather on a plain stretch. Feather
///   on a fitted or nine-sliced image is rejected because the halo would sample
///   stretched edge texels.
/// - `RoundedClips` accepts at most [`super::MAX_DECLARED_CLIPS`] declared
///   masks plus one slot for the painted surface or image edge
///   ([`super::MAX_ROUNDED_CLIPS`] total). More are rejected, never dropped.
pub const COVERAGE: &[(Feature, Coverage)] = &[
    (Clear, Restricted),
    (Pipeline, Restricted),
    (ShadowReceiver, Restricted),
    (DrawMesh, Restricted),
    (DrawMeshBatch, Restricted),
    (DrawLine, Supported),
    (DrawLineBatch, Supported),
    (DrawGrid, Restricted),
    (Sky, Restricted),
    (DrawScreenTriangleBatch, Supported),
    (Light, Supported),
    (ShadowMap, Restricted),
    (SceneFilter, Restricted),
    (Solid, Supported),
    (Stroke, Supported),
    (Text, Supported),
    (Image, Supported),
    (RoundedClips, Supported),
    (Gradient, Supported),
    (ImageFit, Supported),
    (Feather, Supported),
    (TextAtlasPatch, Supported),
    (CustomPass, Unsupported),
    (DepthBias, Supported),
];

/// All CAD layers accept caller-ordered flat paint and screen triangles.
/// Geometry semantics (including visibility and picking) remain in CAD.
pub const CAD_COVERAGE: &[(CadLayer, Coverage)] = &[
    (CadLayer::Schematic, Restricted),
    (CadLayer::PcbTopCopper, Restricted),
    (CadLayer::PcbBottomCopper, Restricted),
    (CadLayer::PcbSilkscreen, Restricted),
    (CadLayer::Airwire, Restricted),
    (CadLayer::Overlay, Restricted),
    (CadLayer::Drc, Restricted),
];
