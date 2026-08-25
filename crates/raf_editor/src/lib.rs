//! # raf_editor
//!
//! AuraRafi editor boundary.
//!
//! The editor-chrome layer is being rebuilt as retained RafUI surfaces. This
//! crate currently owns the loading/Hub entry surfaces, project/document
//! wiring, domain commands, the beta downbar, and full-client Game/Electronics
//! canvas hosts. The engine and renderer remain independent.

pub mod agent_executor;
pub mod application_bar_host;
pub mod application_bar_surface;
pub mod application_menu;
pub mod attached;
pub mod building_mode;
pub mod commands;
pub mod console;
pub mod editor_command_registry;
pub mod editor_layout;
pub mod editor_shortcuts;
pub mod editor_viewport_app;
pub mod electronics_assets;
pub mod electronics_controller;
pub mod electronics_history;
pub(crate) mod electronics_minimap;
pub mod folder_picker;
pub mod game_runtime;
pub mod native_application;
pub(crate) mod native_attached_executor;
pub(crate) mod native_editor_commands;
pub mod native_editor_runtime;
pub mod native_electronics;
pub mod native_input;
pub(crate) mod native_project_controller;
pub mod native_studio;
pub mod native_surface;
pub mod native_workbench;
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
