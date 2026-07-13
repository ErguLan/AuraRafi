//! # raf_editor
//!
//! Visual editor for AuraRafi. Provides:
//! - Loading screen with branding
//! - Project Hub (recent projects + create new)
//! - Main editor with panels (viewport, hierarchy, properties, assets, console, AI chat)
//! - Theme system (dark/light + orange accent)
//! - Internationalization (EN/ES)
//! - Settings panel

pub mod agent_executor;
pub mod app;
pub mod commands;
pub mod electronics_assets;
mod frame_timing;
pub mod game_runtime;
pub mod panels;
pub mod pcb_document;
pub mod schematic_document;
pub mod script_support;
pub mod session_document;
pub mod studio_surface;
pub mod theme;
pub mod ui_icons;

pub use app::AuraRafiApp;
