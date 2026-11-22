//! Retained RafUI host. It owns UI state and produces ApiGraphicBasic paint.
//!
//! The host has no graphics device and no presentation target of its own. A
//! platform shell or compositor supplies the full target geometry, and the host
//! appends normalized `Command::Paint` / `Command::StyledPaint` entries into a
//! shared [`raf_agb::product::ProductPacket`]. The same retained document and
//! draw list feed the CPU recovery host.

use std::sync::Arc;

use raf_core::{CaptureMode, InputOwner, InputRouter, PointerButton};

use super::compilation::{
    update_surface_node, UiSurfaceCompilationCache, UiSurfaceCompileMetrics, UiSurfaceUpdateImpact,
};
use super::product_paint::{append_paint, UiPaintMetrics, UiTextureCache};
use super::{
    NativeUiInputBridge, UiControlState, UiDispatchedAction, UiDocumentUpdate,
    UiDocumentUpdateError, UiFocusState, UiInputState, UiSurface, UiSurfaceDiagnostics,
    UiSurfaceDrawList, UiSurfaceFrame, UiSurfaceImageStore, UiSurfaceSession,
};
use raf_ui::UiEnvironment;

use crate::api_graphic_basic::capabilities::GraphicsMemoryBudget;

pub struct DirectUiSurfaceHost {
    surface: UiSurface,
    surface_revision: u64,
    /// Source revision for paint-only document changes.
    source_paint_revision: u64,
    /// Monotonic revision of everything that can change the emitted paint.
    paint_generation: u64,
    session: UiSurfaceSession,
    images: UiSurfaceImageStore,
    clear_color: [u8; 4],
    compilation: UiSurfaceCompilationCache,
    textures: UiTextureCache,
    last_paint_metrics: UiPaintMetrics,
    last_produced_frame: Option<DirectUiSurfaceFrame>,
    observed_atlas_generation: u64,
    observed_image_generation: u64,
    observed_motion_revision: u64,
    observed_tooltip_revision: u32,
    observed_focus: UiFocusState,
    observed_controls: UiControlState,
    automation: Option<raf_ui::automation::UiAutomationInbox>,
}

impl DirectUiSurfaceHost {
    /// No graphics device: retained UI is produced on the CPU.
    pub fn new(surface: UiSurface, clear_color: [u8; 4]) -> Self {
        Self::with_budget(surface, clear_color, GraphicsMemoryBudget::default())
    }

    pub fn with_budget(
        surface: UiSurface,
        clear_color: [u8; 4],
        budget: GraphicsMemoryBudget,
    ) -> Self {
        let session = UiSurfaceSession::default();
        let images = UiSurfaceImageStore::default();
        let observed_atlas_generation = session.text_atlas.revision();
        let observed_image_generation = images.generation();
        let observed_motion_revision = session.paint_motion_revision();
        let observed_tooltip_revision = session.transient_motion_revision();
        Self {
            surface,
            surface_revision: 0,
            source_paint_revision: 0,
            paint_generation: 1,
            session,
            images,
            clear_color,
            compilation: UiSurfaceCompilationCache::default(),
            textures: UiTextureCache::with_budget(budget),
            last_paint_metrics: UiPaintMetrics::default(),
            last_produced_frame: None,
            observed_atlas_generation,
            observed_image_generation,
            observed_motion_revision,
            observed_tooltip_revision,
            observed_focus: UiFocusState::default(),
            observed_controls: UiControlState::default(),
            automation: None,
        }
    }

    pub fn surface(&self) -> &UiSurface {
        &self.surface
    }

    pub fn surface_mut(&mut self) -> &mut UiSurface {
        self.surface_revision = self.surface_revision.wrapping_add(1).max(1);
        self.source_paint_revision = self.source_paint_revision.wrapping_add(1).max(1);
        self.touch_paint();
        &mut self.surface
    }

    pub fn set_surface(&mut self, surface: UiSurface) {
        if self.surface != surface {
            if let Some(inbox) = self.automation.take() {
                inbox.cancel();
            }
            self.surface = surface;
            self.surface_revision = self.surface_revision.wrapping_add(1).max(1);
            self.source_paint_revision = self.source_paint_revision.wrapping_add(1).max(1);
            self.touch_paint();
        }
    }

    /// Updates a fixed-size text node without invalidating the retained
    /// geometry. Hosts use this for diagnostics and other dynamic labels whose
    /// content cannot change the layout contract.
    pub fn patch_text_value(&mut self, id: &str, value: impl Into<String>) -> bool {
        self.patch_text_value_with_layout(id, value, false)
    }

    pub fn patch_text_value_with_layout(
        &mut self,
        id: &str,
        value: impl Into<String>,
        layout_affects: bool,
    ) -> bool {
        let value = value.into();
        let changed = {
            let Some(node) = self.surface.root.find_mut(id) else {
                return false;
            };
            if node.text_value.as_deref() == Some(value.as_str()) {
                false
            } else {
                node.text_value = Some(value.clone());
                true
            }
        };
        if !changed {
            return false;
        }
        if layout_affects {
            self.surface_revision = self.surface_revision.wrapping_add(1).max(1);
        } else {
            self.compilation.patch_text_value(id, &value);
        }
        self.source_paint_revision = self.source_paint_revision.wrapping_add(1).max(1);
        self.touch_paint();
        true
    }

    /// Applies one typed document update. Paint-only changes refresh the
    /// retained snapshot; visibility, localization keys, image replacement,
    /// and text that can affect intrinsic extent rebuild layout.
    pub fn update_node(
        &mut self,
        id: &str,
        update: UiDocumentUpdate,
    ) -> Result<bool, UiDocumentUpdateError> {
        let impact = update_surface_node(&mut self.surface, id, update)?;
        if impact == UiSurfaceUpdateImpact::NoChange {
            return Ok(false);
        }
        if impact == UiSurfaceUpdateImpact::Layout {
            self.surface_revision = self.surface_revision.wrapping_add(1).max(1);
        } else {
            self.compilation.invalidate_paint_snapshot();
        }
        self.source_paint_revision = self.source_paint_revision.wrapping_add(1).max(1);
        self.touch_paint();
        Ok(true)
    }

    pub fn session(&self) -> &UiSurfaceSession {
        &self.session
    }

    /// Reads already compiled geometry; never triggers layout or rendering.
    pub fn accessibility_frame(&self) -> Option<Arc<UiSurfaceFrame>> {
        self.compilation.cached_frame()
    }

    pub fn automation_inbox(&mut self) -> raf_ui::automation::UiAutomationInbox {
        self.automation
            .get_or_insert_with(Default::default)
            .clone()
    }

    /// Mutable access to transient focus, text, and scroll state. The retained
    /// document stays declarative while platform hosts can restore short-lived
    /// values after a renderer or surface transition.
    pub fn session_mut(&mut self) -> &mut UiSurfaceSession {
        // Arbitrary session mutation can change paint without passing through a
        // typed host mutator, so the produced revision advances conservatively.
        self.touch_paint();
        &mut self.session
    }

    pub fn layout_rect(&self, id: &str) -> Option<raf_ui::UiRect> {
        self.compilation.layout_rect(id)
    }

    /// Returns the latest logical rect for the focused text input. The
    /// platform shell uses it to place native IME candidate windows next to
    /// the caret without making RafUI depend on Winit.
    pub fn focused_text_rect(&self) -> Option<raf_ui::UiRect> {
        let id = self.session.interaction.focus.focused.as_deref()?;
        self.surface.root.find(id)?.control.text_input()?;
        self.layout_rect(id)
    }

    pub fn has_pointer_capture(&self) -> bool {
        self.session.interaction.has_pointer_capture()
    }

    /// True when the latest retained hit-test found an interactive control.
    /// Native hosts use this to shield secondary/middle camera gestures over
    /// toolbars without treating passive hover over transparent chrome as a
    /// capture.
    pub fn has_interactive_hover(&self) -> bool {
        self.session.interaction.focus.hovered.is_some()
    }

    pub fn has_active_motion(&self) -> bool {
        self.session.has_active_motion()
    }

    pub fn has_active_text_repeat(&self) -> bool {
        self.session.has_active_text_repeat()
    }

    pub fn captures_keyboard_input(&self) -> bool {
        self.session.captures_keyboard_input(&self.surface)
    }

    /// Returns the semantic cursor requested by the currently hovered
    /// retained node. Native shells translate this into their platform cursor
    /// without making RafUI depend on Winit.
    pub fn cursor_hint(&self) -> raf_ui::UiCursorIcon {
        self.session.cursor_hint(&self.surface)
    }

    pub fn set_environment(&mut self, environment: UiEnvironment) {
        self.session.set_environment(environment);
        self.touch_paint();
    }

    pub fn images(&self) -> &UiSurfaceImageStore {
        &self.images
    }

    pub fn images_mut(&mut self) -> &mut UiSurfaceImageStore {
        self.touch_paint();
        &mut self.images
    }

    pub fn compilation_metrics(&self) -> UiSurfaceCompileMetrics {
        self.compilation.metrics()
    }

    pub fn last_paint_metrics(&self) -> UiPaintMetrics {
        self.last_paint_metrics
    }

    /// Number of source-to-RGBA texture conversions performed by this host.
    /// Re-appending an unchanged atlas generation or image revision reuses the
    /// cached bytes and leaves this counter unchanged.
    pub fn texture_conversions(&self) -> u64 {
        self.textures.conversions()
    }

    /// Revision of everything that can change the emitted paint.
    ///
    /// Compositors fingerprint it together with the layer rect and raster scale
    /// so an idle frame keeps its prepared packet instead of re-uploading the
    /// scene and atlas. Read it after `append_product_paint` for the exact
    /// revision of the produced commands.
    pub fn paint_revision(&self) -> u64 {
        self.paint_generation
    }

    /// Revision of the retained text atlas pixels. A newly rasterized glyph
    /// always advances it.
    pub fn atlas_generation(&self) -> u64 {
        self.session.text_atlas.revision()
    }

    /// Retained frame and draw list from the latest append, if any.
    pub fn last_produced_frame(&self) -> Option<DirectUiSurfaceFrame> {
        self.last_produced_frame.clone()
    }

    pub fn performance_snapshot(&self) -> raf_ui::UiPerformanceSnapshot {
        let mut snapshot = self.session.performance.snapshot();
        snapshot.append(self.images.performance_snapshot());
        snapshot
    }

    pub fn record_document_work(
        &mut self,
        elapsed: std::time::Duration,
        cause: raf_ui::UiWorkCause,
    ) {
        super::cost_observation::observe(
            &mut self.session.performance,
            &self.surface.id,
            raf_ui::UiWorkPhase::Document,
            cause,
            elapsed,
            self.surface.root.children.len(),
            0,
            false,
        );
    }

    pub fn record_document_region(
        &mut self,
        id: &str,
        elapsed: std::time::Duration,
        nodes: usize,
        reused: bool,
    ) {
        super::cost_observation::observe(
            &mut self.session.performance,
            id,
            raf_ui::UiWorkPhase::Document,
            raf_ui::UiWorkCause::Document,
            elapsed,
            nodes,
            0,
            reused,
        );
    }

    pub fn build_frame<F>(&mut self, size: [u32; 2], resolve: F) -> UiSurfaceFrame
    where
        F: FnMut(&str) -> String,
    {
        self.compilation.invalidate();
        let frame = self.session.build_frame_with_resolved_text(
            &self.surface,
            size[0].max(1),
            size[1].max(1),
            self.clear_color,
            resolve,
        );
        self.sync_observed_generations();
        self.sync_observed_interaction();
        frame
    }

    pub fn process_input<F>(
        &mut self,
        size: [u32; 2],
        resolve: F,
        input: &UiInputState,
    ) -> Vec<UiDispatchedAction>
    where
        F: FnMut(&str) -> String,
    {
        self.process_input_at_scale(size, 1.0, resolve, input)
    }

    /// Processes pointer input in logical surface points while keeping the
    /// text atlas at the physical density of the current presentation target.
    pub fn process_input_at_scale<F>(
        &mut self,
        size: [u32; 2],
        raster_scale: f32,
        _resolve: F,
        input: &UiInputState,
    ) -> Vec<UiDispatchedAction>
    where
        F: FnMut(&str) -> String,
    {
        let frame = self.compilation.layout(
            &self.surface,
            &mut self.session,
            self.surface_revision,
            size,
            raster_scale,
            self.clear_color,
        );
        let mut actions = self
            .automation
            .as_ref()
            .map(|inbox| inbox.drain(&self.surface.root, &mut self.session.interaction))
            .unwrap_or_default();
        actions.extend(self.session.process_input(&self.surface, &frame, input));
        self.sync_observed_interaction();
        actions
    }

    /// Routes one native snapshot through the shared ownership arbiter before
    /// RafUI sees it. A passive hover never mutates `InputRouter`; primary
    /// capture begins only after the retained surface accepted a real press.
    pub fn process_routed_input<F>(
        &mut self,
        size: [u32; 2],
        raster_scale: f32,
        resolve: F,
        native_input: &NativeUiInputBridge,
        router: &mut InputRouter,
        owner: InputOwner,
        window_rect: raf_ui::UiRect,
    ) -> Vec<UiDispatchedAction>
    where
        F: FnMut(&str) -> String,
    {
        let input = native_input.ui_state_for_owner(window_rect, owner, router);
        let actions = self.process_input_at_scale(size, raster_scale, resolve, &input);
        for action in &actions {
            if let raf_ui::UiAction::SetClipboard { text } = &action.action {
                native_input.write_clipboard(text);
            }
        }
        let snapshot = native_input.snapshot();

        if self.has_pointer_capture() && snapshot.button_down(PointerButton::Primary) {
            let origin = snapshot.pointer_position.unwrap_or([
                window_rect.x + window_rect.width * 0.5,
                window_rect.y + window_rect.height * 0.5,
            ]);
            let mode = if matches!(owner, InputOwner::Modal(_) | InputOwner::DragDrop(_)) {
                CaptureMode::Exclusive
            } else {
                CaptureMode::PerButton
            };
            let _ = router.try_capture_pointer(
                PointerButton::Primary,
                owner,
                mode,
                origin,
                snapshot.time_seconds,
            );
        }

        if self.has_interactive_hover() {
            for button in [PointerButton::Secondary, PointerButton::Middle] {
                if snapshot.button_pressed(button) {
                    let _ = router.try_capture_pointer(
                        button,
                        owner,
                        CaptureMode::PerButton,
                        snapshot
                            .pointer_position
                            .unwrap_or([window_rect.x, window_rect.y]),
                        snapshot.time_seconds,
                    );
                }
            }
        }

        if self.captures_keyboard_input() {
            let _ = router.try_capture_keyboard(owner);
        } else {
            let _ = router.release_keyboard(owner);
        }
        actions
    }

    /// Compiles if needed, syncs the text atlas, uploads texture pixels into
    /// `packet`, and appends paint commands in retained order.
    ///
    /// `target_size` is the full physical target, `target_rect` is the physical
    /// pixel rectangle where this layer draws, and `raster_scale` is the density
    /// used for glyph rasterization. The clear color belongs to the compositor;
    /// this method never emits `Command::Clear`.
    pub fn append_product_paint(
        &mut self,
        packet: &mut raf_agb::product::ProductPacket,
        target_size: [u32; 2],
        target_rect: [f32; 4],
        logical_size: [u32; 2],
        raster_scale: f32,
        resolve: impl FnMut(&str) -> String,
    ) -> Result<UiPaintMetrics, String> {
        self.produce_paint(
            packet,
            target_size,
            target_rect,
            logical_size,
            raster_scale,
            resolve,
        )
        .map(|produced| produced.metrics)
    }

    /// [`Self::append_product_paint`] plus the retained frame it produced.
    pub fn append_product_paint_frame(
        &mut self,
        packet: &mut raf_agb::product::ProductPacket,
        target_size: [u32; 2],
        target_rect: [f32; 4],
        logical_size: [u32; 2],
        raster_scale: f32,
        resolve: impl FnMut(&str) -> String,
    ) -> Result<DirectUiSurfaceFrame, String> {
        self.produce_paint(
            packet,
            target_size,
            target_rect,
            logical_size,
            raster_scale,
            resolve,
        )
    }

    fn produce_paint(
        &mut self,
        packet: &mut raf_agb::product::ProductPacket,
        target_size: [u32; 2],
        target_rect: [f32; 4],
        logical_size: [u32; 2],
        raster_scale: f32,
        mut resolve: impl FnMut(&str) -> String,
    ) -> Result<DirectUiSurfaceFrame, String> {
        let raster_scale = raster_scale.clamp(1.0, 4.0);
        if target_rect[2] <= 0.0 || target_rect[3] <= 0.0 {
            // A collapsed layer paints nothing. Layout still has to stay
            // current for hit testing and accessibility on the next frame.
            let frame = self.compilation.layout(
                &self.surface,
                &mut self.session,
                self.surface_revision,
                logical_size,
                raster_scale,
                self.clear_color,
            );
            let produced = DirectUiSurfaceFrame {
                frame,
                draw_list: Arc::new(UiSurfaceDrawList::default()),
                metrics: UiPaintMetrics::default(),
                compilation: self.compilation.metrics(),
            };
            self.last_produced_frame = Some(produced.clone());
            return Ok(produced);
        }
        let compiled = self.compilation.compile_with_paint_revision(
            &self.surface,
            &mut self.session,
            self.surface_revision,
            logical_size,
            raster_scale,
            self.clear_color,
            self.source_paint_revision,
            &mut resolve,
        );
        for quad in &compiled.draw_list.images {
            self.images.ensure_builtin_key(&quad.source_key);
        }
        let submission_started = std::time::Instant::now();
        let metrics = append_paint(
            packet,
            &compiled.draw_list,
            &self.session.text_atlas,
            &self.images,
            &mut self.textures,
            &self.surface.id,
            target_size,
            target_rect,
            logical_size,
            raster_scale,
        )?;
        super::cost_observation::observe(
            &mut self.session.performance,
            &self.surface.id,
            raf_ui::UiWorkPhase::Submission,
            raf_ui::UiWorkCause::Resources,
            submission_started.elapsed(),
            0,
            metrics.upload_bytes(),
            self.compilation.metrics().paint_cache_hit,
        );
        self.last_paint_metrics = metrics;
        self.sync_observed_generations();
        self.sync_observed_interaction();
        let produced = DirectUiSurfaceFrame {
            frame: compiled.frame,
            draw_list: compiled.draw_list,
            metrics,
            compilation: self.compilation.metrics(),
        };
        self.last_produced_frame = Some(produced.clone());
        Ok(produced)
    }

    /// Renders a logical retained layout into a CPU recovery texture.
    pub fn rasterize_cpu_texture(
        &mut self,
        size: [u32; 2],
        renderer: &mut super::UiSurfaceCpuRenderer,
        mut resolve: impl FnMut(&str) -> String,
    ) -> Vec<u8> {
        let compiled = self.compilation.compile_with_paint_revision(
            &self.surface,
            &mut self.session,
            self.surface_revision,
            size,
            1.0,
            self.clear_color,
            self.source_paint_revision,
            &mut resolve,
        );
        for quad in &compiled.draw_list.images {
            self.images.ensure_builtin_key(&quad.source_key);
        }
        renderer.render(
            &compiled.draw_list,
            &self.session.text_atlas,
            &self.images,
            size,
            self.clear_color,
        );
        self.sync_observed_generations();
        renderer.pixels().to_vec()
    }

    fn touch_paint(&mut self) {
        self.paint_generation = self.paint_generation.wrapping_add(1).max(1);
    }

    /// Folds state that can change without a typed host mutator into the
    /// produced revision: atlas growth and image residency changes.
    fn sync_observed_generations(&mut self) {
        let atlas = self.session.text_atlas.revision();
        if atlas != self.observed_atlas_generation {
            self.observed_atlas_generation = atlas;
            self.touch_paint();
        }
        let images = self.images.generation();
        if images != self.observed_image_generation {
            self.observed_image_generation = images;
            self.touch_paint();
        }
    }

    /// Interaction updates focus, hover, control values and caret state. Only a
    /// real change advances the produced revision, so an event-free frame keeps
    /// its prepared packet.
    fn sync_observed_interaction(&mut self) {
        let focus_changed = self.observed_focus != self.session.interaction.focus;
        let controls_changed = self.observed_controls != self.session.interaction.controls;
        let motion = self.session.paint_motion_revision();
        let tooltip = self.session.transient_motion_revision();
        let motion_changed =
            motion != self.observed_motion_revision || tooltip != self.observed_tooltip_revision;
        if focus_changed || controls_changed || motion_changed {
            if focus_changed || controls_changed {
                self.observed_focus = self.session.interaction.focus.clone();
                self.observed_controls = self.session.interaction.controls.clone();
            }
            self.observed_motion_revision = motion;
            self.observed_tooltip_revision = tooltip;
            self.touch_paint();
        }
    }
}

#[derive(Debug, Clone)]
pub struct DirectUiSurfaceFrame {
    pub frame: Arc<UiSurfaceFrame>,
    pub draw_list: Arc<UiSurfaceDrawList>,
    pub metrics: UiPaintMetrics,
    pub compilation: UiSurfaceCompileMetrics,
}

impl DirectUiSurfaceFrame {
    pub fn diagnostics(&self) -> UiSurfaceDiagnostics {
        UiSurfaceDiagnostics::from_frame(&self.frame)
    }
}
