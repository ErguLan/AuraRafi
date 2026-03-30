//! Scene graph module - parent-child hierarchy, colliders, mesh operations.

pub mod collider;
pub mod graph;
pub mod merge;

pub use graph::{NodeColor, Primitive, SceneGraph, SceneNode, SceneNodeId};
pub use collider::{Aabb, Collider, ColliderType};
pub use merge::{MeshGroup, MergedMesh, merge_meshes, weld_vertices};
