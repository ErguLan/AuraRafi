//! Scene graph module - parent-child hierarchy, colliders, mesh operations.

pub mod anim_collider;
pub mod character;
pub mod collider;
pub mod graph;
pub mod geometry;
pub mod ik;
pub mod merge;
pub mod prefab;
pub mod runtime;
pub mod transforms;

pub use anim_collider::{
    AnimCollider, AnimCollisionConfig, AnimCollisionHit, AnimCollisionResponse,
};
pub use collider::{Aabb, Collider, ColliderType};
pub use character::{CharacterController, CharacterState};
pub use graph::{NodeColor, Primitive, SceneGraph, SceneNode, SceneNodeId};
pub use ik::{IkChain, IkError, IkSolution};
pub use merge::{merge_meshes, weld_vertices, MergedMesh, MeshGroup};
pub use runtime::{AudioSource, RigidBody, RigidBodyType, SceneVariable, VariableValue};
pub use transforms::WorldTransformCache;
pub mod joint;
