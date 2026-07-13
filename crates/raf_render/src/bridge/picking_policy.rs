use serde::{Deserialize, Serialize};

use crate::render_config::RenderConfig;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PickingTier {
    RaycastCpu,
    IdBufferGpu,
    Hybrid,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum PickingPriority {
    Background = 0,
    Mesh = 10,
    Gizmo = 20,
    UiOverlay = 30,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PickingLayer {
    World,
    Gizmo,
    EditMesh,
    UiOverlay,
    ElectronicsCanvas,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct PickingLayerMask {
    bits: u32,
}

impl PickingLayerMask {
    pub const WORLD: u32 = 1 << 0;
    pub const GIZMO: u32 = 1 << 1;
    pub const EDIT_MESH: u32 = 1 << 2;
    pub const UI_OVERLAY: u32 = 1 << 3;
    pub const ELECTRONICS_CANVAS: u32 = 1 << 4;

    pub const fn empty() -> Self {
        Self { bits: 0 }
    }

    pub const fn all() -> Self {
        Self {
            bits: Self::WORLD
                | Self::GIZMO
                | Self::EDIT_MESH
                | Self::UI_OVERLAY
                | Self::ELECTRONICS_CANVAS,
        }
    }

    pub const fn world_editing() -> Self {
        Self {
            bits: Self::WORLD | Self::GIZMO | Self::EDIT_MESH | Self::UI_OVERLAY,
        }
    }

    pub fn contains(&self, layer: PickingLayer) -> bool {
        self.bits & layer_bit(layer) != 0
    }

    pub fn insert(&mut self, layer: PickingLayer) {
        self.bits |= layer_bit(layer);
    }
}

impl Default for PickingLayerMask {
    fn default() -> Self {
        Self::world_editing()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct IdBufferSpec {
    pub enabled: bool,
    pub resolution_scale_percent: u8,
    pub readback_ring_frames: u8,
}

impl Default for IdBufferSpec {
    fn default() -> Self {
        Self {
            enabled: false,
            resolution_scale_percent: 100,
            readback_ring_frames: 3,
        }
    }
}

impl IdBufferSpec {
    pub fn scaled_extent(&self, width: u32, height: u32) -> [u32; 2] {
        let scale = (self.resolution_scale_percent.clamp(25, 100) as f32) / 100.0;
        [
            ((width.max(1) as f32) * scale).ceil().max(1.0) as u32,
            ((height.max(1) as f32) * scale).ceil().max(1.0) as u32,
        ]
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct PickingPolicy {
    pub tier: PickingTier,
    pub layer_mask: PickingLayerMask,
    pub id_buffer: IdBufferSpec,
    pub max_cpu_candidates: usize,
    pub ray_triangle_narrow_phase: bool,
    pub gizmo_priority: PickingPriority,
    pub mesh_priority: PickingPriority,
    pub ui_priority: PickingPriority,
}

impl Default for PickingPolicy {
    fn default() -> Self {
        Self {
            tier: PickingTier::RaycastCpu,
            layer_mask: PickingLayerMask::world_editing(),
            id_buffer: IdBufferSpec::default(),
            max_cpu_candidates: 256,
            ray_triangle_narrow_phase: true,
            gizmo_priority: PickingPriority::Gizmo,
            mesh_priority: PickingPriority::Mesh,
            ui_priority: PickingPriority::UiOverlay,
        }
    }
}

impl PickingPolicy {
    pub fn potato() -> Self {
        Self {
            tier: PickingTier::RaycastCpu,
            max_cpu_candidates: 96,
            id_buffer: IdBufferSpec {
                enabled: false,
                ..IdBufferSpec::default()
            },
            ..Self::default()
        }
    }

    pub fn hybrid_id_buffer() -> Self {
        Self {
            tier: PickingTier::Hybrid,
            id_buffer: IdBufferSpec {
                enabled: true,
                resolution_scale_percent: 100,
                readback_ring_frames: 3,
            },
            ..Self::default()
        }
    }

    /// Derives a bounded CPU picking budget from the active render tier.
    /// GPU ID-buffer selection stays opt-in until a real readback pass exists.
    pub fn for_render_config(config: &RenderConfig) -> Self {
        if config.max_triangles <= 2_000 || config.frame_budget_ms >= 45.0 {
            return Self::potato();
        }

        let mut policy = Self::default();
        policy.max_cpu_candidates = if config.max_triangles <= 20_000 {
            192
        } else if config.max_triangles <= 100_000 {
            384
        } else {
            768
        };
        policy
    }
}

fn layer_bit(layer: PickingLayer) -> u32 {
    match layer {
        PickingLayer::World => PickingLayerMask::WORLD,
        PickingLayer::Gizmo => PickingLayerMask::GIZMO,
        PickingLayer::EditMesh => PickingLayerMask::EDIT_MESH,
        PickingLayer::UiOverlay => PickingLayerMask::UI_OVERLAY,
        PickingLayer::ElectronicsCanvas => PickingLayerMask::ELECTRONICS_CANVAS,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_policy_prioritizes_ui_and_gizmo_over_meshes() {
        let policy = PickingPolicy::default();

        assert!(policy.layer_mask.contains(PickingLayer::World));
        assert!(policy.layer_mask.contains(PickingLayer::Gizmo));
        assert!(policy.ui_priority > policy.gizmo_priority);
        assert!(policy.gizmo_priority > policy.mesh_priority);
    }

    #[test]
    fn id_buffer_extent_clamps_scale() {
        let spec = IdBufferSpec {
            enabled: true,
            resolution_scale_percent: 10,
            readback_ring_frames: 3,
        };

        assert_eq!(spec.scaled_extent(100, 80), [25, 20]);
    }

    #[test]
    fn picking_budget_tracks_render_resource_tier() {
        assert_eq!(
            PickingPolicy::for_render_config(&RenderConfig::potato()).max_cpu_candidates,
            96
        );
        assert_eq!(
            PickingPolicy::for_render_config(&RenderConfig::medium()).max_cpu_candidates,
            384
        );
    }
}
