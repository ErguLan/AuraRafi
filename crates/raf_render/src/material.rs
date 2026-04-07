//! PBR material system.
//!
//! Defines materials that describe how surfaces look under light.
//! Uses the metallic/roughness PBR workflow 
//!
//! Today: CpuPainter ignores most of this - just uses base_color.
//! Tomorrow: wgpu backend uses full PBR with textures and normal maps.
//! Future: RT backend uses this + subsurface scattering, anisotropy.
//!
//! Zero cost: just structs. No texture loading happens until a GPU backend is active.

use glam::{Vec3, Vec4};
use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// Material
// ---------------------------------------------------------------------------

/// A PBR material. Describes how a surface looks.
/// Compatible with glTF metallic-roughness workflow.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Material {
    /// Unique ID (auto-incremented).
    pub id: u64,
    /// Display name.
    pub name: String,
    /// Base color (albedo). RGBA linear, alpha for transparency.
    pub base_color: Vec4,
    /// Metallic factor (0.0 = dielectric/plastic, 1.0 = metal).
    pub metallic: f32,
    /// Roughness factor (0.0 = mirror smooth, 1.0 = fully rough).
    pub roughness: f32,
    /// Emissive color (self-illumination). RGB linear.
    pub emissive: Vec3,
    /// Emissive intensity multiplier (for HDR bloom).
    pub emissive_intensity: f32,
    /// Normal map strength (0.0 = flat, 1.0 = full effect).
    pub normal_strength: f32,
    /// Ambient occlusion strength.
    pub ao_strength: f32,
    /// Alpha mode.
    pub alpha_mode: AlphaMode,
    /// Alpha cutoff threshold (for Mask mode).
    pub alpha_cutoff: f32,
    /// Whether this material is double-sided.
    pub double_sided: bool,
    /// Texture references (paths or asset IDs). Empty = no texture.
    pub textures: MaterialTextures,
    /// Physics properties for this material.
    pub physics: MaterialPhysics,
}

impl Default for Material {
    fn default() -> Self {
        static NEXT_ID: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);
        Self {
            id: NEXT_ID.fetch_add(1, std::sync::atomic::Ordering::Relaxed),
            name: "Default Material".into(),
            base_color: Vec4::new(0.8, 0.8, 0.8, 1.0),
            metallic: 0.0,
            roughness: 0.5,
            emissive: Vec3::ZERO,
            emissive_intensity: 1.0,
            normal_strength: 1.0,
            ao_strength: 1.0,
            alpha_mode: AlphaMode::Opaque,
            alpha_cutoff: 0.5,
            double_sided: false,
            textures: MaterialTextures::default(),
            physics: MaterialPhysics::default(),
        }
    }
}

impl Material {
    /// Create a simple colored material.
    pub fn color(r: f32, g: f32, b: f32) -> Self {
        Self {
            base_color: Vec4::new(r, g, b, 1.0),
            ..Default::default()
        }
    }

    /// Create a metallic material (e.g., steel, gold).
    pub fn metal(r: f32, g: f32, b: f32, roughness: f32) -> Self {
        Self {
            base_color: Vec4::new(r, g, b, 1.0),
            metallic: 1.0,
            roughness,
            ..Default::default()
        }
    }

    /// Create a glass/transparent material.
    pub fn glass(opacity: f32) -> Self {
        Self {
            base_color: Vec4::new(0.9, 0.95, 1.0, opacity),
            metallic: 0.0,
            roughness: 0.05,
            alpha_mode: AlphaMode::Blend,
            ..Default::default()
        }
    }

    /// Create an emissive material (glowing).
    pub fn emissive(r: f32, g: f32, b: f32, intensity: f32) -> Self {
        Self {
            base_color: Vec4::new(r, g, b, 1.0),
            emissive: Vec3::new(r, g, b),
            emissive_intensity: intensity,
            ..Default::default()
        }
    }

    /// Display label.
    pub fn label(&self) -> &str {
        &self.name
    }
}

// ---------------------------------------------------------------------------
// Alpha mode
// ---------------------------------------------------------------------------

/// How transparency is handled for a material.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AlphaMode {
    /// Fully opaque (fastest).
    Opaque,
    /// Binary transparent: alpha < cutoff = invisible (good for foliage).
    Mask,
    /// Smooth transparency (most expensive, requires sorting).
    Blend,
}

impl Default for AlphaMode {
    fn default() -> Self {
        Self::Opaque
    }
}

// ---------------------------------------------------------------------------
// Material textures (references, not loaded data)
// ---------------------------------------------------------------------------

/// Texture slot references for a material.
/// These are paths or asset IDs - the actual texture data is loaded
/// by the asset system and uploaded to GPU by the render backend.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct MaterialTextures {
    /// Base color / albedo texture path.
    pub base_color_map: Option<String>,
    /// Normal map path.
    pub normal_map: Option<String>,
    /// Metallic-roughness map path (R=unused, G=roughness, B=metallic).
    pub metallic_roughness_map: Option<String>,
    /// Emissive map path.
    pub emissive_map: Option<String>,
    /// Ambient occlusion map path.
    pub ao_map: Option<String>,
    /// Height/displacement map path (for parallax or tessellation).
    pub height_map: Option<String>,
}

// ---------------------------------------------------------------------------
// Material physics (for simulation)
// ---------------------------------------------------------------------------

/// Physical properties of a material surface.
/// Used by physics engine and animation collision system.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MaterialPhysics {
    /// Friction coefficient (0.0 = ice, 1.0 = rubber).
    pub friction: f32,
    /// Restitution / bounciness (0.0 = no bounce, 1.0 = perfect bounce).
    pub restitution: f32,
    /// Density in kg/m^3 (affects mass calculation).
    pub density: f32,
    /// Whether this material is destructible.
    pub destructible: bool,
    /// Hardness (affects damage, deformation, sound on impact).
    pub hardness: f32,
    /// Sound type on impact (for audio system to pick the right sound).
    pub impact_sound: ImpactSoundType,
}

impl Default for MaterialPhysics {
    fn default() -> Self {
        Self {
            friction: 0.5,
            restitution: 0.3,
            density: 1000.0, // Water density as baseline
            destructible: false,
            hardness: 0.5,
            impact_sound: ImpactSoundType::Generic,
        }
    }
}

/// What sound to play when something hits this material.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ImpactSoundType {
    Generic,
    Metal,
    Wood,
    Stone,
    Glass,
    Dirt,
    Water,
    Flesh,
    Cloth,
}

impl Default for ImpactSoundType {
    fn default() -> Self {
        Self::Generic
    }
}

impl ImpactSoundType {
    pub fn label(&self) -> &'static str {
        match self {
            Self::Generic => "Generic",
            Self::Metal => "Metal",
            Self::Wood => "Wood",
            Self::Stone => "Stone",
            Self::Glass => "Glass",
            Self::Dirt => "Dirt",
            Self::Water => "Water",
            Self::Flesh => "Flesh",
            Self::Cloth => "Cloth",
        }
    }

    pub fn label_es(&self) -> &'static str {
        match self {
            Self::Generic => "Generico",
            Self::Metal => "Metal",
            Self::Wood => "Madera",
            Self::Stone => "Piedra",
            Self::Glass => "Vidrio",
            Self::Dirt => "Tierra",
            Self::Water => "Agua",
            Self::Flesh => "Carne",
            Self::Cloth => "Tela",
        }
    }
}

// ---------------------------------------------------------------------------
// Material library (collection of materials)
// ---------------------------------------------------------------------------

/// A collection of materials for a project.
#[derive(Debug, Clone, Default)]
pub struct MaterialLibrary {
    /// All materials.
    pub materials: Vec<Material>,
}

impl MaterialLibrary {
    pub fn new() -> Self {
        Self {
            materials: vec![Material::default()],
        }
    }

    /// Get material by index.
    pub fn get(&self, idx: usize) -> Option<&Material> {
        self.materials.get(idx)
    }

    /// Add a material, returns its index.
    pub fn add(&mut self, mat: Material) -> usize {
        let idx = self.materials.len();
        self.materials.push(mat);
        idx
    }

    /// Find material by name.
    pub fn find_by_name(&self, name: &str) -> Option<usize> {
        self.materials.iter().position(|m| m.name == name)
    }

    /// Number of materials.
    pub fn len(&self) -> usize {
        self.materials.len()
    }

    /// Is empty.
    pub fn is_empty(&self) -> bool {
        self.materials.is_empty()
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_material() {
        let mat = Material::default();
        assert_eq!(mat.metallic, 0.0);
        assert_eq!(mat.alpha_mode, AlphaMode::Opaque);
    }

    #[test]
    fn metal_material() {
        let mat = Material::metal(1.0, 0.8, 0.2, 0.3);
        assert_eq!(mat.metallic, 1.0);
        assert_eq!(mat.roughness, 0.3);
    }

    #[test]
    fn material_library() {
        let mut lib = MaterialLibrary::new();
        let idx = lib.add(Material::metal(1.0, 0.0, 0.0, 0.5));
        assert!(lib.get(idx).is_some());
    }
}
