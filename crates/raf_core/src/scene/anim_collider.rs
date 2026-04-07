//! Animation-aware collision system.
//!
//! Prevents the classic problem: animations clipping through objects.
//! Instead of playing a full punch/kick/mount animation blindly,
//! the system checks colliders during playback and reacts:
//! - Stop the animation at the collision point
//! - Blend to a "hit" pose
//! - Slide along the surface
//!
//! **Strategy**: enabled by DEFAULT. Other engines make you configure this
//! manually per animation. AuraRafi does it automatically.
//!
//! **Status**: Structure prepared. Requires animation system (v0.8.0+).
//! When animations are implemented, these colliders attach to bones/keyframes
//! and the playback system calls `check_collision()` each animation step.

use glam::Vec3;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

// ---------------------------------------------------------------------------
// Collision response (what happens when animation hits something)
// ---------------------------------------------------------------------------

/// What happens when an animated limb/part collides with something.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AnimCollisionResponse {
    /// Stop the animation at the collision point (punch hits wall = fist stops).
    Stop,
    /// Blend to a pre-defined "contact" pose (hand grabs ledge, foot lands).
    BlendToContact,
    /// Slide along the collision surface (sword slash slides on armor).
    Slide,
    /// Bounce back slightly (kick hits shield, leg recoils).
    Recoil,
    /// Ignore collision (for effects-only animations like magic auras).
    Ignore,
}

impl Default for AnimCollisionResponse {
    fn default() -> Self {
        Self::Stop // Safest default: just stop at contact point.
    }
}

impl AnimCollisionResponse {
    pub fn label(&self) -> &'static str {
        match self {
            Self::Stop => "Stop",
            Self::BlendToContact => "Blend to contact",
            Self::Slide => "Slide",
            Self::Recoil => "Recoil",
            Self::Ignore => "Ignore",
        }
    }

    pub fn label_es(&self) -> &'static str {
        match self {
            Self::Stop => "Detener",
            Self::BlendToContact => "Mezclar al contacto",
            Self::Slide => "Deslizar",
            Self::Recoil => "Rebote",
            Self::Ignore => "Ignorar",
        }
    }
}

// ---------------------------------------------------------------------------
// Animation collider (attached to a bone or animation channel)
// ---------------------------------------------------------------------------

/// A collider that moves with an animation bone/channel.
/// When the animation plays, this collider is checked against the world
/// at each animation step. If it hits something, the response triggers.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnimCollider {
    /// Unique ID.
    pub id: Uuid,
    /// Name (e.g., "right_fist", "left_foot", "sword_tip").
    pub name: String,
    /// Which bone/channel this collider is attached to.
    /// Index into the animation's bone list (future).
    pub bone_index: usize,
    /// Collider shape: sphere radius from bone position.
    /// Sphere is cheapest to test. Radius in local units.
    pub radius: f32,
    /// Offset from bone position (local space).
    pub offset: Vec3,
    /// What happens on collision.
    pub response: AnimCollisionResponse,
    /// Whether this collider is active (can be toggled per-animation).
    pub active: bool,
    /// Damage or force value (for gameplay, optional).
    pub impact_force: f32,
    /// Layer mask: which collision layers this interacts with.
    /// 0 = all layers. Bit mask for selective collision.
    pub layer_mask: u32,
}

impl AnimCollider {
    /// Create a basic animation collider.
    pub fn new(name: &str, bone_index: usize, radius: f32) -> Self {
        Self {
            id: Uuid::new_v4(),
            name: name.to_string(),
            bone_index,
            radius,
            offset: Vec3::ZERO,
            response: AnimCollisionResponse::Stop,
            active: true,
            impact_force: 1.0,
            layer_mask: 0,
        }
    }

    /// Check collision with a world-space point (simplified).
    /// Returns true if the animated collider position is within radius of the point.
    /// `bone_world_pos` is the bone's current world position from the animation.
    pub fn check_point(&self, bone_world_pos: Vec3, target_pos: Vec3) -> bool {
        if !self.active {
            return false;
        }
        let collider_pos = bone_world_pos + self.offset;
        (collider_pos - target_pos).length_squared() < self.radius * self.radius
    }

    /// Check collision with a sphere (another collider or entity bounds).
    pub fn check_sphere(&self, bone_world_pos: Vec3, target_pos: Vec3, target_radius: f32) -> bool {
        if !self.active {
            return false;
        }
        let collider_pos = bone_world_pos + self.offset;
        let combined_radius = self.radius + target_radius;
        (collider_pos - target_pos).length_squared() < combined_radius * combined_radius
    }
}

// ---------------------------------------------------------------------------
// Animation collision config
// ---------------------------------------------------------------------------

/// Global config for animation collision system.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnimCollisionConfig {
    /// Master switch. ON by default (AuraRafi's differentiator).
    pub enabled: bool,
    /// Check frequency: every N animation steps (1 = every step, 2 = every other).
    /// Higher = cheaper but less precise.
    pub check_every_n_steps: u32,
    /// Default response for colliders that don't specify one.
    pub default_response: AnimCollisionResponse,
    /// Whether to auto-generate colliders for common bones.
    /// When true, the system creates colliders for hands, feet, head
    /// when an animation is loaded. The user can then customize them.
    pub auto_generate: bool,
    /// Maximum colliders per animation (prevents spam).
    pub max_colliders_per_anim: usize,
    /// Whether to show debug spheres for animation colliders in editor.
    pub show_debug: bool,
}

impl Default for AnimCollisionConfig {
    fn default() -> Self {
        Self {
            enabled: true,          // ON by default - our marketing advantage
            check_every_n_steps: 1, // Every step (precise)
            default_response: AnimCollisionResponse::Stop,
            auto_generate: true,    // Auto-create for hands/feet
            max_colliders_per_anim: 8,
            show_debug: false,
        }
    }
}

// ---------------------------------------------------------------------------
// Collision hit result
// ---------------------------------------------------------------------------

/// Result of an animation collision check.
#[derive(Debug, Clone)]
pub struct AnimCollisionHit {
    /// Which collider triggered.
    pub collider_name: String,
    /// World position of the hit point.
    pub hit_position: Vec3,
    /// Which entity was hit (scene node index).
    pub hit_entity: usize,
    /// The response to execute.
    pub response: AnimCollisionResponse,
    /// At what fraction of the animation step the hit occurred (0.0 - 1.0).
    /// 0.0 = start of step, 1.0 = end of step.
    pub hit_fraction: f32,
    /// Impact force from the collider.
    pub impact_force: f32,
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_config_enabled() {
        let config = AnimCollisionConfig::default();
        assert!(config.enabled);
        assert!(config.auto_generate);
    }

    #[test]
    fn collider_point_check() {
        let collider = AnimCollider::new("fist", 0, 0.5);
        let bone_pos = Vec3::new(0.0, 1.0, 0.0);
        // Target within radius.
        assert!(collider.check_point(bone_pos, Vec3::new(0.0, 1.3, 0.0)));
        // Target outside radius.
        assert!(!collider.check_point(bone_pos, Vec3::new(0.0, 5.0, 0.0)));
    }

    #[test]
    fn collider_sphere_check() {
        let collider = AnimCollider::new("foot", 0, 0.3);
        let bone_pos = Vec3::ZERO;
        // Overlapping spheres.
        assert!(collider.check_sphere(bone_pos, Vec3::new(0.5, 0.0, 0.0), 0.3));
        // Non-overlapping.
        assert!(!collider.check_sphere(bone_pos, Vec3::new(5.0, 0.0, 0.0), 0.3));
    }

    #[test]
    fn inactive_collider_never_hits() {
        let mut collider = AnimCollider::new("sword", 0, 10.0);
        collider.active = false;
        assert!(!collider.check_point(Vec3::ZERO, Vec3::ZERO));
    }
}
