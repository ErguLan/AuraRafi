//! Native Hub/studio intents: project creation, adoption, duplication and
//! window forwarding.

use raf_core::project::Project;

use crate::native_studio::{forget_project, remember_project, NativeStudioIntent};

use super::NativeEditorApplication;

impl NativeEditorApplication {
    pub(super) fn apply_studio_intents(&mut self, intents: Vec<NativeStudioIntent>) {
        for intent in intents {
            match intent {
                NativeStudioIntent::RetryOpen => self.retry_editor_project(),
                NativeStudioIntent::CancelOpen => {
                    self.cancel_editor_project();
                    if let Some(runtime) = self.runtime.as_mut() {
                        runtime.request_ui_frame();
                    }
                }
                NativeStudioIntent::Create {
                    name,
                    parent,
                    project_type,
                    initial_view,
                    with_agent,
                } => match Project::create_with_initial_view(
                    &name,
                    project_type,
                    &parent,
                    initial_view,
                ) {
                    Ok(project) => {
                        remember_project(&project);
                        if with_agent {
                            self.open_agents(Some(project.path));
                        } else {
                            self.request_editor_project(project.path);
                        }
                    }
                    Err(error) => {
                        tracing::warn!(%error, "native project creation failed");
                        if let Some(studio) = self.studio.as_mut() {
                            studio.set_create_error(error.to_string());
                        }
                    }
                },
                NativeStudioIntent::OpenSettings { section } => {
                    self.pending_settings_open = Some(section);
                }
                NativeStudioIntent::Window(command) => self.execute_window_command(command),
                NativeStudioIntent::OpenAgents(path) => self.open_agents(path),
                NativeStudioIntent::Open(path) => self.request_editor_project(path),
                NativeStudioIntent::Duplicate(path) => {
                    if let Err(error) = self.duplicate_project(&path) {
                        tracing::warn!(%error, path = %path.display(), "native Hub project duplicate failed");
                        if let Some(studio) = self.studio.as_mut() {
                            studio.set_create_error(error.to_string());
                        }
                    } else if let Some(studio) = self.studio.as_mut() {
                        studio.reset_for_hub(false);
                    }
                }
                NativeStudioIntent::Forget(path) => {
                    forget_project(&path);
                    if let Some(studio) = self.studio.as_mut() {
                        studio.reset_for_hub(false);
                    }
                }
                NativeStudioIntent::AdoptProject {
                    path,
                    name,
                    project_type,
                } => match Project::adopt(&path, &name, project_type) {
                    Ok(project) => {
                        remember_project(&project);
                        self.request_editor_project(project.path);
                    }
                    Err(error) => {
                        tracing::warn!(%error, path = %path.display(), "native Hub project adoption failed");
                        if let Some(studio) = self.studio.as_mut() {
                            studio.set_open_error(error.to_string());
                        }
                    }
                },
            }
        }
    }
}
