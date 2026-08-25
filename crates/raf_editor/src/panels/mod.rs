//! Editor panels module.
//!
//! Only the retained entry surfaces and full-client canvas adapters remain
//! mounted here during the editor-chrome decommission.

pub mod agent_surface;
pub mod ai_chat;
pub mod editor_bottom_dock_host;
pub mod editor_bottom_dock_surface;
pub mod editor_panel_splitter_surface;
pub mod electronics_canvas_overlay_surface;
pub mod electronics_context_menu_surface;
pub mod electronics_inspector_surface;
pub mod electronics_navigator_surface;
pub mod electronics_surface;
pub mod electronics_toolbar_surface;
pub mod exit_confirmation_surface_host;
pub mod gpu_canvas;
pub mod hierarchy_model;
pub mod hierarchy_surface;
pub mod hierarchy_surface_host;
pub mod hub_surface_host;
pub mod inspector_surface;
pub mod inspector_surface_host;
pub mod loading_surface;
pub mod new_project_surface;
pub mod nodes_surface;
pub mod primitive_create;
pub mod project_settings_surface_host;
pub mod raf_ui_surface_bridge;
pub mod raf_ui_tooltip;
pub mod search_surface;
pub mod search_surface_host;
pub mod settings_surface_host;
pub mod viewport;
pub mod viewport_controller;
pub mod viewport_grid;
pub mod viewport_interaction;
pub mod viewport_overlay;
pub mod viewport_surface_host;
pub mod viewport_toolbar_surface;
pub mod viewport_toolbar_surface_host;
