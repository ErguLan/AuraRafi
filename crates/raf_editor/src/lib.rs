//! # raf_editor
//!
//! AuraRafi editor boundary.
//!
//! The editor-chrome layer is being rebuilt as retained RafUI surfaces. This
//! crate currently owns the loading/Hub entry surfaces, project/document
//! wiring, domain commands, the beta downbar, and full-client Game/Electronics
//! canvas hosts. The engine and renderer remain independent.

pub mod agent_executor;
#[path = "editor_viewport_app.rs"]
pub mod app;
pub mod application_bar_host;
pub mod application_bar_surface;
pub mod application_menu;
pub mod attached;
pub mod commands;
pub mod console;
pub mod electronics_assets;
pub mod electronics_history;
pub mod game_runtime;
pub mod panels;
pub mod pcb_document;
pub mod project_catalog;
pub mod project_settings_surface;
pub mod scene_history;
pub mod schematic_document;
pub mod script_support;
pub mod settings_surface;
pub mod studio_surface;
pub mod theme;

pub use app::AuraRafiApp;
