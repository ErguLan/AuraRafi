//! Game world container wrapping the hecs ECS.
//!
//! The [`GameWorld`] is the central data store for all entities and components
//! in a scene. It delegates to hecs for storage and query, keeping the hot
//! path allocation-free and cache-friendly.

use glam::Vec3;
use hecs::{Entity, World};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

// ---------------------------------------------------------------------------
// Common components
// ---------------------------------------------------------------------------

/// Human-readable label for an entity.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NameComponent {
    pub name: String,
}

/// Spatial transform: position, rotation (Euler degrees), and scale.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TransformComponent {
    pub position: Vec3,
    pub rotation: Vec3,
    pub scale: Vec3,
}

impl Default for TransformComponent {
    fn default() -> Self {
        Self {
            position: Vec3::ZERO,
            rotation: Vec3::ZERO,
            scale: Vec3::ONE,
        }
    }
}

/// Tag that stores a stable, serializable identifier for an entity.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct EntityId(pub Uuid);

impl EntityId {
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }
}

impl Default for EntityId {
    fn default() -> Self {
        Self::new()
    }
}

/// Visibility toggle.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VisibleComponent {
    pub visible: bool,
}

impl Default for VisibleComponent {
    fn default() -> Self {
        Self { visible: true }
    }
}

// ---------------------------------------------------------------------------
// GameWorld
// ---------------------------------------------------------------------------

/// Central ECS world. Thin wrapper around [`hecs::World`] with convenience
/// methods that mirror the command bus operations.
pub struct GameWorld {
    world: World,
}

impl GameWorld {
    /// Create an empty world.
    pub fn new() -> Self {
        Self {
            world: World::new(),
        }
    }

    /// Spawn a default entity with a name, transform, id, and visibility.
    pub fn spawn_named(&mut self, name: &str) -> Entity {
        self.world.spawn((
            EntityId::new(),
            NameComponent {
                name: name.to_string(),
            },
            TransformComponent::default(),
            VisibleComponent::default(),
        ))
    }

    /// Spawn an entity with an arbitrary component bundle.
    pub fn spawn<B: hecs::DynamicBundle>(&mut self, bundle: B) -> Entity {
        self.world.spawn(bundle)
    }

    /// Despawn an entity.
    pub fn despawn(&mut self, entity: Entity) -> Result<(), hecs::NoSuchEntity> {
        self.world.despawn(entity)
    }

    /// Immutable access to the inner hecs world for queries.
    pub fn inner(&self) -> &World {
        &self.world
    }

    /// Mutable access to the inner hecs world.
    pub fn inner_mut(&mut self) -> &mut World {
        &mut self.world
    }

    /// Count of living entities.
    pub fn entity_count(&self) -> u32 {
        self.world.len()
    }
}

impl Default for GameWorld {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spawn_and_query() {
        let mut world = GameWorld::new();
        let entity = world.spawn_named("TestEntity");
        assert_eq!(world.entity_count(), 1);

        let name = world
            .inner()
            .get::<&NameComponent>(entity)
            .expect("should have name");
        assert_eq!(name.name, "TestEntity");
    }

    #[test]
    fn despawn_entity() {
        let mut world = GameWorld::new();
        let entity = world.spawn_named("Temporary");
        assert_eq!(world.entity_count(), 1);
        world.despawn(entity).unwrap();
        assert_eq!(world.entity_count(), 0);
    }
}
