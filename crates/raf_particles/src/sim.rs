//! Deterministic CPU particle simulation.
//!
//! The simulator owns no GPU resources and performs no file I/O. It produces
//! billboard sprites with position, size, tint, atlas UVs and camera depth so a
//! renderer can consume them directly.

use std::f32::consts::TAU;

use glam::{Mat4, Vec3};
use serde::{Deserialize, Serialize};

use crate::asset::{EmissionShape, Flipbook, ParticleEmitter};

const DEFAULT_SEED: u64 = 0x9E37_79B9_7F4A_7C15;
const MAX_DT: f32 = 0.1;
const MAX_SCALED_DT: f32 = 0.25;
const MAX_TIME_SCALE: f32 = 10.0;
const MAX_RATE: f32 = 100_000.0;

/// Per-frame controls supplied by gameplay, the editor or the player.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ParticleUpdate {
    /// When false, no new particles are spawned but existing ones keep ageing.
    pub enabled: bool,
    /// Freezes time and spawning while keeping particles visible.
    pub paused: bool,
    /// Clears and hides the system. The next active frame starts with a burst.
    pub stopped: bool,
    /// Explicit freeze, equivalent to [`ParticleUpdate::paused`].
    pub frozen: bool,
    pub time_scale: f32,
    /// Multiplier applied to every emitter rate.
    pub rate_scale: f32,
    /// One-shot particles requested this update, distributed across enabled
    /// emitters. Budgets still apply.
    pub emit: u32,
    /// Additional runtime particle cap. `0` uses the asset budget.
    pub max_particles: u32,
    /// Additional runtime sprite cap. `0` means "use the asset budget".
    pub max_sprites: u32,
    pub wind: [f32; 3],
}

impl Default for ParticleUpdate {
    fn default() -> Self {
        Self {
            enabled: true,
            paused: false,
            stopped: false,
            frozen: false,
            time_scale: 1.0,
            rate_scale: 1.0,
            emit: 0,
            max_particles: 0,
            max_sprites: 0,
            wind: [0.0, 0.0, 0.0],
        }
    }
}

/// Camera data needed for depth sorting. Billboard orientation stays a
/// renderer concern; `right`/`up` are carried for consumers that need them.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ParticleCamera {
    pub position: [f32; 3],
    pub right: [f32; 3],
    pub up: [f32; 3],
}

impl Default for ParticleCamera {
    fn default() -> Self {
        Self {
            position: [0.0, 0.0, 0.0],
            right: [1.0, 0.0, 0.0],
            up: [0.0, 1.0, 0.0],
        }
    }
}

/// A single billboard ready to be drawn by the renderer.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ParticleSprite {
    pub position: [f32; 3],
    pub size: [f32; 2],
    pub rotation: f32,
    pub color: [u8; 4],
    /// `u0, v0, u1, v1`, always inside `0..=1`.
    pub uv: [f32; 4],
    /// Distance to the camera. Sprites are returned far-to-near.
    pub depth: f32,
    pub emitter: u32,
}

/// Per-frame simulation counters.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct ParticleStats {
    /// Live particles after this update.
    pub active: u32,
    /// Particles spawned during this update.
    pub spawned: u32,
    /// Spawn attempts rejected by particle or per-emitter budgets this update.
    pub capped: u32,
    /// Number of emitters in the asset, including disabled ones.
    pub emitters: u32,
}

/// Output of one simulation step.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ParticleFrame {
    pub sprites: Vec<ParticleSprite>,
    pub stats: ParticleStats,
}

/// Deterministic particle simulator. Create one per particle system instance.
#[derive(Debug, Clone)]
pub struct ParticleSimulator {
    rng: XorShift64,
    initial_seed: u64,
    emitters: Vec<EmitterState>,
}

#[derive(Debug, Clone, Default)]
struct EmitterState {
    name: String,
    particles: Vec<Particle>,
    accumulator: f32,
    primed: bool,
}

#[derive(Debug, Clone, Copy)]
struct Particle {
    position: Vec3,
    velocity: Vec3,
    age: f32,
    lifetime: f32,
    flip_offset: u32,
}

#[derive(Debug, Clone)]
struct XorShift64 {
    state: u64,
}

impl ParticleSimulator {
    pub fn new() -> Self {
        Self::with_seed(DEFAULT_SEED)
    }

    /// Creates a simulator with an explicit seed for reproducible simulations.
    pub fn with_seed(seed: u64) -> Self {
        let mut simulator = Self {
            rng: XorShift64::new(seed),
            initial_seed: seed,
            emitters: Vec::new(),
        };
        simulator.reset();
        simulator
    }

    /// Restores the initial seed and clears all particles, accumulators and
    /// burst state. The next update bursts again.
    pub fn reset(&mut self) {
        self.rng = XorShift64::new(self.initial_seed);
        for state in &mut self.emitters {
            state.particles.clear();
            state.accumulator = 0.0;
            state.primed = false;
        }
    }

    /// Changes the seed and resets the simulation.
    pub fn set_seed(&mut self, seed: u64) {
        self.initial_seed = seed;
        self.reset();
    }

    pub fn seed(&self) -> u64 {
        self.initial_seed
    }

    /// Live particles across all emitters.
    pub fn active_particles(&self) -> usize {
        self.emitters
            .iter()
            .map(|state| state.particles.len())
            .sum()
    }

    /// Advances the simulation and returns draw-ready sprites.
    ///
    /// `transform` maps emitter-local positions and directions to world space.
    /// Gravity, wind and collision planes are world-space.
    pub fn update(
        &mut self,
        asset: &crate::ParticleSystemAsset,
        transform: Mat4,
        camera: &ParticleCamera,
        dt: f32,
        update: &ParticleUpdate,
    ) -> ParticleFrame {
        self.sync_emitters(asset);

        let emitter_count = asset.emitters.len() as u32;

        if update.stopped {
            for state in &mut self.emitters {
                state.particles.clear();
                state.accumulator = 0.0;
                state.primed = false;
            }
            return ParticleFrame {
                sprites: Vec::new(),
                stats: ParticleStats {
                    active: 0,
                    spawned: 0,
                    capped: 0,
                    emitters: emitter_count,
                },
            };
        }

        let raw_dt = if dt.is_finite() {
            dt.clamp(0.0, MAX_DT)
        } else {
            0.0
        };
        let time_scale = if update.time_scale.is_finite() {
            update.time_scale.clamp(0.0, MAX_TIME_SCALE)
        } else {
            1.0
        };
        let sim_dt = (raw_dt * time_scale).min(MAX_SCALED_DT);

        let paused = update.paused || update.frozen;
        let wind = {
            let wind = Vec3::from_array(update.wind);
            if wind.is_finite() {
                wind
            } else {
                Vec3::ZERO
            }
        };

        if !paused && sim_dt > 0.0 {
            for (state, emitter) in self.emitters.iter_mut().zip(asset.emitters.iter()) {
                advance_particles(state, emitter, sim_dt, wind);
            }
        }

        let mut spawned = 0u32;
        let mut capped = 0u32;

        if !paused && update.enabled {
            let particle_budget = asset.budget.max_particles as usize;
            let particle_budget = if update.max_particles == 0 {
                particle_budget
            } else {
                particle_budget.min(update.max_particles as usize)
            };
            let mut total_active: usize = self
                .emitters
                .iter()
                .map(|state| state.particles.len())
                .sum();

            let rng = &mut self.rng;
            for (state, emitter) in self.emitters.iter_mut().zip(asset.emitters.iter()) {
                if !emitter.enabled {
                    continue;
                }

                if !state.primed {
                    let wanted = emitter.burst as usize;
                    let available = particle_budget.saturating_sub(total_active).min(
                        (emitter.max_particles as usize).saturating_sub(state.particles.len()),
                    );
                    let count = wanted.min(available);

                    for _ in 0..count {
                        spawn_particle(rng, emitter, transform, state);
                    }

                    spawned = spawned.saturating_add(count as u32);
                    capped = capped.saturating_add((wanted - count) as u32);
                    total_active += count;
                    state.primed = true;
                }

                let rate = if emitter.rate.is_finite() {
                    emitter.rate.clamp(0.0, MAX_RATE)
                } else {
                    0.0
                };
                let rate_scale = if update.rate_scale.is_finite() {
                    update.rate_scale.clamp(0.0, 8.0)
                } else {
                    1.0
                };
                let rate = rate * rate_scale;
                if sim_dt > 0.0 && rate > 0.0 {
                    state.accumulator += rate * sim_dt;
                    let due = state.accumulator.floor();
                    if due >= 1.0 {
                        state.accumulator -= due;
                        let due = due.min(u32::MAX as f32) as u32;
                        let available = particle_budget
                            .saturating_sub(total_active)
                            .min(
                                (emitter.max_particles as usize)
                                    .saturating_sub(state.particles.len()),
                            )
                            .min(u32::MAX as usize) as u32;
                        let count = due.min(available);

                        for _ in 0..count {
                            spawn_particle(rng, emitter, transform, state);
                        }

                        spawned = spawned.saturating_add(count);
                        capped = capped.saturating_add(due - count);
                        total_active += count as usize;
                    }
                }
            }
        }

        if !paused && update.enabled && update.emit > 0 {
            let particle_budget = asset.budget.max_particles as usize;
            let particle_budget = if update.max_particles == 0 {
                particle_budget
            } else {
                particle_budget.min(update.max_particles as usize)
            };
            let mut total_active: usize = self
                .emitters
                .iter()
                .map(|state| state.particles.len())
                .sum();
            let enabled = asset
                .emitters
                .iter()
                .enumerate()
                .filter(|(_, emitter)| emitter.enabled)
                .map(|(index, _)| index)
                .collect::<Vec<_>>();
            if enabled.is_empty() {
                capped = capped.saturating_add(update.emit);
            } else {
                let mut remaining = update.emit as usize;
                let rng = &mut self.rng;
                let mut cursor = 0usize;
                while remaining > 0 && total_active < particle_budget {
                    let mut spawned_any = false;
                    for offset in 0..enabled.len() {
                        let index = enabled[(cursor + offset) % enabled.len()];
                        let emitter = &asset.emitters[index];
                        let state = &mut self.emitters[index];
                        if state.particles.len() >= emitter.max_particles as usize {
                            continue;
                        }
                        spawn_particle(rng, emitter, transform, state);
                        spawned = spawned.saturating_add(1);
                        remaining -= 1;
                        total_active += 1;
                        spawned_any = true;
                        if remaining == 0 {
                            break;
                        }
                    }
                    if !spawned_any {
                        break;
                    }
                    cursor = cursor.wrapping_add(1) % enabled.len();
                }
                capped = capped.saturating_add(remaining as u32);
            }
        }

        let active: u32 = self.emitters.iter().fold(0u32, |total, state| {
            total.saturating_add(state.particles.len() as u32)
        });
        let sprites = build_sprites(asset, &self.emitters, camera, update);

        ParticleFrame {
            sprites,
            stats: ParticleStats {
                active,
                spawned,
                capped,
                emitters: emitter_count,
            },
        }
    }

    fn sync_emitters(&mut self, asset: &crate::ParticleSystemAsset) {
        self.emitters.truncate(asset.emitters.len());
        while self.emitters.len() < asset.emitters.len() {
            self.emitters.push(EmitterState::default());
        }

        for (state, emitter) in self.emitters.iter_mut().zip(asset.emitters.iter()) {
            if state.name != emitter.name {
                state.name.clone_from(&emitter.name);
                state.particles.clear();
                state.accumulator = 0.0;
                state.primed = false;
            }
        }
    }
}

impl Default for ParticleSimulator {
    fn default() -> Self {
        Self::new()
    }
}

impl XorShift64 {
    fn new(seed: u64) -> Self {
        let state = if seed == 0 { DEFAULT_SEED } else { seed };
        Self { state }
    }

    fn next_u64(&mut self) -> u64 {
        let mut value = self.state;
        value ^= value << 13;
        value ^= value >> 7;
        value ^= value << 17;
        self.state = value;
        value
    }

    /// Uniform in `[0, 1)`.
    fn next_f32(&mut self) -> f32 {
        ((self.next_u64() >> 40) as f32) / ((1u32 << 24) as f32)
    }

    fn range(&mut self, min: f32, max: f32) -> f32 {
        let min = if min.is_finite() { min } else { 0.0 };
        let max = if max.is_finite() { max } else { min };
        if max <= min {
            min
        } else {
            min + (max - min) * self.next_f32()
        }
    }

    fn unit_vector(&mut self) -> Vec3 {
        let z = self.range(-1.0, 1.0);
        let angle = self.range(0.0, TAU);
        let radius = (1.0 - z * z).max(0.0).sqrt();
        Vec3::new(radius * angle.cos(), radius * angle.sin(), z)
    }
}

fn advance_particles(state: &mut EmitterState, emitter: &ParticleEmitter, dt: f32, wind: Vec3) {
    let gravity =
        Vec3::from_array(
            emitter
                .gravity
                .map(|value| if value.is_finite() { value } else { 0.0 }),
        );
    let drag = if emitter.drag.is_finite() {
        emitter.drag.clamp(0.0, 1000.0)
    } else {
        0.0
    };
    let collision = emitter.collision.and_then(|collision| {
        if collision.ground_y.is_finite() {
            Some((
                collision.ground_y,
                if collision.bounce.is_finite() {
                    collision.bounce.clamp(0.0, 1.0)
                } else {
                    0.0
                },
                collision.kill,
            ))
        } else {
            None
        }
    });

    state.particles.retain_mut(|particle| {
        particle.age += dt;
        if particle.age >= particle.lifetime {
            return false;
        }

        particle.velocity += gravity * dt;

        // With wind-aware drag the particle is damped toward the wind
        // velocity. Otherwise absolute velocity is damped and wind acts as an
        // acceleration. Either way the result is stable for large drag values.
        if emitter.wind_affects_drag {
            let relative = particle.velocity - wind;
            particle.velocity = wind + relative / (1.0 + drag * dt);
        } else {
            particle.velocity += wind * dt;
            particle.velocity /= 1.0 + drag * dt;
        }

        particle.position += particle.velocity * dt;

        if let Some((ground_y, bounce, kill)) = collision {
            if particle.position.y < ground_y {
                if kill {
                    return false;
                }
                particle.position.y = ground_y;
                if particle.velocity.y < 0.0 {
                    particle.velocity.y = -particle.velocity.y * bounce;
                }
            }
        }

        true
    });
}

fn spawn_particle(
    rng: &mut XorShift64,
    emitter: &ParticleEmitter,
    transform: Mat4,
    state: &mut EmitterState,
) {
    let local_position = sample_shape(rng, &emitter.shape);
    let local_direction = sample_direction(rng, emitter.direction, emitter.spread);

    let min_speed = sanitize_non_negative(emitter.speed[0]);
    let max_speed = sanitize_non_negative(emitter.speed[1]).max(min_speed);
    let speed = rng.range(min_speed, max_speed);

    let min_lifetime = sanitize_positive(emitter.lifetime[0], 0.0001);
    let max_lifetime = sanitize_positive(emitter.lifetime[1], min_lifetime).max(min_lifetime);
    let lifetime = rng.range(min_lifetime, max_lifetime);

    let position = transform.transform_point3(local_position);
    let mut direction = transform.transform_vector3(local_direction);
    if !direction.is_finite() || direction.length_squared() <= 1.0e-12 {
        direction = local_direction;
    } else {
        direction = direction.normalize();
    }
    let velocity = direction * speed;

    let flip_offset = match &emitter.flipbook {
        Some(flipbook) if flipbook.start_random => {
            let frames = flipbook_frame_count(flipbook);
            (rng.next_u64() % frames as u64) as u32
        }
        _ => 0,
    };

    state.particles.push(Particle {
        position,
        velocity,
        age: 0.0,
        lifetime,
        flip_offset,
    });
}

fn sample_shape(rng: &mut XorShift64, shape: &EmissionShape) -> Vec3 {
    match shape {
        EmissionShape::Point => Vec3::ZERO,
        EmissionShape::Sphere { radius } => {
            let radius = sanitize_non_negative(*radius);
            rng.unit_vector() * (radius * rng.next_f32().cbrt())
        }
        EmissionShape::Box { half_extents } => Vec3::new(
            rng.range(
                -sanitize_non_negative(half_extents[0]),
                sanitize_non_negative(half_extents[0]),
            ),
            rng.range(
                -sanitize_non_negative(half_extents[1]),
                sanitize_non_negative(half_extents[1]),
            ),
            rng.range(
                -sanitize_non_negative(half_extents[2]),
                sanitize_non_negative(half_extents[2]),
            ),
        ),
        EmissionShape::Cone { radius, height } => {
            let radius = sanitize_non_negative(*radius);
            let height = sanitize_non_negative(*height);

            if height <= 1.0e-6 {
                sample_disc(rng, radius, 0.0)
            } else {
                let u = rng.next_f32();
                let y = height * (1.0 - (1.0 - u).cbrt());
                let max_radius = radius * (1.0 - (y / height)).max(0.0);
                let local_radius = max_radius * rng.next_f32().sqrt();
                let angle = rng.range(0.0, TAU);
                Vec3::new(local_radius * angle.cos(), y, local_radius * angle.sin())
            }
        }
        EmissionShape::Disc { radius, inner } => {
            let radius = sanitize_non_negative(*radius);
            let inner = sanitize_non_negative(*inner).min(radius);
            sample_disc(rng, radius, inner)
        }
    }
}

fn sample_disc(rng: &mut XorShift64, radius: f32, inner: f32) -> Vec3 {
    let angle = rng.range(0.0, TAU);
    let radius_squared = rng.range(inner * inner, radius * radius).max(0.0);
    let local_radius = radius_squared.sqrt();
    Vec3::new(local_radius * angle.cos(), 0.0, local_radius * angle.sin())
}

fn sample_direction(rng: &mut XorShift64, direction: [f32; 3], spread: [f32; 2]) -> Vec3 {
    let mut base = Vec3::from_array(direction);
    if !base.is_finite() || base.length_squared() <= 1.0e-12 {
        base = Vec3::Y;
    } else {
        base = base.normalize();
    }

    // Tangent-space offsets avoid undefined bases when the direction is
    // vertical and stay finite at the 180-degree limit.
    let spread_x = sanitize_angle(spread[0]);
    let spread_y = sanitize_angle(spread[1]);
    if spread_x <= 0.0 && spread_y <= 0.0 {
        return base;
    }

    let (right, up) = orthonormal_basis(base);
    let offset_x = if spread_x > 0.0 {
        rng.range(-spread_x * 0.5, spread_x * 0.5)
            .to_radians()
            .tan()
    } else {
        0.0
    };
    let offset_y = if spread_y > 0.0 {
        rng.range(-spread_y * 0.5, spread_y * 0.5)
            .to_radians()
            .tan()
    } else {
        0.0
    };

    let direction = base + right * offset_x + up * offset_y;
    if direction.is_finite() && direction.length_squared() > 1.0e-12 {
        direction.normalize()
    } else {
        base
    }
}

fn orthonormal_basis(normal: Vec3) -> (Vec3, Vec3) {
    let helper = if normal.y.abs() < 0.999 {
        Vec3::Y
    } else {
        Vec3::X
    };
    let right = normal.cross(helper).normalize();
    let up = right.cross(normal).normalize();
    (right, up)
}

fn build_sprites(
    asset: &crate::ParticleSystemAsset,
    states: &[EmitterState],
    camera: &ParticleCamera,
    update: &ParticleUpdate,
) -> Vec<ParticleSprite> {
    let budget_cap = asset.budget.max_sprites as usize;
    let requested_cap = update.max_sprites as usize;
    let max_sprites = if requested_cap == 0 {
        budget_cap
    } else {
        requested_cap.min(budget_cap)
    };
    if max_sprites == 0 {
        return Vec::new();
    }

    let camera_position = Vec3::from_array(camera.position);
    let mut sprites = Vec::with_capacity(max_sprites.min(4096));

    'emitters: for (emitter_index, (emitter, state)) in
        asset.emitters.iter().zip(states.iter()).enumerate()
    {
        for particle in &state.particles {
            if sprites.len() >= max_sprites {
                break 'emitters;
            }

            let t = if particle.lifetime > 0.0 {
                (particle.age / particle.lifetime).clamp(0.0, 1.0)
            } else {
                1.0
            };

            let size = finite_or(emitter.size.eval(t), 1.0).max(0.0);
            let opacity = finite_or(emitter.opacity.eval(t), 1.0).clamp(0.0, 1.0);
            let base_color = emitter.color.eval(t);
            let alpha = ((base_color[3] as f32) * opacity).round().clamp(0.0, 255.0) as u8;

            let uv = match &emitter.flipbook {
                Some(flipbook) => flipbook_uv(flipbook, particle.age, particle.flip_offset),
                None => [0.0, 0.0, 1.0, 1.0],
            };

            sprites.push(ParticleSprite {
                position: particle.position.to_array(),
                size: [size, size],
                rotation: 0.0,
                color: [base_color[0], base_color[1], base_color[2], alpha],
                uv,
                depth: (particle.position - camera_position).length(),
                emitter: emitter_index as u32,
            });
        }
    }

    // Transparent billboards need back-to-front painting.
    sprites.sort_by(|a, b| b.depth.total_cmp(&a.depth));
    sprites
}

fn flipbook_uv(flipbook: &Flipbook, age: f32, offset: u32) -> [f32; 4] {
    let columns = flipbook.columns.max(1);
    let rows = flipbook.rows.max(1);
    let frame_count = flipbook_frame_count(flipbook);
    let fps = if flipbook.fps.is_finite() {
        flipbook.fps.max(0.0)
    } else {
        0.0
    };

    let advanced = (age.max(0.0) * fps).floor() as u64;
    let frame = (advanced + offset as u64) % frame_count as u64;
    let column = frame % columns as u64;
    let row = frame / columns as u64;

    [
        column as f32 / columns as f32,
        row as f32 / rows as f32,
        (column + 1) as f32 / columns as f32,
        (row + 1) as f32 / rows as f32,
    ]
}

fn flipbook_frame_count(flipbook: &Flipbook) -> u32 {
    flipbook
        .columns
        .max(1)
        .saturating_mul(flipbook.rows.max(1))
        .max(1)
}

fn finite_or(value: f32, fallback: f32) -> f32 {
    if value.is_finite() {
        value
    } else {
        fallback
    }
}

fn sanitize_non_negative(value: f32) -> f32 {
    if value.is_finite() {
        value.max(0.0)
    } else {
        0.0
    }
}

fn sanitize_positive(value: f32, fallback: f32) -> f32 {
    if value.is_finite() && value > 0.0 {
        value
    } else {
        fallback
    }
}

fn sanitize_angle(value: f32) -> f32 {
    if value.is_finite() {
        value.clamp(0.0, 179.0)
    } else {
        0.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        load_asset, ColorGradient, ColorKey, NumberCurve, NumberKey, ParticleBudget,
        ParticleCollision, ParticleSystemAsset,
    };
    use std::path::Path;

    fn camera() -> ParticleCamera {
        ParticleCamera {
            position: [0.0, 0.0, 10.0],
            right: [1.0, 0.0, 0.0],
            up: [0.0, 1.0, 0.0],
        }
    }

    fn base_emitter() -> ParticleEmitter {
        let mut emitter = ParticleEmitter::default();
        emitter.name = "test".to_string();
        emitter.texture = Some("particles/textures/soft.png".to_string());
        emitter.max_particles = 256;
        emitter.rate = 0.0;
        emitter.burst = 5;
        emitter.lifetime = [10.0, 10.0];
        emitter.speed = [0.0, 0.0];
        emitter.spread = [0.0, 0.0];
        emitter.direction = [0.0, 1.0, 0.0];
        emitter.gravity = [0.0, 0.0, 0.0];
        emitter.drag = 0.0;
        emitter
    }

    fn base_asset(emitter: ParticleEmitter) -> ParticleSystemAsset {
        let mut asset = ParticleSystemAsset {
            version: 1,
            name: "test".to_string(),
            budget: ParticleBudget {
                max_particles: 512,
                max_sprites: 512,
            },
            emitters: vec![emitter],
        };
        asset.validate();
        asset
    }

    fn template(name: &str) -> ParticleSystemAsset {
        let path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../assets/particles/templates")
            .join(format!("{name}.rafparticle"));
        load_asset(&path).unwrap_or_else(|error| panic!("template '{name}': {error}"))
    }

    fn assert_close(actual: f32, expected: f32) {
        assert!(
            (actual - expected).abs() <= 1.0e-4,
            "expected {expected}, got {actual}"
        );
    }

    #[test]
    fn runtime_particle_cap_limits_the_burst() {
        let mut emitter = base_emitter();
        emitter.burst = 10;
        let asset = base_asset(emitter);
        let update = ParticleUpdate {
            max_particles: 3,
            ..Default::default()
        };
        let mut simulator = ParticleSimulator::with_seed(7);
        let frame = simulator.update(&asset, Mat4::IDENTITY, &camera(), 0.016, &update);
        assert_eq!(frame.stats.active, 3);
        assert_eq!(frame.stats.spawned, 3);
        assert_eq!(frame.stats.capped, 7);
    }

    #[test]
    fn rate_scale_and_explicit_emit_add_particles() {
        let mut emitter = base_emitter();
        emitter.burst = 0;
        emitter.rate = 60.0;
        let asset = base_asset(emitter);
        let update = ParticleUpdate {
            rate_scale: 0.0,
            emit: 4,
            ..Default::default()
        };
        let mut simulator = ParticleSimulator::with_seed(11);
        let frame = simulator.update(&asset, Mat4::IDENTITY, &camera(), 0.1, &update);
        assert_eq!(frame.stats.active, 4);
        assert_eq!(frame.stats.spawned, 4);
    }

    #[test]
    fn same_seed_produces_identical_frames() {
        let asset = template("fire");
        let update = ParticleUpdate::default();
        let mut first = ParticleSimulator::with_seed(0x1234_5678_9ABC_DEF0);
        let mut second = ParticleSimulator::with_seed(0x1234_5678_9ABC_DEF0);

        for _ in 0..40 {
            let first_frame = first.update(&asset, Mat4::IDENTITY, &camera(), 0.016, &update);
            let second_frame = second.update(&asset, Mat4::IDENTITY, &camera(), 0.016, &update);

            assert_eq!(first_frame.stats, second_frame.stats);
            assert_eq!(first_frame.sprites, second_frame.sprites);
        }

        assert!(first.active_particles() > 0);

        first.reset();
        second.reset();
        let first_frame = first.update(&asset, Mat4::IDENTITY, &camera(), 0.016, &update);
        let second_frame = second.update(&asset, Mat4::IDENTITY, &camera(), 0.016, &update);
        assert_eq!(first_frame.sprites, second_frame.sprites);
    }

    #[test]
    fn pause_and_freeze_keep_particles_still() {
        let mut emitter = base_emitter();
        emitter.burst = 7;
        emitter.speed = [0.4, 0.4];
        emitter.gravity = [0.0, -2.0, 0.0];
        let asset = base_asset(emitter);
        let mut simulator = ParticleSimulator::with_seed(7);

        let initial = simulator.update(
            &asset,
            Mat4::IDENTITY,
            &camera(),
            0.016,
            &ParticleUpdate::default(),
        );
        assert_eq!(initial.stats.active, 7);
        assert_eq!(initial.sprites.len(), 7);

        for frozen in [false, true] {
            let update = ParticleUpdate {
                paused: !frozen,
                frozen,
                ..Default::default()
            };
            let held = simulator.update(&asset, Mat4::IDENTITY, &camera(), 5.0, &update);

            assert_eq!(held.stats.active, initial.stats.active);
            assert_eq!(held.stats.spawned, 0);
            assert_eq!(held.sprites, initial.sprites);
        }

        let resumed = simulator.update(
            &asset,
            Mat4::IDENTITY,
            &camera(),
            0.016,
            &ParticleUpdate::default(),
        );
        assert_eq!(resumed.stats.active, 7);
        assert_ne!(resumed.sprites, initial.sprites);
    }

    #[test]
    fn stopped_clears_and_restarts_with_a_burst() {
        let mut emitter = base_emitter();
        emitter.burst = 6;
        let asset = base_asset(emitter);
        let mut simulator = ParticleSimulator::with_seed(11);

        let initial = simulator.update(
            &asset,
            Mat4::IDENTITY,
            &camera(),
            0.016,
            &ParticleUpdate::default(),
        );
        assert_eq!(initial.stats.active, 6);

        let stopped = ParticleUpdate {
            stopped: true,
            ..Default::default()
        };
        let stopped_frame = simulator.update(&asset, Mat4::IDENTITY, &camera(), 0.016, &stopped);
        assert!(stopped_frame.sprites.is_empty());
        assert_eq!(stopped_frame.stats.active, 0);
        assert_eq!(simulator.active_particles(), 0);

        let restarted = simulator.update(
            &asset,
            Mat4::IDENTITY,
            &camera(),
            0.016,
            &ParticleUpdate::default(),
        );
        assert_eq!(restarted.stats.active, 6);
        assert_eq!(restarted.stats.spawned, 6);
    }

    #[test]
    fn disabled_update_stops_spawning_but_keeps_existing_particles() {
        let mut emitter = base_emitter();
        emitter.burst = 3;
        emitter.rate = 10.0;
        emitter.lifetime = [100.0, 100.0];
        let asset = base_asset(emitter);
        let mut simulator = ParticleSimulator::with_seed(13);

        let initial = simulator.update(
            &asset,
            Mat4::IDENTITY,
            &camera(),
            0.05,
            &ParticleUpdate::default(),
        );
        assert_eq!(initial.stats.active, 3);

        let disabled = ParticleUpdate {
            enabled: false,
            ..Default::default()
        };
        let frozen_spawns = simulator.update(&asset, Mat4::IDENTITY, &camera(), 0.05, &disabled);
        assert_eq!(frozen_spawns.stats.active, 3);
        assert_eq!(frozen_spawns.stats.spawned, 0);

        let resumed = simulator.update(
            &asset,
            Mat4::IDENTITY,
            &camera(),
            0.05,
            &ParticleUpdate::default(),
        );
        assert_eq!(resumed.stats.spawned, 1);
        assert_eq!(resumed.stats.active, 4);
    }

    #[test]
    fn disabled_emitter_does_not_burst_until_enabled() {
        let mut emitter = base_emitter();
        emitter.enabled = false;
        emitter.burst = 4;
        let asset = base_asset(emitter.clone());
        let mut simulator = ParticleSimulator::with_seed(17);

        let disabled = simulator.update(
            &asset,
            Mat4::IDENTITY,
            &camera(),
            0.016,
            &ParticleUpdate::default(),
        );
        assert_eq!(disabled.stats.active, 0);

        let enabled_asset = base_asset(ParticleEmitter {
            enabled: true,
            ..emitter
        });
        let enabled = simulator.update(
            &enabled_asset,
            Mat4::IDENTITY,
            &camera(),
            0.016,
            &ParticleUpdate::default(),
        );
        assert_eq!(enabled.stats.active, 4);
    }

    #[test]
    fn particle_and_sprite_budgets_are_enforced_across_emitters() {
        let mut first = base_emitter();
        first.name = "first".to_string();
        first.burst = 10;
        first.max_particles = 10;

        let mut second = base_emitter();
        second.name = "second".to_string();
        second.burst = 10;
        second.max_particles = 10;

        let mut asset = base_asset(first);
        asset.emitters.push(second);
        asset.budget.max_particles = 5;
        asset.budget.max_sprites = 3;
        asset.validate();

        let update = ParticleUpdate::default();
        let frame = simulator_update(&asset, &update);

        assert_eq!(frame.stats.active, 5);
        assert_eq!(frame.stats.spawned, 5);
        assert!(frame.stats.capped >= 15);
        assert_eq!(frame.sprites.len(), 3);
        assert!(frame
            .sprites
            .windows(2)
            .all(|window| window[0].depth >= window[1].depth));
    }

    #[test]
    fn runtime_sprite_cap_can_only_reduce_the_budget() {
        let mut emitter = base_emitter();
        emitter.burst = 10;
        emitter.max_particles = 10;
        let mut asset = base_asset(emitter);
        asset.budget.max_particles = 10;
        asset.budget.max_sprites = 8;
        asset.validate();

        let update = ParticleUpdate {
            max_sprites: 2,
            ..Default::default()
        };
        let frame = simulator_update(&asset, &update);
        assert_eq!(frame.stats.active, 10);
        assert_eq!(frame.sprites.len(), 2);

        let larger_request = ParticleUpdate {
            max_sprites: 100,
            ..Default::default()
        };
        let frame = simulator_update(&asset, &larger_request);
        assert_eq!(frame.sprites.len(), 8);
    }

    fn simulator_update(asset: &ParticleSystemAsset, update: &ParticleUpdate) -> ParticleFrame {
        let mut simulator = ParticleSimulator::with_seed(23);
        simulator.update(asset, Mat4::IDENTITY, &camera(), 0.016, update)
    }

    #[test]
    fn particles_expire_after_their_lifetime() {
        let mut emitter = base_emitter();
        emitter.burst = 4;
        emitter.lifetime = [0.05, 0.05];
        let asset = base_asset(emitter);
        let mut simulator = ParticleSimulator::with_seed(29);
        let update = ParticleUpdate::default();

        let initial = simulator.update(&asset, Mat4::IDENTITY, &camera(), 0.016, &update);
        assert_eq!(initial.stats.active, 4);

        let expired = simulator.update(&asset, Mat4::IDENTITY, &camera(), 0.1, &update);
        assert_eq!(expired.stats.active, 0);
        assert!(expired.sprites.is_empty());
        assert_eq!(expired.stats.spawned, 0);

        // The burst only happens once per start/reset.
        let still_empty = simulator.update(&asset, Mat4::IDENTITY, &camera(), 0.1, &update);
        assert_eq!(still_empty.stats.active, 0);
    }

    #[test]
    fn flipbook_uvs_stay_normalized_and_advance() {
        let mut emitter = base_emitter();
        emitter.burst = 1;
        emitter.flipbook = Some(Flipbook {
            columns: 4,
            rows: 2,
            fps: 10.0,
            start_random: false,
        });
        let asset = base_asset(emitter);
        let mut simulator = ParticleSimulator::with_seed(31);
        let update = ParticleUpdate::default();

        let first = simulator.update(&asset, Mat4::IDENTITY, &camera(), 0.016, &update);
        assert_eq!(first.sprites.len(), 1);
        assert_eq!(first.sprites[0].uv, [0.0, 0.0, 0.25, 0.5]);

        let second = simulator.update(&asset, Mat4::IDENTITY, &camera(), 0.1, &update);
        assert_eq!(second.sprites[0].uv, [0.25, 0.0, 0.5, 0.5]);

        let third = simulator.update(&asset, Mat4::IDENTITY, &camera(), 0.1, &update);
        assert_eq!(third.sprites[0].uv, [0.5, 0.0, 0.75, 0.5]);

        let fire = template("fire");
        let mut fire_simulator = ParticleSimulator::with_seed(37);
        let mut saw_frame_change = false;
        let mut previous_uv = [0.0f32; 4];
        for step in 0..60 {
            let frame = fire_simulator.update(&fire, Mat4::IDENTITY, &camera(), 0.033, &update);
            for sprite in &frame.sprites {
                assert!((0.0..=1.0).contains(&sprite.uv[0]));
                assert!((0.0..=1.0).contains(&sprite.uv[1]));
                assert!((0.0..=1.0).contains(&sprite.uv[2]));
                assert!((0.0..=1.0).contains(&sprite.uv[3]));
                assert!(sprite.uv[2] > sprite.uv[0]);
                assert!(sprite.uv[3] > sprite.uv[1]);
                if step > 0 && sprite.uv != previous_uv {
                    saw_frame_change = true;
                }
                previous_uv = sprite.uv;
            }
        }
        assert!(saw_frame_change, "flipbook UVs never changed");
    }

    #[test]
    fn curves_are_applied_to_sprite_output() {
        let mut emitter = base_emitter();
        emitter.burst = 1;
        emitter.lifetime = [1.0, 1.0];
        emitter.size = NumberCurve {
            keys: vec![
                NumberKey { t: 0.0, value: 2.0 },
                NumberKey { t: 1.0, value: 4.0 },
            ],
        };
        emitter.opacity = NumberCurve {
            keys: vec![
                NumberKey { t: 0.0, value: 0.5 },
                NumberKey { t: 1.0, value: 1.0 },
            ],
        };
        emitter.color = ColorGradient {
            keys: vec![
                ColorKey {
                    t: 0.0,
                    color: [255, 0, 0, 255],
                },
                ColorKey {
                    t: 1.0,
                    color: [0, 0, 255, 255],
                },
            ],
        };
        let asset = base_asset(emitter);
        let mut simulator = ParticleSimulator::with_seed(41);
        let update = ParticleUpdate::default();

        let first = simulator.update(&asset, Mat4::IDENTITY, &camera(), 0.016, &update);
        assert_eq!(first.sprites[0].size, [2.0, 2.0]);
        assert_eq!(first.sprites[0].color, [255, 0, 0, 128]);

        let second = simulator.update(&asset, Mat4::IDENTITY, &camera(), 0.1, &update);
        assert_eq!(second.sprites[0].size, [2.2, 2.2]);
        assert_eq!(second.sprites[0].color[0], 230);
        assert_eq!(second.sprites[0].color[2], 26);
    }

    #[test]
    fn collision_ground_bounces_or_kills() {
        let mut emitter = base_emitter();
        emitter.burst = 1;
        emitter.speed = [5.0, 5.0];
        emitter.direction = [0.0, -1.0, 0.0];
        emitter.collision = Some(ParticleCollision {
            ground_y: -0.25,
            bounce: 0.5,
            kill: false,
        });
        let asset = base_asset(emitter.clone());
        let mut simulator = ParticleSimulator::with_seed(43);
        let update = ParticleUpdate::default();

        let spawned = simulator.update(&asset, Mat4::IDENTITY, &camera(), 0.1, &update);
        assert_close(spawned.sprites[0].position[1], 0.0);

        let bounced = simulator.update(&asset, Mat4::IDENTITY, &camera(), 0.1, &update);
        assert_close(bounced.sprites[0].position[1], -0.25);

        let rising = simulator.update(&asset, Mat4::IDENTITY, &camera(), 0.1, &update);
        assert_close(rising.sprites[0].position[1], 0.0);

        let mut killing = emitter;
        killing.collision = Some(ParticleCollision {
            ground_y: -0.25,
            bounce: 0.5,
            kill: true,
        });
        let killing_asset = base_asset(killing);
        let mut killer_simulator = ParticleSimulator::with_seed(47);
        killer_simulator.update(&killing_asset, Mat4::IDENTITY, &camera(), 0.1, &update);
        let killed =
            killer_simulator.update(&killing_asset, Mat4::IDENTITY, &camera(), 0.1, &update);
        assert_eq!(killed.stats.active, 0);
        assert!(killed.sprites.is_empty());
    }

    #[test]
    fn transform_maps_local_position_and_direction_to_world() {
        let mut emitter = base_emitter();
        emitter.burst = 1;
        emitter.speed = [1.0, 1.0];
        emitter.direction = [1.0, 0.0, 0.0];
        let asset = base_asset(emitter);
        let mut simulator = ParticleSimulator::with_seed(53);
        let update = ParticleUpdate::default();
        let transform =
            Mat4::from_translation(Vec3::new(10.0, 2.0, 0.0)) * Mat4::from_rotation_y(TAU / 4.0);

        let spawned = simulator.update(&asset, transform, &camera(), 0.1, &update);
        assert_close(spawned.sprites[0].position[0], 10.0);
        assert_close(spawned.sprites[0].position[1], 2.0);
        assert_close(spawned.sprites[0].position[2], 0.0);

        let moved = simulator.update(&asset, transform, &camera(), 0.1, &update);
        assert_close(moved.sprites[0].position[0], 10.0);
        assert_close(moved.sprites[0].position[1], 2.0);
        assert_close(moved.sprites[0].position[2], -0.1);
    }

    #[test]
    fn delta_time_is_clamped_and_time_scale_is_applied() {
        let mut emitter = base_emitter();
        emitter.burst = 1;
        emitter.speed = [1.0, 1.0];
        emitter.direction = [0.0, 0.0, 1.0];
        let asset = base_asset(emitter);
        let mut simulator = ParticleSimulator::with_seed(59);

        simulator.update(
            &asset,
            Mat4::IDENTITY,
            &camera(),
            0.1,
            &ParticleUpdate::default(),
        );

        let huge_step = simulator.update(
            &asset,
            Mat4::IDENTITY,
            &camera(),
            1000.0,
            &ParticleUpdate::default(),
        );
        assert_close(huge_step.sprites[0].position[2], 0.1);

        let half_speed = ParticleUpdate {
            time_scale: 0.5,
            ..Default::default()
        };
        let half_step = simulator.update(&asset, Mat4::IDENTITY, &camera(), 0.1, &half_speed);
        assert_close(half_step.sprites[0].position[2], 0.15);
    }

    #[test]
    fn depth_matches_camera_distance() {
        let mut emitter = base_emitter();
        emitter.burst = 1;
        let asset = base_asset(emitter);
        let mut simulator = ParticleSimulator::with_seed(61);
        let camera = camera();
        let frame = simulator.update(
            &asset,
            Mat4::IDENTITY,
            &camera,
            0.016,
            &ParticleUpdate::default(),
        );

        let position = Vec3::from_array(frame.sprites[0].position);
        let expected = (position - Vec3::from_array(camera.position)).length();
        assert_close(frame.sprites[0].depth, expected);
    }
}
