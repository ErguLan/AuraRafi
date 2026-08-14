//! Shared retained-UI compilation cache.
//!
//! Layout, hit-testing, atlas synchronization, and paint-list construction
//! have distinct invalidation needs. Keeping them together here prevents the
//! CPU fallback and direct GPU host from accidentally rebuilding the same UI
//! representation every frame.

use std::sync::Arc;

use super::{
    UiControlState, UiFocusState, UiSurface, UiSurfaceDrawList, UiSurfaceFrame, UiSurfaceSession,
};

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
    focus: UiFocusState,
    logical_size: [u32; 2],
    raster_scale_bits: u32,
    motion_value_bits: u32,
    clear_color: [u8; 4],
}

impl UiSurfaceLayoutKey {
    fn new(
        surface_revision: u64,
        session: &UiSurfaceSession,
        logical_size: [u32; 2],
        raster_scale: f32,
        clear_color: [u8; 4],
    ) -> Self {
        Self {
            surface_revision,
            controls: session.interaction.controls.clone(),
            focus: session.interaction.focus.clone(),
            logical_size: [logical_size[0].max(1), logical_size[1].max(1)],
            raster_scale_bits: raster_scale.clamp(1.0, 4.0).to_bits(),
            motion_value_bits: session.transient_motion_revision(),
            clear_color,
        }
    }
}

struct CachedLayout {
    key: UiSurfaceLayoutKey,
    revision: u64,
    frame: Arc<UiSurfaceFrame>,
}

struct CachedPaint {
    layout_revision: u64,
    atlas_revision: u64,
    resolved_text: Vec<String>,
    frame: Arc<UiSurfaceFrame>,
    draw_list: Arc<UiSurfaceDrawList>,
}

#[derive(Clone)]
pub(super) struct UiSurfaceCompiledFrame {
    pub frame: Arc<UiSurfaceFrame>,
    pub draw_list: Arc<UiSurfaceDrawList>,
}

/// Cache owned by a surface host. It is deliberately renderer-agnostic, so
/// GPU-first presentation and CPU recovery mode compile the exact same data.
#[derive(Default)]
pub(super) struct UiSurfaceCompilationCache {
    layout: Option<CachedLayout>,
    paint: Option<CachedPaint>,
    next_layout_revision: u64,
    metrics: UiSurfaceCompileMetrics,
}

impl UiSurfaceCompilationCache {
    pub fn invalidate(&mut self) {
        self.layout = None;
        self.paint = None;
    }

    pub fn metrics(&self) -> UiSurfaceCompileMetrics {
        self.metrics
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
            surface_revision,
            session,
            logical_size,
            raster_scale,
            clear_color,
        );
        self.metrics.layout_cache_hit =
            self.layout.as_ref().is_some_and(|cached| cached.key == key);
        self.metrics.paint_cache_hit = false;
        if let Some(cached) = self.layout.as_ref().filter(|cached| cached.key == key) {
            return Arc::clone(&cached.frame);
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
            revision: self.next_layout_revision,
            frame: Arc::clone(&frame),
        });
        self.paint = None;
        frame
    }

    pub fn compile<F>(
        &mut self,
        surface: &UiSurface,
        session: &mut UiSurfaceSession,
        surface_revision: u64,
        logical_size: [u32; 2],
        raster_scale: f32,
        clear_color: [u8; 4],
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
        let layout_revision = self
            .layout
            .as_ref()
            .expect("layout cache exists after compilation")
            .revision;
        let resolved_text = { session.resolve_text_requests(&cached_frame, |key| resolve(key)) };
        session.sync_resolved_text(&cached_frame, &resolved_text);
        let atlas_revision = session.text_atlas.revision();
        self.metrics.paint_cache_hit = self.paint.as_ref().is_some_and(|cached| {
            cached.layout_revision == layout_revision
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
            };
        }
        let intrinsic_frame = needs_intrinsic_fit.then(|| {
            let intrinsic_sizes =
                session.intrinsic_sizes_for_frame(&cached_frame, &resolved_text, raster_scale);
            session.rebuild_layout_with_intrinsic_sizes(
                surface,
                logical_size[0],
                logical_size[1],
                clear_color,
                raster_scale,
                &intrinsic_sizes,
            )
        });
        let frame = intrinsic_frame
            .map(Arc::new)
            .unwrap_or_else(|| Arc::clone(&cached_frame));
        let draw_list = if let Some(cached) = self.paint.as_ref().filter(|cached| {
            cached.layout_revision == layout_revision
                && cached.atlas_revision == atlas_revision
                && cached.resolved_text == resolved_text
        }) {
            Arc::clone(&cached.draw_list)
        } else {
            session.sync_resolved_text(&frame, &resolved_text);
            let draw_list = Arc::new(UiSurfaceDrawList::build_with_resolved_text_values_at_scale(
                &frame,
                &session.text_atlas,
                raster_scale,
                &resolved_text,
            ));
            self.metrics.paint_builds = self.metrics.paint_builds.saturating_add(1);
            self.paint = Some(CachedPaint {
                layout_revision,
                atlas_revision: session.text_atlas.revision(),
                resolved_text,
                frame: Arc::clone(&frame),
                draw_list: Arc::clone(&draw_list),
            });
            draw_list
        };

        UiSurfaceCompiledFrame { frame, draw_list }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::*;
    use crate::api_graphic_basic::ui_surface::{StudioUiPalette, UiLayout, UiNode, UiNodeKind};

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
}
