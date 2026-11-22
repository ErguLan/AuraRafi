//! Viewport toolbar actions and runtime toolbar projection.

use super::*;

impl NativeGameWorkbench {
    pub fn set_runtime_toolbar(&mut self, state: raf_player::surface::RuntimeToolbarState) -> bool {
        if self.runtime_toolbar == state {
            return false;
        }
        self.runtime_toolbar = state;
        self.toolbar_revision = self.toolbar_revision.wrapping_add(1).max(1);
        true
    }

    /// Presents or clears the local Play failure banner.
    pub fn set_runtime_notice(&mut self, notice: Option<RuntimeNotice>) -> bool {
        if self.runtime_notice == notice {
            return false;
        }
        self.runtime_notice = notice;
        self.toolbar_revision = self.toolbar_revision.wrapping_add(1).max(1);
        true
    }

    pub(super) fn apply_toolbar_action(&mut self, action: ViewportToolbarAction) -> bool {
        let state = &mut self.toolbar_state;
        match action {
            ViewportToolbarAction::Select => {
                state.select_mode = true;
                state.tool = ViewportTool::Select;
            }
            ViewportToolbarAction::Move => {
                state.select_mode = false;
                state.tool = ViewportTool::Move;
            }
            ViewportToolbarAction::Rotate => {
                state.select_mode = false;
                state.tool = ViewportTool::Rotate;
            }
            ViewportToolbarAction::Scale => {
                state.select_mode = false;
                state.tool = ViewportTool::Scale;
            }
            ViewportToolbarAction::Solid => {
                state.render_style = ViewportRenderStyle::Solid;
                state.shading_menu_open = false;
            }
            ViewportToolbarAction::TogglePolygons => {
                state.polygons_visible = !state.polygons_visible
            }
            ViewportToolbarAction::ToggleGrid => state.grid_visible = !state.grid_visible,
            ViewportToolbarAction::ToggleLabels => state.labels_visible = !state.labels_visible,
            ViewportToolbarAction::View2d => {
                state.view_mode = ViewportViewMode::View2d;
                state.view_menu_open = false;
            }
            ViewportToolbarAction::View3d => {
                state.view_mode = ViewportViewMode::View3d;
                state.view_menu_open = false;
            }
            ViewportToolbarAction::Focus
            | ViewportToolbarAction::ResetView
            | ViewportToolbarAction::CreatePrimitive(_) => {
                state.view_menu_open = false;
                state.shading_menu_open = false;
                state.primitive_menu_open = false;
            }
            ViewportToolbarAction::SetBuildingStyle(style) => {
                state.building_style = style;
                state.building_menu_open = false;
                state.view_menu_open = false;
                state.shading_menu_open = false;
                state.primitive_menu_open = false;
            }
        }
        let settings_changed = match action {
            ViewportToolbarAction::Solid => {
                let next = raf_core::config::ViewportRenderMode::Solid;
                let changed = self.agent_settings.viewport_render_mode != next;
                self.agent_settings.viewport_render_mode = next;
                changed
            }
            ViewportToolbarAction::ToggleGrid => {
                let next = self.toolbar_state.grid_visible;
                let changed = self.agent_settings.grid_visible != next;
                self.agent_settings.grid_visible = next;
                changed
            }
            ViewportToolbarAction::ToggleLabels => {
                let next = self.toolbar_state.labels_visible;
                let changed = self.agent_settings.show_viewport_labels != next;
                self.agent_settings.show_viewport_labels = next;
                changed
            }
            _ => false,
        };
        self.toolbar_revision = self.toolbar_revision.wrapping_add(1).max(1);
        settings_changed
    }
}
