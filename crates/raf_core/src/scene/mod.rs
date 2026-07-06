//! Scene graph module - parent-child hierarchy, colliders, mesh operations.

pub mod anim_collider;
pub mod collider;
pub mod graph;
pub mod merge;
pub mod runtime;

pub use anim_collider::{
    AnimCollider, AnimCollisionConfig, AnimCollisionHit, AnimCollisionResponse,
};
pub use collider::{Aabb, Collider, ColliderType};
pub use graph::{NodeColor, Primitive, SceneGraph, SceneNode, SceneNodeId};
pub use merge::{merge_meshes, weld_vertices, MergedMesh, MeshGroup};
pub use runtime::{AudioSource, RigidBody, RigidBodyType, SceneVariable, VariableValue};
