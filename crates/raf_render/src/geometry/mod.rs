//! Geometry layer: pure mesh data and primitive constructors.
//!
//! This module provides the fundamental data structures for 3D geometry
//! (indexed triangle meshes) and constructors for built-in primitives.
//! No rendering logic lives here -- only positions, normals, and indices.
//!
//! Design: all primitives generate indexed triangle meshes with per-vertex
//! normals. Vertices are duplicated where necessary for hard edges (e.g.
//! cube corners, cylinder cap rims) so that normals remain correct for
//! flat shading and future smooth shading.

pub mod mesh_data;
pub mod primitives;
