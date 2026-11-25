//! Scene component for a data-driven particle system.
//!
//! The effect definition lives in a `.rafparticle` JSON asset. The component
//! only stores the instance reference, playback state and bounded overrides so
//! several entities can share one effect without duplicating its data.

use serde::{Deserialize, Serialize};

/// Quality tier used to clamp the runtime particle budget.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum ParticleQuality {
    Potato,
    Low,
    #[default]
    Medium,
    High,
    Epic,
}

impl ParticleQuality {
    pub const ALL: [Self; 5] = [
        Self::Potato,
        Self::Low,
        Self::Medium,
        Self::High,
        Self::Epic,
    ];

    /// Conservative sprite caps. The renderer may clamp them further when the
    /// per-instance or asset budget is lower.
    pub const fn max_sprites(self) -> u32 {
        match self {
            Self::Potato => 2_000,
            Self::Low => 8_000,
            Self::Medium => 30_000,
            Self::High => 100_000,
            Self::Epic => 250_000,
        }
    }

    pub const fn key(self) -> &'static str {
        match self {
            Self::Potato => "app.particle_quality.potato",
            Self::Low => "app.particle_quality.low",
            Self::Medium => "app.particle_quality.medium",
            Self::High => "app.particle_quality.high",
            Self::Epic => "app.particle_quality.epic",
        }
    }
}

/// Current playback state for one particle-system instance.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum ParticlePlayback {
    #[default]
    Playing,
    /// Freeze time and spawning; existing particles stay visible.
    Paused,
    /// Clear existing particles and hide the system.
    Stopped,
    /// Explicit freeze for editor inspection; same simulation behavior as
    /// paused but kept separate so tooling can explain why motion stopped.
    Frozen,
}

impl ParticlePlayback {
    pub const fn is_running(self) -> bool {
        matches!(self, Self::Playing)
    }

    pub const fn is_paused(self) -> bool {
        matches!(self, Self::Paused | Self::Frozen)
    }

    pub const fn key(self) -> &'static str {
        match self {
            Self::Playing => "app.particle_playback.playing",
            Self::Paused => "app.particle_playback.paused",
            Self::Stopped => "app.particle_playback.stopped",
            Self::Frozen => "app.particle_playback.frozen",
        }
    }
}

/// Global editor-only particle preview controls.
///
/// These never change the game runtime. They exist so a heavy effect cannot
/// make the authoring editor unusable on a low-end machine.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ParticlePreviewSettings {
    pub enabled: bool,
    pub paused: bool,
    pub frozen: bool,
    pub stopped: bool,
    pub time_scale: f32,
    /// World-space wind velocity for previews.
    pub wind: [f32; 3],
    /// Extra hard cap for the editor preview; `0` uses the asset/quality cap.
    pub max_sprites: u32,
}

impl Default for ParticlePreviewSettings {
    fn default() -> Self {
        Self {
            enabled: true,
            paused: false,
            frozen: false,
            stopped: false,
            time_scale: 1.0,
            wind: [0.0; 3],
            max_sprites: 4_096,
        }
    }
}

/// Project-scoped runtime particle policy used by Play.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ParticleRuntimeSettings {
    pub enabled: bool,
    pub quality: ParticleQuality,
    /// `0` uses the asset budget for the selected quality.
    pub max_particles: u32,
    pub time_scale: f32,
    /// World-space wind velocity applied to every runtime particle system.
    pub wind: [f32; 3],
}

impl Default for ParticleRuntimeSettings {
    fn default() -> Self {
        Self {
            enabled: true,
            quality: ParticleQuality::Medium,
            max_particles: 0,
            time_scale: 1.0,
            wind: [0.0; 3],
        }
    }
}

/// Per-instance overrides for a shared particle asset.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ParticleSystemComponent {
    /// Project-relative `.rafparticle` path under the project's `assets` folder.
    pub asset: String,
    /// Start emitting when the scene/runtime starts.
    pub play_on_start: bool,
    /// Whether the effect loops. One-shot effects stop after the asset duration.
    pub looping: bool,
    pub playback: ParticlePlayback,
    /// Hide without changing simulation state.
    pub visible: bool,
    /// Multiplier applied to the simulation time.
    pub time_scale: f32,
    /// Multiplier applied to every emitter spawn rate.
    pub rate_scale: f32,
    /// Cumulative one-shot emission requests. Scripts and tools increment this
    /// counter; the renderer consumes the delta once per node. It is runtime
    /// state and is not saved with the scene.
    #[serde(skip)]
    pub emit_requests: u64,
    /// Multiplicative RGBA tint; white means no tint.
    pub color_tint: [u8; 4],
    /// Per-instance hard cap. `0` uses the asset and quality budgets.
    pub max_particles: u32,
    pub quality: ParticleQuality,
}

impl Default for ParticleSystemComponent {
    fn default() -> Self {
        Self {
            asset: String::new(),
            play_on_start: true,
            looping: true,
            playback: ParticlePlayback::Playing,
            visible: true,
            time_scale: 1.0,
            rate_scale: 1.0,
            emit_requests: 0,
            color_tint: [255, 255, 255, 255],
            max_particles: 0,
            quality: ParticleQuality::Medium,
        }
    }
}

impl ParticleSystemComponent {
    pub fn new(asset: impl Into<String>) -> Self {
        Self {
            asset: asset.into(),
            ..Self::default()
        }
    }

    pub fn is_active(&self) -> bool {
        self.visible
            && !matches!(
                self.playback,
                ParticlePlayback::Stopped | ParticlePlayback::Frozen
            )
    }

    pub fn normalized_time_scale(&self) -> f32 {
        self.time_scale.clamp(0.0, 8.0)
    }

    pub fn normalized_rate_scale(&self) -> f32 {
        self.rate_scale.clamp(0.0, 8.0)
    }

    /// Requests `count` extra particles on the next rendered frame.
    pub fn request_emit(&mut self, count: u32) {
        self.emit_requests = self.emit_requests.saturating_add(u64::from(count));
    }
}
