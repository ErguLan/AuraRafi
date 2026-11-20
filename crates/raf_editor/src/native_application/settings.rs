//! Persistent engine settings surface and its committed draft.

use raf_core::config::EngineSettings;
use raf_render::api_graphic_basic::ui_surface::{UiAction, UiDispatchedAction};

use crate::native_studio::NativeStudioSurface;
use crate::native_workbench::NativeGameWorkbench;
use crate::settings_surface::SettingsSection;

use super::{native_environment, native_palette, NativeEditorApplication};

impl NativeEditorApplication {
    pub(super) fn open_settings(&mut self, section: SettingsSection) {
        self.ensure_settings_surface();
        self.settings_restore_focus = self
            .workbench
            .as_ref()
            .and_then(NativeGameWorkbench::focused_control_id);
        let workbench_settings = self
            .workbench
            .as_ref()
            .map(|workbench| workbench.engine_settings().clone());
        if let Some(workbench_settings) = workbench_settings {
            self.settings_state = workbench_settings;
        }
        self.settings_section = section;
        self.settings_open = true;
        if let Some(settings_surface) = self.settings_surface.as_mut() {
            settings_surface.reset_draft(&self.settings_state);
        }
        // Settings replaces the workbench as the active retained surface. If
        // a workbench textbox or command control owned the shared keyboard,
        // Settings could still receive pointer selection but its text events
        // were rejected by InputRouter::try_capture_keyboard.
        let studio_owner = self.studio.as_ref().map(NativeStudioSurface::owner);
        if let Some(runtime) = self.runtime.as_mut() {
            if let Some(workbench) = self.workbench.as_mut() {
                workbench.reset_input_state(runtime.input_router_mut());
            }
            if let Some(owner) = studio_owner {
                runtime.input_router_mut().cancel_owner(owner);
            }
            if let Some(settings_surface) = self.settings_surface.as_mut() {
                settings_surface.reset_input_state(runtime.input_router_mut());
            }
            // The viewport and electronics canvas are separate consumers of
            // the same router. Opening a modal revokes any drag they may
            // still own so the modal cannot render while filtering input for
            // an obsolete owner.
            runtime.input_router_mut().cancel_all();
            runtime.request_animation_frame();
        }
        if let Some(window) = self.window.as_ref() {
            window.request_redraw();
        }
    }

    fn close_settings_input(&mut self) {
        if let (Some(settings_surface), Some(runtime)) =
            (self.settings_surface.as_mut(), self.runtime.as_mut())
        {
            settings_surface.reset_input_state(runtime.input_router_mut());
        }
        self.settings_open = false;
        if let Some(id) = self.settings_restore_focus.take() {
            if let Some(workbench) = self.workbench.as_mut() {
                workbench.restore_focus(id);
            }
        }
    }

    pub(super) fn process_settings_input(&mut self) -> Vec<UiDispatchedAction> {
        if !self.settings_open {
            return Vec::new();
        }
        let Some(runtime) = self.runtime.as_mut() else {
            return Vec::new();
        };
        let rect = runtime.layout().window;
        let Some(settings_surface) = self.settings_surface.as_mut() else {
            return Vec::new();
        };
        let preview_theme = if settings_surface.is_dirty() {
            settings_surface.draft.theme
        } else {
            self.settings_state.theme
        };
        settings_surface.sync(
            native_palette(preview_theme),
            &self.settings_state,
            self.settings_section,
            rect,
        );
        let preview_settings = settings_surface.draft.clone();
        settings_surface.set_environment(native_environment(
            &preview_settings,
            [rect.width, rect.height],
        ));
        settings_surface.process_input(&self.input, runtime.input_router_mut())
    }

    pub(super) fn apply_committed_settings(&mut self) {
        self.apply_live_project_settings();
        let present_refresh_hz = if let Some(window_host) = self.window_host.as_mut() {
            window_host.set_vsync(self.settings_state.vsync);
            window_host.effective_present_refresh_hz()
        } else {
            None
        };
        if let Some(runtime) = self.runtime.as_mut() {
            runtime.set_present_refresh_hz(present_refresh_hz);
        }
        if let Some(workbench) = self.workbench.as_mut() {
            workbench.set_engine_settings(self.settings_state.clone());
        }
        let window_size = self
            .runtime
            .as_ref()
            .map(|runtime| {
                let window = runtime.layout().window;
                [window.width, window.height]
            })
            .unwrap_or([1.0; 2]);
        if let Some(studio) = self.studio.as_mut() {
            studio.set_language(self.settings_state.language);
            studio.set_environment(native_environment(&self.settings_state, window_size));
        }
        if let Some(exit_confirmation_surface) = self.exit_confirmation_surface.as_mut() {
            exit_confirmation_surface
                .set_environment(native_environment(&self.settings_state, window_size));
        }
        if let Some(loading) = self.loading.as_mut() {
            loading.set_environment(native_environment(&self.settings_state, window_size));
        }
        if let Some(editor) = self.electronics_editor.as_mut() {
            editor.apply_engine_settings(&self.settings_state);
            if let Some(project) = self.project.as_ref() {
                editor.apply_project_settings(&project.settings);
            }
        }
    }

    fn accept_settings_draft(&mut self) -> bool {
        let Some(settings_surface) = self.settings_surface.as_mut() else {
            return false;
        };
        if !settings_surface.try_commit_numeric_drafts() {
            return false;
        }
        self.settings_state = settings_surface.accept();
        self.apply_committed_settings();
        self.persist_settings();
        true
    }

    pub(super) fn apply_settings_actions(&mut self, actions: Vec<UiDispatchedAction>) {
        if !self.settings_open {
            return;
        }
        let mut changed = false;
        let escape_closed_select = actions.iter().any(|dispatched| {
            matches!(
                &dispatched.action,
                UiAction::SetSelectOpen { open: false, .. }
            ) || matches!(
                &dispatched.action,
                UiAction::Command { name } if name == "settings.numeric.cancelled"
            )
        });
        for dispatched in actions {
            match dispatched.action {
                UiAction::SetToggle { key, value } => {
                    if let Some(settings_surface) = self.settings_surface.as_mut() {
                        if crate::native_workbench::apply_settings_toggle(
                            &mut settings_surface.draft,
                            &key,
                            value,
                        ) {
                            settings_surface.mark_dirty();
                            changed = true;
                        }
                    }
                }
                UiAction::SetRange { key, value } => {
                    if let Some(settings_surface) = self.settings_surface.as_mut() {
                        if crate::native_workbench::apply_settings_range(
                            &mut settings_surface.draft,
                            &key,
                            value,
                        ) {
                            settings_surface.mark_dirty();
                            changed = true;
                        }
                    }
                }
                UiAction::SetSelect { key, value, .. } => {
                    if let Some(settings_surface) = self.settings_surface.as_mut() {
                        if crate::native_workbench::apply_settings_select(
                            &mut settings_surface.draft,
                            &key,
                            &value,
                        ) {
                            settings_surface.mark_dirty();
                            changed = true;
                        }
                    }
                }
                UiAction::SetSelectOpen { id, open } => {
                    if let Some(settings_surface) = self.settings_surface.as_mut() {
                        settings_surface.set_select_open(&id, open);
                        changed = true;
                    }
                }
                UiAction::SetText { key, value } => {
                    if key == "settings.search" {
                        if let Some(settings_surface) = self.settings_surface.as_mut() {
                            settings_surface.set_search_query(value);
                            self.settings_section = settings_surface.section();
                            changed = true;
                        }
                    } else if let Some(settings_surface) = self.settings_surface.as_mut() {
                        if settings_surface.is_numeric_text_key(&key) {
                            settings_surface.mark_draft_dirty();
                            changed = true;
                        } else if crate::native_workbench::apply_settings_text(
                            &mut settings_surface.draft,
                            &key,
                            &value,
                        ) {
                            settings_surface.mark_dirty();
                            changed = true;
                        }
                    }
                }
                UiAction::Command { name } => {
                    if name.starts_with("settings.account.") {
                        if let Some(settings_surface) = self.settings_surface.as_mut() {
                            changed |= settings_surface.account_command(&name);
                        }
                    } else if let Some(section) = SettingsSection::from_command(&name) {
                        self.settings_section = section;
                        if let Some(settings_surface) = self.settings_surface.as_mut() {
                            settings_surface.set_section(section);
                        }
                        changed = true;
                    } else if let Some(provider_id) =
                        name.strip_prefix("settings.ai_provider.reveal.")
                    {
                        if let Some(provider) =
                            crate::native_workbench::ai_provider_from_id(provider_id)
                        {
                            if let Some(settings_surface) = self.settings_surface.as_mut() {
                                settings_surface.toggle_api_key(provider);
                                changed = true;
                            }
                        }
                    } else if let Some(provider_id) =
                        name.strip_prefix("settings.ai_provider.clear.")
                    {
                        if crate::native_workbench::ai_provider_from_id(provider_id).is_some() {
                            let key = format!("settings.ai_provider.{provider_id}.api_key");
                            if let Some(settings_surface) = self.settings_surface.as_mut() {
                                if crate::native_workbench::apply_settings_text(
                                    &mut settings_surface.draft,
                                    &key,
                                    "",
                                ) {
                                    settings_surface.mark_dirty();
                                    changed = true;
                                }
                            }
                        }
                    } else if name == "settings.numeric.cancelled" {
                        changed = true;
                    } else if name == "settings.apply" {
                        self.accept_settings_draft();
                        // Rejected drafts still change validation presentation.
                        changed = true;
                    } else if name == "settings.save" {
                        let accepted = self.accept_settings_draft();
                        changed = true;
                        if accepted {
                            self.close_settings_input();
                        }
                    } else if name == "settings.cancel" {
                        if let Some(settings_surface) = self.settings_surface.as_mut() {
                            settings_surface.cancel(&self.settings_state);
                        }
                        self.close_settings_input();
                        changed = true;
                    } else if name == "settings.reset_defaults" {
                        if let Some(settings_surface) = self.settings_surface.as_mut() {
                            settings_surface.restore_defaults();
                            settings_surface.set_section(SettingsSection::Appearance);
                            self.settings_section = SettingsSection::Appearance;
                        }
                        changed = true;
                    } else if name == "settings.dismiss" {
                        let can_dismiss = self
                            .settings_surface
                            .as_ref()
                            .is_none_or(|settings_surface| !settings_surface.is_dirty());
                        if can_dismiss {
                            if let Some(settings_surface) = self.settings_surface.as_mut() {
                                settings_surface.cancel(&self.settings_state);
                            }
                            self.close_settings_input();
                            changed = true;
                        }
                    } else if name.starts_with("settings.modal.drag.")
                        || name.starts_with("settings.modal.resize.")
                    {
                        changed = true;
                    } else if let Some(settings_surface) = self.settings_surface.as_mut() {
                        if crate::native_workbench::apply_settings_command(
                            &mut settings_surface.draft,
                            &name,
                        ) {
                            settings_surface.mark_dirty();
                            changed = true;
                        }
                    }
                }
                _ => {}
            }
        }
        if self.settings_open
            && !escape_closed_select
            && self
                .input
                .snapshot()
                .key_pressed(raf_core::InputKey::Escape)
        {
            let can_dismiss = self
                .settings_surface
                .as_ref()
                .is_none_or(|settings_surface| !settings_surface.is_dirty());
            if can_dismiss {
                if let Some(settings_surface) = self.settings_surface.as_mut() {
                    settings_surface.cancel(&self.settings_state);
                }
                self.close_settings_input();
                changed = true;
            }
        }
        if changed {
            if let Some(runtime) = self.runtime.as_mut() {
                runtime.request_animation_frame();
            }
            if let Some(window) = self.window.as_ref() {
                window.request_redraw();
            }
        }
    }

    pub(super) fn persist_settings(&self) {
        let directory = EngineSettings::user_config_dir();
        let mut settings = self.settings_state.clone();
        if !settings.ai_persist_credentials {
            for provider in &mut settings.ai_providers {
                provider.api_key.clear();
            }
        }
        if let Err(error) = settings.save(&directory) {
            tracing::warn!(
                path = %directory.display(),
                %error,
                "global engine settings could not be saved"
            );
        }
    }
}

