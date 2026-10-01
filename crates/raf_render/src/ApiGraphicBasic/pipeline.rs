/// Pre-configured rendering pipelines supported by `ApiGraphicBasic`.
/// These represent high-level shaders and state setups without exposing low-level pipeline details.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum BasicPipelineKind {
    /// Solid rendering with basic flat shading.
    FlatColor,
    /// Solid flat-shaded rendering without back-face culling.
    FlatColorTwoSided,
    /// Wireframe rendering for grids, outlines, and bounds.
    UnlitWireframe,
    /// PBR rendering with lighting and materials (primarily GPU-driven).
    PbrLit,
    /// Lit rendering without back-face culling.
    PbrLitTwoSided,
    /// X-Ray style rendering that bypasses the depth test (e.g. for gizmos and active selection grid).
    XRay,
}

impl Default for BasicPipelineKind {
    fn default() -> Self {
        Self::FlatColor
    }
}

impl BasicPipelineKind {
    pub const fn is_lit(self) -> bool {
        matches!(self, Self::PbrLit | Self::PbrLitTwoSided)
    }

    pub const fn is_two_sided(self) -> bool {
        matches!(self, Self::FlatColorTwoSided | Self::PbrLitTwoSided)
    }
}
