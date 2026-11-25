//! # raf_particles
//!
//! Deterministic, data-driven CPU particles for AuraRafi.
//!
//! Effect definitions live in JSON assets ([`ParticleSystemAsset`]). The
//! simulator ([`ParticleSimulator`]) spawns and integrates particles on the
//! CPU and emits [`ParticleSprite`] values with atlas UVs for a renderer. It
//! performs no GPU work and no file I/O.
//!
//! The bundled templates under `assets/particles/templates` are data, not
//! hardcoded Rust effects. Copy one and edit the JSON to author a new effect.

pub mod asset;
pub mod sim;

pub use asset::{
    load_asset, parse_asset, ColorGradient, ColorKey, EmissionShape, Flipbook, NumberCurve,
    NumberKey, ParticleBudget, ParticleCollision, ParticleEmitter, ParticleSystemAsset,
    PARTICLE_ASSET_VERSION,
};
pub use sim::{
    ParticleCamera, ParticleFrame, ParticleSimulator, ParticleSprite, ParticleStats, ParticleUpdate,
};
