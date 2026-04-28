//! # raf_editor
//!
//! Visual editor for AuraRafi. Provides:
//! - Loading screen with branding
//! - Project Hub (recent projects + create new)
//! - Main editor with panels (viewport, hierarchy, properties, assets, console, AI chat)
//! - Theme system (dark/light + orange accent)
//! - Internationalization (EN/ES)
//! - Settings panel

pub mod app;
pub mod game_runtime;
pub mod panels;
pub mod pcb_document;
pub mod schematic_document;
pub mod script_support;
pub mod theme;
pub mod ui_icons;

pub use app::AuraRafiApp;
