//! Native retained loading surface.
//!
//! The loading document used to be placed through a legacy bridge. Keeping
//! the document and the tiny host here preserves the splash UX while the
//! native Winit application initializes the Hub.

use std::time::{SystemTime, UNIX_EPOCH};

use crate::editor_layout::EditorRect;
use raf_core::config::Language;
use raf_core::i18n::t;
use raf_render::api_graphic_basic::ui_surface::loading::{
    build_loading_surface, insert_loading_brand,
};
use raf_render::api_graphic_basic::ui_surface::{
    DirectUiSurfaceHost, NativeGraphicsContext, StudioUiPalette,
};
use raf_render::api_graphic_basic::EditorUiLayer;

const CLEAR: [u8; 4] = [8, 11, 15, 255];
const LOADING_MESSAGES: [&str; 3] = [
    "Even the IDE we used to build this engine is heavier than the engine itself.",
    "Work in progress.",
    "World and UI design matter just as much as gameplay.",
];

pub struct LoadingSurfaceHost {
    rect: EditorRect,
    host: DirectUiSurfaceHost,
    last_key: Option<LoadingKey>,
    loading_message: &'static str,
}

#[derive(Debug, Clone, PartialEq)]
struct LoadingKey {
    rect: EditorRect,
    palette: StudioUiPalette,
    progress: u16,
    language: Language,
}

impl LoadingSurfaceHost {
    pub fn new(
        graphics: &NativeGraphicsContext<'_>,
        rect: EditorRect,
        palette: StudioUiPalette,
    ) -> Self {
        let loading_message = select_loading_message();
        let surface = build_loading_surface(
            palette,
            0.0,
            Language::English,
            loading_message,
            &t("app.loading_workspace", Language::English),
            "0%",
        );
        let mut host = graphics.create_ui_host(surface, CLEAR);
        insert_loading_brand(&mut host);
        Self {
            rect,
            host,
            last_key: None,
            loading_message,
        }
    }

    pub fn sync(
        &mut self,
        rect: EditorRect,
        palette: StudioUiPalette,
        progress: f32,
        language: Language,
    ) {
        let key = LoadingKey {
            rect,
            palette,
            progress: (progress.clamp(0.0, 1.0) * 1000.0).round() as u16,
            language,
        };
        self.rect = rect;
        if self.last_key.as_ref() == Some(&key) {
            return;
        }
        self.host.set_surface(build_loading_surface(
            palette,
            progress,
            language,
            self.loading_message,
            &t("app.loading_workspace", language),
            &format!("{:.0}%", progress.clamp(0.0, 1.0) * 100.0),
        ));
        self.last_key = Some(key);
    }

    pub fn set_environment(&mut self, environment: raf_ui::UiEnvironment) {
        self.host.set_environment(environment);
    }

    pub fn compositor_layer(
        &mut self,
        scale_factor: f32,
        target_size: [u32; 2],
    ) -> EditorUiLayer<'_> {
        EditorUiLayer {
            host: &mut self.host,
            target_rect: self.rect.to_physical(scale_factor, target_size),
            logical_size: self.rect.logical_size(),
            raster_scale: scale_factor.max(1.0),
        }
    }
}

fn select_loading_message() -> &'static str {
    let tick = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_nanos() as usize)
        .unwrap_or_default();
    LOADING_MESSAGES[tick % LOADING_MESSAGES.len()]
}
