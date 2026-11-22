use super::*;

impl NativeGameWorkbench {
    pub(super) fn handle_layout_command(
        &mut self,
        name: &String,
        native_input: &NativeUiInputBridge,
        intents: &mut Vec<NativeWorkbenchIntent>,
    ) -> bool {
        if let Some(raw_target) = name.strip_prefix("layout.resize.start.") {
            let kind = match raw_target {
                "layout.left" => WorkbenchResizeKind::LeftPanel,
                "layout.right" => WorkbenchResizeKind::RightPanel,
                "layout.bottom" => WorkbenchResizeKind::BottomDock,
                _ => return true,
            };
            let Some(pointer) = native_input.snapshot().pointer_position else {
                return true;
            };
            let Some(layout) = self.last_layout else {
                return true;
            };
            let origin_value = match kind {
                WorkbenchResizeKind::LeftPanel => {
                    layout.left_panel.map_or(0.0, |panel| panel.width)
                }
                WorkbenchResizeKind::RightPanel => {
                    layout.right_panel.map_or(0.0, |panel| panel.width)
                }
                WorkbenchResizeKind::BottomDock => layout.bottom_dock.height,
            };
            self.panel_resize = Some(WorkbenchResizeState {
                kind,
                origin_pointer: pointer,
                origin_value,
            });
            self.toolbar_revision = self.toolbar_revision.wrapping_add(1).max(1);
            return true;
        }
        if let Some(raw_target) = name.strip_prefix("layout.resize.move.") {
            let Some(resize) = self.panel_resize else {
                return true;
            };
            let target_kind = match raw_target {
                "layout.left" => WorkbenchResizeKind::LeftPanel,
                "layout.right" => WorkbenchResizeKind::RightPanel,
                "layout.bottom" => WorkbenchResizeKind::BottomDock,
                _ => return true,
            };
            if resize.kind != target_kind {
                return true;
            }
            let Some(pointer) = native_input.snapshot().pointer_position else {
                return true;
            };
            let delta_x = pointer[0] - resize.origin_pointer[0];
            let delta_y = pointer[1] - resize.origin_pointer[1];
            let value = match resize.kind {
                WorkbenchResizeKind::LeftPanel => resize.origin_value + delta_x,
                WorkbenchResizeKind::RightPanel => resize.origin_value - delta_x,
                WorkbenchResizeKind::BottomDock => resize.origin_value - delta_y,
            };
            let side = match resize.kind {
                WorkbenchResizeKind::LeftPanel => "left",
                WorkbenchResizeKind::RightPanel => "right",
                WorkbenchResizeKind::BottomDock => "bottom",
            };
            intents.push(NativeWorkbenchIntent::Command(format!(
                "layout.resize.{side}:{value:.2}"
            )));
            return true;
        }
        if name.starts_with("layout.resize.end.") {
            self.panel_resize = None;
            if name.ends_with("layout.bottom") {
                self.bottom_dock.persist_project_layout();
            }
            self.toolbar_revision = self.toolbar_revision.wrapping_add(1).max(1);
            return true;
        }
        if let Some(raw_target) = name.strip_prefix("bottom.resize.start.") {
            if let Some((left_group_id, right_group_id)) = raw_target.split_once('|') {
                if let Some([pointer_x, _]) = native_input.snapshot().pointer_position {
                    if self
                        .bottom_dock
                        .begin_resize(left_group_id, right_group_id, pointer_x)
                    {
                        self.toolbar_revision = self.toolbar_revision.wrapping_add(1).max(1);
                    }
                }
            }
            return true;
        }
        if let Some(raw_target) = name.strip_prefix("bottom.resize.move.") {
            if let Some((left_group_id, right_group_id)) = raw_target.split_once('|') {
                if let Some([pointer_x, _]) = native_input.snapshot().pointer_position {
                    if self.bottom_dock.resize().is_some_and(|resize| {
                        resize.left_group_id == left_group_id
                            && resize.right_group_id == right_group_id
                    }) {
                        if let Some(layout) = self.last_layout {
                            if self.bottom_dock.update_resize(
                                pointer_x,
                                layout.bottom_dock.width,
                                4.0,
                            ) {
                                self.toolbar_revision =
                                    self.toolbar_revision.wrapping_add(1).max(1);
                            }
                        }
                    }
                }
            }
            return true;
        }
        if name.starts_with("bottom.resize.end.") {
            if self.bottom_dock.end_resize() {
                self.toolbar_revision = self.toolbar_revision.wrapping_add(1).max(1);
                self.bottom_dock.persist_project_layout();
            }
            return true;
        }
        if let Some(raw_target) = name.strip_prefix("bottom.drag.start.") {
            if let Some((group_id, tab_id)) = raw_target.split_once('|') {
                if let Some(pointer) = native_input.snapshot().pointer_position {
                    if self.bottom_dock.begin_drag(group_id, tab_id, pointer) {
                        self.drag_motion.set_immediate(0.0);
                        self.drag_motion.set_target(1.0);
                        self.toolbar_revision = self.toolbar_revision.wrapping_add(1).max(1);
                    }
                }
            }
            return true;
        }
        if let Some(raw_target) = name.strip_prefix("bottom.drag.move.") {
            if let Some((group_id, tab_id)) = raw_target.split_once('|') {
                if let Some(pointer) = native_input.snapshot().pointer_position {
                    if self.bottom_dock.drag().is_some_and(|drag| {
                        drag.source_group_id == group_id && drag.source_tab_id == tab_id
                    }) {
                        if let Some(layout) = self.last_layout {
                            if self.bottom_dock.update_drag_at(
                                pointer,
                                [
                                    layout.bottom_dock.x,
                                    layout.bottom_dock.y,
                                    layout.bottom_dock.width,
                                    layout.bottom_dock.height,
                                ],
                                4.0,
                            ) {
                                self.toolbar_revision =
                                    self.toolbar_revision.wrapping_add(1).max(1);
                            }
                        }
                    }
                }
            }
            return true;
        }
        if let Some(raw_target) = name.strip_prefix("bottom.drag.end.") {
            let _ = raw_target;
            self.finish_bottom_drag();
            return true;
        }
        if let Some(raw_target) = name.strip_prefix("bottom.context.open.") {
            if let Some((group_id, tab_id)) = raw_target.split_once('|') {
                self.bottom_dock.open_context_menu(
                    group_id,
                    tab_id,
                    native_input
                        .snapshot()
                        .pointer_position
                        .unwrap_or([self.rect.x, self.rect.y]),
                );
                self.toolbar_revision = self.toolbar_revision.wrapping_add(1).max(1);
            }
            return true;
        }
        false
    }
}

impl NativeGameWorkbench {
    pub(super) fn handle_dock_command(
        &mut self,
        name: &String,
        native_input: &NativeUiInputBridge,
        router: &mut InputRouter,
    ) -> bool {
        if name == "bottom.toggle-collapsed" {
            self.bottom_dock.toggle_collapsed();
            self.toolbar_revision = self.toolbar_revision.wrapping_add(1).max(1);
            // The dock already consumed this chrome action. The
            // application synchronizes its layout after input;
            // forwarding it also dispatches a false domain error.
            return true;
        }
        if let Some(raw_tab) = name.strip_prefix("bottom.tab.") {
            if let Some((group_id, tab_id)) = raw_tab.split_once('|') {
                if self.bottom_dock.select_in_group(group_id, tab_id) {
                    self.reset_input_state(router);
                    self.toolbar_revision = self.toolbar_revision.wrapping_add(1).max(1);
                }
            }
            return true;
        }
        if let Some(raw_target) = name.strip_prefix("bottom.context.split-left.") {
            if let Some((group_id, tab_id)) = raw_target.split_once('|') {
                self.bottom_dock.open_context_menu(
                    group_id,
                    tab_id,
                    native_input
                        .snapshot()
                        .pointer_position
                        .unwrap_or([self.rect.x, self.rect.y]),
                );
                if self.bottom_dock.split_context(true) {
                    self.reset_input_state(router);
                    self.toolbar_revision = self.toolbar_revision.wrapping_add(1).max(1);
                    self.bottom_dock.persist_project_layout();
                }
            }
            return true;
        }
        if let Some(raw_target) = name.strip_prefix("bottom.context.split-right.") {
            if let Some((group_id, tab_id)) = raw_target.split_once('|') {
                self.bottom_dock.open_context_menu(
                    group_id,
                    tab_id,
                    native_input
                        .snapshot()
                        .pointer_position
                        .unwrap_or([self.rect.x, self.rect.y]),
                );
                if self.bottom_dock.split_context(false) {
                    self.reset_input_state(router);
                    self.toolbar_revision = self.toolbar_revision.wrapping_add(1).max(1);
                    self.bottom_dock.persist_project_layout();
                }
            }
            return true;
        }
        if name == "bottom.context.close" {
            self.bottom_dock.close_context_menu();
            self.toolbar_revision = self.toolbar_revision.wrapping_add(1).max(1);
            return true;
        }
        if name == "bottom.context.reset" {
            self.bottom_dock.reset_group(self.project_type);
            self.reset_input_state(router);
            self.toolbar_revision = self.toolbar_revision.wrapping_add(1).max(1);
            return true;
        }
        false
    }
}

impl NativeGameWorkbench {
    pub(super) fn handle_pointer_cancel(
        &mut self,
        native_input: &NativeUiInputBridge,
        router: &mut InputRouter,
    ) {
        let snapshot = native_input.snapshot();
        // Pointer capture can be interrupted by a retained-surface rebuild,
        // focus loss, or a platform release that RafUI did not route back to
        // the captured tab node. The native snapshot is authoritative for
        // this gesture; never leave the drop preview alive after release.
        if (!snapshot.window_focused || snapshot.button_released(raf_core::PointerButton::Primary))
            && self.bottom_dock.drag().is_some()
        {
            self.finish_bottom_drag();
            self.host.session_mut().interaction.cancel_pointer_gesture();
            router.release_pointer_unchecked(raf_core::PointerButton::Primary);
        }
        if (!snapshot.window_focused || snapshot.button_released(raf_core::PointerButton::Primary))
            && self.hierarchy_drag.is_some()
        {
            self.hierarchy_drag = None;
            self.toolbar_revision = self.toolbar_revision.wrapping_add(1).max(1);
            self.host.session_mut().interaction.cancel_pointer_gesture();
            router.release_pointer_unchecked(raf_core::PointerButton::Primary);
        }
        if (!snapshot.window_focused || snapshot.button_released(raf_core::PointerButton::Primary))
            && self.panel_resize.is_some()
        {
            self.panel_resize = None;
            self.toolbar_revision = self.toolbar_revision.wrapping_add(1).max(1);
            self.host.session_mut().interaction.cancel_pointer_gesture();
            router.release_pointer_unchecked(raf_core::PointerButton::Primary);
        }
    }
}
