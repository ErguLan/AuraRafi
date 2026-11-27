//! Workbench status line, console projection and assets operation receipts.

use super::*;

impl NativeGameWorkbench {
    pub(crate) fn authoring_status(&mut self, key: &str) {
        let label = raf_core::i18n::t(key, self.agent_settings.language);
        self.assets_surface.status = Some(
            if key == "assets.placed" || key == "assets.placement_cancelled" {
                AssetsStatus::success(label)
            } else {
                AssetsStatus::error(label)
            },
        );
        self.toolbar_revision = self.toolbar_revision.wrapping_add(1).max(1);
    }
    pub fn log_console_output(&mut self, mut output: crate::commands::CommandOutput) {
        if output.title.starts_with("ui.") {
            output.title = raf_core::i18n::t(&output.title, self.agent_settings.language);
        }
        for line in &mut output.lines {
            if line.starts_with("ui.") {
                *line = raf_core::i18n::t(line, self.agent_settings.language);
            }
            if line.starts_with("commands.error.dimensions_") {
                *line = raf_core::i18n::t(line, self.agent_settings.language);
            }
        }
        if output.title == raf_core::i18n::t("authoring.edit_pending", self.agent_settings.language)
        {
            self.authoring_notice = Some(output.title.clone());
        }
        self.console.log_command_output(output);
        self.toolbar_revision = self.toolbar_revision.wrapping_add(1).max(1);
    }

    /// Appends a structured runtime event with severity, source, code and time.
    ///
    /// The console block keeps the bounded player log tail and the machine
    /// fields in JSON while the row itself shows the human message.
    pub fn log_runtime_entry(&mut self, entry: crate::native_game_runtime::RuntimeConsoleEntry) {
        let crate::native_game_runtime::RuntimeConsoleEntry {
            level,
            code,
            source,
            message,
            detail,
            at_ms,
            lines,
        } = entry;
        let language = self.agent_settings.language;
        let message = crate::native_runtime_log::localized_runtime_text(&message, language);
        let mut block_lines = Vec::new();
        if !code.is_empty() {
            block_lines.push(format!("code: {code}"));
        }
        if let Some(detail) = detail.as_deref() {
            block_lines.push(crate::native_runtime_log::localized_runtime_text(
                detail, language,
            ));
        }
        if !lines.is_empty() {
            block_lines.push(crate::native_runtime_log::localized_runtime_text(
                "console.runtime_log",
                language,
            ));
            block_lines.extend(lines);
        }
        let json = serde_json::json!({
            "code": code.as_str(),
            "severity": level.as_str(),
            "source": source.as_str(),
            "at_ms": at_ms,
        })
        .to_string();
        self.console.log_structured(
            level,
            source,
            message.clone(),
            Some(code),
            Some(crate::console::ConsoleBlock {
                title: message,
                lines: block_lines,
                json,
            }),
        );
        self.toolbar_revision = self.toolbar_revision.wrapping_add(1).max(1);
    }

    pub(crate) fn performance_status_text(&self) -> String {
        if !self.agent_settings.show_fps_counter {
            return "FPS: hidden".to_string();
        }
        let effective_target = if self.target_fps == 0 {
            "max".to_string()
        } else {
            self.target_fps.to_string()
        };
        let target = if self.requested_target_fps != self.target_fps {
            let requested_target = if self.requested_target_fps == 0 {
                "max".to_string()
            } else {
                self.requested_target_fps.to_string()
            };
            format!("{requested_target}>{effective_target}")
        } else {
            effective_target
        };
        let gpu = if self.frame_gpu_timing_sampled {
            format!("{:.1}ms", self.frame_gpu_ms)
        } else {
            "--".to_string()
        };
        let viewport = if self.viewport_fps > f32::EPSILON {
            format!("{:.0}", self.viewport_fps)
        } else {
            "--".to_string()
        };
        let upload_kib = self.scene_upload_bytes as f64 / 1024.0;
        let upload_warning = if self.scene_upload_budget_exceeded {
            "!"
        } else {
            ""
        };
        let capacity = if self.estimated_capacity_fps > 0 {
            format!("~{}", self.estimated_capacity_fps)
        } else {
            "--".to_string()
        };
        let shadow = if self.shadow_metrics.shadow_budget_exceeded {
            " | SH:budget!".to_string()
        } else if self.shadow_metrics.shadow_active {
            format!(
                " | SH:{}{} {:.1}M",
                self.shadow_metrics.shadow_resolution,
                if self.shadow_metrics.shadow_map_rebuilds > 0 {
                    "R"
                } else {
                    "C"
                },
                self.shadow_metrics.shadow_resident_bytes as f32 / 1048576.0
            )
        } else {
            String::new()
        };
        // The compact memory segment is cached by `sync_performance_readout`
        // and only changes when a displayed ledger value changes.
        let memory = self.last_performance_segment.as_str();
        let memory_separator = if memory.is_empty() { "" } else { " | " };
        format!(
            "CAP:{capacity} | FPS:{presented_fps:.0}/{target} | VP:{viewport} | CPU:{frame_cpu_ms:.1}ms | GPU:{gpu} | P95:{p95_frame_time_ms:.1}ms{memory_separator}{memory} | D{scene_draw_calls} | U{upload_kib:.0}K{upload_warning}{shadow} | H{hitch_count} | {frame_activity_label} | {redraw_reason_label} | {canvas_status_label} | {presentation_label}",
            capacity = capacity,
            presented_fps = self.presented_fps,
            target = target,
            viewport = viewport,
            frame_cpu_ms = self.frame_cpu_ms,
            gpu = gpu,
            p95_frame_time_ms = self.p95_frame_time_ms,
            memory = memory,
            memory_separator = memory_separator,
            scene_draw_calls = self.scene_draw_calls,
            upload_kib = upload_kib,
            upload_warning = upload_warning,
            hitch_count = self.hitch_count,
            frame_activity_label = self.frame_activity_label,
            redraw_reason_label = self.redraw_reason_label,
            canvas_status_label = self.canvas_status_label,
            presentation_label = self.presentation_label,
        )
    }

    /// Records what the panel expects to see in the next catalog and shows the
    /// pending line while the filesystem work runs. The panel never claims
    /// success up front: `verify_assets_operation` decides the outcome.
    pub(super) fn begin_assets_operation(
        &mut self,
        expected_row: String,
        expect_present: bool,
        success_key: &'static str,
        error_key: &'static str,
    ) {
        self.assets_surface.operation = Some(AssetsOperation {
            expected_row,
            expect_present,
            started_seconds: self.last_sync_time_seconds,
            catalog_revision: self.project_catalog.revision(),
            success_key,
            error_key,
        });
        self.assets_surface.refresh_requested = true;
        self.assets_surface.status = Some(AssetsStatus::pending(raf_core::i18n::t(
            "app.assets_status_working",
            self.agent_panel.borrow_mut().language,
        )));
    }

    /// Compares the refreshed catalog with the operation the panel is waiting
    /// for so the status line only reports work the filesystem really did.
    pub(super) fn verify_assets_operation(&mut self, now_seconds: f64) {
        let Some(operation) = self.assets_surface.operation.clone() else {
            return;
        };
        let language = self.agent_panel.borrow_mut().language;
        let revision_changed = self.project_catalog.revision() != operation.catalog_revision;
        let observed = revision_changed
            && (self
                .project_catalog
                .assets()
                .iter()
                .any(|row| *row == operation.expected_row)
                == operation.expect_present);
        let timed_out = now_seconds - operation.started_seconds > ASSET_OPERATION_TIMEOUT_SECONDS;
        if !observed && !timed_out {
            return;
        }
        let row = operation.expected_row.clone();
        self.assets_surface.operation = None;
        self.assets_surface.status = Some(if observed {
            AssetsStatus::success(format_row_name(operation.success_key, &row, language))
        } else {
            AssetsStatus::error(format_row_name(operation.error_key, &row, language))
        });
        if observed {
            self.assets_surface.highlight_asset = Some(row);
            self.assets_highlight_motion.set_immediate(0.0);
        }
        self.toolbar_revision = self.toolbar_revision.wrapping_add(1).max(1);
    }
}
