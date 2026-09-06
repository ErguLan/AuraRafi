//! Native host for the retained application bar and menu popup.
//!
//! The old version only placed RafUI through a legacy widget bridge. This host keeps the same
//! menu/document contract and places both surfaces directly through AGB.

use raf_core::config::Language;
use raf_core::project::ProjectType;
use raf_core::{InputOwner, InputRegionId, InputRouter};
use raf_render::api_graphic_basic::ui_surface::{
    DirectUiSurfaceHost, NativeGraphicsContext, NativeUiInputBridge, StudioUiPalette, UiAction,
    UiDispatchedAction,
};
use raf_render::api_graphic_basic::EditorUiLayer;
use raf_ui::{UiMotionSpec, UiRect, UiTween};

use crate::application_bar_surface::{
    build_application_bar_surface, build_application_menu_popup_surface,
    APPLICATION_MENU_POPUP_WIDTH,
};
use crate::application_menu::{build_application_menu, ApplicationMenuState};
use crate::editor_layout::EditorRect;

pub struct ApplicationBarHost {
    region: InputRegionId,
    rect: EditorRect,
    menu_rect: EditorRect,
    host: DirectUiSurfaceHost,
    menu_host: DirectUiSurfaceHost,
    open_menu: Option<String>,
    menu_motion: UiTween,
    last_time_seconds: f64,
    command_query: String,
}

impl ApplicationBarHost {
    pub fn new(
        graphics: &NativeGraphicsContext<'_>,
        palette: StudioUiPalette,
        rect: EditorRect,
        project_name: &str,
        project_type: ProjectType,
    ) -> Self {
        let surface = build_application_bar_surface(palette, project_name, project_type, None);
        let menu_surface = build_application_menu_popup_surface(
            palette,
            &build_application_menu(ApplicationMenuState {
                project_type,
                active_view: crate::application_menu::ApplicationView::Scene,
                grid_visible: true,
                hierarchy_visible: true,
                inspector_visible: true,
                undo_available: false,
                redo_available: false,
                selection_available: false,
                select_all_available: false,
                copy_available: false,
                paste_available: false,
            })
            .menus[0],
        );
        let mut host = graphics.create_ui_host(surface, [0, 0, 0, 0]);
        let mut menu_host = graphics.create_ui_host(menu_surface, [0, 0, 0, 0]);
        register_bar_images(host.images_mut());
        register_bar_images(menu_host.images_mut());
        Self {
            region: InputRegionId::from_static("native.editor.application-bar"),
            rect,
            menu_rect: EditorRect::new(
                rect.x,
                rect.y + rect.height,
                APPLICATION_MENU_POPUP_WIDTH,
                1.0,
            ),
            host,
            menu_host,
            open_menu: None,
            menu_motion: UiTween::new(0.0, UiMotionSpec::dock()),
            last_time_seconds: 0.0,
            command_query: String::new(),
        }
    }

    pub fn owner(&self) -> InputOwner {
        InputOwner::RetainedUi(self.region)
    }

    pub fn command_query(&self) -> &str {
        &self.command_query
    }

    pub fn sync(
        &mut self,
        palette: StudioUiPalette,
        project_name: &str,
        project_type: ProjectType,
        menu_state: ApplicationMenuState,
        rect: EditorRect,
        now_seconds: f64,
    ) {
        self.rect = rect;
        self.menu_motion
            .set_target(self.open_menu.as_ref().map_or(0.0, |_| 1.0));
        let delta = (now_seconds - self.last_time_seconds).clamp(0.0, 0.25) as f32;
        self.last_time_seconds = now_seconds;
        self.menu_motion.advance(delta, false);
        self.host.set_surface(build_application_bar_surface(
            palette,
            project_name,
            project_type,
            self.open_menu.as_deref(),
        ));
        if let Some(menu_id) = self.open_menu.as_deref() {
            if let Some(menu) = build_application_menu(menu_state)
                .menus
                .into_iter()
                .find(|menu| menu.id == menu_id)
            {
                let x = match menu_id {
                    "file" => 238.0,
                    "edit" => 294.0,
                    "view" => 350.0,
                    "project" => 410.0,
                    "help" => 486.0,
                    _ => 238.0,
                };
                let y = rect.y + rect.height - (1.0 - self.menu_motion.value()) * 6.0;
                self.menu_rect = EditorRect::new(x, y, APPLICATION_MENU_POPUP_WIDTH, 320.0);
                self.menu_host
                    .set_surface(build_application_menu_popup_surface(palette, &menu));
            }
        }
    }

    pub fn process_input(
        &mut self,
        input: &NativeUiInputBridge,
        router: &mut InputRouter,
        language: Language,
    ) -> Vec<UiDispatchedAction> {
        let mut actions = self.host.process_routed_input(
            self.rect.logical_size(),
            input.scale_factor() as f32,
            |key| raf_core::i18n::t(key, language),
            input,
            router,
            self.owner(),
            UiRect::new(self.rect.x, self.rect.y, self.rect.width, self.rect.height),
        );
        let menu_actions = self.menu_host.process_routed_input(
            self.menu_rect.logical_size(),
            input.scale_factor() as f32,
            |key| raf_core::i18n::t(key, language),
            input,
            router,
            self.owner(),
            UiRect::new(
                self.menu_rect.x,
                self.menu_rect.y,
                self.menu_rect.width,
                self.menu_rect.height,
            ),
        );
        for action in &actions {
            if let UiAction::Command { name } = &action.action {
                if let Some(menu_id) = name.strip_prefix("application.menu.") {
                    self.open_menu =
                        (self.open_menu.as_deref() != Some(menu_id)).then(|| menu_id.to_string());
                } else if name == "window.close" || name == "window.minimize" {
                    self.open_menu = None;
                }
            }
        }
        actions.extend(menu_actions);
        if input.snapshot().key_pressed(raf_core::InputKey::Escape) {
            self.open_menu = None;
        }
        actions
    }

    pub fn compositor_layers(
        &mut self,
        scale_factor: f32,
        target_size: [u32; 2],
    ) -> Vec<EditorUiLayer<'_>> {
        let main = EditorUiLayer {
            host: &mut self.host,
            target_rect: self.rect.to_physical(scale_factor, target_size),
            logical_size: self.rect.logical_size(),
            raster_scale: scale_factor.max(1.0),
        };
        if self.open_menu.is_some() {
            let menu = EditorUiLayer {
                host: &mut self.menu_host,
                target_rect: self.menu_rect.to_physical(scale_factor, target_size),
                logical_size: self.menu_rect.logical_size(),
                raster_scale: scale_factor.max(1.0),
            };
            vec![main, menu]
        } else {
            vec![main]
        }
    }
}

pub(crate) fn register_bar_images(
    store: &mut raf_render::api_graphic_basic::ui_surface::UiSurfaceImageStore,
) {
    for (key, bytes) in [
        (
            "editor.top.logo",
            include_bytes!("../../../editor/icon.png").as_slice(),
        ),
        (
            "editor.top.file",
            include_bytes!("../../../editor/assets/ui_icons/top/file.png").as_slice(),
        ),
        (
            "editor.top.edit",
            include_bytes!("../../../editor/assets/ui_icons/top/edit.png").as_slice(),
        ),
        (
            "editor.top.view",
            include_bytes!("../../../editor/assets/ui_icons/top/view.png").as_slice(),
        ),
        (
            "editor.top.project",
            include_bytes!("../../../editor/assets/ui_icons/top/project.png").as_slice(),
        ),
        (
            "editor.top.help",
            include_bytes!("../../../editor/assets/ui_icons/top/help.png").as_slice(),
        ),
        (
            "editor.top.save",
            include_bytes!("../../../editor/assets/ui_icons/top/save.png").as_slice(),
        ),
        (
            "editor.top.minimize",
            include_bytes!("../../../editor/assets/ui_icons/top/minimize.png").as_slice(),
        ),
        (
            "editor.top.maximize",
            include_bytes!("../../../editor/assets/ui_icons/top/maximize.png").as_slice(),
        ),
        (
            "editor.top.close",
            include_bytes!("../../../editor/assets/ui_icons/top/close.png").as_slice(),
        ),
    ] {
        if let Ok(decoded) = image::load_from_memory(bytes) {
            let decoded = decoded.to_rgba8();
            let _ = store.insert_rgba(
                key.to_string(),
                [decoded.width(), decoded.height()],
                decoded.into_raw(),
            );
        }
    }
}

pub(crate) fn register_electronics_images(
    store: &mut raf_render::api_graphic_basic::ui_surface::UiSurfaceImageStore,
) {
    for (key, bytes) in [
        (
            "electronics://library/resistor.png",
            include_bytes!("../../../editor/assets/electronics/library/resistor.png").as_slice(),
        ),
        (
            "electronics://library/capacitor.png",
            include_bytes!("../../../editor/assets/electronics/library/capacitor.png").as_slice(),
        ),
        (
            "electronics://library/led.png",
            include_bytes!("../../../editor/assets/electronics/library/led.png").as_slice(),
        ),
        (
            "electronics://library/magnet.png",
            include_bytes!("../../../editor/assets/electronics/library/magnet.png").as_slice(),
        ),
        (
            "electronics://library/battery.png",
            include_bytes!("../../../editor/assets/electronics/library/battery.png").as_slice(),
        ),
        (
            "electronics://library/ground.png",
            include_bytes!("../../../editor/assets/electronics/library/ground.png").as_slice(),
        ),
        (
            "electronics://library/generic.png",
            include_bytes!("../../../editor/assets/electronics/library/generic.png").as_slice(),
        ),
    ] {
        if let Ok(decoded) = image::load_from_memory(bytes) {
            let decoded = decoded.to_rgba8();
            let _ = store.insert_rgba(key, [decoded.width(), decoded.height()], decoded.into_raw());
        }
    }
}
