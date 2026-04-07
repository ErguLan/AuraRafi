//! Complements - advanced rendering extensions.
//!
//! These modules provide high-fidelity rendering features that are
//! completely optional and zero-cost when disabled. They complement
//! the core render pipeline without being required.
//!
//! All complements follow the same pattern:
//! - Disabled by default (zero CPU/GPU cost)
//! - Activated only when the user explicitly enables them
//! - Designed from day 1 in the architecture (not patched on later)

pub mod complement_trace;

pub use complement_trace::{
    AccelerationStructure, BvhNode, Ray, RayHit,
    RayTraceConfig, RayTraceFeatures, RayTraceMode,
};
