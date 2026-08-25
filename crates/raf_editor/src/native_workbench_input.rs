//! Native workbench input routing.
//!
//! Input dispatch is kept separate from workbench state and retained-surface
//! composition so the main workbench remains a coordinator rather than a
//! monolithic event handler.

use super::*;

impl NativeGameWorkbench {
    pub fn process_input<F>(
        &mut self,
        native_input: &NativeUiInputBridge,
        router: &mut InputRouter,
        scene: &SceneGraph,
        selected: &[SceneNodeId],
        project: Option<&Project>,
        resolve: F,
    ) -> (Vec<NativeWorkbenchIntent>, Vec<UiDispatchedAction>)
    where
        F: FnMut(&str) -> String,
    {
        let agent_actions = if self.bottom_dock.has_active_tab("agent") {
            self.agent_surface.process_input(
                native_input,
                router,
                &self.agent_panel,
                &self.agent_settings,
                project,
            )
        } else {
            Vec::new()
        };
        if !agent_actions.is_empty() {
            self.toolbar_revision = self.toolbar_revision.wrapping_add(1).max(1);
        }
        for action in agent_actions {
            match action {
                AgentAction::Approve => self.pending_agent_decision = Some(true),
                AgentAction::Deny => self.pending_agent_decision = Some(false),
                action => self
                    .agent_panel
                    .apply_action(action, &mut self.agent_settings),
            }
        }
        let pointer_over_agent = self.bottom_dock.has_active_tab("agent")
            && native_input
                .snapshot()
                .pointer_position
                .is_some_and(|pointer| self.agent_surface.rect().contains(pointer));
        let main_host_has_capture = self.host.has_pointer_capture();
        let actions = if pointer_over_agent && !main_host_has_capture {
            // Agent is a topmost compositor layer. Do not let the full-window
            // workbench underneath it consume passive hover, scroll, or a
            // click-away while the pointer is inside the Agent rectangle.
            router.release_keyboard(self.owner());
            Vec::new()
        } else {
            self.host.process_routed_input(
                self.rect.logical_size(),
                native_input.scale_factor() as f32,
                resolve,
                native_input,
                router,
                self.owner(),
                raf_ui::UiRect::new(self.rect.x, self.rect.y, self.rect.width, self.rect.height),
            )
        };
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
        let mut intents = Vec::new();
        for dispatched in &actions {
            match &dispatched.action {
                UiAction::SetText { key, value } => match key.as_str() {
                    "assets.search" => {
                        self.assets_query = value.clone();
                        self.toolbar_revision = self.toolbar_revision.wrapping_add(1).max(1);
                    }
                    "hierarchy.search" | "hierarchy.search.global" => {
                        self.hierarchy_query = value.clone();
                        self.toolbar_revision = self.toolbar_revision.wrapping_add(1).max(1);
                    }
                    "electronics.library.search" => {
                        self.electronics_library_query = value.clone();
                        self.toolbar_revision = self.toolbar_revision.wrapping_add(1).max(1);
                    }
                    "electronics.inspector.value" => {
                        if let Some((_id, draft)) = self.electronics_value_draft.as_mut() {
                            *draft = value.clone();
                            self.toolbar_revision = self.toolbar_revision.wrapping_add(1).max(1);
                        }
                    }
                    "inspector.session.new_name" => {
                        self.inspector_session_name = value.clone();
                        self.toolbar_revision = self.toolbar_revision.wrapping_add(1).max(1);
                    }
                    "assets.file-name" => {
                        self.assets_file_name = value.clone();
                        self.toolbar_revision = self.toolbar_revision.wrapping_add(1).max(1);
                    }
                    "assets.script-name" => {
                        self.assets_script_name = value.clone();
                        self.toolbar_revision = self.toolbar_revision.wrapping_add(1).max(1);
                    }
                    "console.input" => {
                        self.console.set_input(value.clone());
                        self.toolbar_revision = self.toolbar_revision.wrapping_add(1).max(1);
                    }
                    "inspector.name" => {
                        if let Some(id) = selected.first().copied() {
                            self.inspector_name_editing = Some((id, value.clone()));
                            self.toolbar_revision = self.toolbar_revision.wrapping_add(1).max(1);
                        }
                    }
                    key if key.starts_with("hierarchy.rename.") => {
                        if let Some((id, current)) = self.hierarchy_renaming.as_mut() {
                            let expected = format!("hierarchy.rename.{}", id.0);
                            if key == &expected {
                                *current = value.clone();
                                self.toolbar_revision =
                                    self.toolbar_revision.wrapping_add(1).max(1);
                            }
                        }
                    }
                    _ => {}
                },
                UiAction::SetToggle { key, value } => {
                    if key.starts_with("inspector.") {
                        if let Some(target) = selected.first().copied() {
                            intents.push(NativeWorkbenchIntent::Command(format!(
                                "inspector.toggle:{key}:{value}:{}",
                                target.0
                            )));
                            self.toolbar_revision = self.toolbar_revision.wrapping_add(1).max(1);
                        }
                    } else if key.starts_with("project-settings.") {
                        self.toolbar_revision = self.toolbar_revision.wrapping_add(1).max(1);
                        intents.push(NativeWorkbenchIntent::ProjectSettingToggle {
                            key: key.clone(),
                            value: *value,
                        });
                    } else if apply_settings_toggle(&mut self.agent_settings, key, *value) {
                        self.toolbar_revision = self.toolbar_revision.wrapping_add(1).max(1);
                    }
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
                UiAction::SetRange { key, value } if key == "inspector.audio.volume" => {
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
                UiAction::Command { name } => {
                    if let Some(menu_id) = name.strip_prefix("application.menu.") {
                        self.open_menu = (self.open_menu.as_deref() != Some(menu_id))
                            .then(|| menu_id.to_string());
                        self.toolbar_revision = self.toolbar_revision.wrapping_add(1).max(1);
                        continue;
                    }
                    self.open_menu = None;
                    match name.as_str() {
                        "viewport.dropdown.view.toggle" => {
                            self.toolbar_state.view_menu_open = !self.toolbar_state.view_menu_open;
                            self.toolbar_state.shading_menu_open = false;
                            self.toolbar_state.primitive_menu_open = false;
                            self.toolbar_revision = self.toolbar_revision.wrapping_add(1).max(1);
                            continue;
                        }
                        "viewport.dropdown.view.cancel" => {
                            self.toolbar_state.view_menu_open = false;
                            self.toolbar_revision = self.toolbar_revision.wrapping_add(1).max(1);
                            continue;
                        }
                        "viewport.dropdown.shading.toggle" => {
                            self.toolbar_state.shading_menu_open =
                                !self.toolbar_state.shading_menu_open;
                            self.toolbar_state.view_menu_open = false;
                            self.toolbar_state.primitive_menu_open = false;
                            self.toolbar_revision = self.toolbar_revision.wrapping_add(1).max(1);
                            continue;
                        }
                        "viewport.dropdown.shading.cancel" => {
                            self.toolbar_state.shading_menu_open = false;
                            self.toolbar_revision = self.toolbar_revision.wrapping_add(1).max(1);
                            continue;
                        }
                        "viewport.create-primitive.toggle" => {
                            self.toolbar_state.primitive_menu_open =
                                !self.toolbar_state.primitive_menu_open;
                            self.toolbar_state.view_menu_open = false;
                            self.toolbar_state.shading_menu_open = false;
                            self.toolbar_revision = self.toolbar_revision.wrapping_add(1).max(1);
                            continue;
                        }
                        "viewport.create-primitive.cancel" => {
                            self.toolbar_state.primitive_menu_open = false;
                            self.toolbar_revision = self.toolbar_revision.wrapping_add(1).max(1);
                            continue;
                        }
                        "viewport.dropdown.building.toggle" => {
                            self.toolbar_state.building_menu_open =
                                !self.toolbar_state.building_menu_open;
                            self.toolbar_state.view_menu_open = false;
                            self.toolbar_state.shading_menu_open = false;
                            self.toolbar_state.primitive_menu_open = false;
                            self.toolbar_revision = self.toolbar_revision.wrapping_add(1).max(1);
                            continue;
                        }
                        "viewport.dropdown.building.cancel" => {
                            self.toolbar_state.building_menu_open = false;
                            self.toolbar_revision = self.toolbar_revision.wrapping_add(1).max(1);
                            continue;
                        }
                        "viewport.building.free" | "viewport.building.professional" => {
                            let style = if name == "viewport.building.free" {
                                BuildingStyle::Free
                            } else {
                                BuildingStyle::Organized
                            };
                            self.apply_toolbar_action(ViewportToolbarAction::SetBuildingStyle(
                                style,
                            ));
                            intents.push(NativeWorkbenchIntent::ProjectSettingToggle {
                                key: format!("project-settings.building-style.{}", style.slug()),
                                value: true,
                            });
                            continue;
                        }
                        _ => {}
                    }
                    if let Some(tab) = name.strip_prefix("electronics.navigator.tab:") {
                        if matches!(tab, "project" | "library" | "components" | "wires") {
                            self.electronics_navigator_tab = tab.to_string();
                            self.toolbar_revision = self.toolbar_revision.wrapping_add(1).max(1);
                        }
                        continue;
                    }
                    if let Some(key) = name.strip_prefix("project-settings.commit_numeric:") {
                        let text = self
                            .host
                            .session()
                            .interaction
                            .controls
                            .text(&format!("{key}.text"))
                            .to_string();
                        if let Ok(value) = text.trim().parse::<f32>() {
                            intents.push(NativeWorkbenchIntent::ProjectSettingRange {
                                key: key.to_string(),
                                value,
                            });
                            self.toolbar_revision = self.toolbar_revision.wrapping_add(1).max(1);
                        }
                        continue;
                    }
                    if let Some(raw_id) = name.strip_prefix("nodes.drag.start.") {
                        if let Some([x, y]) = native_input.snapshot().pointer_position {
                            intents.push(NativeWorkbenchIntent::Command(format!(
                                "nodes.drag.start:{raw_id}:{x:.3}:{y:.3}"
                            )));
                        }
                        continue;
                    }
                    if let Some(raw_id) = name.strip_prefix("nodes.drag.move.") {
                        if let Some([x, y]) = native_input.snapshot().pointer_position {
                            intents.push(NativeWorkbenchIntent::Command(format!(
                                "nodes.drag.move:{raw_id}:{x:.3}:{y:.3}"
                            )));
                        }
                        continue;
                    }
                    if name.starts_with("nodes.drag.end.") {
                        intents.push(NativeWorkbenchIntent::Command("nodes.drag.end".to_string()));
                        continue;
                    }
                    if let Some(raw_target) = name.strip_prefix("layout.resize.start.") {
                        let kind = match raw_target {
                            "layout.left" => WorkbenchResizeKind::LeftPanel,
                            "layout.right" => WorkbenchResizeKind::RightPanel,
                            "layout.bottom" => WorkbenchResizeKind::BottomDock,
                            _ => continue,
                        };
                        let Some(pointer) = native_input.snapshot().pointer_position else {
                            continue;
                        };
                        let Some(layout) = self.last_layout else {
                            continue;
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
                        continue;
                    }
                    if let Some(raw_target) = name.strip_prefix("layout.resize.move.") {
                        let Some(resize) = self.panel_resize else {
                            continue;
                        };
                        let target_kind = match raw_target {
                            "layout.left" => WorkbenchResizeKind::LeftPanel,
                            "layout.right" => WorkbenchResizeKind::RightPanel,
                            "layout.bottom" => WorkbenchResizeKind::BottomDock,
                            _ => continue,
                        };
                        if resize.kind != target_kind {
                            continue;
                        }
                        let Some(pointer) = native_input.snapshot().pointer_position else {
                            continue;
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
                        continue;
                    }
                    if name.starts_with("layout.resize.end.") {
                        self.panel_resize = None;
                        if name.ends_with("layout.bottom") {
                            self.bottom_dock.persist_project_layout();
                        }
                        self.toolbar_revision = self.toolbar_revision.wrapping_add(1).max(1);
                        continue;
                    }
                    if let Some(raw_target) = name.strip_prefix("bottom.resize.start.") {
                        if let Some((left_group_id, right_group_id)) = raw_target.split_once('|') {
                            if let Some([pointer_x, _]) = native_input.snapshot().pointer_position {
                                if self.bottom_dock.begin_resize(
                                    left_group_id,
                                    right_group_id,
                                    pointer_x,
                                ) {
                                    self.toolbar_revision =
                                        self.toolbar_revision.wrapping_add(1).max(1);
                                }
                            }
                        }
                        continue;
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
                        continue;
                    }
                    if name.starts_with("bottom.resize.end.") {
                        if self.bottom_dock.end_resize() {
                            self.toolbar_revision = self.toolbar_revision.wrapping_add(1).max(1);
                            self.bottom_dock.persist_project_layout();
                        }
                        continue;
                    }
                    if let Some(raw_target) = name.strip_prefix("bottom.drag.start.") {
                        if let Some((group_id, tab_id)) = raw_target.split_once('|') {
                            if let Some(pointer) = native_input.snapshot().pointer_position {
                                if self.bottom_dock.begin_drag(group_id, tab_id, pointer) {
                                    self.drag_motion.set_immediate(0.0);
                                    self.drag_motion.set_target(1.0);
                                    self.toolbar_revision =
                                        self.toolbar_revision.wrapping_add(1).max(1);
                                }
                            }
                        }
                        continue;
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
                        continue;
                    }
                    if let Some(raw_target) = name.strip_prefix("bottom.drag.end.") {
                        let _ = raw_target;
                        self.finish_bottom_drag();
                        continue;
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
                        continue;
                    }
                    if let Some(tab) = name.strip_prefix("hierarchy.tab:") {
                        if matches!(
                            tab,
                            "hierarchy" | "assets" | "world" | "bookmarks" | "search"
                        ) {
                            self.hierarchy_active_tab = tab.to_string();
                            self.toolbar_revision = self.toolbar_revision.wrapping_add(1).max(1);
                        }
                        continue;
                    }
                    if name == "hierarchy.toggle-hidden" {
                        self.agent_settings.hierarchy_show_hidden =
                            !self.agent_settings.hierarchy_show_hidden;
                        self.toolbar_revision = self.toolbar_revision.wrapping_add(1).max(1);
                        continue;
                    }
                    if name == "hierarchy.search.open" {
                        self.hierarchy_active_tab = "search".to_string();
                        self.toolbar_revision = self.toolbar_revision.wrapping_add(1).max(1);
                        continue;
                    }
                    if let Some(tab) = name.strip_prefix("hierarchy.open-bottom:") {
                        if self.bottom_dock.select(tab) {
                            self.toolbar_revision = self.toolbar_revision.wrapping_add(1).max(1);
                        }
                        continue;
                    }
                    if let Some(slot) = name
                        .strip_prefix("hierarchy.bookmark.save:")
                        .and_then(|value| value.parse::<usize>().ok())
                    {
                        if let Some(bookmark) = self.hierarchy_bookmarks.get_mut(slot) {
                            *bookmark = true;
                            self.toolbar_revision = self.toolbar_revision.wrapping_add(1).max(1);
                        }
                        continue;
                    }
                    match name.as_str() {
                        "console.submit" => {
                            if let Some(submission) = self.console.submit_input() {
                                let text = submission.text;
                                self.console.log_user("User", &text);
                                self.host.session_mut().interaction.controls.set_text(
                                    "console.input",
                                    "",
                                    4096,
                                );
                                intents.push(NativeWorkbenchIntent::Command(format!(
                                    "console.submit:{text}"
                                )));
                            }
                            continue;
                        }
                        "console.clear" => {
                            self.console.clear_entries();
                            self.toolbar_revision = self.toolbar_revision.wrapping_add(1).max(1);
                            continue;
                        }
                        "console.history.previous" => {
                            self.console.select_previous_history();
                            let input = self.console.input().to_string();
                            self.host.session_mut().interaction.controls.set_text(
                                "console.input",
                                input,
                                4096,
                            );
                            continue;
                        }
                        "console.history.next" => {
                            self.console.select_next_history();
                            let input = self.console.input().to_string();
                            self.host.session_mut().interaction.controls.set_text(
                                "console.input",
                                input,
                                4096,
                            );
                            continue;
                        }
                        "console.autocomplete" => {
                            let names = self
                                .agent_catalog
                                .commands
                                .iter()
                                .map(|command| format!("/{}", command.name))
                                .collect::<Vec<_>>();
                            self.console.autocomplete_command(&names);
                            let input = self.console.input().to_string();
                            self.host.session_mut().interaction.controls.set_text(
                                "console.input",
                                input,
                                4096,
                            );
                            continue;
                        }
                        "electronics.inspector.value.commit" => {
                            let value = self
                                .host
                                .session()
                                .interaction
                                .controls
                                .text("electronics.inspector.value")
                                .to_string();
                            self.electronics_value_draft = None;
                            intents.push(NativeWorkbenchIntent::Command(format!(
                                "electronics.inspector.value.commit:{value}"
                            )));
                            self.toolbar_revision = self.toolbar_revision.wrapping_add(1).max(1);
                            continue;
                        }
                        "window.drag" => {
                            intents.push(NativeWorkbenchIntent::Window(UiWindowCommand::BeginDrag));
                            continue;
                        }
                        "window.minimize" => {
                            intents.push(NativeWorkbenchIntent::Window(UiWindowCommand::Minimize));
                            continue;
                        }
                        "window.maximize" => {
                            intents.push(NativeWorkbenchIntent::Window(
                                UiWindowCommand::ToggleMaximize,
                            ));
                            continue;
                        }
                        "window.close" => {
                            intents.push(NativeWorkbenchIntent::Window(UiWindowCommand::Close));
                            continue;
                        }
                        "assets.create-primitive.toggle" => {
                            self.assets_primitive_menu_open = !self.assets_primitive_menu_open;
                            self.toolbar_revision = self.toolbar_revision.wrapping_add(1).max(1);
                            continue;
                        }
                        "assets.create-primitive.cancel" => {
                            self.assets_primitive_menu_open = false;
                            self.toolbar_revision = self.toolbar_revision.wrapping_add(1).max(1);
                            continue;
                        }
                        "nodes.zoom.in" => {
                            self.nodes_zoom = (self.nodes_zoom * 1.12).clamp(0.55, 1.8);
                            self.toolbar_revision = self.toolbar_revision.wrapping_add(1).max(1);
                            continue;
                        }
                        "nodes.zoom.out" => {
                            self.nodes_zoom = (self.nodes_zoom / 1.12).clamp(0.55, 1.8);
                            self.toolbar_revision = self.toolbar_revision.wrapping_add(1).max(1);
                            continue;
                        }
                        "nodes.zoom.reset" => {
                            self.nodes_zoom = 1.0;
                            self.toolbar_revision = self.toolbar_revision.wrapping_add(1).max(1);
                            continue;
                        }
                        "assets.create-script" => {
                            self.assets_script_menu_open = true;
                            self.assets_file_menu_open = false;
                            self.toolbar_revision = self.toolbar_revision.wrapping_add(1).max(1);
                            continue;
                        }
                        "assets.create-file" => {
                            self.assets_file_menu_open = true;
                            self.assets_script_menu_open = false;
                            self.toolbar_revision = self.toolbar_revision.wrapping_add(1).max(1);
                            continue;
                        }
                        "assets.script.cancel" => {
                            self.assets_script_menu_open = false;
                            self.toolbar_revision = self.toolbar_revision.wrapping_add(1).max(1);
                            continue;
                        }
                        "assets.file.cancel" => {
                            self.assets_file_menu_open = false;
                            self.toolbar_revision = self.toolbar_revision.wrapping_add(1).max(1);
                            continue;
                        }
                        "assets.refresh" | "assets.open-folder" => {
                            self.project_catalog.refresh();
                            continue;
                        }
                        "agent.open" => {
                            if self.bottom_dock.select("agent") {
                                self.toolbar_revision =
                                    self.toolbar_revision.wrapping_add(1).max(1);
                            }
                            continue;
                        }
                        "editor.settings" => {
                            intents.push(NativeWorkbenchIntent::OpenSettings {
                                section: self.settings_section,
                            });
                            continue;
                        }
                        "project.settings" => {
                            if self.bottom_dock.select("project-settings") {
                                self.toolbar_revision =
                                    self.toolbar_revision.wrapping_add(1).max(1);
                            }
                            continue;
                        }
                        "application.exit_to_hub" | "project.close" => {
                            intents.push(NativeWorkbenchIntent::ReturnToHub { open_create: false });
                            continue;
                        }
                        "project.new" => {
                            intents.push(NativeWorkbenchIntent::ReturnToHub { open_create: true });
                            continue;
                        }
                        "project.open" => {
                            intents.push(NativeWorkbenchIntent::ReturnToHub { open_create: false });
                            continue;
                        }
                        "hierarchy.empty-menu" => {
                            self.hierarchy_empty_menu_open = true;
                            self.hierarchy_primitive_menu_open = false;
                            self.hierarchy_menu_target = None;
                            self.hierarchy_menu_position = native_input.snapshot().pointer_position;
                            self.toolbar_revision = self.toolbar_revision.wrapping_add(1).max(1);
                            continue;
                        }
                        "hierarchy.empty.create-primitive-menu" => {
                            self.hierarchy_empty_menu_open = true;
                            self.hierarchy_primitive_menu_open = true;
                            self.hierarchy_menu_target = None;
                            self.toolbar_revision = self.toolbar_revision.wrapping_add(1).max(1);
                            continue;
                        }
                        "hierarchy.menu.close" => {
                            self.hierarchy_empty_menu_open = false;
                            self.hierarchy_primitive_menu_open = false;
                            self.hierarchy_menu_target = None;
                            self.hierarchy_menu_position = None;
                            self.toolbar_revision = self.toolbar_revision.wrapping_add(1).max(1);
                            continue;
                        }
                        "hierarchy.rename.cancel" => {
                            self.hierarchy_renaming = None;
                            self.toolbar_revision = self.toolbar_revision.wrapping_add(1).max(1);
                            continue;
                        }
                        _ => {}
                    }
                    if name.starts_with("assets.create-primitive:") {
                        self.assets_primitive_menu_open = false;
                        self.toolbar_revision = self.toolbar_revision.wrapping_add(1).max(1);
                    }
                    if let Some(slug) = name.strip_prefix("inspector.section.toggle:") {
                        if let Some(section) = inspector_section_from_slug(slug) {
                            let expanded = !self.inspector_view.section(section);
                            set_inspector_section(&mut self.inspector_view, section, expanded);
                            self.toolbar_revision = self.toolbar_revision.wrapping_add(1).max(1);
                        }
                        continue;
                    }
                    if let Some(tab) = name.strip_prefix("inspector.tab:") {
                        self.inspector_view.tab = match tab {
                            "sessions" => InspectorTab::Sessions,
                            _ => InspectorTab::Properties,
                        };
                        self.toolbar_revision = self.toolbar_revision.wrapping_add(1).max(1);
                        continue;
                    }
                    if name == "inspector.color.toggle" {
                        self.inspector_view.color_picker = !self.inspector_view.color_picker;
                        self.toolbar_revision = self.toolbar_revision.wrapping_add(1).max(1);
                        continue;
                    }
                    if let Some(slug) = name.strip_prefix("inspector.dropdown.toggle:") {
                        let next = match slug {
                            "primitive" => Some(InspectorDropdown::Primitive),
                            "collider" => Some(InspectorDropdown::Collider),
                            "body-type" => Some(InspectorDropdown::BodyType),
                            _ => None,
                        };
                        self.inspector_view.dropdown = (self.inspector_view.dropdown != next)
                            .then_some(next)
                            .flatten();
                        self.toolbar_revision = self.toolbar_revision.wrapping_add(1).max(1);
                        continue;
                    }
                    if let Some(filter) = name.strip_prefix("assets.filter.") {
                        self.assets_filter = match filter {
                            "images" => AssetFilter::Images,
                            "models" => AssetFilter::Models,
                            "audio" => AssetFilter::Audio,
                            "scripts" => AssetFilter::Scripts,
                            _ => AssetFilter::All,
                        };
                        self.toolbar_revision = self.toolbar_revision.wrapping_add(1).max(1);
                        continue;
                    }
                    if let Some(asset) = name.strip_prefix("assets.open:") {
                        if let Some(slug) = asset.strip_prefix("builtin://primitive/") {
                            intents.push(NativeWorkbenchIntent::Command(format!(
                                "assets.create-primitive:{slug}"
                            )));
                        }
                        continue;
                    }
                    if let Some(language) = name.strip_prefix("assets.script.create:") {
                        self.assets_script_menu_open = false;
                        self.assets_refresh_requested = true;
                        intents.push(NativeWorkbenchIntent::Command(format!(
                            "assets.create-script:{language}:{}",
                            self.assets_script_name.trim()
                        )));
                        self.toolbar_revision = self.toolbar_revision.wrapping_add(1).max(1);
                        continue;
                    }
                    if name == "assets.file.create" {
                        self.assets_file_menu_open = false;
                        self.assets_refresh_requested = true;
                        intents.push(NativeWorkbenchIntent::Command(format!(
                            "assets.create-file:{}",
                            self.assets_file_name.trim()
                        )));
                        self.toolbar_revision = self.toolbar_revision.wrapping_add(1).max(1);
                        continue;
                    }
                    if let Some(raw_id) = name.strip_prefix("hierarchy.menu:") {
                        if let Ok(id) = raw_id.parse::<usize>() {
                            let target = SceneNodeId(id);
                            self.hierarchy_menu_target =
                                Some((target, self.hierarchy_target_is_folder(target)));
                            self.hierarchy_empty_menu_open = false;
                            self.hierarchy_primitive_menu_open = false;
                            self.hierarchy_menu_position = native_input.snapshot().pointer_position;
                            self.toolbar_revision = self.toolbar_revision.wrapping_add(1).max(1);
                            continue;
                        }
                    }
                    if let Some(section) = SettingsSection::from_command(name) {
                        let section_changed = self.settings_section != section;
                        self.settings_section = section;
                        if section_changed {
                            self.toolbar_revision = self.toolbar_revision.wrapping_add(1).max(1);
                        }
                        intents.push(NativeWorkbenchIntent::OpenSettings { section });
                        continue;
                    }
                    if let Some(raw_id) = name.strip_prefix("hierarchy.expand:") {
                        if let Ok(id) = raw_id.parse::<usize>() {
                            self.hierarchy_model.toggle_expanded(SceneNodeId(id));
                            self.toolbar_revision = self.toolbar_revision.wrapping_add(1).max(1);
                            continue;
                        }
                    }
                    if let Some(raw_id) = name.strip_prefix("hierarchy.expand-recursive:") {
                        if let Ok(id) = raw_id.parse::<usize>() {
                            self.hierarchy_model
                                .expand_recursive(scene, SceneNodeId(id), true);
                            self.toolbar_revision = self.toolbar_revision.wrapping_add(1).max(1);
                            continue;
                        }
                    }
                    if let Some(raw_id) = name.strip_prefix("hierarchy.collapse-recursive:") {
                        if let Ok(id) = raw_id.parse::<usize>() {
                            self.hierarchy_model
                                .expand_recursive(scene, SceneNodeId(id), false);
                            self.toolbar_revision = self.toolbar_revision.wrapping_add(1).max(1);
                            continue;
                        }
                    }
                    if let Some(raw_id) = name.strip_prefix("hierarchy.rename.begin:") {
                        if let Ok(id) = raw_id.parse::<usize>() {
                            if let Some(node) = scene.get(SceneNodeId(id)) {
                                self.hierarchy_renaming =
                                    Some((SceneNodeId(id), node.name.clone()));
                                self.toolbar_revision =
                                    self.toolbar_revision.wrapping_add(1).max(1);
                            }
                            continue;
                        }
                    }
                    if let Some(raw_id) = name.strip_prefix("hierarchy.select:") {
                        if let Ok(id) = raw_id.parse::<usize>() {
                            let additive = native_input.snapshot().modifiers.shift;
                            if additive {
                                if let Some(anchor) = selected.last().copied() {
                                    let ordered =
                                        self.hierarchy_model.row_ids().collect::<Vec<_>>();
                                    if ordered.contains(&anchor)
                                        && ordered.contains(&SceneNodeId(id))
                                    {
                                        let ids = ordered
                                            .iter()
                                            .map(|id| id.0.to_string())
                                            .collect::<Vec<_>>()
                                            .join(",");
                                        intents.push(NativeWorkbenchIntent::Command(format!(
                                            "hierarchy.select.range:{}:{}:{ids}",
                                            anchor.0, id
                                        )));
                                        continue;
                                    }
                                }
                            }
                            intents.push(NativeWorkbenchIntent::Command(format!(
                                "hierarchy.select:{id}:{}",
                                if additive { "shift" } else { "replace" }
                            )));
                            continue;
                        }
                    }
                    if let Some(raw_id) = name.strip_prefix("hierarchy.drag.start:") {
                        if let Ok(id) = raw_id.parse::<usize>() {
                            let id = SceneNodeId(id);
                            let Some(node) = scene.get(id) else {
                                continue;
                            };
                            let pointer = native_input
                                .snapshot()
                                .pointer_position
                                .unwrap_or([0.0, 0.0]);
                            let source = if selected.contains(&id) {
                                selected.to_vec()
                            } else {
                                vec![id]
                            };
                            self.hierarchy_drag = Some(HierarchyDragState {
                                source,
                                target: None,
                                pointer,
                                label: node.name.clone(),
                            });
                            if !selected.contains(&id) {
                                intents.push(NativeWorkbenchIntent::Command(format!(
                                    "hierarchy.select:{}:replace",
                                    id.0
                                )));
                            }
                            self.toolbar_revision = self.toolbar_revision.wrapping_add(1).max(1);
                            continue;
                        }
                    }
                    if let Some(raw_id) = name.strip_prefix("hierarchy.drag.move:") {
                        if raw_id.parse::<usize>().is_ok() {
                            let pointer = native_input
                                .snapshot()
                                .pointer_position
                                .unwrap_or([0.0, 0.0]);
                            let hovered = self.host.session().interaction.focus.hovered.as_deref();
                            let target =
                                hierarchy_drop_target_from_hovered(hovered).filter(|target| {
                                    self.hierarchy_drag.as_ref().is_some_and(|drag| {
                                        hierarchy_drop_target_is_valid(
                                            scene,
                                            &drag.source,
                                            Some(*target),
                                        )
                                    })
                                });
                            if let Some(drag) = self.hierarchy_drag.as_mut() {
                                drag.pointer = pointer;
                                drag.target = target;
                                self.toolbar_revision =
                                    self.toolbar_revision.wrapping_add(1).max(1);
                            }
                            continue;
                        }
                    }
                    if name == "hierarchy.drag.over:root" {
                        if let Some(drag) = self.hierarchy_drag.as_mut() {
                            drag.pointer = native_input
                                .snapshot()
                                .pointer_position
                                .unwrap_or(drag.pointer);
                            drag.target = None;
                            self.toolbar_revision = self.toolbar_revision.wrapping_add(1).max(1);
                        }
                        continue;
                    }
                    if name.starts_with("hierarchy.drag.end:") {
                        if let Some(drag) = self.hierarchy_drag.take() {
                            if hierarchy_drop_target_is_valid(scene, &drag.source, drag.target) {
                                let sources = drag
                                    .source
                                    .iter()
                                    .map(|id| id.0.to_string())
                                    .collect::<Vec<_>>()
                                    .join(",");
                                let target = drag
                                    .target
                                    .map(|id| id.0.to_string())
                                    .unwrap_or_else(|| "root".to_string());
                                intents.push(NativeWorkbenchIntent::Command(format!(
                                    "hierarchy.reparent:{sources}:{target}"
                                )));
                            }
                            self.toolbar_revision = self.toolbar_revision.wrapping_add(1).max(1);
                        }
                        continue;
                    }
                    if name == "bottom.toggle-collapsed" {
                        self.bottom_dock.toggle_collapsed();
                        self.toolbar_revision = self.toolbar_revision.wrapping_add(1).max(1);
                        intents.push(NativeWorkbenchIntent::Command(name.clone()));
                        continue;
                    }
                    if let Some(raw_tab) = name.strip_prefix("bottom.tab.") {
                        if let Some((group_id, tab_id)) = raw_tab.split_once('|') {
                            if self.bottom_dock.select_in_group(group_id, tab_id) {
                                self.toolbar_revision =
                                    self.toolbar_revision.wrapping_add(1).max(1);
                            }
                        }
                        continue;
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
                                self.toolbar_revision =
                                    self.toolbar_revision.wrapping_add(1).max(1);
                                self.bottom_dock.persist_project_layout();
                            }
                        }
                        continue;
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
                                self.toolbar_revision =
                                    self.toolbar_revision.wrapping_add(1).max(1);
                                self.bottom_dock.persist_project_layout();
                            }
                        }
                        continue;
                    }
                    if name == "bottom.context.close" {
                        self.bottom_dock.close_context_menu();
                        self.toolbar_revision = self.toolbar_revision.wrapping_add(1).max(1);
                        continue;
                    }
                    if name == "bottom.context.reset" {
                        self.bottom_dock.reset_group(self.project_type);
                        self.toolbar_revision = self.toolbar_revision.wrapping_add(1).max(1);
                        continue;
                    }
                    if let Some(intent) = self.process_session_command(&name, project) {
                        intents.push(intent);
                        continue;
                    }
                    if let Some(raw_id) = name.strip_prefix("inspector.rename.commit:") {
                        if let Ok(id) = raw_id.parse::<usize>() {
                            let value = self
                                .host
                                .session()
                                .interaction
                                .controls
                                .text("inspector.name")
                                .to_string();
                            intents.push(NativeWorkbenchIntent::InspectorCommit {
                                target: SceneNodeId(id),
                                field: "name".to_string(),
                                value,
                            });
                            self.inspector_name_editing = None;
                            self.toolbar_revision = self.toolbar_revision.wrapping_add(1).max(1);
                            continue;
                        }
                    }
                    if let Some(raw_id) = name.strip_prefix("hierarchy.rename.commit:") {
                        if let Ok(id) = raw_id.parse::<usize>() {
                            let value = self
                                .host
                                .session()
                                .interaction
                                .controls
                                .text(&format!("hierarchy.rename.{id}"))
                                .to_string();
                            intents.push(NativeWorkbenchIntent::InspectorCommit {
                                target: SceneNodeId(id),
                                field: "name".to_string(),
                                value,
                            });
                            self.hierarchy_renaming = None;
                            self.toolbar_revision = self.toolbar_revision.wrapping_add(1).max(1);
                            continue;
                        }
                    }
                    if name == "inspector.color.hex.commit" {
                        let Some(target) = selected.first().copied() else {
                            continue;
                        };
                        let value = self
                            .host
                            .session()
                            .interaction
                            .controls
                            .text("inspector.color.hex")
                            .to_string();
                        intents.push(NativeWorkbenchIntent::InspectorCommit {
                            target,
                            field: "color".to_string(),
                            value,
                        });
                        self.inspector_view.color_picker = false;
                        self.toolbar_revision = self.toolbar_revision.wrapping_add(1).max(1);
                        continue;
                    }
                    if let Some(hex) = name.strip_prefix("inspector.color.preset:") {
                        let Some(target) = selected.first().copied() else {
                            continue;
                        };
                        intents.push(NativeWorkbenchIntent::InspectorCommit {
                            target,
                            field: "color".to_string(),
                            value: hex.to_string(),
                        });
                        self.inspector_view.color_picker = false;
                        self.toolbar_revision = self.toolbar_revision.wrapping_add(1).max(1);
                        continue;
                    }
                    if name == "inspector.numeric.commit" {
                        let Some(target) = selected.first().copied() else {
                            continue;
                        };
                        let Some((field, key)) = numeric_commit_field(&dispatched.target_id) else {
                            continue;
                        };
                        let value = self
                            .host
                            .session()
                            .interaction
                            .controls
                            .text(&key)
                            .to_string();
                        intents.push(NativeWorkbenchIntent::InspectorCommit {
                            target,
                            field,
                            value,
                        });
                        self.toolbar_revision = self.toolbar_revision.wrapping_add(1).max(1);
                        continue;
                    }
                    if let Some(action) = parse_viewport_toolbar_action(&dispatched.action) {
                        self.apply_toolbar_action(action);
                        intents.push(NativeWorkbenchIntent::Viewport(action));
                    } else {
                        self.hierarchy_empty_menu_open = false;
                        self.hierarchy_primitive_menu_open = false;
                        self.hierarchy_menu_target = None;
                        self.hierarchy_menu_position = None;
                        intents.push(NativeWorkbenchIntent::Command(name.clone()));
                    }
                }
                UiAction::OpenMenu { id } => {
                    self.open_menu = (self.open_menu.as_deref() != Some(id)).then(|| id.clone());
                    self.toolbar_revision = self.toolbar_revision.wrapping_add(1).max(1);
                }
                _ => {}
            }
        }
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
        if native_input
            .snapshot()
            .key_pressed(raf_core::InputKey::Escape)
        {
            self.open_menu = None;
            self.hierarchy_renaming = None;
            self.hierarchy_drag = None;
            self.inspector_name_editing = None;
            self.hierarchy_empty_menu_open = false;
            self.hierarchy_primitive_menu_open = false;
            self.hierarchy_menu_target = None;
            self.hierarchy_menu_position = None;
            self.assets_primitive_menu_open = false;
            self.toolbar_state.view_menu_open = false;
            self.toolbar_state.shading_menu_open = false;
            self.toolbar_state.primitive_menu_open = false;
            self.toolbar_state.building_menu_open = false;
            self.toolbar_revision = self.toolbar_revision.wrapping_add(1).max(1);
        }
        if snapshot.button_pressed(raf_core::PointerButton::Primary) {
            let hovered = self.host.session().interaction.focus.hovered.as_deref();
            if self.hierarchy_renaming.is_some()
                && !hovered.is_some_and(|id| id.starts_with("hierarchy.rename.control."))
            {
                self.hierarchy_renaming = None;
                self.toolbar_revision = self.toolbar_revision.wrapping_add(1).max(1);
            }
            if self.inspector_name_editing.is_some()
                && !hovered.is_some_and(|id| id == "inspector.name")
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
        }
        if snapshot.button_pressed(raf_core::PointerButton::Primary)
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
            self.hierarchy_menu_target = None;
            self.toolbar_revision = self.toolbar_revision.wrapping_add(1).max(1);
        }
        (intents, actions)
    }
}
