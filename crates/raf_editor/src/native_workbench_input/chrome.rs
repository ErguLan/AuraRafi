use super::*;

impl NativeGameWorkbench {
    pub(super) fn handle_app_command(
        &mut self,
        name: &String,
        dispatched: &UiDispatchedAction,
        intents: &mut Vec<NativeWorkbenchIntent>,
    ) -> bool {
        if name.starts_with("runtime.") || name.starts_with("agent.lab.open.") {
            intents.push(NativeWorkbenchIntent::Command(name.clone()));
            return true;
        }
        if name == crate::application_menu::command::SEARCH_OPEN {
            self.open_search();
            self.open_menu = None;
            return true;
        }
        if name == "agent.workspace.save.retry" {
            self.agent_save.borrow_mut().retry();
            self.toolbar_revision = self.toolbar_revision.wrapping_add(1);
            return true;
        }
        if let Some(action) = parse_agent_action(name, self.agent_settings.language) {
            let reset_history = matches!(
                &action,
                AgentAction::NewChat
                    | AgentAction::SelectSession(_)
                    | AgentAction::DeleteSession(_)
            );
            self.apply_agent_action(action, intents, reset_history);
            return true;
        }
        if let Some(submenu_id) = name.strip_prefix("application.menu.submenu.open:") {
            self.open_submenu = Some(submenu_id.to_string());
            self.toolbar_revision = self.toolbar_revision.wrapping_add(1).max(1);
            return true;
        }
        if let Some(menu_id) = name.strip_prefix("application.menu.") {
            self.open_menu =
                (self.open_menu.as_deref() != Some(menu_id)).then(|| menu_id.to_string());
            self.open_submenu = None;
            self.toolbar_revision = self.toolbar_revision.wrapping_add(1).max(1);
            return true;
        }
        // Hierarchy rows publish hover/drag-over commands from
        // PointerMove. They are not menu selections and must not
        // dismiss an application menu merely because the cursor
        // crossed the panel on its way to the popup.
        if !matches!(&dispatched.event, raf_ui::UiEventKind::PointerMove) {
            self.open_menu = None;
        }
        match name.as_str() {
            "viewport.dropdown.view.toggle" => {
                self.toolbar_state.view_menu_open = !self.toolbar_state.view_menu_open;
                self.toolbar_state.shading_menu_open = false;
                self.toolbar_state.primitive_menu_open = false;
                self.toolbar_revision = self.toolbar_revision.wrapping_add(1).max(1);
                return true;
            }
            "viewport.dropdown.view.cancel" => {
                self.toolbar_state.view_menu_open = false;
                self.toolbar_revision = self.toolbar_revision.wrapping_add(1).max(1);
                return true;
            }
            "viewport.dropdown.shading.toggle" => {
                self.toolbar_state.shading_menu_open = !self.toolbar_state.shading_menu_open;
                self.toolbar_state.view_menu_open = false;
                self.toolbar_state.primitive_menu_open = false;
                self.toolbar_revision = self.toolbar_revision.wrapping_add(1).max(1);
                return true;
            }
            "viewport.dropdown.shading.cancel" => {
                self.toolbar_state.shading_menu_open = false;
                self.toolbar_revision = self.toolbar_revision.wrapping_add(1).max(1);
                return true;
            }
            "viewport.create-primitive.toggle" => {
                self.toolbar_state.primitive_menu_open = !self.toolbar_state.primitive_menu_open;
                self.toolbar_state.view_menu_open = false;
                self.toolbar_state.shading_menu_open = false;
                self.toolbar_revision = self.toolbar_revision.wrapping_add(1).max(1);
                return true;
            }
            "viewport.create-primitive.cancel" => {
                self.toolbar_state.primitive_menu_open = false;
                self.toolbar_revision = self.toolbar_revision.wrapping_add(1).max(1);
                return true;
            }
            "viewport.dropdown.building.toggle" => {
                self.toolbar_state.building_menu_open = !self.toolbar_state.building_menu_open;
                self.toolbar_state.view_menu_open = false;
                self.toolbar_state.shading_menu_open = false;
                self.toolbar_state.primitive_menu_open = false;
                self.toolbar_revision = self.toolbar_revision.wrapping_add(1).max(1);
                return true;
            }
            "viewport.dropdown.building.cancel" => {
                self.toolbar_state.building_menu_open = false;
                self.toolbar_revision = self.toolbar_revision.wrapping_add(1).max(1);
                return true;
            }
            "viewport.building.free" | "viewport.building.professional" => {
                let style = if name == "viewport.building.free" {
                    BuildingStyle::Free
                } else {
                    BuildingStyle::Organized
                };
                self.apply_toolbar_action(ViewportToolbarAction::SetBuildingStyle(style));
                intents.push(NativeWorkbenchIntent::ProjectSettingToggle {
                    key: format!("project-settings.building-style.{}", style.slug()),
                    value: true,
                });
                return true;
            }
            _ => {}
        }
        false
    }
}

impl NativeGameWorkbench {
    pub(super) fn handle_window_command(
        &mut self,
        name: &String,
        intents: &mut Vec<NativeWorkbenchIntent>,
    ) -> bool {
        match name.as_str() {
            "window.drag" => {
                intents.push(NativeWorkbenchIntent::Window(UiWindowCommand::BeginDrag));
                return true;
            }
            "window.minimize" => {
                intents.push(NativeWorkbenchIntent::Window(UiWindowCommand::Minimize));
                return true;
            }
            "window.maximize" => {
                intents.push(NativeWorkbenchIntent::Window(
                    UiWindowCommand::ToggleMaximize,
                ));
                return true;
            }
            "window.close" => {
                intents.push(NativeWorkbenchIntent::Window(UiWindowCommand::Close));
                return true;
            }
            "window.system-menu" => {
                intents.push(NativeWorkbenchIntent::Window(
                    UiWindowCommand::ShowSystemMenu,
                ));
                return true;
            }
            _ => {}
        }
        false
    }
}

impl NativeGameWorkbench {
    pub(super) fn handle_escape_dismissals(&mut self, native_input: &NativeUiInputBridge) {
        if native_input
            .snapshot()
            .key_pressed(raf_core::InputKey::Escape)
        {
            if self.search_surface.is_open() {
                self.search_surface.close();
                self.restore_search_focus();
            }
            self.open_menu = None;
            self.open_submenu = None;
            self.hierarchy_renaming = None;
            self.hierarchy_drag = None;
            self.inspector_name_editing = None;
            self.hierarchy_empty_menu_open = false;
            self.hierarchy_primitive_menu_open = false;
            self.hierarchy_menu_target = None;
            self.hierarchy_menu_position = None;
            self.nodes_host.close_context_menu();
            self.assets_surface.close_menus();
            self.assets_surface.close_modals();
            self.toolbar_state.view_menu_open = false;
            self.toolbar_state.shading_menu_open = false;
            self.toolbar_state.primitive_menu_open = false;
            self.toolbar_state.building_menu_open = false;
            self.inspector_view.dropdown = None;
            self.inspector_view.color_picker = false;
            if self.agent_panel.borrow_mut().has_open_menu() {
                self.agent_panel.borrow_mut().close_menus();
            }
            self.toolbar_revision = self.toolbar_revision.wrapping_add(1).max(1);
        }
    }
}

impl NativeGameWorkbench {
    pub(super) fn handle_primary_press_dismissals(
        &mut self,
        native_input: &NativeUiInputBridge,
        actions: &[UiDispatchedAction],
        scene: &SceneGraph,
        selected: &[SceneNodeId],
        intents: &mut Vec<NativeWorkbenchIntent>,
    ) {
        let snapshot = native_input.snapshot();
        if snapshot.button_pressed(raf_core::PointerButton::Primary) {
            if self.agent_panel.borrow_mut().has_open_menu()
                && !actions.iter().any(|action| {
                    action.target_id.starts_with("agent.model")
                        || action.target_id.starts_with("agent.mode")
                        || action.target_id.starts_with("agent.add-model")
                        || action.target_id.starts_with("agent.new-model")
                })
            {
                self.agent_panel.borrow_mut().close_menus();
                self.toolbar_revision = self.toolbar_revision.wrapping_add(1).max(1);
            }
            let hovered = self.host.session().interaction.focus.hovered.clone();
            // The Electronics context menu is drawn by this retained surface, but
            // the CAD canvas only runs its own input path while nothing is
            // hovered. A press that landed on a panel therefore left the menu
            // open with no way to dismiss it, which is the outside-click rule of
            // the menu contract. One pointer gesture closes it here through the
            // same semantic command its own items already dispatch, so there is
            // no second close path, and nothing steals focus from the control the
            // user actually pressed.
            //
            // `hovered` is what separates this from a press on the bare canvas:
            // the canvas has no retained target under it and already dismisses
            // the menu itself. `electronics.context.cancel` also ends a pending
            // wire, and deliberately does not resolve an armed deletion
            // confirmation, so it can never cancel a dialog the user did not
            // click.
            if self.electronics_menu_open
                && hovered.is_some()
                && !hovered
                    .as_deref()
                    .is_some_and(electronics::is_electronics_context_menu_target)
            {
                intents.push(NativeWorkbenchIntent::Command(
                    "electronics.context.cancel".to_string(),
                ));
                // The dismissal is emitted after the activity accounting for this
                // frame, so the revision is bumped here to guarantee the retained
                // chrome is rebuilt for the closed menu.
                self.toolbar_revision = self.toolbar_revision.wrapping_add(1).max(1);
            }
            if self.hierarchy_renaming.is_some()
                && !hovered
                    .as_deref()
                    .is_some_and(|id| id.starts_with("hierarchy.rename.control."))
            {
                self.hierarchy_renaming = None;
                self.toolbar_revision = self.toolbar_revision.wrapping_add(1).max(1);
            }
            if self.inspector_name_editing.is_some() && hovered.as_deref() != Some("inspector.name")
            {
                self.inspector_name_editing = None;
                if let Some(id) = selected.first().copied() {
                    if let Some(node) = scene.get(id) {
                        self.host.session_mut().interaction.controls.set_text(
                            "inspector.name",
                            node.name.clone(),
                            256,
                        );
                    }
                }
                self.toolbar_revision = self.toolbar_revision.wrapping_add(1).max(1);
            }
            if self.nodes_host.has_open_menu()
                && !hovered
                    .as_deref()
                    .is_some_and(|id| id.starts_with("nodes.context-menu"))
            {
                self.nodes_host.close_context_menu();
                self.toolbar_revision = self.toolbar_revision.wrapping_add(1).max(1);
            }
            if self.nodes_host.palette_popup().is_some()
                && !hovered
                    .as_deref()
                    .is_some_and(|id| id.starts_with("nodes.palette-popup"))
            {
                self.nodes_host.close_palette_popup();
                self.toolbar_revision = self.toolbar_revision.wrapping_add(1).max(1);
            }
            if self.assets_surface.has_open_menu()
                && !hovered
                    .as_deref()
                    .is_some_and(AssetsSurfaceHost::is_overlay_target)
            {
                self.assets_surface.close_menus();
                self.toolbar_revision = self.toolbar_revision.wrapping_add(1).max(1);
            }
            if self.inspector_view.dropdown.is_some() || self.inspector_view.color_picker {
                let inside_popup_bounds = snapshot.pointer_position.is_some_and(|point| {
                    self.host
                        .layout_rect("inspector.color.popover")
                        .is_some_and(|rect| {
                            rect.contains([point[0] - self.rect.x, point[1] - self.rect.y])
                        })
                });
                let inside_inspector_popup = inside_popup_bounds
                    || hovered.as_deref().is_some_and(|id| {
                        id.starts_with("inspector.")
                            && (id.contains("dropdown")
                                || id.contains("color")
                                || id.contains(".trigger")
                                || id.contains(".menu")
                                || id.contains(".option."))
                    });
                if !inside_inspector_popup {
                    self.inspector_view.dropdown = None;
                    self.inspector_view.color_picker = false;
                    self.toolbar_revision = self.toolbar_revision.wrapping_add(1).max(1);
                }
            }
        }
    }
}

impl NativeGameWorkbench {
    pub(super) fn handle_toolbar_popup_dismissals(&mut self, native_input: &NativeUiInputBridge) {
        let snapshot = native_input.snapshot();
        let inside_toolbar_popup = snapshot.pointer_position.is_some_and(|point| {
            [
                "viewport.toolbar.primitive-menu",
                "viewport.toolbar.view-menu",
                "viewport.toolbar.shading-menu",
                "viewport.toolbar.building-menu",
            ]
            .iter()
            .any(|id| {
                self.host.layout_rect(id).is_some_and(|rect| {
                    rect.contains([point[0] - self.rect.x, point[1] - self.rect.y])
                })
            })
        });
        if snapshot.button_pressed(raf_core::PointerButton::Primary)
            && !inside_toolbar_popup
            && (self.toolbar_state.view_menu_open
                || self.toolbar_state.shading_menu_open
                || self.toolbar_state.primitive_menu_open
                || self.toolbar_state.building_menu_open)
            && !self
                .host
                .session()
                .interaction
                .focus
                .hovered
                .as_deref()
                .is_some_and(|target| {
                    target.starts_with("viewport.toolbar.")
                        || target.starts_with("viewport.primitive-option.")
                        || target.starts_with("viewport.building-option.")
                })
        {
            self.toolbar_state.view_menu_open = false;
            self.toolbar_state.shading_menu_open = false;
            self.toolbar_state.primitive_menu_open = false;
            self.toolbar_state.building_menu_open = false;
            self.toolbar_revision = self.toolbar_revision.wrapping_add(1).max(1);
        }
        if self.open_menu.is_some()
            && snapshot.button_pressed(raf_core::PointerButton::Primary)
            && !self
                .host
                .session()
                .interaction
                .focus
                .hovered
                .as_deref()
                .is_some_and(|target| {
                    target.starts_with("application-bar.menu.")
                        || target.starts_with("application-menu.")
                })
        {
            self.open_menu = None;
            self.open_submenu = None;
            self.hierarchy_menu_target = None;
            self.toolbar_revision = self.toolbar_revision.wrapping_add(1).max(1);
        }
    }
}
