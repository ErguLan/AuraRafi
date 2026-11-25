# Particle assets

Particle effects in AuraRafi are JSON data. `raf_particles` contains the schema,
the CPU simulator and the renderer-facing sprite output; it does **not** contain
hardcoded fire, smoke, rain or magic effects. The files in `templates/` are
example data you can copy and edit.

## Templates

| File | Description |
| --- | --- |
| `templates/fire.rafparticle` | Rising flames plus embers |
| `templates/smoke.rafparticle` | Slow smoke puffs and wisps |
| `templates/sparks.rafparticle` | Fast gravity-driven sparks and a short glow |
| `templates/rain.rafparticle` | Dense rain volume plus splashes |
| `templates/dust.rafparticle` | Floating dust motes and faint haze |
| `templates/magic.rafparticle` | Soft magical aura and bright sparkles |

All templates reference textures under `particles/textures/`. `fire`, `smoke`
and `magic` use 4x4 atlases (`fire_atlas.png`, `smoke_atlas.png`,
`magic_atlas.png`); the rest use `soft.png` or `spark.png`. Texture files are
project assets; the simulator never opens them. The path is passed through to
the renderer so it can bind the correct texture.

The editor can create these assets directly: Assets > Create > Particle system
writes the template plus the starter textures into the project, and dropping the
resulting `.rafparticle` row into the hierarchy creates an entity with a
`ParticleSystemComponent`. Existing textures are never overwritten.

## Loading

```rust
use std::path::Path;
use raf_particles::load_asset;

let asset = load_asset(Path::new(
    "assets/particles/templates/fire.rafparticle",
))?;
```

`load_asset` reads the file, parses the JSON, ignores unknown fields and
validates/clamps the result. `parse_asset(json)` does the same for an in-memory
string. Use `ParticleSystemAsset::validate()` when constructing assets by hand.

## Asset schema

```json
{
  "version": 1,
  "name": "Fire",
  "budget": { "max_particles": 1500, "max_sprites": 1500 },
  "emitters": []
}
```

`version` defaults to `1`. Unknown fields are allowed so future additions do
not break older runtimes.

### Budget

`budget.max_particles` caps live particles across **all** emitters.
`budget.max_sprites` is the renderer budget. Per-emitter `max_particles` is an
additional cap.

### Emitter

```json
{
  "name": "flames",
  "texture": "particles/textures/soft.png",
  "enabled": true,
  "max_particles": 1000,
  "rate": 160.0,
  "burst": 48,
  "lifetime": [0.45, 0.95],
  "speed": [0.55, 1.5],
  "spread": [12.0, 20.0],
  "direction": [0.0, 1.0, 0.0],
  "gravity": [0.0, 0.55, 0.0],
  "drag": 0.45,
  "size": { "keys": [] },
  "opacity": { "keys": [] },
  "color": { "keys": [] },
  "shape": { "type": "sphere", "radius": 0.18 },
  "flipbook": null,
  "collision": null,
  "wind_affects_drag": true
}
```

- `texture` is project-relative. Empty, absolute and parent-traversal paths are
  removed during validation.
- `rate` is spawns per second. `burst` spawns once when the emitter first
  becomes active and again after reset or stop.
- `lifetime` and `speed` are inclusive ranges. A random value is chosen per
  particle. Inverted ranges are reordered.
- `spread` is `[x, y]` in degrees around `direction`, capped to `0..=180`.
- `direction` is normalized. Positions and directions are in emitter-local
  space and are transformed by the matrix passed to `update`.
- `gravity`, wind and collision planes are world-space.
- `drag` is a stable damping coefficient. With `wind_affects_drag: false`, drag
  damps absolute velocity and wind is added as acceleration. With `true`, drag
  pulls velocity toward the wind velocity.
- `size` is in world units and scales both sprite axes. Empty curves evaluate
  to `1.0`.
- `opacity` multiplies the gradient alpha and is clamped to `0..=1`. Empty
  curves evaluate to `1.0`.
- `color` channels are `0..=255`. Empty gradients evaluate to opaque white.
- `flipbook` is optional. `columns` and `rows` must be at least 1; `fps` drives
  the frame over particle age. `start_random` gives each particle a random
  starting frame. The bundled fire/smoke/magic textures are real 4x4 atlases.
- `collision` is an optional world-space ground plane. `kill` removes particles;
  otherwise `bounce` reflects the vertical velocity. `0` stops vertical motion.
- `enabled: false` stops new spawns but existing particles keep simulating.

### Curves

```json
"size": {
  "keys": [
    { "t": 0.0, "value": 0.5 },
    { "t": 1.0, "value": 2.0 }
  ]
}
```

`t` is the normalized particle lifetime in `0..=1`; values outside the range
clamp to the nearest key. Interpolation is linear. Empty curves use `1.0`.

```json
"color": {
  "keys": [
    { "t": 0.0, "color": [255, 245, 200, 255] },
    { "t": 1.0, "color": [60, 12, 6, 0] }
  ]
}
```

### Shapes

Shapes are local-space spawn volumes centered at the emitter origin:

```json
{ "type": "point" }
{ "type": "sphere", "radius": 0.25 }
{ "type": "box", "half_extents": [1.0, 0.2, 1.0] }
{ "type": "cone", "radius": 0.4, "height": 1.0 }
{ "type": "disc", "radius": 1.0, "inner": 0.2 }
```

Spheres are sampled by volume. Cones grow along local +Y with the base on the
local XZ plane. Discs lie on local XZ and `inner` is the hole radius. Use a
transform to place and orient the emission volume in the world.

## Simulation

```rust
use glam::Mat4;
use raf_particles::{
    load_asset, ParticleCamera, ParticleSimulator, ParticleUpdate,
};

let asset = load_asset(path)?;
let mut simulator = ParticleSimulator::with_seed(0x1234_5678);
let camera = ParticleCamera {
    position: [0.0, 1.7, 8.0],
    right: [1.0, 0.0, 0.0],
    up: [0.0, 1.0, 0.0],
};
let update = ParticleUpdate::default();

let frame = simulator.update(
    &asset,
    Mat4::IDENTITY,
    &camera,
    1.0 / 60.0,
    &update,
);

for sprite in &frame.sprites {
    // position, size, rotation, color, uv, depth and emitter index
}
```

`dt` is clamped to `0..=0.1` seconds before `time_scale` is applied; the scaled
step is capped at `0.25` seconds. This avoids particle explosions after long
frame stalls or debugger pauses.

`ParticleSimulator` is deterministic for a given seed, asset and call sequence.
`reset()` restores the seed and clears particles. `set_seed()` changes the seed
and resets. The random generator is an internal xorshift64 implementation; no
`rand` dependency is used.

`ParticleUpdate` controls runtime state:

- `enabled = false`: no new particles, existing particles continue.
- `paused = true` or `frozen = true`: time and spawning freeze; particles remain
  visible.
- `stopped = true`: particles are cleared and an empty frame is returned. The
  next active update starts with the `burst`.
- `time_scale` scales simulation time.
- `rate_scale` multiplies every emitter spawn rate.
- `emit` spawns that many extra particles once, distributed across enabled
  emitters and still bounded by budgets.
- `max_sprites` further limits the asset sprite budget for this frame. `0`
  means use the asset budget.
- `wind` is world-space velocity.

`ParticleStats` reports live particles, particles spawned this frame, spawn
attempts rejected by budget, and the emitter count. Sprites are returned
far-to-near by camera distance so a renderer can draw them with alpha blending.

## Scene component

`ParticleSystemComponent` stores the project-relative asset path, playback
(`Playing`, `Paused`, `Stopped`, `Frozen`), visibility, time/rate multipliers,
tint, quality tier, an optional per-instance cap and `play_on_start`. It is
serialized with the scene, so several entities can share one asset with
different playback and budgets.

- The editor preview is controlled by Settings > Graphics > Particle Preview
  (enable, pause, freeze, stop, time scale, sprite cap).
- Play uses Project Settings > Particle simulation (enable, quality, cap, time
  scale). Scripts and nodes control each entity at runtime.

## Scripting and nodes

Rhai exposes flat functions and a static module:

```rhai
particles::play(effect);
particles::pause(effect);
particles::stop(effect);
particles::freeze(effect);
particles::emit(effect, 24);
particles::set_rate(effect, 0.5);
particles::set_time_scale(effect, 0.25);
// Or particle_play(effect), particle_emit(effect, 24), ...
```

The Node graph palette includes matching `Play/Pause/Stop/Freeze Particles`,
`Emit Particles`, `Set Particle Rate` and `Set Particle Time Scale` actions.
The Agent tool `particles_generate` creates a validated asset from a built-in
template or a complete `ParticleSystemAsset` JSON definition.

## Renderer contract

`ParticleSprite` contains:

- `position`: world-space sprite center.
- `size`: `[width, height]` in world units.
- `rotation`: radians around the view axis (currently always 0).
- `color`: RGBA tint.
- `uv`: `[u0, v0, u1, v1]` for the current atlas frame, always inside `0..=1`.
- `depth`: distance to `ParticleCamera::position`.
- `emitter`: index into `ParticleSystemAsset::emitters`.

Texture selection, billboarding and blending stay in the renderer. The
simulator performs no GPU calls and no file I/O.
