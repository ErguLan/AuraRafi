//! Play/runtime bridge and embedded player presentation calls.

use std::time::Instant;

use super::shortcuts::electronics_input_edge_pending;
use super::NativeEditorApplication;

impl NativeEditorApplication {
    pub(super) fn begin_frame_gate(&mut self) -> Option<f64> {
        let now = self.elapsed_seconds();
        let present_refresh_hz = self
            .window_host
            .as_ref()
            .and_then(|host| host.effective_present_refresh_hz());
        if let Some(runtime) = self.runtime.as_mut() {
            runtime.set_present_refresh_hz(present_refresh_hz);
        }
        let Some(runtime) = self.runtime.as_ref() else {
            return None;
        };
        let dispatch_input_edge = electronics_input_edge_pending(
            self.project.as_ref().map(|project| project.project_type),
            self.input.snapshot(),
        );
        if !dispatch_input_edge
            && runtime
                .seconds_until_next_frame(now)
                .is_none_or(|seconds| seconds > f64::EPSILON)
        {
            return None;
        }
        Some(now)
    }

    pub(super) fn apply_runtime_command(&mut self, command: &str) {
        if matches!(command,"runtime.play"|"runtime.reload") {
            if let Some(workbench)=&mut self.workbench {
                if let Err(error)=workbench.flush_interface_document(){
                    workbench.log_console_output(crate::commands::CommandOutput::error(raf_core::i18n::t("ui.commands.title",self.settings_state.language),raf_core::i18n::t(&error,self.settings_state.language)));
                    return;
                }
            }
        }
        let result = match command {
            "runtime.play" => match (self.project.as_ref(), self.window_host.as_ref()) {
                (Some(project), Some(host)) => self
                    .game_runtime
                    .launch(
                        project,
                        &self.scene,
                        self.runtime.as_ref().map(|runtime| runtime.node_graph()),
                        &self.settings_state,
                        host,
                    )
                    .map(|_| ()),
                _ => Err("open a Game project before starting runtime".into()),
            },
            "runtime.pause" => self
                .game_runtime
                .control(raf_runtime::RuntimeControl::Pause),
            "runtime.resume" => self
                .game_runtime
                .control(raf_runtime::RuntimeControl::Resume),
            "runtime.step" => self.game_runtime.control(raf_runtime::RuntimeControl::Step),
            "runtime.stop" => self.game_runtime.control(raf_runtime::RuntimeControl::Stop),
            "runtime.reload" => self
                .game_runtime
                .control(raf_runtime::RuntimeControl::Reload),
            "runtime.next" => {
                self.game_runtime.select_next();
                Ok(())
            }
            _ => Err(format!("unknown local runtime command: {command}")),
        };
        if let Err(error) = result {
            if let Some(workbench) = &mut self.workbench {
                workbench
                    .log_console_output(crate::commands::CommandOutput::error("Runtime", error));
            }
        }
        if let Some(runtime) = &mut self.runtime {
            runtime.input_router_mut().cancel_all();
            runtime.request_ui_frame();
        }
        self.input.snapshot_mut().cancel_all();
        self.next_runtime_frame = Instant::now();
        if let Some(window) = &self.window {
            window.request_redraw();
        }
    }

    pub(super) fn present_in_place_runtime(&mut self) {
        let mut finished = false;
        if let (Some(player), Some(host), Some(compositor)) = (
            self.game_runtime.in_place.as_mut(),
            self.window_host.as_mut(),
            self.compositor.as_mut(),
        ) {
            if let Err(error) = player.draw(host, compositor, &self.input) {
                tracing::warn!(%error, "in-place runtime presentation failed");
                player.control(raf_runtime::RuntimeControl::Stop);
            }
            finished = player.should_close();
            if let Some(workbench) = &mut self.workbench {
                for message in player.drain_diagnostics() {
                    workbench.log_console_output(crate::commands::CommandOutput::info(
                        "Runtime",
                        vec![message],
                        serde_json::json!({ "source": "local-runtime" }),
                    ));
                }
            }
            self.next_runtime_frame = Instant::now() + player.frame_interval();
        }
        self.input.begin_frame();
        if finished {
            self.game_runtime.finish_in_place();
            if let Some(runtime) = &mut self.runtime {
                runtime.request_canvas_frame();
            }
            if let Some(window) = &self.window {
                window.request_redraw();
            }
        }
    }
}

