use super::*;

use raf_core::InputOwner;

impl NativeGameWorkbench {
    pub(super) fn process_input_routed<F>(
        &mut self,
        native_input: &NativeUiInputBridge,
        router: &mut InputRouter,
        scene: &SceneGraph,
        selected: &[SceneNodeId],
        project: Option<&Project>,
        mut resolve: F,
    ) -> (Vec<NativeWorkbenchIntent>, Vec<UiDispatchedAction>)
    where
        F: FnMut(&str) -> String,
    {
        self.popup_press_consumed = native_input
            .snapshot()
            .button_pressed(raf_core::PointerButton::Primary)
            && (self.toolbar_state.primitive_menu_open
                || self.toolbar_state.view_menu_open
                || self.toolbar_state.shading_menu_open
                || self.toolbar_state.building_menu_open
                || self.inspector_view.color_picker
                || self.inspector_view.dropdown.is_some());
        let diagnostic_shortcut = native_input.snapshot();
        if diagnostic_shortcut.window_focused
            && !matches!(router.keyboard_owner(), Some(InputOwner::Modal(_)))
            && diagnostic_shortcut.modifiers.command_modifier()
            && diagnostic_shortcut.modifiers.shift
            && diagnostic_shortcut.key_pressed(raf_core::InputKey::F12)
        {
            self.handle_ui_performance_command("ui.performance");
        }
        let dropped_on_composer = native_input
            .snapshot()
            .pointer_position
            .is_some_and(|point| {
                self.host.layout_rect("agent.composer").is_some_and(|rect| {
                    rect.contains([point[0] - self.rect.x, point[1] - self.rect.y])
                })
            });
        if dropped_on_composer
            && native_input.snapshot().window_focused
            && !matches!(router.keyboard_owner(), Some(InputOwner::Modal(_)))
            && !self.search_surface.is_open()
            && !native_input.dropped_files().is_empty()
        {
            self.agent_panel.borrow_mut().apply_action(
                AgentAction::AttachFiles(native_input.dropped_files().to_vec()),
                &mut self.agent_settings,
            );
        }
        if native_input.snapshot().modifiers.command_modifier()
            && native_input.snapshot().window_focused
            && router.keyboard_owner() == Some(self.owner())
            && native_input.snapshot().key_pressed(raf_core::InputKey::V)
            && !native_input.has_clipboard_text()
            && self
                .host
                .session()
                .interaction
                .focus
                .has_focus("agent.input")
        {
            self.agent_panel
                .borrow_mut()
                .apply_action(AgentAction::PasteImage, &mut self.agent_settings);
        }
        if self.electronics_workspace.pcb_menu.is_some()
            && native_input
                .snapshot()
                .key_pressed(raf_core::InputKey::Escape)
        {
            self.electronics_workspace.pcb_menu = None;
            self.toolbar_revision = self.toolbar_revision.wrapping_add(1).max(1);
        }
        if self.console.filters_open
            && native_input
                .snapshot()
                .key_pressed(raf_core::InputKey::Escape)
        {
            self.console.filters_open = false;
            self.toolbar_revision = self.toolbar_revision.wrapping_add(1).max(1);
        }
        if (self.lighting_color_editor.is_some() || self.lighting_eyedropper.is_some())
            && native_input
                .snapshot()
                .key_pressed(raf_core::InputKey::Escape)
        {
            self.lighting_color_editor = None;
            self.lighting_eyedropper = None;
            self.toolbar_revision = self.toolbar_revision.wrapping_add(1).max(1);
        }
        let electronics_transform_had_focus = self
            .host
            .session()
            .interaction
            .focus
            .focused
            .clone()
            .filter(|id| id.starts_with("electronics.inspector.transform."));
        let project_numeric_focus = native_input
            .snapshot()
            .key_pressed(raf_core::InputKey::Escape)
            .then(|| self.host.session().interaction.focus.focused.clone())
            .flatten();
        if native_input
            .snapshot()
            .key_pressed(raf_core::InputKey::Escape)
            && self
                .host
                .session()
                .interaction
                .focus
                .focused
                .as_deref()
                .is_some_and(|id| id.starts_with("electronics.inspector.transform."))
        {
            self.host.session_mut().interaction.focus.clear_focus();
            self.toolbar_revision = self.toolbar_revision.wrapping_add(1).max(1);
        }
        let electronics_value_had_focus = self
            .host
            .session()
            .interaction
            .focus
            .has_focus("electronics.inspector.value");
        let search_open = self.search_surface.is_open();
        let mut intents = Vec::new();
        self.flush_lighting_session(scene, &mut intents);
        if self.hierarchy_drag.is_some() {
            if let Some((pointer, rect)) = native_input
                .snapshot()
                .pointer_position
                .zip(self.host.layout_rect("hierarchy.tree"))
            {
                let direction = if pointer[1] < rect.y + 28.0 {
                    -1.0
                } else if pointer[1] > rect.y + rect.height - 28.0 {
                    1.0
                } else {
                    0.0
                };
                if direction != 0.0 {
                    let dt = native_input.snapshot().delta_seconds.clamp(0.001, 0.05);
                    let controls = &mut self.host.session_mut().interaction.controls;
                    let next = controls.scroll_by("hierarchy.tree", [0.0, direction * 420.0 * dt]);
                    self.note_hierarchy_scroll(next[1]);
                }
            }
        }
        if search_open {
            router.release_keyboard(self.owner());
            for search_intent in self.search_surface.process_input(native_input, router) {
                match search_intent {
                    crate::panels::search_surface_host::SearchIntent::QueryChanged(_) => {
                        self.toolbar_revision = self.toolbar_revision.wrapping_add(1).max(1);
                    }
                    crate::panels::search_surface_host::SearchIntent::Close => {
                        self.restore_search_focus();
                        self.toolbar_revision = self.toolbar_revision.wrapping_add(1).max(1);
                    }
                    crate::panels::search_surface_host::SearchIntent::Activate(result) => {
                        self.search_surface.close();
                        self.restore_search_focus();
                        self.toolbar_revision = self.toolbar_revision.wrapping_add(1).max(1);
                        match result.kind {
                            SearchResultKind::Command(command) => {
                                intents.push(NativeWorkbenchIntent::Command(command));
                            }
                            SearchResultKind::Hierarchy(id) => {
                                intents.push(NativeWorkbenchIntent::Command(format!(
                                    "hierarchy.focus:{}",
                                    id.0
                                )));
                            }
                            SearchResultKind::Project(_) => {
                                intents.push(NativeWorkbenchIntent::Command(
                                    "project.settings".to_string(),
                                ));
                            }
                            SearchResultKind::Asset(asset) => {
                                intents.push(NativeWorkbenchIntent::Command(format!(
                                    "assets.open:{asset}"
                                )));
                            }
                        }
                    }
                }
            }
            if native_input
                .snapshot()
                .button_pressed(raf_core::PointerButton::Primary)
                && !native_input
                    .snapshot()
                    .pointer_position
                    .is_some_and(|point| self.search_surface.contains_point(point))
            {
                self.search_surface.close();
                self.restore_search_focus();
                self.toolbar_revision = self.toolbar_revision.wrapping_add(1).max(1);
            }
        }
        let mut actions = Vec::new();
        self.interface_keyboard(native_input, router);
        if !search_open {
            // The compass is a true overlay. Give it the first chance to
            // consume a pointer press so an underlying toolbar control cannot
            // win when the two rectangles overlap in a compact viewport.
            let compass_actions = if self.interface.workspace.active() {
                Vec::new()
            } else {
                self.viewport_compass
                    .process_input(native_input, router, &mut resolve)
            };
            let compass_clicked = compass_actions
                .iter()
                .any(|action| matches!(&action.action, UiAction::Command { .. }));
            if compass_clicked {
                // The compass is an overlay over the renderer-owned canvas.
                // A click there must release any text/button focus from the
                // main workbench before the viewport consumes the frame.
                self.host.session_mut().interaction.focus.clear_focus();
                router.cancel_owner(self.owner());
            }
            actions.extend(compass_actions);
            if self.electronics_overlay_active
                && self
                    .electronics_workspace
                    .pcb_menu
                    .as_deref()
                    .is_none_or(|menu| menu == "rules")
                && !self.console.filters_open
            {
                let overlay_actions = self.electronics_overlay_host.process_routed_input(
                    self.electronics_overlay_canvas.logical_size(),
                    native_input.scale_factor() as f32,
                    &mut resolve,
                    native_input,
                    router,
                    raf_core::InputOwner::RetainedUi(raf_core::InputRegionId::from_static(
                        "electronics.overlay",
                    )),
                    raf_ui::UiRect::new(
                        self.electronics_overlay_canvas.x,
                        self.electronics_overlay_canvas.y,
                        self.electronics_overlay_canvas.width,
                        self.electronics_overlay_canvas.height,
                    ),
                );
                if overlay_actions
                    .iter()
                    .any(|action| matches!(&action.action, UiAction::Command { .. }))
                {
                    self.host.session_mut().interaction.focus.clear_focus();
                    router.cancel_owner(self.owner());
                }
                actions.extend(overlay_actions);
            }
            actions.extend(self.host.process_routed_input(
                self.rect.logical_size(),
                native_input.scale_factor() as f32,
                &mut resolve,
                native_input,
                router,
                self.owner(),
                raf_ui::UiRect::new(self.rect.x, self.rect.y, self.rect.width, self.rect.height),
            ));
        }
        if electronics_transform_had_focus.is_some()
            && electronics_transform_had_focus != self.host.session().interaction.focus.focused
        {
            // An unsubmitted field reverts on focus loss. Re-seed the visible
            // value even when the click did not mutate any document or toolbar.
            self.toolbar_revision = self.toolbar_revision.wrapping_add(1).max(1);
        }
        if !search_open && project_numeric_focus.is_some() {
            self.cancel_project_numeric_input(project_numeric_focus.as_deref());
        }
        if native_input
            .snapshot()
            .button_pressed(raf_core::PointerButton::Primary)
            && self.bottom_dock.context_menu().is_some()
            && !actions.iter().any(|action| {
                matches!(
                    &action.action,
                    UiAction::Command { name } if name.starts_with("bottom.context.")
                )
            })
        {
            self.bottom_dock.close_context_menu();
            self.toolbar_revision = self.toolbar_revision.wrapping_add(1).max(1);
        }
        // Finish the previous field before canvas/toolbar commands change the
        // selection. The edit carries the UUID of the original component.
        if electronics_value_had_focus {
            let current_text = self
                .host
                .session()
                .interaction
                .controls
                .text("electronics.inspector.value")
                .to_string();
            if let Some(draft) = self.electronics_value_draft.as_mut() {
                draft.value = current_text;
            }
        }
        if let Some(finished) = electronics_model::finish_inspector_value_edit(
            electronics_value_had_focus,
            &mut self.host.session_mut().interaction.focus,
            native_input.snapshot(),
            &mut self.electronics_value_draft,
        ) {
            if let electronics_model::InspectorValueEditEnd::Apply { source_id, value } = finished {
                intents.insert(
                    0,
                    NativeWorkbenchIntent::Command(
                        crate::electronics_action::ElectronicsAction::CommitInspectorValueFor {
                            source_id,
                            value,
                        }
                        .to_command_string(),
                    ),
                );
            }
            self.toolbar_revision = self.toolbar_revision.wrapping_add(1).max(1);
        }
        for dispatched in &actions {
            if self.handle_animation_action(&dispatched.action, scene, selected, &mut intents) {
                continue;
            }
            if self.handle_interface_action(&dispatched.action, native_input, scene, selected) {
                continue;
            }
            if dispatched.target_id.starts_with("ui.designer.preview.") {
                continue;
            }
            if self.handle_project_numeric_action(&dispatched.action, &mut intents) {
                continue;
            }
            if self.handle_lighting_action(&dispatched.action, scene, &mut intents) {
                continue;
            }
            match &dispatched.action {
                UiAction::SetText { key, value } => {
                    self.handle_set_text(key, value, selected, &mut intents);
                }
                UiAction::SetToggle { key, value } => {
                    self.handle_set_toggle(key, value, selected, project, &mut intents);
                }
                UiAction::SetSelect {
                    key,
                    value,
                    index: _,
                } => {
                    if let Some(command) =
                        crate::native_behavior_authoring::select_action(scene, selected, key, value)
                    {
                        intents.push(NativeWorkbenchIntent::Command(command));
                        self.inspector_view.dropdown = None;
                        self.host
                            .session_mut()
                            .interaction
                            .focus
                            .request_focus(format!("{key}.trigger"));
                    } else if key == "agent.model" {
                        let keep_open = self.agent_panel.borrow_mut().model_menu_open;
                        self.apply_agent_action(
                            AgentAction::SelectModel(value.clone()),
                            &mut intents,
                            false,
                        );
                        if keep_open {
                            self.agent_panel.borrow_mut().model_menu_open = true;
                        }
                    } else if key == "agent.mode" {
                        let mode = match value.as_str() {
                            "inspect" => Some(AgentMode::Inspect),
                            "plan" => Some(AgentMode::Plan),
                            "active" => Some(AgentMode::Active),
                            _ => None,
                        };
                        if let Some(mode) = mode {
                            let keep_open = self.agent_panel.borrow_mut().mode_menu_open;
                            self.apply_agent_action(
                                AgentAction::SetMode(mode),
                                &mut intents,
                                false,
                            );
                            if keep_open {
                                self.agent_panel.borrow_mut().mode_menu_open = true;
                            }
                        }
                    } else if let (Some(target), Some(command_prefix)) = (
                        selected.first().copied(),
                        inspector_select_command_prefix(key),
                    ) {
                        intents.push(NativeWorkbenchIntent::Command(format!(
                            "{command_prefix}:{}:{value}",
                            target.0
                        )));
                        self.inspector_view.dropdown = None;
                        self.inspector_view.color_picker = false;
                        self.host
                            .session_mut()
                            .interaction
                            .focus
                            .request_focus(format!("{key}.trigger"));
                    } else if let Some(action) = parse_viewport_toolbar_select_action(key, value) {
                        let settings_changed = self.apply_toolbar_action(action);
                        if settings_changed {
                            intents.push(NativeWorkbenchIntent::AgentSettingsChanged(
                                self.agent_settings.clone(),
                            ));
                        }
                        if let ViewportToolbarAction::SetBuildingStyle(style) = action {
                            intents.push(NativeWorkbenchIntent::ProjectSettingToggle {
                                key: format!("project-settings.building-style.{}", style.slug()),
                                value: true,
                            });
                        }
                    }
                    self.toolbar_revision = self.toolbar_revision.wrapping_add(1).max(1);
                }
                UiAction::SetSelectOpen { id, open } => {
                    let open = *open;
                    if let Some(dropdown) = text_fields::inspector_dropdown_from_trigger(id) {
                        self.inspector_view.dropdown = open.then_some(dropdown);
                        if open {
                            self.inspector_view.color_picker = false;
                        }
                    }
                    if !open {
                        let focus_target = if id == "viewport.toolbar.actions.more.menu" {
                            "viewport.toolbar.actions.more.trigger"
                        } else {
                            id.as_str()
                        };
                        self.host
                            .session_mut()
                            .interaction
                            .focus
                            .request_focus(focus_target.to_string());
                    }
                    match id.as_str() {
                        "agent.model.trigger" => {
                            self.agent_panel.borrow_mut().model_menu_open = open;
                            if open {
                                self.agent_panel.borrow_mut().mode_menu_open = false;
                                self.agent_panel.borrow_mut().add_model_open = false;
                            }
                        }
                        "agent.mode.trigger" => {
                            self.agent_panel.borrow_mut().mode_menu_open = open;
                            if open {
                                self.agent_panel.borrow_mut().model_menu_open = false;
                                self.agent_panel.borrow_mut().add_model_open = false;
                            }
                        }
                        "viewport.toolbar.view-mode.trigger" => {
                            self.toolbar_state.view_menu_open = open;
                            if open {
                                self.toolbar_state.shading_menu_open = false;
                                self.toolbar_state.building_menu_open = false;
                                self.toolbar_state.primitive_menu_open = false;
                                self.toolbar_state.actions_menu_open = false;
                            }
                        }
                        "viewport.toolbar.shading.trigger" => {
                            self.toolbar_state.shading_menu_open = open;
                            if open {
                                self.toolbar_state.view_menu_open = false;
                                self.toolbar_state.building_menu_open = false;
                                self.toolbar_state.primitive_menu_open = false;
                                self.toolbar_state.actions_menu_open = false;
                            }
                        }
                        "viewport.toolbar.building-style" => {
                            self.toolbar_state.building_menu_open = open;
                            if open {
                                self.toolbar_state.view_menu_open = false;
                                self.toolbar_state.shading_menu_open = false;
                                self.toolbar_state.primitive_menu_open = false;
                                self.toolbar_state.actions_menu_open = false;
                            }
                        }
                        "viewport.toolbar.actions.more.trigger"
                        | "viewport.toolbar.actions.more.menu" => {
                            self.toolbar_state.actions_menu_open = open;
                            if open {
                                self.toolbar_state.view_menu_open = false;
                                self.toolbar_state.shading_menu_open = false;
                                self.toolbar_state.building_menu_open = false;
                                self.toolbar_state.primitive_menu_open = false;
                            }
                        }
                        _ => {}
                    }
                    self.toolbar_revision = self.toolbar_revision.wrapping_add(1).max(1);
                }
                UiAction::SetRange { key, value } if key.starts_with("inspector.color.") => {
                    if let Some(target) = selected.first().copied() {
                        intents.push(NativeWorkbenchIntent::Command(format!(
                            "inspector.color.range:{key}:{value}:{}",
                            target.0
                        )));
                        self.toolbar_revision = self.toolbar_revision.wrapping_add(1).max(1);
                    }
                }
                UiAction::SetColorHsv {
                    key,
                    hue,
                    saturation,
                    value,
                } if key == "inspector.color" => {
                    if let Some(target) = selected.first().copied() {
                        intents.push(NativeWorkbenchIntent::Command(format!(
                            "inspector.color.hsv:{hue}:{saturation}:{value}:{}",
                            target.0
                        )));
                        self.toolbar_revision = self.toolbar_revision.wrapping_add(1).max(1);
                    }
                }
                UiAction::SetRange { key, value } if key == "inspector.appearance.opacity" => {
                    if let Some(target) = selected.first().copied() {
                        intents.push(NativeWorkbenchIntent::Command(format!(
                            "inspector.appearance.opacity:{value}:{}",
                            target.0
                        )));
                        self.toolbar_revision = self.toolbar_revision.wrapping_add(1).max(1);
                    }
                }
                UiAction::SetRange { key, value }
                    if key == "inspector.audio.volume" || key == "inspector.physics.damping" =>
                {
                    if let Some(target) = selected.first().copied() {
                        intents.push(NativeWorkbenchIntent::Command(format!(
                            "inspector.range:{key}:{value}:{}",
                            target.0
                        )));
                        self.toolbar_revision = self.toolbar_revision.wrapping_add(1).max(1);
                    }
                }
                UiAction::SetRange { key, value } if key.starts_with("project-settings.") => {
                    intents.push(NativeWorkbenchIntent::ProjectSettingRange {
                        key: key.clone(),
                        value: *value,
                    });
                    self.toolbar_revision = self.toolbar_revision.wrapping_add(1).max(1);
                }
                UiAction::ScrollTo { id, offset } if id == "hierarchy.tree" => {
                    let next = offset[1].max(0.0);
                    if (next - self.hierarchy_scroll_offset).abs() > f32::EPSILON {
                        self.note_hierarchy_scroll(next);
                    }
                }
                UiAction::ScrollTo { id, offset } if id == ASSETS_GRID_ID => {
                    let next = offset[1].max(0.0);
                    if (next - self.assets_scroll_offset).abs() > f32::EPSILON {
                        let projection_changed =
                            crate::panels::assets_surface::asset_scroll_projection_changed(
                                self.assets_scroll_offset,
                                next,
                                self.assets_surface.view,
                            );
                        self.assets_scroll_offset = next;
                        if projection_changed {
                            self.toolbar_revision = self.toolbar_revision.wrapping_add(1).max(1);
                        }
                    }
                }
                UiAction::ScrollTo { id, offset } if id == "agent.history" => {
                    if let Some(next) =
                        next_agent_scroll_projection(self.agent_scroll_projection_offset, offset[1])
                    {
                        self.agent_scroll_projection_offset = next;
                        self.toolbar_revision = self.toolbar_revision.wrapping_add(1).max(1);
                    }
                }
                UiAction::ScrollTo { id, offset } if id == "console.log-scroll" => {
                    let max = self
                        .host
                        .session()
                        .interaction
                        .controls
                        .scroll_max_offset("console.log-scroll")[1];
                    let at_bottom = offset[1] >= max - 4.0;
                    if self.console.auto_scroll && !at_bottom {
                        self.console.set_auto_scroll(false);
                        self.toolbar_revision = self.toolbar_revision.wrapping_add(1).max(1);
                    } else if !self.console.auto_scroll && at_bottom {
                        self.console.set_auto_scroll(true);
                        self.toolbar_revision = self.toolbar_revision.wrapping_add(1).max(1);
                    }
                }
                UiAction::Command { name } => {
                    if self.handle_electronics_commit_command(name, &mut intents) {
                        continue;
                    }
                    if self.handle_project_save_command(name) {
                        continue;
                    }
                    if self.handle_electronics_project_command(name, project, router, &mut intents)
                    {
                        continue;
                    }
                    if self.handle_ui_performance_command(name) {
                        continue;
                    }
                    if name == "authoring.notice.dismiss" {
                        self.authoring_notice = None;
                        self.toolbar_revision = self.toolbar_revision.wrapping_add(1).max(1);
                        continue;
                    }
                    if name == "runtime.notice.retry" || name == "runtime.notice.dismiss" {
                        self.runtime_notice = None;
                        self.toolbar_revision = self.toolbar_revision.wrapping_add(1).max(1);
                        intents.push(NativeWorkbenchIntent::Command(
                            if name == "runtime.notice.retry" {
                                "runtime.play".to_string()
                            } else {
                                name.clone()
                            },
                        ));
                        continue;
                    }
                    if self.handle_hierarchy_structure_command(name, scene, selected, &mut intents)
                    {
                        continue;
                    }
                    if self.handle_app_command(name, dispatched, &mut intents) {
                        continue;
                    }
                    if self.handle_electronics_navigator_command(name) {
                        continue;
                    }
                    if self.handle_settings_command(name, &mut intents) {
                        continue;
                    }
                    if self.handle_nodes_command(name, native_input, &mut intents) {
                        continue;
                    }
                    if self.handle_layout_command(name, native_input, &mut intents) {
                        continue;
                    }
                    if self.handle_hierarchy_tab_command(name, router) {
                        continue;
                    }
                    if self.handle_console_command(name, project, &mut intents) {
                        continue;
                    }
                    if self.handle_electronics_value_commit_command(name, &mut intents) {
                        continue;
                    }
                    if self.handle_window_command(name, &mut intents) {
                        continue;
                    }
                    if self.handle_assets_menu_command(name) {
                        continue;
                    }
                    if self.handle_nodes_zoom_command(name, &mut intents) {
                        continue;
                    }
                    if self.handle_assets_create_menu_command(name, &mut intents) {
                        continue;
                    }
                    if self.handle_project_command(name, router, &mut intents) {
                        continue;
                    }
                    if self.handle_hierarchy_menu_command(name, native_input) {
                        continue;
                    }
                    if self.handle_assets_create_primitive_command(name, scene, &mut intents) {
                        continue;
                    }
                    if self.handle_inspector_dropdown_trigger_command(name) {
                        continue;
                    }
                    if self.handle_inspector_tab_command(name) {
                        continue;
                    }
                    if self.handle_inspector_color_command(name, selected, scene, &mut intents) {
                        continue;
                    }
                    if self.handle_assets_command(name, scene, native_input, &mut intents) {
                        continue;
                    }
                    if self.handle_hierarchy_menu_pick_command(
                        name,
                        native_input,
                        selected,
                        &mut intents,
                    ) {
                        continue;
                    }
                    if self.handle_settings_section_command(name, &mut intents) {
                        continue;
                    }
                    if self.handle_hierarchy_script_link_command(name) {
                        continue;
                    }
                    if self.handle_hierarchy_row_command(
                        name,
                        native_input,
                        scene,
                        selected,
                        &mut intents,
                    ) {
                        continue;
                    }
                    if self.handle_dock_command(name, native_input, router) {
                        continue;
                    }
                    if self.handle_session_command(name, project, &mut intents) {
                        continue;
                    }
                    if self.handle_inspector_field_command(
                        name,
                        dispatched,
                        selected,
                        scene,
                        &mut intents,
                    ) {
                        continue;
                    }
                    if let Some(action) = parse_viewport_toolbar_action(&dispatched.action) {
                        let settings_changed = self.apply_toolbar_action(action);
                        if settings_changed {
                            intents.push(NativeWorkbenchIntent::AgentSettingsChanged(
                                self.agent_settings.clone(),
                            ));
                        }
                        if let ViewportToolbarAction::CreatePrimitive(primitive) = action {
                            if let Some(parent) = self.insertion_parent(scene) {
                                intents.push(NativeWorkbenchIntent::Command(format!(
                                    "hierarchy.create-primitive:{}:{}",
                                    parent.0,
                                    primitive.descriptor().slug
                                )));
                            } else {
                                intents.push(NativeWorkbenchIntent::Viewport(action));
                            }
                        } else {
                            intents.push(NativeWorkbenchIntent::Viewport(action));
                        }
                    } else {
                        self.hierarchy_empty_menu_open = false;
                        self.hierarchy_primitive_menu_open = false;
                        self.hierarchy_menu_target = None;
                        self.hierarchy_menu_position = None;
                        // Any other dispatched command counts as an outside
                        // interaction, so the node menu closes with it.
                        if self.nodes_host.close_context_menu() {
                            self.toolbar_revision = self.toolbar_revision.wrapping_add(1).max(1);
                        }
                        intents.push(NativeWorkbenchIntent::Command(name.clone()));
                    }
                }
                UiAction::OpenMenu { id } => {
                    if self.handle_nodes_open_menu(id, native_input, &mut intents) {
                        continue;
                    }
                    if self.handle_hierarchy_open_menu(id, native_input, selected, &mut intents) {
                        continue;
                    }
                    self.open_menu = (self.open_menu.as_deref() != Some(id)).then(|| id.clone());
                    self.toolbar_revision = self.toolbar_revision.wrapping_add(1).max(1);
                }
                _ => {}
            }
        }
        self.handle_nodes_pointer_and_keyboard(native_input, &mut intents);
        self.handle_pointer_cancel(native_input, router);
        self.handle_escape_dismissals(native_input);
        self.handle_primary_press_dismissals(native_input, &actions, scene, selected, &mut intents);
        self.handle_toolbar_popup_dismissals(native_input);
        (intents, actions)
    }
}

impl NativeGameWorkbench {
    pub(super) fn handle_project_save_command(&mut self, name: &String) -> bool {
        if name == crate::application_menu::command::PROJECT_SAVE {
            if let Err(error) = self.flush_interface_document() {
                self.interface.diagnostic = Some(error);
                self.toolbar_revision = self.toolbar_revision.wrapping_add(1).max(1);
                return true;
            }
        }
        if name == "assets.create-interface" {
            self.create_interface_asset();
            return true;
        }
        false
    }
}

impl NativeGameWorkbench {
    pub(super) fn apply_agent_action(
        &mut self,
        action: AgentAction,
        intents: &mut Vec<NativeWorkbenchIntent>,
        reset_history: bool,
    ) {
        self.agent_panel
            .borrow_mut()
            .apply_action(action, &mut self.agent_settings);
        if reset_history {
            self.agent_scroll_projection_offset = 0.0;
            self.host
                .session_mut()
                .reset_interaction_for_surface_change(Some("agent.history"));
        }
        if self.agent_panel.borrow_mut().take_open_settings_request() {
            self.settings_section = SettingsSection::Ai;
            intents.push(NativeWorkbenchIntent::OpenSettings {
                section: SettingsSection::Ai,
            });
        }
        if self.agent_panel.borrow_mut().take_settings_changed() {
            intents.push(NativeWorkbenchIntent::AgentSettingsChanged(
                self.agent_settings.clone(),
            ));
        }
        self.toolbar_revision = self.toolbar_revision.wrapping_add(1).max(1);
    }
}

pub(crate) fn parse_agent_action(name: &str, language: raf_core::Language) -> Option<AgentAction> {
    let action = match name {
        "agent.sidebar.toggle" => AgentAction::ToggleSidebar,
        "agent.sidebar.close" => AgentAction::CloseSidebar,
        "agent.new-chat" => AgentAction::NewChat,
        "agent.model.toggle" => AgentAction::ToggleModelMenu,
        "agent.mode.toggle" => AgentAction::ToggleModeMenu,
        "agent.model.confirm" => AgentAction::AddModel,
        "agent.open-settings" => AgentAction::OpenSettings,
        "agent.catalog.refresh" => AgentAction::RefreshModelCatalog,
        "agent.account.usage" => AgentAction::OpenPlanUsage,
        "agent.tool-warning.dismiss" => AgentAction::DismissToolCallWarning,
        "agent.submit" => AgentAction::Submit,
        "agent.image.paste" => AgentAction::PasteImage,
        "agent.reasoning.cycle" => AgentAction::CycleReasoning,
        "agent.resume" => AgentAction::Resume,
        "agent.stop" => AgentAction::Stop,
        "agent.approve" => AgentAction::Approve,
        "agent.deny" => AgentAction::Deny,
        "agent.review.toggle" => AgentAction::ToggleReview,
        "agent.mode.inspect" => AgentAction::SetMode(AgentMode::Inspect),
        "agent.mode.passive" | "agent.mode.plan" => AgentAction::SetMode(AgentMode::Plan),
        "agent.mode.active" => AgentAction::SetMode(AgentMode::Active),
        _ => {
            if let Some(file) = name.strip_prefix("agent.image.remove:") {
                return Some(AgentAction::RemoveImage(file.into()));
            }
            if let Some(id) = name.strip_prefix("agent.call.approve:") {
                AgentAction::ApproveCall(id.to_string())
            } else if let Some(id) = name.strip_prefix("agent.call.deny:") {
                AgentAction::DenyCall(id.to_string())
            } else if let Some(id) = name.strip_prefix("agent.plan.apply:") {
                AgentAction::ApplyPlan(id.to_string())
            } else if let Some(id) = name.strip_prefix("agent.plan.cancel:") {
                AgentAction::CancelPlan(id.to_string())
            } else if let Some(uri) = name.strip_prefix("agent.evidence.select:") {
                AgentAction::SelectEvidence(uri.to_string())
            } else if let Some(uri) = name.strip_prefix("agent.evidence.pin:") {
                AgentAction::PinEvidence(uri.to_string())
            } else if name == "agent.evidence.clear" {
                AgentAction::ClearEvidence
            } else if name == "agent.evidence.compare" {
                AgentAction::ToggleEvidenceCompare
            } else if let Some(id) = name.strip_prefix("agent.tool.toggle:") {
                AgentAction::ToggleTool(id.to_string())
            } else if let Some(raw) = name.strip_prefix("agent.session.select:") {
                AgentAction::SelectSession(raw.parse().ok()?)
            } else if let Some(raw) = name.strip_prefix("agent.session.delete:") {
                AgentAction::DeleteSession(raw.parse().ok()?)
            } else if let Some(raw) = name.strip_prefix("agent.model.select:") {
                AgentAction::SelectModel(raw.to_string())
            } else if let Some(raw) = name.strip_prefix("agent.model.pin:") {
                AgentAction::PinModel(raw.to_string())
            } else if let Some(key) = name.strip_prefix("agent.suggestion:") {
                AgentAction::UseSuggestion(raf_core::i18n::t(key, language))
            } else {
                return None;
            }
        }
    };
    Some(action)
}

fn inspector_select_command_prefix(key: &str) -> Option<&'static str> {
    match key {
        "inspector.primitive" => Some("inspector.primitive"),
        "inspector.collider" => Some("inspector.collider"),
        "inspector.body-type" => Some("inspector.body-type"),
        "inspector.base-color-texture" => Some("inspector.base-color-texture"),
        _ => None,
    }
}
