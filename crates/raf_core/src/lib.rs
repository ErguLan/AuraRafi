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

pub mod ai;
pub mod capabilities;
pub mod command;
pub mod command_protocol;
pub mod complement;
pub mod config;
pub mod ecs;
pub mod event;
pub mod ffi;
pub mod hot_reload;
pub mod i18n;
pub mod ipc;
pub mod project;
pub mod save_system;
pub mod scene;
pub mod session;
pub mod transaction;
pub mod units;
pub mod world_state;

pub use capabilities::{CapabilityCatalog, CapabilityDefinition, CapabilityParameter};
/// Re-export commonly used types at the crate root.
pub use command::{Command, CommandBus, CommandId};
pub use command_protocol::{
    decode_line, encode_line, serve_lines, CommandEndpoint, CommandSource, EngineCommandRequest,
    EngineCommandResponse, IpcEndpoint, COMMAND_PROTOCOL_VERSION, MAX_COMMAND_FRAME_BYTES,
};
pub use complement::*;
pub use config::{EngineSettings, Language, RenderQuality, TargetPlatform, Theme};
pub use ecs::world::GameWorld;
pub use event::{EventBus, EventId};
pub use hot_reload::{FileChange, HotReloadConfig, HotReloadState, WatchCategory};
pub use ipc::{
    decode_frame, encode_frame, validate_hello, AttachHello, AttachWelcome, EndpointDescriptor,
    IpcFrame, ATTACH_DESCRIPTOR_FILE, ATTACH_DIRECTORY, ATTACH_PROTOCOL_VERSION,
};
pub use project::{Project, ProjectType};
pub use scene::{NodeColor, Primitive, SceneGraph, SceneNode, SceneNodeId, WorldTransformCache};
pub use transaction::{
    ArtifactRef, ExecutionBudget, Revision, TransactionId, TransactionLedger, TransactionRecord,
    UndoToken, VerificationSummary,
};
pub use world_state::{Weather, WorldState, WorldTime};
