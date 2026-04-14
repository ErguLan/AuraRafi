//! # raf_core
//!
//! Core engine module for AuraRafi. Provides the foundational systems:
//! - **ECS**: Entity Component System via hecs
//! - **Scene**: Scene graph with parent-child transform hierarchy
//! - **Command**: Command bus for undo/redo and AI tool-calling
//! - **Event**: Pub/sub event system for decoupled communication
//! - **Config**: Engine settings (theme, language, performance)
//! - **Project**: Project management (create, load, save)
//! - **WorldState**: Lightweight game world snapshot for AI observation
//! - **HotReload**: Polling-based file watcher for live project updates

pub mod command;
pub mod complement;
pub mod config;
pub mod ecs;
pub mod event;
pub mod ffi;
pub mod hot_reload;
pub mod i18n;
pub mod project;
pub mod save_system;
pub mod scene;
pub mod world_state;

/// Re-export commonly used types at the crate root.
pub use command::{Command, CommandBus, CommandId};
pub use complement::*;
pub use config::{EngineSettings, Language, RenderQuality, TargetPlatform, Theme};
pub use ecs::world::GameWorld;
pub use event::{EventBus, EventId};
pub use project::{Project, ProjectType};
pub use scene::{NodeColor, Primitive, SceneGraph, SceneNode, SceneNodeId};
pub use world_state::{WorldState, WorldTime, Weather};
pub use hot_reload::{HotReloadConfig, HotReloadState, FileChange, WatchCategory};
