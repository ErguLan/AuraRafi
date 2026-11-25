//! Scene graph module - parent-child hierarchy, colliders, mesh operations.

pub mod anim_collider;
pub mod animation;
pub mod character;
pub mod collider;
pub mod editable;
pub mod geometry;
pub mod graph;
pub mod ik;
pub mod light;
pub mod merge;
pub mod mesh_asset;
pub mod particles;
pub mod prefab;
pub mod runtime;
pub mod transforms;

pub use anim_collider::{
    AnimCollider, AnimCollisionConfig, AnimCollisionHit, AnimCollisionResponse,
};
pub use character::{CharacterController, CharacterState};
pub use collider::{Aabb, Collider, ColliderType};
pub use editable::{
    EditableMeshData, LinkedAssembly, MeshEditOutcome, PivotMode, SpawnMarker, SurfaceStyle,
    MAX_EDITABLE_TRIANGLES, MAX_EDITABLE_VERTICES,
};
pub use graph::{NodeColor, Primitive, SceneGraph, SceneNode, SceneNodeId};
pub use ik::{IkChain, IkError, IkSolution};
pub use light::{LightKind, SceneLight};
pub use merge::{merge_meshes, weld_vertices, MergedMesh, MeshGroup};
pub use mesh_asset::{normalize_reference, MeshAssetRef};
pub use particles::{
    ParticlePlayback, ParticlePreviewSettings, ParticleQuality, ParticleRuntimeSettings,
    ParticleSystemComponent,
};
pub use runtime::{AudioBus, AudioSource, RigidBody, RigidBodyType, SceneVariable, VariableValue};
pub use transforms::WorldTransformCache;
pub mod joint;
