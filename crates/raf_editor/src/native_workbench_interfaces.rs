//! Interface authoring bridges only; document edits remain in its focused host.
use super::*;
use crate::panels::interface_workspace::{InterfaceView, PREVIEW_PREFIX};

impl NativeGameWorkbench {
    pub(super) fn handle_interface_action(
        &mut self,
        action: &UiAction,
        input: &NativeUiInputBridge,
        scene: &SceneGraph,
        selected: &[SceneNodeId],
    ) -> bool {
        let name = match action {
            UiAction::Command { name } => name.as_str(),
            UiAction::OpenMenu { id } if id == "ui.designer.insert.menu" => {
                "ui.designer.insert.toggle"
            }
            _ => return false,
        };
        if !name.starts_with("ui.designer.") {
            return false;
        }
        self.interface.workspace.selection_box = self
            .host
            .layout_rect(&format!("{PREVIEW_PREFIX}{}", self.interface.selected));
        self.interface.workspace.parent_box = self
            .interface
            .parent_id()
            .and_then(|id| self.host.layout_rect(&format!("{PREVIEW_PREFIX}{id}")));
        let handled = self.interface.handle(
            action,
            &self.host.session().interaction.controls,
            input.snapshot().pointer_position,
            selected.first().and_then(|id| scene.get(*id)).map(|node| {
                (
                    node.uuid.to_string(),
                    node.primitive.dimensions(glam::Vec3::ONE).z * 0.5 + 0.002,
                )
            }),
            self.agent_settings.language,
        );
        if handled {
            if name == "ui.designer.insert.toggle" && self.interface.workspace.insert_open {
                self.host
                    .session_mut()
                    .interaction
                    .focus
                    .request_focus("ui.designer.insert.frame");
            }
            if name == "ui.designer.export" && self.interface.diagnostic.is_none() {
                self.project_catalog.refresh();
            }
            if self.interface.workspace.active() {
                self.hierarchy_active_tab = "hierarchy".into();
                if name.starts_with("ui.designer.select:")
                    || name.starts_with("ui.designer.add:")
                    || name.starts_with("ui.designer.asset-image:")
                    || name == "ui.designer.open"
                    || name == "ui.designer.place"
                {
                    self.inspector_view.tab = InspectorTab::Properties;
                }
            }
            self.toolbar_revision = self.toolbar_revision.wrapping_add(1).max(1);
        }
        handled
    }
    pub(super) fn interface_keyboard(
        &mut self,
        input: &NativeUiInputBridge,
        router: &mut InputRouter,
    ) {
        if !self.interface.workspace.active() {
            return;
        }
        let snapshot = input.snapshot();
        if self.interface.workspace.insert_open
            && (snapshot.key_pressed(raf_core::InputKey::Escape) || !snapshot.window_focused)
        {
            self.interface.workspace.insert_open = false;
            self.host
                .session_mut()
                .interaction
                .focus
                .request_focus("ui.designer.insert");
            self.toolbar_revision = self.toolbar_revision.wrapping_add(1).max(1);
        }
        if self.interface.workspace.insert_open
            && snapshot.button_pressed(raf_core::PointerButton::Primary)
        {
            let inside = snapshot.pointer_position.is_some_and(|point| {
                self.host
                    .layout_rect("ui.designer.insert.menu")
                    .is_some_and(|r| r.contains(point))
                    || self
                        .host
                        .layout_rect("ui.designer.insert")
                        .is_some_and(|r| r.contains(point))
            });
            if !inside {
                self.interface.workspace.insert_open = false;
                self.popup_press_consumed = true;
                self.toolbar_revision = self.toolbar_revision.wrapping_add(1).max(1);
            }
        }
        if !snapshot.window_focused {
            if self.interface.cancel_gesture() {
                self.toolbar_revision = self.toolbar_revision.wrapping_add(1).max(1);
            }
            return;
        }
        if self.search_surface.is_open()
            || self.open_menu.is_some()
            || matches!(
                router.keyboard_owner(),
                Some(raf_core::InputOwner::Modal(_))
            )
        {
            return;
        }
        if snapshot.key_pressed(raf_core::InputKey::Escape) && self.interface.cancel_gesture() {
            self.host.session_mut().interaction.cancel_pointer_gesture();
            router.release_pointer_unchecked(raf_core::PointerButton::Primary);
            self.toolbar_revision = self.toolbar_revision.wrapping_add(1).max(1);
        }
        let focus = self.host.session().interaction.focus.focused.as_deref();
        let field_focused = focus.is_some_and(|id| id.starts_with("ui.designer.field."));
        let own_focus = focus.is_none_or(|id| id.starts_with("ui.designer."));
        if field_focused || !own_focus {
            return;
        }
        let command = if snapshot.modifiers.command_modifier() {
            if snapshot.key_pressed(raf_core::InputKey::Z) {
                Some(if snapshot.modifiers.shift {
                    "redo"
                } else {
                    "undo"
                })
            } else if snapshot.key_pressed(raf_core::InputKey::Y) {
                Some("redo")
            } else {
                None
            }
        } else if snapshot.key_pressed(raf_core::InputKey::Delete)
            && self.interface.selected != self.interface.document.root.id
        {
            Some("delete")
        } else {
            None
        };
        if let Some(command) = command {
            self.interface.handle(
                &UiAction::Command {
                    name: format!("ui.designer.{command}"),
                },
                &self.host.session().interaction.controls,
                None,
                None,
                self.agent_settings.language,
            );
            self.toolbar_revision = self.toolbar_revision.wrapping_add(1).max(1);
        }
    }
    pub(super) fn activate_interface_authoring(&mut self) {
        self.interface.open_live();
        self.hierarchy_active_tab = "hierarchy".into();
        self.inspector_view.tab = InspectorTab::Properties;
        self.assets_surface.close_menus();
        self.toolbar_revision = self.toolbar_revision.wrapping_add(1).max(1);
    }
    pub(super) fn create_interface_asset(&mut self) {
        self.interface.diagnostic = self
            .interface
            .create_asset(self.agent_settings.language)
            .err();
        if self.interface.diagnostic.is_none() {
            self.project_catalog.refresh();
        }
        self.activate_interface_authoring();
    }
    pub(super) fn open_interface_asset(&mut self, row: &str) {
        self.interface.diagnostic = if row.starts_with("builtin://interface/") {
            self.interface
                .insert_asset(row, self.agent_settings.language)
                .err()
        } else {
            self.interface.open_asset(row).err()
        };
        self.activate_interface_authoring();
    }
    pub(super) fn interface_overlay_visible(&self) -> bool {
        !self.interface.document.root.children.is_empty()
            && self.interface.document.mounts.first().is_none_or(|mount| {
                matches!(
                    mount.kind,
                    raf_ui::game_ui::UiPresentationKind::Screen
                        | raf_ui::game_ui::UiPresentationKind::CameraOverlay
                )
            })
    }
    pub(super) fn interface_scene_view(&mut self) {
        self.interface.cancel_gesture();
        self.interface.workspace.view = InterfaceView::Scene;
        self.interface.editing_asset = None;
    }
}
