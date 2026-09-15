//! Native workbench input routing.
//!
//! Input dispatch is kept separate from workbench state and retained-surface
//! composition so the main workbench remains a coordinator rather than a
//! monolithic event handler.

use super::*;
use crate::console::LogLevel;
use crate::panels::assets_surface::AssetFilter;

impl NativeGameWorkbench {
    pub fn process_input<F>(
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
        let search_open = self.search_surface.is_open();
        let mut intents = Vec::new();
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
        if !search_open {
            // The compass is a true overlay. Give it the first chance to
            // consume a pointer press so an underlying toolbar control cannot
            // win when the two rectangles overlap in a compact viewport.
            let compass_actions =
                self.viewport_compass
                    .process_input(native_input, router, &mut resolve);
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
        for dispatched in &actions {
            match &dispatched.action {
                UiAction::SetText { key, value } => match key.as_str() {
                    "application-bar.command-search.value" => {
                        self.search_surface.set_query(value.clone());
                        self.toolbar_revision = self.toolbar_revision.wrapping_add(1).max(1);
                    }
                    "assets.search" => {
                        self.assets_surface.query = value.clone();
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
                        self.assets_surface.file_name = value.clone();
                        self.toolbar_revision = self.toolbar_revision.wrapping_add(1).max(1);
                    }
                    "assets.script-name" => {
                        self.assets_surface.script_name = value.clone();
                        self.toolbar_revision = self.toolbar_revision.wrapping_add(1).max(1);
                    }
                    "console.input" => {
                        self.console.set_input(value.clone());
                        self.toolbar_revision = self.toolbar_revision.wrapping_add(1).max(1);
                    }
                    "agent.input" => {
                        self.agent_panel.apply_action(
                            AgentAction::SetInput(value.clone()),
                            &mut self.agent_settings,
                        );
                        self.toolbar_revision = self.toolbar_revision.wrapping_add(1).max(1);
                    }
                    "agent.new-model.label" => {
                        self.agent_panel.apply_action(
                            AgentAction::SetNewModelLabel(value.clone()),
                            &mut self.agent_settings,
                        );
                        self.toolbar_revision = self.toolbar_revision.wrapping_add(1).max(1);
                    }
                    "agent.new-model.id" => {
                        self.agent_panel.apply_action(
                            AgentAction::SetNewModelId(value.clone()),
                            &mut self.agent_settings,
                        );
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
                    "project-settings.default_scene_name" => {
                        self.toolbar_revision = self.toolbar_revision.wrapping_add(1).max(1);
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
                    } else if key == "console.auto-scroll" {
                        if self.console.set_auto_scroll(*value) {
                            self.toolbar_revision = self.toolbar_revision.wrapping_add(1).max(1);
                        }
                    } else if apply_settings_toggle(&mut self.agent_settings, key, *value) {
                        self.toolbar_revision = self.toolbar_revision.wrapping_add(1).max(1);
                    }
                }
                UiAction::SetSelect {
                    key,
                    value,
                    index: _,
                } => {
                    if key == "agent.model" {
                        let keep_open = self.agent_panel.model_menu_open;
                        self.apply_agent_action(
                            AgentAction::SelectModel(value.clone()),
                            &mut intents,
                            false,
                        );
                        if keep_open {
                            self.agent_panel.model_menu_open = true;
                        }
                    } else if key == "agent.mode" {
                        let mode = match value.as_str() {
                            "inspect" => Some(AgentMode::Inspect),
                            "plan" => Some(AgentMode::Plan),
                            "active" => Some(AgentMode::Active),
                            _ => None,
                        };
                        if let Some(mode) = mode {
                            let keep_open = self.agent_panel.mode_menu_open;
                            self.apply_agent_action(
                                AgentAction::SetMode(mode),
                                &mut intents,
                                false,
                            );
                            if keep_open {
                                self.agent_panel.mode_menu_open = true;
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
                    if let Some(dropdown) = inspector_dropdown_from_trigger(id) {
                        self.inspector_view.dropdown = open.then_some(dropdown);
                        if open {
                            self.inspector_view.color_picker = false;
                        }
                    }
                    if !open {
                        self.host
                            .session_mut()
                            .interaction
                            .focus
                            .request_focus(id.clone());
                    }
                    match id.as_str() {
                        "agent.model.trigger" => {
                            self.agent_panel.model_menu_open = open;
                            if open {
                                self.agent_panel.mode_menu_open = false;
                                self.agent_panel.add_model_open = false;
                            }
                        }
                        "agent.mode.trigger" => {
                            self.agent_panel.mode_menu_open = open;
                            if open {
                                self.agent_panel.model_menu_open = false;
                                self.agent_panel.add_model_open = false;
                            }
                        }
                        "viewport.toolbar.view-mode.trigger" => {
                            self.toolbar_state.view_menu_open = open;
                            if open {
                                self.toolbar_state.shading_menu_open = false;
                                self.toolbar_state.building_menu_open = false;
                                self.toolbar_state.primitive_menu_open = false;
                            }
                        }
                        "viewport.toolbar.shading.trigger" => {
                            self.toolbar_state.shading_menu_open = open;
                            if open {
                                self.toolbar_state.view_menu_open = false;
                                self.toolbar_state.building_menu_open = false;
                                self.toolbar_state.primitive_menu_open = false;
                            }
                        }
                        "viewport.toolbar.building-style" => {
                            self.toolbar_state.building_menu_open = open;
                            if open {
                                self.toolbar_state.view_menu_open = false;
                                self.toolbar_state.shading_menu_open = false;
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
                UiAction::ScrollTo { id, offset } if id == "hierarchy.tree" => {
                    let next = offset[1].max(0.0);
                    if (next - self.hierarchy_scroll_offset).abs() > f32::EPSILON {
                        self.hierarchy_scroll_offset = next;
                        self.toolbar_revision = self.toolbar_revision.wrapping_add(1).max(1);
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
                UiAction::Command { name } => {
                    if name == crate::application_menu::command::SEARCH_OPEN {
                        self.open_search();
                        self.open_menu = None;
                        continue;
                    }
                    if let Some(action) = parse_agent_action(name, self.agent_settings.language) {
                        let reset_history = matches!(
                            &action,
                            AgentAction::NewChat
                                | AgentAction::SelectSession(_)
                                | AgentAction::DeleteSession(_)
                        );
                        self.apply_agent_action(action, &mut intents, reset_history);
                        continue;
                    }
                    if let Some(submenu_id) = name.strip_prefix("application.menu.submenu.open:") {
                        self.open_submenu = Some(submenu_id.to_string());
                        self.toolbar_revision = self.toolbar_revision.wrapping_add(1).max(1);
                        continue;
                    }
                    if let Some(menu_id) = name.strip_prefix("application.menu.") {
                        self.open_menu = (self.open_menu.as_deref() != Some(menu_id))
                            .then(|| menu_id.to_string());
                        self.open_submenu = None;
                        self.toolbar_revision = self.toolbar_revision.wrapping_add(1).max(1);
                        continue;
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
                    if let Some(key) = name.strip_prefix("project-settings.commit_text:") {
                        let value_key = format!("project-settings.{key}");
                        let value = self
                            .host
                            .session()
                            .interaction
                            .controls
                            .text(&value_key)
                            .to_string();
                        intents.push(NativeWorkbenchIntent::ProjectSettingText {
                            key: value_key,
                            value,
                        });
                        self.toolbar_revision = self.toolbar_revision.wrapping_add(1).max(1);
                        continue;
                    }
                    if name.starts_with("project-settings.") {
                        intents.push(NativeWorkbenchIntent::ProjectSettingCommand(name.clone()));
                        self.toolbar_revision = self.toolbar_revision.wrapping_add(1).max(1);
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
                            self.reset_input_state(router);
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
                        "console.filter.all" => {
                            if self.console.set_filter_level(None) {
                                self.toolbar_revision =
                                    self.toolbar_revision.wrapping_add(1).max(1);
                            }
                            continue;
                        }
                        "console.filter.info" => {
                            if self.console.set_filter_level(Some(LogLevel::Info)) {
                                self.toolbar_revision =
                                    self.toolbar_revision.wrapping_add(1).max(1);
                            }
                            continue;
                        }
                        "console.filter.warning" => {
                            if self.console.set_filter_level(Some(LogLevel::Warning)) {
                                self.toolbar_revision =
                                    self.toolbar_revision.wrapping_add(1).max(1);
                            }
                            continue;
                        }
                        "console.filter.error" => {
                            if self.console.set_filter_level(Some(LogLevel::Error)) {
                                self.toolbar_revision =
                                    self.toolbar_revision.wrapping_add(1).max(1);
                            }
                            continue;
                        }
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
                        _ if name.starts_with("console.block.toggle:") => {
                            if let Some(raw_id) = name.strip_prefix("console.block.toggle:") {
                                if let Ok(id) = raw_id.parse::<u64>() {
                                    if self.console.toggle_json_disclosure(id) {
                                        self.toolbar_revision =
                                            self.toolbar_revision.wrapping_add(1).max(1);
                                    }
                                }
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
                            self.assets_surface.primitive_menu_open =
                                !self.assets_surface.primitive_menu_open;
                            self.toolbar_revision = self.toolbar_revision.wrapping_add(1).max(1);
                            continue;
                        }
                        "assets.create-primitive.cancel" => {
                            self.assets_surface.primitive_menu_open = false;
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
                            self.assets_surface.script_menu_open = true;
                            self.assets_surface.file_menu_open = false;
                            self.toolbar_revision = self.toolbar_revision.wrapping_add(1).max(1);
                            continue;
                        }
                        "assets.create-file" => {
                            self.assets_surface.file_menu_open = true;
                            self.assets_surface.script_menu_open = false;
                            self.toolbar_revision = self.toolbar_revision.wrapping_add(1).max(1);
                            continue;
                        }
                        "assets.script.cancel" => {
                            self.assets_surface.script_menu_open = false;
                            self.toolbar_revision = self.toolbar_revision.wrapping_add(1).max(1);
                            continue;
                        }
                        "assets.file.cancel" => {
                            self.assets_surface.file_menu_open = false;
                            self.toolbar_revision = self.toolbar_revision.wrapping_add(1).max(1);
                            continue;
                        }
                        "assets.refresh" | "assets.open-folder" => {
                            self.project_catalog.refresh();
                            continue;
                        }
                        "project.open_folder" => {
                            self.bottom_dock.select("assets");
                            self.toolbar_revision = self.toolbar_revision.wrapping_add(1).max(1);
                            intents.push(NativeWorkbenchIntent::OpenProjectFolder);
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
                                self.reset_input_state(router);
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
                        self.assets_surface.primitive_menu_open = false;
                        self.toolbar_revision = self.toolbar_revision.wrapping_add(1).max(1);
                    }
                    if name.starts_with("inspector.primitive:")
                        || name.starts_with("inspector.collider:")
                        || name.starts_with("inspector.body-type:")
                    {
                        let trigger_id = if name.starts_with("inspector.primitive:") {
                            "inspector.primitive.trigger"
                        } else if name.starts_with("inspector.collider:") {
                            "inspector.collider.trigger"
                        } else {
                            "inspector.body-type.trigger"
                        };
                        self.inspector_view.dropdown = None;
                        self.inspector_view.color_picker = false;
                        self.host
                            .session_mut()
                            .interaction
                            .focus
                            .request_focus(trigger_id);
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
                        self.inspector_view.dropdown = None;
                        self.toolbar_revision = self.toolbar_revision.wrapping_add(1).max(1);
                        continue;
                    }
                    if let Some(hue) = name.strip_prefix("inspector.color.hue:") {
                        let Some(target) = selected.first().copied() else {
                            continue;
                        };
                        intents.push(NativeWorkbenchIntent::Command(format!(
                            "inspector.color.hue:{hue}:{}",
                            target.0
                        )));
                        self.toolbar_revision = self.toolbar_revision.wrapping_add(1).max(1);
                        continue;
                    }
                    if let Some(slug) = name.strip_prefix("inspector.dropdown.toggle:") {
                        if let Some((dropdown, navigation)) = slug.rsplit_once(':') {
                            if matches!(navigation, "next" | "previous" | "first" | "last") {
                                if matches!(dropdown, "primitive" | "collider" | "body-type") {
                                    self.inspector_view.dropdown = match dropdown {
                                        "primitive" => Some(InspectorDropdown::Primitive),
                                        "collider" => Some(InspectorDropdown::Collider),
                                        _ => Some(InspectorDropdown::BodyType),
                                    };
                                    if let Some(command) = inspector_dropdown_navigation_command(
                                        scene, selected, dropdown, navigation,
                                    ) {
                                        intents.push(NativeWorkbenchIntent::Command(command));
                                        self.inspector_view.dropdown = None;
                                    }
                                    self.toolbar_revision =
                                        self.toolbar_revision.wrapping_add(1).max(1);
                                }
                                continue;
                            }
                        }
                        let next = match slug {
                            "primitive" => Some(InspectorDropdown::Primitive),
                            "collider" => Some(InspectorDropdown::Collider),
                            "body-type" => Some(InspectorDropdown::BodyType),
                            _ => None,
                        };
                        self.inspector_view.dropdown = (self.inspector_view.dropdown != next)
                            .then_some(next)
                            .flatten();
                        self.inspector_view.color_picker = false;
                        self.toolbar_revision = self.toolbar_revision.wrapping_add(1).max(1);
                        continue;
                    }
                    if let Some(filter) = name.strip_prefix("assets.filter.") {
                        self.assets_surface.filter = match filter {
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
                        self.assets_surface.script_menu_open = false;
                        self.assets_surface.refresh_requested = true;
                        intents.push(NativeWorkbenchIntent::Command(format!(
                            "assets.create-script:{language}:{}",
                            self.assets_surface.script_name.trim()
                        )));
                        self.toolbar_revision = self.toolbar_revision.wrapping_add(1).max(1);
                        continue;
                    }
                    if name == "assets.file.create" {
                        self.assets_surface.file_menu_open = false;
                        self.assets_surface.refresh_requested = true;
                        intents.push(NativeWorkbenchIntent::Command(format!(
                            "assets.create-file:{}",
                            self.assets_surface.file_name.trim()
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
                        if raw_id.parse::<usize>().is_ok() && self.hierarchy_drag.is_some() {
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
                    if name == "hierarchy.drag.over:root" && self.hierarchy_drag.is_some() {
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
                                self.reset_input_state(router);
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
                                self.reset_input_state(router);
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
                                self.reset_input_state(router);
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
                        self.reset_input_state(router);
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
                        self.inspector_view.dropdown = None;
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
                    if let Some(hex) = name.strip_prefix("inspector.color.apply:") {
                        let Some(target) = selected.first().copied() else {
                            continue;
                        };
                        intents.push(NativeWorkbenchIntent::InspectorCommit {
                            target,
                            field: "color".to_string(),
                            value: hex.to_string(),
                        });
                        // Hue changes are iterative: leave the picker open so
                        // the user can refine channels in one pass.
                        self.inspector_view.dropdown = None;
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
                        let value = if field.starts_with("position.") {
                            value
                                .trim()
                                .parse::<f32>()
                                .ok()
                                .filter(|value| value.is_finite())
                                .map(|value| {
                                    self.agent_settings
                                        .display_unit
                                        .to_meters(value)
                                        .to_string()
                                })
                                .unwrap_or(value)
                        } else {
                            value
                        };
                        intents.push(NativeWorkbenchIntent::InspectorCommit {
                            target,
                            field,
                            value,
                        });
                        self.toolbar_revision = self.toolbar_revision.wrapping_add(1).max(1);
                        continue;
                    }
                    if let Some(action) = parse_viewport_toolbar_action(&dispatched.action) {
                        let settings_changed = self.apply_toolbar_action(action);
                        if settings_changed {
                            intents.push(NativeWorkbenchIntent::AgentSettingsChanged(
                                self.agent_settings.clone(),
                            ));
                        }
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
                    if let Some(raw_id) = id.strip_prefix("hierarchy.menu:") {
                        if let Ok(id) = raw_id.parse::<usize>() {
                            let target = SceneNodeId(id);
                            self.open_menu = None;
                            self.hierarchy_menu_target =
                                Some((target, self.hierarchy_target_is_folder(target)));
                            self.hierarchy_empty_menu_open = false;
                            self.hierarchy_primitive_menu_open = false;
                            self.hierarchy_menu_position = native_input.snapshot().pointer_position;
                            self.toolbar_revision = self.toolbar_revision.wrapping_add(1).max(1);
                            continue;
                        }
                    }
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
            self.assets_surface.primitive_menu_open = false;
            self.toolbar_state.view_menu_open = false;
            self.toolbar_state.shading_menu_open = false;
            self.toolbar_state.primitive_menu_open = false;
            self.toolbar_state.building_menu_open = false;
            self.inspector_view.dropdown = None;
            self.inspector_view.color_picker = false;
            if self.agent_panel.has_open_menu() {
                self.agent_panel.close_menus();
            }
            self.toolbar_revision = self.toolbar_revision.wrapping_add(1).max(1);
        }
        if snapshot.button_pressed(raf_core::PointerButton::Primary) {
            if self.agent_panel.has_open_menu()
                && !actions.iter().any(|action| {
                    action.target_id.starts_with("agent.model")
                        || action.target_id.starts_with("agent.mode")
                        || action.target_id.starts_with("agent.add-model")
                        || action.target_id.starts_with("agent.new-model")
                })
            {
                self.agent_panel.close_menus();
                self.toolbar_revision = self.toolbar_revision.wrapping_add(1).max(1);
            }
            let hovered = self.host.session().interaction.focus.hovered.clone();
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
            if self.inspector_view.dropdown.is_some() || self.inspector_view.color_picker {
                let inside_inspector_popup = hovered.as_deref().is_some_and(|id| {
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
        (intents, actions)
    }

    fn apply_agent_action(
        &mut self,
        action: AgentAction,
        intents: &mut Vec<NativeWorkbenchIntent>,
        reset_history: bool,
    ) {
        let focus_new_model_label = matches!(&action, AgentAction::OpenAddModel);
        if matches!(&action, AgentAction::Approve) {
            self.pending_agent_decision = Some(true);
        } else if matches!(&action, AgentAction::Deny) {
            self.pending_agent_decision = Some(false);
        }
        self.agent_panel
            .apply_action(action, &mut self.agent_settings);
        if focus_new_model_label {
            self.host
                .session_mut()
                .interaction
                .focus
                .request_focus("agent.new-model.label");
        }
        if reset_history {
            self.agent_scroll_projection_offset = 0.0;
            self.host
                .session_mut()
                .reset_interaction_for_surface_change(Some("agent.history"));
        }
        if self.agent_panel.take_open_settings_request() {
            self.settings_section = SettingsSection::Ai;
            intents.push(NativeWorkbenchIntent::OpenSettings {
                section: SettingsSection::Ai,
            });
        }
        if self.agent_panel.take_settings_changed() {
            intents.push(NativeWorkbenchIntent::AgentSettingsChanged(
                self.agent_settings.clone(),
            ));
        }
        self.toolbar_revision = self.toolbar_revision.wrapping_add(1).max(1);
    }
}

fn parse_agent_action(name: &str, language: raf_core::Language) -> Option<AgentAction> {
    let action = match name {
        "agent.sidebar.toggle" => AgentAction::ToggleSidebar,
        "agent.sidebar.close" => AgentAction::CloseSidebar,
        "agent.new-chat" => AgentAction::NewChat,
        "agent.model.toggle" => AgentAction::ToggleModelMenu,
        "agent.mode.toggle" => AgentAction::ToggleModeMenu,
        "agent.model.add" => AgentAction::OpenAddModel,
        "agent.model.cancel" => AgentAction::CloseAddModel,
        "agent.model.confirm" => AgentAction::AddModel,
        "agent.open-settings" => AgentAction::OpenSettings,
        "agent.tool-warning.dismiss" => AgentAction::DismissToolCallWarning,
        "agent.submit" => AgentAction::Submit,
        "agent.stop" => AgentAction::Stop,
        "agent.approve" => AgentAction::Approve,
        "agent.deny" => AgentAction::Deny,
        "agent.mode.inspect" => AgentAction::SetMode(AgentMode::Inspect),
        "agent.mode.passive" | "agent.mode.plan" => AgentAction::SetMode(AgentMode::Plan),
        "agent.mode.active" => AgentAction::SetMode(AgentMode::Active),
        _ => {
            if let Some(raw) = name.strip_prefix("agent.session.select:") {
                AgentAction::SelectSession(raw.parse().ok()?)
            } else if let Some(raw) = name.strip_prefix("agent.session.delete:") {
                AgentAction::DeleteSession(raw.parse().ok()?)
            } else if let Some(raw) = name.strip_prefix("agent.model.select:") {
                AgentAction::SelectModel(raw.to_string())
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
        _ => None,
    }
}

fn inspector_dropdown_from_trigger(id: &str) -> Option<InspectorDropdown> {
    match id {
        "inspector.primitive.trigger" => Some(InspectorDropdown::Primitive),
        "inspector.collider.trigger" => Some(InspectorDropdown::Collider),
        "inspector.body-type.trigger" => Some(InspectorDropdown::BodyType),
        _ => None,
    }
}

fn inspector_dropdown_navigation_command(
    scene: &SceneGraph,
    selected: &[SceneNodeId],
    dropdown: &str,
    navigation: &str,
) -> Option<String> {
    let id = selected.first().copied()?;
    let node = scene.get(id)?;
    let (current, labels): (&str, &[&str]) = match dropdown {
        "primitive" => (
            match node.primitive {
                raf_core::scene::Primitive::Empty => "Empty",
                raf_core::scene::Primitive::Cube => "Cube",
                raf_core::scene::Primitive::Sphere => "Sphere",
                raf_core::scene::Primitive::Plane => "Plane",
                raf_core::scene::Primitive::Cylinder => "Cylinder",
            },
            &["Empty", "Cube", "Sphere", "Plane", "Cylinder"],
        ),
        "collider" => (
            match node.collider.collider_type {
                raf_core::scene::ColliderType::None => "None",
                raf_core::scene::ColliderType::Aabb => "Aabb",
                raf_core::scene::ColliderType::ConvexHull => "ConvexHull",
                raf_core::scene::ColliderType::MeshCollider => "MeshCollider",
            },
            &["None", "Aabb", "ConvexHull", "MeshCollider"],
        ),
        "body-type" => (
            match node.rigid_body.body_type {
                raf_core::scene::RigidBodyType::Static => "Static",
                raf_core::scene::RigidBodyType::Dynamic => "Dynamic",
                raf_core::scene::RigidBodyType::Kinematic => "Kinematic",
            },
            &["Static", "Dynamic", "Kinematic"],
        ),
        _ => return None,
    };
    let current_index = labels.iter().position(|label| *label == current)?;
    let next_index = match navigation {
        "first" => 0,
        "last" => labels.len().saturating_sub(1),
        "next" => (current_index + 1) % labels.len(),
        "previous" => (current_index + labels.len() - 1) % labels.len(),
        _ => return None,
    };
    let value = labels[next_index];
    Some(match dropdown {
        "primitive" => format!("inspector.primitive:{}:{value}", id.0),
        "collider" => format!("inspector.collider:{}:{value}", id.0),
        "body-type" => format!("inspector.body-type:{}:{value}", id.0),
        _ => return None,
    })
}
