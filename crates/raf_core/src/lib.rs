//! # raf_core
//!
//! Core engine module for AuraRafi. Provides the foundational systems:
//! - **ECS**: Entity Component System via hecs
//! - **Scene**: Scene graph with parent-child transform hierarchy
//! - **Command**: Command bus for undo/redo and AI tool-calling
//! - **Event**: Pub/sub event system for decoupled communication
//! - **Config**: Engine settings (theme, language, performance)
//! - **Project**: Project management (create, load, save)

pub mod command;
pub mod config;
pub mod ecs;
pub mod event;
pub mod project;
pub mod scene;

/// Re-export commonly used types at the crate root.
pub use command::{Command, CommandBus, CommandId};
pub use config::{EngineSettings, Language, RenderQuality, TargetPlatform, Theme};
pub use ecs::world::GameWorld;
pub use event::{EventBus, EventId};
pub use project::{Project, ProjectType};
pub use scene::{NodeColor, Primitive, SceneGraph, SceneNode, SceneNodeId};
