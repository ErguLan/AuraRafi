//! Shared retained-UI compilation cache.
//!
//! Layout, hit-testing, atlas synchronization, and paint-list construction
//! have distinct invalidation needs. Keeping them together here prevents the
//! CPU fallback and direct GPU host from accidentally rebuilding the same UI
//! representation every frame.

use raf_ui::{UiWorkCause, UiWorkPhase};
use std::sync::Arc;
use std::time::Instant;

use super::paint_motion::{layout_control_snapshot, UiSurfacePaintStateKey};
use super::{UiControlState, UiSurface, UiSurfaceDrawList, UiSurfaceFrame, UiSurfaceSession};
use raf_ui::{UiDocument, UiDocumentUpdate, UiDocumentUpdateError, UiNode, UiSizeMode};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum UiSurfaceUpdateImpact {
    NoChange,
    PaintOnly,
    Layout,
}

pub(super) fn update_surface_node(
    surface: &mut UiSurface,
    id: &str,
    update: UiDocumentUpdate,
) -> Result<UiSurfaceUpdateImpact, UiDocumentUpdateError> {
    let original = surface
        .root
        .find(id)
        .cloned()
        .ok_or_else(|| UiDocumentUpdateError::NodeNotFound(id.to_string()))?;
    let layout_affects = match &update {
        UiDocumentUpdate::Visible(_)
        | UiDocumentUpdate::TextKey(_)
        | UiDocumentUpdate::Image(_) => true,
        UiDocumentUpdate::TextValue(_) => !text_value_geometry_is_stable(&surface.root, id),
        UiDocumentUpdate::Enabled(_) | UiDocumentUpdate::ImageOptions(_) => false,
    };

    // Reuse the document's validation and update semantics without cloning or
    // mutating unrelated siblings in the retained surface.
    let mut document = UiDocument::blank(surface.id.clone());
    document.root = original.clone();
    document.update_node(id, update)?;
    if document.root == original {
        return Ok(UiSurfaceUpdateImpact::NoChange);
    }
    let Some(target) = surface.root.find_mut(id) else {
        return Err(UiDocumentUpdateError::NodeNotFound(id.to_string()));
    };
    *target = document.root;
    Ok(if layout_affects {
        UiSurfaceUpdateImpact::Layout
    } else {
        UiSurfaceUpdateImpact::PaintOnly
    })
}

pub(super) fn text_value_geometry_is_stable(root: &UiNode, target_id: &str) -> bool {
    fn stable_mode(mode: UiSizeMode) -> bool {
        matches!(mode, UiSizeMode::Fixed | UiSizeMode::Fill)
    }

    fn visit(
        node: &UiNode,
        target_id: &str,
        ancestors_stable: bool,
        is_root: bool,
    ) -> Option<bool> {
        let stable = ancestors_stable
            && (is_root
                || (stable_mode(node.layout.width_mode) && stable_mode(node.layout.height_mode)));
        if node.id == target_id {
            return Some(stable);
        }
        node.children
            .iter()
            .find_map(|child| visit(child, target_id, stable, false))
    }

    visit(root, target_id, true, true).unwrap_or(false)
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct UiSurfaceCompileMetrics {
    pub layout_cache_hit: bool,
    pub paint_cache_hit: bool,
    pub layout_builds: u64,
    pub paint_builds: u64,
}

#[derive(Debug, Clone, PartialEq)]
struct UiSurfaceLayoutKey {
    surface_revision: u64,
    controls: UiControlState,
    logical_size: [u32; 2],
    raster_scale_bits: u32,
    text_scale_bits: u32,
    theme_experimental_bits: u32,
    clear_color: [u8; 4],
    high_contrast: bool,
    reduce_transparency: bool,
}

impl UiSurfaceLayoutKey {
    fn new(
        surface: &UiSurface,
        surface_revision: u64,
        session: &UiSurfaceSession,
        logical_size: [u32; 2],
        raster_scale: f32,
        clear_color: [u8; 4],
    ) -> Self {
        Self {
            surface_revision,
            controls: layout_control_snapshot(surface, &session.interaction.controls),
            logical_size: [logical_size[0].max(1), logical_size[1].max(1)],
            raster_scale_bits: raster_scale.clamp(1.0, 4.0).to_bits(),
            text_scale_bits: session.text_scale().to_bits(),
            theme_experimental_bits: session.environment_theme_experimental().to_bits(),
            clear_color,
            high_contrast: session.high_contrast(),
            reduce_transparency: session.reduce_transparency(),
        }
    }
}

struct CachedLayout {
    key: UiSurfaceLayoutKey,
    paint_state_key: UiSurfacePaintStateKey,
    revision: u64,
    frame: Arc<UiSurfaceFrame>,
}

struct CachedPaint {
    layout_revision: u64,
    paint_revision: u64,
    atlas_revision: u64,
    draw_list_revision: u64,
    resolved_text: Vec<String>,
    frame: Arc<UiSurfaceFrame>,
    draw_list: Arc<UiSurfaceDrawList>,
}

#[derive(Clone)]
pub(super) struct UiSurfaceCompiledFrame {
    pub frame: Arc<UiSurfaceFrame>,
    pub draw_list: Arc<UiSurfaceDrawList>,
    pub draw_list_revision: u64,
}

/// Cache owned by a surface host. It is deliberately renderer-agnostic, so
/// GPU-first presentation and CPU recovery mode compile the exact same data.
#[derive(Default)]
pub(super) struct UiSurfaceCompilationCache {
    regional_paint: super::regional_paint::UiRegionalPaintCache,
    layout: Option<CachedLayout>,
    paint: Option<CachedPaint>,
    next_layout_revision: u64,
    next_draw_list_revision: u64,
    paint_only_invalidated: bool,
    paint_snapshot_invalidated: bool,
    intrinsic_fit_pending: bool,
    metrics: UiSurfaceCompileMetrics,
}

impl UiSurfaceCompilationCache {
    pub fn invalidate(&mut self) {
        self.regional_paint.clear();
        self.layout = None;
        self.paint = None;
        self.paint_only_invalidated = false;
        self.paint_snapshot_invalidated = false;
        self.intrinsic_fit_pending = false;
    }

    pub fn metrics(&self) -> UiSurfaceCompileMetrics {
        self.metrics
    }

    pub fn cached_frame(&self) -> Option<Arc<UiSurfaceFrame>> {
        self.paint.as_ref().map(|cache| Arc::clone(&cache.frame))
            .or_else(|| self.layout.as_ref().map(|cache| Arc::clone(&cache.frame)))
    }

    pub fn layout_rect(&self, id: &str) -> Option<raf_ui::UiRect> {
        self.layout.as_ref().and_then(|cached| {
            cached
                .frame
                .layout_boxes
                .iter()
                .find(|layout| layout.id == id)
                .map(|layout| layout.rect)
        })
    }

    pub fn layout(
        &mut self,
        surface: &UiSurface,
        session: &mut UiSurfaceSession,
        surface_revision: u64,
        logical_size: [u32; 2],
        raster_scale: f32,
        clear_color: [u8; 4],
    ) -> Arc<UiSurfaceFrame> {
        let key = UiSurfaceLayoutKey::new(
            surface,
            surface_revision,
            session,
            logical_size,
            raster_scale,
            clear_color,
        );
        self.metrics.layout_cache_hit =
            self.layout.as_ref().is_some_and(|cached| cached.key == key);
        self.metrics.paint_cache_hit = false;
        if self.layout.as_ref().is_some_and(|cached| cached.key == key) {
            let paint_state_key = UiSurfacePaintStateKey::new(surface, session);
            let needs_paint_refresh = self.paint_snapshot_invalidated
                || self
                    .layout
                    .as_ref()
                    .is_some_and(|cached| cached.paint_state_key != paint_state_key);
            if needs_paint_refresh {
                let retained = self
                    .layout
                    .as_ref()
                    .map(|cached| Arc::clone(&cached.frame))
                    .expect("layout cache exists after a layout-key hit");
                let frame = Arc::new(session.refresh_paint_frame(surface, &retained));
                let paint_state_key = UiSurfacePaintStateKey::new(surface, session);
                if let Some(cached) = self.layout.as_mut() {
                    cached.frame = Arc::clone(&frame);
                    cached.paint_state_key = paint_state_key;
                }
                self.paint = None;
                self.paint_only_invalidated = true;
                self.paint_snapshot_invalidated = false;
                return frame;
            }
            return Arc::clone(
                &self
                    .layout
                    .as_ref()
                    .expect("layout cache exists after a layout-key hit")
                    .frame,
            );
        }

        let frame = Arc::new(session.build_layout_frame_at_scale(
            surface,
            key.logical_size[0],
            key.logical_size[1],
            clear_color,
            f32::from_bits(key.raster_scale_bits),
        ));
        self.next_layout_revision = self.next_layout_revision.wrapping_add(1).max(1);
        self.metrics.layout_builds = self.metrics.layout_builds.saturating_add(1);
        self.layout = Some(CachedLayout {
            key,
            paint_state_key: UiSurfacePaintStateKey::new(surface, session),
            revision: self.next_layout_revision,
            frame: Arc::clone(&frame),
        });
        self.paint = None;
        self.paint_snapshot_invalidated = false;
        self.intrinsic_fit_pending = true;
        frame
    }

    pub fn patch_text_value(&mut self, id: &str, value: &str) {
        self.paint = None;
        if let Some(cached) = self.layout.as_mut() {
            let frame = Arc::make_mut(&mut cached.frame);
            if let Some(layout) = frame.layout_boxes.iter_mut().find(|layout| layout.id == id) {
                layout.text_value = Some(value.to_string());
                if layout.text_key.is_none() {
                    if let Some(request) = frame.text_requests.iter_mut().find(|r| r.node_id == id)
                    {
                        request.text_key = value.to_string();
                    }
                }
            }
        }
        self.paint_only_invalidated = true;
    }

    pub fn invalidate_paint_snapshot(&mut self) {
        self.paint = None;
        self.paint_only_invalidated = true;
        self.paint_snapshot_invalidated = true;
    }

    pub fn compile<F>(
        &mut self,
        surface: &UiSurface,
        session: &mut UiSurfaceSession,
        surface_revision: u64,
        logical_size: [u32; 2],
        raster_scale: f32,
        clear_color: [u8; 4],
        resolve: F,
    ) -> UiSurfaceCompiledFrame
    where
        F: FnMut(&str) -> String,
    {
        self.compile_with_paint_revision(
            surface,
            session,
            surface_revision,
            logical_size,
            raster_scale,
            clear_color,
            0,
            resolve,
        )
    }

    pub fn compile_with_paint_revision<F>(
        &mut self,
        surface: &UiSurface,
        session: &mut UiSurfaceSession,
        surface_revision: u64,
        logical_size: [u32; 2],
        raster_scale: f32,
        clear_color: [u8; 4],
        paint_revision: u64,
        mut resolve: F,
    ) -> UiSurfaceCompiledFrame
    where
        F: FnMut(&str) -> String,
    {
        let cached_frame = self.layout(
            surface,
            session,
            surface_revision,
            logical_size,
            raster_scale,
            clear_color,
        );
        let layout_revision = self
            .layout
            .as_ref()
            .expect("layout cache exists after compilation")
            .revision;
        let paint_only_invalidated = std::mem::take(&mut self.paint_only_invalidated);

        // A retained frame already contains the resolved values and atlas
        // revision that produced its paint list. Avoid walking every text
        // request, hashing every string, and touching the atlas again when
        // another editor surface merely asks for an idle present.
        if let Some(cached) = self.paint.as_ref().filter(|cached| {
            cached.layout_revision == layout_revision
                && cached.paint_revision == paint_revision
                && cached.atlas_revision == session.text_atlas.revision()
        }) {
            self.metrics.paint_cache_hit = true;
            return UiSurfaceCompiledFrame {
                frame: Arc::clone(&cached.frame),
                draw_list: Arc::clone(&cached.draw_list),
                draw_list_revision: cached.draw_list_revision,
            };
        }

        let needs_intrinsic_fit = cached_frame.layout_boxes.iter().any(|layout| {
            matches!(
                layout.width_mode,
                raf_ui::UiSizeMode::FitContent
                    | raf_ui::UiSizeMode::MinContent
                    | raf_ui::UiSizeMode::MaxContent
            ) || matches!(
                layout.height_mode,
                raf_ui::UiSizeMode::FitContent
                    | raf_ui::UiSizeMode::MinContent
                    | raf_ui::UiSizeMode::MaxContent
            )
        });
        let text_started = Instant::now();
        let atlas_before = session.text_atlas.revision();
        let resolved_text = { session.resolve_text_requests(&cached_frame, |key| resolve(key)) };
        session.sync_resolved_text(&cached_frame, &resolved_text);
        super::cost_observation::observe(
            &mut session.performance,
            &surface.id,
            UiWorkPhase::Text,
            UiWorkCause::Text,
            text_started.elapsed(),
            cached_frame.text_requests.len(),
            0,
            session.text_atlas.revision() == atlas_before,
        );
        let atlas_revision = session.text_atlas.revision();
        self.metrics.paint_cache_hit = self.paint.as_ref().is_some_and(|cached| {
            cached.layout_revision == layout_revision
                && cached.paint_revision == paint_revision
                && cached.atlas_revision == atlas_revision
                && cached.resolved_text == resolved_text
        });
        if self.metrics.paint_cache_hit {
            let cached = self
                .paint
                .as_ref()
                .expect("paint cache exists after a cache hit");
            return UiSurfaceCompiledFrame {
                frame: Arc::clone(&cached.frame),
                draw_list: Arc::clone(&cached.draw_list),
                draw_list_revision: cached.draw_list_revision,
            };
        }
        let frame =
            if needs_intrinsic_fit && (self.intrinsic_fit_pending || !paint_only_invalidated) {
                let intrinsic_sizes =
                    session.intrinsic_sizes_for_frame(&cached_frame, &resolved_text, raster_scale);
                let frame = Arc::new(session.rebuild_layout_with_intrinsic_sizes(
                    surface,
                    logical_size[0],
                    logical_size[1],
                    clear_color,
                    raster_scale,
                    &intrinsic_sizes,
                ));
                // Input is processed before presentation on the next native
                // frame. Keep the measured layout, hit regions and scroll metrics
                // together so interaction never falls back to the provisional
                // pre-text-fit extent while the renderer shows the intrinsic one.
                if let Some(cached) = self.layout.as_mut() {
                    cached.frame = Arc::clone(&frame);
                    cached.paint_state_key = UiSurfacePaintStateKey::new(surface, session);
                }
                frame
            } else {
                Arc::clone(&cached_frame)
            };
        self.intrinsic_fit_pending = false;
        let (draw_list, draw_list_revision) = if let Some(cached) =
            self.paint.as_ref().filter(|cached| {
                cached.layout_revision == layout_revision
                    && cached.paint_revision == paint_revision
                    && cached.atlas_revision == atlas_revision
                    && cached.resolved_text == resolved_text
            }) {
            (Arc::clone(&cached.draw_list), cached.draw_list_revision)
        } else {
            let paint_started = Instant::now();
            session.sync_resolved_text(&frame, &resolved_text);
            let draw_list = Arc::new(self.regional_paint.build(
                surface,
                &frame,
                &session.text_atlas,
                raster_scale,
                &resolved_text,
                &mut session.performance,
            ));
            super::cost_observation::observe(
                &mut session.performance,
                &surface.id,
                UiWorkPhase::Paint,
                if paint_only_invalidated {
                    UiWorkCause::Visual
                } else {
                    UiWorkCause::Document
                },
                paint_started.elapsed(),
                frame.layout_boxes.len(),
                0,
                false,
            );
            self.metrics.paint_builds = self.metrics.paint_builds.saturating_add(1);
            self.next_draw_list_revision = self.next_draw_list_revision.wrapping_add(1).max(1);
            let draw_list_revision = self.next_draw_list_revision;
            self.paint = Some(CachedPaint {
                layout_revision,
                paint_revision,
                atlas_revision: session.text_atlas.revision(),
                draw_list_revision,
                resolved_text,
                frame: Arc::clone(&frame),
                draw_list: Arc::clone(&draw_list),
            });
            (draw_list, draw_list_revision)
        };

        UiSurfaceCompiledFrame {
            frame,
            draw_list,
            draw_list_revision,
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::*;
    use crate::api_graphic_basic::ui_surface::{
        StudioUiPalette, UiFlow, UiInputState, UiLayout, UiNode, UiNodeKind, UiScrollAxis,
        UiSizeMode,
    };

    #[test]
    fn rafui_regression_hidpi_hover_preserves_text_metrics_and_atlas_slots() {
        for scale in [1.25, 1.5, 2.0] {
            let mut surface = UiSurface::new(
                "hidpi-hover",
                StudioUiPalette::IndustrialDark,
                UiNode::new("root", UiNodeKind::Root)
                    .with_layout(UiLayout::fill(UiFlow::Row))
                    .with_child(
                        UiNode::new("action", UiNodeKind::Button)
                            .with_text_value("3D")
                            .with_icon(raf_ui::UiIcon::new(raf_ui::UiIconId::Cube))
                            .with_layout(UiLayout::fit_content().fixed_height(30.0))
                            .focusable(),
                    ),
            );
            surface.retained_tooltips = false;
            let mut session = UiSurfaceSession::default();
            let mut cache = UiSurfaceCompilationCache::default();
            let original = cache.compile(
                &surface,
                &mut session,
                0,
                [320, 100],
                scale,
                [0; 4],
                str::to_owned,
            );
            let request = original.frame.text_requests[0].clone();
            let atlas_revision = session.text_atlas.revision();
            for hovered in [true, false, true, false] {
                session.interaction.focus.hovered = hovered.then(|| "action".to_owned());
                let refreshed = cache.compile(
                    &surface,
                    &mut session,
                    0,
                    [320, 100],
                    scale,
                    [0; 4],
                    str::to_owned,
                );
                let current = &refreshed.frame.text_requests[0];
                assert_eq!(current.max_width, request.max_width);
                assert_eq!(current.style.size_px, request.style.size_px);
                assert_eq!(current.style.line_height_px, request.style.line_height_px);
                assert_eq!(
                    refreshed.frame.layout_boxes[1].rect,
                    original.frame.layout_boxes[1].rect
                );
                assert_eq!(session.text_atlas.revision(), atlas_revision);
            }
            assert_eq!(cache.metrics().layout_builds, 1);
        }
    }

    #[test]
    fn rafui_regression_input_layout_does_not_skip_intrinsic_fit_after_hover() {
        let mut surface = UiSurface::new(
            "input-before-paint",
            StudioUiPalette::IndustrialDark,
            UiNode::new("root", UiNodeKind::Root)
                .with_layout(UiLayout::fill(UiFlow::Row))
                .with_child(
                    UiNode::new("label", UiNodeKind::Button)
                        .with_text_value("A translated action with a long natural label")
                        .with_layout(UiLayout::fit_content())
                        .focusable(),
                ),
        );
        surface.retained_tooltips = false;
        let mut session = UiSurfaceSession::default();
        let mut cache = UiSurfaceCompilationCache::default();
        let original = cache.compile(
            &surface,
            &mut session,
            0,
            [600, 100],
            1.25,
            [0; 4],
            str::to_owned,
        );
        session.interaction.focus.hovered = Some("label".into());
        // Input can refresh paint, then build another provisional layout
        // before presentation. That must not consume the pending measured fit.
        let _ = cache.layout(&surface, &mut session, 0, [600, 100], 1.25, [0; 4]);
        let _ = cache.layout(&surface, &mut session, 1, [601, 100], 1.25, [0; 4]);
        let compiled = cache.compile(
            &surface,
            &mut session,
            1,
            [601, 100],
            1.25,
            [0; 4],
            str::to_owned,
        );
        assert_eq!(
            compiled.frame.layout_boxes[1].rect.width,
            original.frame.layout_boxes[1].rect.width
        );
        assert!(!cache.intrinsic_fit_pending);
    }

    #[test]
    fn rafui_regression_diagnostic_patch_keeps_other_text_requests_unchanged() {
        let surface = UiSurface::new(
            "diagnostics",
            StudioUiPalette::IndustrialDark,
            UiNode::new("root", UiNodeKind::Root)
                .with_layout(UiLayout::fill(UiFlow::Column))
                .with_child(
                    UiNode::new("label", UiNodeKind::Label)
                        .with_text_value("Stable toolbar")
                        .with_layout(UiLayout::fit_content()),
                )
                .with_child(
                    UiNode::new("fps", UiNodeKind::Label)
                        .with_text_value("FPS: 60")
                        .with_layout(UiLayout::fixed(180.0, 24.0)),
                ),
        );
        let mut session = UiSurfaceSession::default();
        let mut cache = UiSurfaceCompilationCache::default();
        let original = cache.compile(
            &surface,
            &mut session,
            0,
            [320, 100],
            1.5,
            [0; 4],
            str::to_owned,
        );
        cache.patch_text_value("fps", "FPS: 59");
        let compiled = cache.compile(
            &surface,
            &mut session,
            0,
            [320, 100],
            1.5,
            [0; 4],
            str::to_owned,
        );
        assert_eq!(
            compiled.frame.text_requests[0],
            original.frame.text_requests[0]
        );
        assert_eq!(
            compiled.frame.layout_boxes,
            original
                .frame
                .layout_boxes
                .iter()
                .cloned()
                .map(|mut b| {
                    if b.id == "fps" {
                        b.text_value = Some("FPS: 59".into());
                    }
                    b
                })
                .collect::<Vec<_>>()
        );
        assert_eq!(
            session.resolve_text_requests(&compiled.frame, str::to_owned),
            ["Stable toolbar", "FPS: 59"]
        );
        assert_eq!(compiled.draw_list.text.len(), 2);
        assert_eq!(cache.metrics().layout_builds, 1);
    }

    #[test]
    fn reuses_layout_and_paint_for_an_unchanged_surface() {
        let surface = UiSurface::new(
            "cache",
            StudioUiPalette::IndustrialDark,
            UiNode::new("root", UiNodeKind::Root).with_child(
                UiNode::new("label", UiNodeKind::Label)
                    .with_text_key("label")
                    .with_layout(UiLayout::fit_content()),
            ),
        );
        let mut session = UiSurfaceSession::default();
        let mut cache = UiSurfaceCompilationCache::default();

        let first = cache.compile(
            &surface,
            &mut session,
            0,
            [320, 120],
            1.0,
            [0, 0, 0, 255],
            |key| key.to_string(),
        );
        let second = cache.compile(
            &surface,
            &mut session,
            0,
            [320, 120],
            1.0,
            [0, 0, 0, 255],
            |key| key.to_string(),
        );

        assert!(Arc::ptr_eq(&first.frame, &second.frame));
        assert!(Arc::ptr_eq(&first.draw_list, &second.draw_list));
        assert!(cache.metrics().layout_cache_hit);
        assert!(cache.metrics().paint_cache_hit);
        assert_eq!(cache.metrics().layout_builds, 1);
        assert_eq!(cache.metrics().paint_builds, 1);
    }

    #[test]
    fn hover_and_enablement_refresh_paint_without_rebuilding_geometry() {
        let mut surface = UiSurface::new(
            "paint-state",
            StudioUiPalette::IndustrialDark,
            UiNode::new("root", UiNodeKind::Root).with_child(
                UiNode::new("button", UiNodeKind::Button)
                    .with_layout(UiLayout::fixed(100.0, 28.0))
                    .with_text_value("Action")
                    .focusable(),
            ),
        );
        surface.retained_tooltips = false;
        let mut session = UiSurfaceSession::default();
        let mut cache = UiSurfaceCompilationCache::default();
        let original = cache.compile(
            &surface,
            &mut session,
            0,
            [200, 100],
            1.0,
            [0; 4],
            str::to_owned,
        );
        session.interaction.focus.focused = Some("button".into());
        session.interaction.focus.hovered = Some("button".into());
        let hovered = cache.compile(
            &surface,
            &mut session,
            0,
            [200, 100],
            1.0,
            [0; 4],
            str::to_owned,
        );
        assert_eq!(cache.metrics().layout_builds, 1);
        assert!(!Arc::ptr_eq(&original.draw_list, &hovered.draw_list));
        assert!(session.has_active_motion());

        assert_eq!(
            update_surface_node(&mut surface, "button", UiDocumentUpdate::Enabled(false)).unwrap(),
            UiSurfaceUpdateImpact::PaintOnly,
        );
        cache.invalidate_paint_snapshot();
        let disabled = cache.compile(
            &surface,
            &mut session,
            0,
            [200, 100],
            1.0,
            [0; 4],
            str::to_owned,
        );
        assert_eq!(cache.metrics().layout_builds, 1);
        assert!(session.interaction.focus.focused.is_none());
        let button = disabled
            .frame
            .layout_boxes
            .iter()
            .find(|node| node.id == "button")
            .unwrap();
        assert!(button.disabled);
        assert!(!button.focused);
    }

    #[test]
    fn surface_revision_invalidates_without_cloning_the_document_into_the_key() {
        let surface = UiSurface::new(
            "cache-revision",
            StudioUiPalette::IndustrialDark,
            UiNode::new("root", UiNodeKind::Root),
        );
        let mut session = UiSurfaceSession::default();
        let mut cache = UiSurfaceCompilationCache::default();

        let first = cache.compile(
            &surface,
            &mut session,
            0,
            [320, 120],
            1.0,
            [0, 0, 0, 255],
            |key| key.to_string(),
        );
        let second = cache.compile(
            &surface,
            &mut session,
            1,
            [320, 120],
            1.0,
            [0, 0, 0, 255],
            |key| key.to_string(),
        );

        assert!(!Arc::ptr_eq(&first.frame, &second.frame));
        assert_eq!(cache.metrics().layout_builds, 2);
    }

    #[test]
    fn fixed_text_patch_reuses_layout_and_rebuilds_only_paint() {
        let surface = UiSurface::new(
            "paint-only",
            StudioUiPalette::IndustrialDark,
            UiNode::new("root", UiNodeKind::Root).with_child(
                UiNode::new("value", UiNodeKind::Label)
                    .with_text_value("before")
                    .with_layout(UiLayout::fixed(180.0, 24.0)),
            ),
        );
        let mut session = UiSurfaceSession::default();
        let mut cache = UiSurfaceCompilationCache::default();

        let _ = cache.compile(
            &surface,
            &mut session,
            0,
            [320, 120],
            1.0,
            [0, 0, 0, 255],
            |key| key.to_string(),
        );
        cache.patch_text_value("value", "after");
        let patched = cache.compile_with_paint_revision(
            &surface,
            &mut session,
            0,
            [320, 120],
            1.0,
            [0, 0, 0, 255],
            1,
            |key| key.to_string(),
        );

        assert_eq!(cache.metrics().layout_builds, 1);
        assert_eq!(cache.metrics().paint_builds, 2);
        assert_eq!(
            patched
                .frame
                .layout_boxes
                .iter()
                .find(|layout| layout.id == "value")
                .and_then(|layout| layout.text_value.as_deref()),
            Some("after")
        );
    }

    #[test]
    fn intrinsic_scroll_extent_remains_authoritative_for_next_input_frame() {
        let mut list = UiNode::scroll_view("list", UiScrollAxis::Vertical).with_layout(UiLayout {
            flow: UiFlow::Column,
            ..UiLayout::fixed(180.0, 80.0)
        });
        for index in 0..8 {
            list = list.with_child(
                UiNode::new(format!("item-{index}"), UiNodeKind::Label)
                    .with_text_value(
                        "A long wrapped row that must contribute its measured height to the scroll extent.",
                    )
                    .with_layout(UiLayout::fit_content().with_width_mode(UiSizeMode::Fill)),
            );
        }
        let surface = UiSurface::new(
            "intrinsic-scroll-cache",
            StudioUiPalette::IndustrialDark,
            UiNode::new("root", UiNodeKind::Root)
                .with_layout(UiLayout::fill(UiFlow::Column))
                .with_child(list),
        );
        let mut session = UiSurfaceSession::default();
        let mut cache = UiSurfaceCompilationCache::default();

        let compiled = cache.compile(
            &surface,
            &mut session,
            0,
            [220, 100],
            1.0,
            [0, 0, 0, 255],
            |key| key.to_string(),
        );
        let max_offset = session.interaction.controls.scroll_max_offset("list")[1];
        assert!(max_offset > 2.0);

        let frame = cache.layout(&surface, &mut session, 0, [220, 100], 1.0, [0, 0, 0, 255]);
        assert!(Arc::ptr_eq(&compiled.frame, &frame));
        session.process_input(
            &surface,
            &frame,
            &UiInputState {
                pointer_position: Some([90.0, 40.0]),
                scroll_delta: [0.0, 48.0],
                ..UiInputState::default()
            },
        );

        assert_eq!(session.interaction.controls.scroll_offset("list")[1], 48.0);

        for expected in [96.0, 144.0, 192.0] {
            let frame = cache.layout(&surface, &mut session, 0, [220, 100], 1.0, [0, 0, 0, 255]);
            session.process_input(
                &surface,
                &frame,
                &UiInputState {
                    pointer_position: Some([90.0, 40.0]),
                    scroll_delta: [0.0, 48.0],
                    ..UiInputState::default()
                },
            );
            assert_eq!(
                session.interaction.controls.scroll_offset("list")[1],
                expected
            );
            let _ = cache.compile(
                &surface,
                &mut session,
                0,
                [220, 100],
                1.0,
                [0, 0, 0, 255],
                |key| key.to_string(),
            );
        }
    }
}
