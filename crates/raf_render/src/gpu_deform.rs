//! GPU vertex deformation interface.
//!
//! Defines how vertex data can be deformed on the GPU for:
//! - Cloth simulation (flags, capes, curtains)
//! - Hair/fur simulation (strand-based, wind-reactive)
//! - Vegetation (grass bending, tree sway)
//! - Water surface (wave displacement)
//! - Skeletal animation (bone transforms applied on GPU)
//!
//! The CPU never touches per-vertex data for these effects.
//! Instead, it sends parameters (wind direction, bone matrices)
//! and the GPU compute shader handles all vertex movement.
//!
//! Zero cost: just data types. No GPU work until a wgpu backend is active
//! and a deformer is attached to a mesh.

use glam::Vec3;
use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// Deformer types
// ---------------------------------------------------------------------------

/// Type of GPU deformation to apply.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum DeformerType {
    /// No deformation.
    None,
    /// Cloth simulation (spring-mass model on GPU).
    Cloth,
    /// Hair/fur strands (Euler integration per strand).
    Hair,
    /// Vegetation sway (wind-based sine displacement).
    Vegetation,
    /// Water surface (sum of sine waves / FFT ocean).
    Water,
    /// Skeletal (bone matrix palette applied per-vertex).
    Skeletal,
    /// Blend shapes / morph targets.
    BlendShape,
    /// Custom compute shader.
    Custom,
}

impl Default for DeformerType {
    fn default() -> Self {
        Self::None
    }
}

impl DeformerType {
    pub fn label(&self) -> &'static str {
        match self {
            Self::None => "None",
            Self::Cloth => "Cloth",
            Self::Hair => "Hair",
            Self::Vegetation => "Vegetation",
            Self::Water => "Water",
            Self::Skeletal => "Skeletal",
            Self::BlendShape => "Blend Shape",
            Self::Custom => "Custom",
        }
    }

    pub fn label_es(&self) -> &'static str {
        match self {
            Self::None => "Ninguno",
            Self::Cloth => "Tela",
            Self::Hair => "Cabello",
            Self::Vegetation => "Vegetacion",
            Self::Water => "Agua",
            Self::Skeletal => "Esqueletal",
            Self::BlendShape => "Forma combinada",
            Self::Custom => "Personalizado",
        }
    }
}

// ---------------------------------------------------------------------------
// Deformer data (parameters sent to GPU)
// ---------------------------------------------------------------------------

/// Parameters for a GPU deformer attached to a mesh.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GpuDeformer {
    /// Type of deformation.
    pub deformer_type: DeformerType,
    /// Whether this deformer is active.
    pub active: bool,
    /// Wind direction (world space). Used by cloth, hair, vegetation.
    pub wind_direction: Vec3,
    /// Wind strength (0.0 = calm, 1.0 = strong).
    pub wind_strength: f32,
    /// Wind turbulence (randomness).
    pub wind_turbulence: f32,
    /// Gravity strength (negative Y typically).
    pub gravity: f32,
    /// Stiffness (how much the material resists deformation).
    /// 0.0 = completely floppy, 1.0 = rigid.
    pub stiffness: f32,
    /// Damping (how quickly oscillations die out).
    pub damping: f32,
    /// Time accumulator for animation (seconds).
    pub time: f32,
    /// Frequency of oscillation (for vegetation sway, water waves).
    pub frequency: f32,
    /// Amplitude of displacement.
    pub amplitude: f32,
}

impl Default for GpuDeformer {
    fn default() -> Self {
        Self {
            deformer_type: DeformerType::None,
            active: false,
            wind_direction: Vec3::new(1.0, 0.0, 0.0),
            wind_strength: 0.3,
            wind_turbulence: 0.1,
            gravity: -9.81,
            stiffness: 0.5,
            damping: 0.9,
            time: 0.0,
            frequency: 1.0,
            amplitude: 0.1,
        }
    }
}

impl GpuDeformer {
    /// Create a cloth deformer.
    pub fn cloth(stiffness: f32, damping: f32) -> Self {
        Self {
            deformer_type: DeformerType::Cloth,
            active: true,
            stiffness,
            damping,
            ..Default::default()
        }
    }

    /// Create a vegetation sway deformer.
    pub fn vegetation(frequency: f32, amplitude: f32) -> Self {
        Self {
            deformer_type: DeformerType::Vegetation,
            active: true,
            frequency,
            amplitude,
            stiffness: 0.8,
            ..Default::default()
        }
    }

    /// Create a water surface deformer.
    pub fn water(wave_frequency: f32, wave_height: f32) -> Self {
        Self {
            deformer_type: DeformerType::Water,
            active: true,
            frequency: wave_frequency,
            amplitude: wave_height,
            ..Default::default()
        }
    }

    /// Advance time (call each frame with delta_time).
    pub fn tick(&mut self, dt: f32) {
        if self.active {
            self.time += dt;
        }
    }

    /// Estimated GPU memory overhead for this deformer (bytes).
    /// The actual compute buffer size depends on vertex count.
    pub fn gpu_overhead_per_vertex(&self) -> usize {
        match self.deformer_type {
            DeformerType::None => 0,
            DeformerType::Cloth => 48,     // position + velocity + prev_pos
            DeformerType::Hair => 32,      // position + velocity
            DeformerType::Vegetation => 4, // Just displacement offset
            DeformerType::Water => 4,      // Height displacement
            DeformerType::Skeletal => 16,  // Bone indices + weights
            DeformerType::BlendShape => 12, // Delta position per target
            DeformerType::Custom => 16,
        }
    }
}

// ---------------------------------------------------------------------------
// Deformer config
// ---------------------------------------------------------------------------

/// Global configuration for GPU deformation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GpuDeformConfig {
    /// Whether GPU deformation is enabled globally.
    pub enabled: bool,
    /// Maximum deformed meshes active at once.
    pub max_deformed_meshes: usize,
    /// Maximum total deformed vertices (GPU budget).
    pub max_deformed_vertices: usize,
    /// Global wind direction (affects all wind-reactive deformers).
    pub global_wind: Vec3,
    /// Global wind strength.
    pub global_wind_strength: f32,
}

impl Default for GpuDeformConfig {
    fn default() -> Self {
        Self {
            enabled: false, // Off until GPU backend is active
            max_deformed_meshes: 16,
            max_deformed_vertices: 20_000,
            global_wind: Vec3::new(1.0, 0.0, 0.3),
            global_wind_strength: 0.2,
        }
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_deformer_inactive() {
        let d = GpuDeformer::default();
        assert!(!d.active);
        assert_eq!(d.deformer_type, DeformerType::None);
    }

    #[test]
    fn cloth_factory() {
        let d = GpuDeformer::cloth(0.8, 0.95);
        assert!(d.active);
        assert_eq!(d.deformer_type, DeformerType::Cloth);
    }

    #[test]
    fn tick_advances_time() {
        let mut d = GpuDeformer::vegetation(2.0, 0.5);
        d.tick(0.016);
        assert!(d.time > 0.0);
    }

    #[test]
    fn config_default_disabled() {
        let config = GpuDeformConfig::default();
        assert!(!config.enabled);
    }
}
