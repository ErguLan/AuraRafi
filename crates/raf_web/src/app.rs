//! Browser application lifecycle for the WASM canvas host.
//!
//! The host owns one AGB `ProductDevice<WebGpu>`, one `SceneRenderer`, one
//! `ProductFrameBuilder`, and one self-scheduling animation-frame callback.
//! The AGB WebGPU executor configures the canvas target and presents every
//! draw; this crate never creates a surface, device, queue, or blit path.
//!
//! The callback renders at a bounded cadence and stops entirely when the page
//! is paused or hidden, so an idle browser tab performs no scene, GPU, or
//! present work.

use std::cell::{Cell, RefCell};

use raf_agb::backends::webgpu::WebGpu;
use raf_agb::capabilities::GraphicsMemoryBudget;
use raf_agb::product::ProductDevice;
use raf_agb::{Error as GraphicsError, Output};
use raf_render::api_graphic_basic::{
    prepare_product_scene, scene_frame_uniforms, ProductFrameBuilder,
};
use raf_render::scene_renderer::SceneRenderer;
use uuid::Uuid;
use wasm_bindgen::prelude::*;
use wasm_bindgen::JsCast;
use web_sys::{Document, HtmlCanvasElement, Window};

use crate::diagnostics;
use crate::scene::{DemoScene, DEMO_SCENE_RECIPE};

const CANVAS_ID: &str = "rafi-canvas";
const MAX_CANVAS_WIDTH: f64 = 1920.0;
const MAX_CANVAS_HEIGHT: f64 = 1080.0;
const MAX_PIXEL_RATIO: f64 = 2.0;
const MAX_RENDER_PIXELS: f64 = 1_440_000.0;
const MIN_RENDER_SCALE: f64 = 0.5;
const DEFAULT_WINDOW_WIDTH: f64 = 1280.0;
const DEFAULT_WINDOW_HEIGHT: f64 = 720.0;
const FRAME_INTERVAL_MS: f64 = 1000.0 / 30.0;
/// Absorbs timer quantization so a 30 Hz display is not halved to 15 Hz.
const FRAME_INTERVAL_TOLERANCE_MS: f64 = 1.0;
const RESIZE_CHECK_INTERVAL_MS: f64 = 250.0;

thread_local! {
    static STATE: RefCell<Option<WebApp>> = RefCell::new(None);
    static FRAME_CALLBACK: RefCell<Option<Closure<dyn FnMut()>>> = RefCell::new(None);
    static VISIBILITY_CALLBACK: RefCell<Option<Closure<dyn FnMut(web_sys::Event)>>> =
        RefCell::new(None);
    static SCHEDULED: Cell<bool> = Cell::new(false);
    static PAUSED: Cell<bool> = Cell::new(false);
    static VISIBLE: Cell<bool> = Cell::new(true);
    static RECOVERING: Cell<bool> = Cell::new(false);
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct ViewportMetrics {
    /// Physical extent used by both the canvas backing store and the AGB
    /// target. The browser stretches it to the CSS viewport.
    target: [u32; 2],
    pixel_ratio: f64,
}

struct FpsCounter {
    window_start_ms: f64,
    frames: u32,
    fps: f32,
}

impl FpsCounter {
    fn new(now_ms: f64) -> Self {
        Self {
            window_start_ms: now_ms,
            frames: 0,
            fps: 0.0,
        }
    }

    fn record(&mut self, now_ms: f64) {
        self.frames = self.frames.saturating_add(1);
        let elapsed = now_ms - self.window_start_ms;
        if elapsed >= 500.0 {
            // A very long gap means the page was paused or the target was
            // unavailable. Report zero instead of a misleading tiny average.
            self.fps = if elapsed <= 2_000.0 {
                (self.frames as f64 * 1000.0 / elapsed) as f32
            } else {
                0.0
            };
            self.window_start_ms = now_ms;
            self.frames = 0;
        }
    }
}

struct WebApp {
    window: Window,
    canvas: HtmlCanvasElement,
    /// Taken while the executor is being reopened. The AGB WebGPU backend
    /// unconfigures the shared canvas context when it is dropped, so the old
    /// executor must be released before the replacement configures the canvas.
    device: Option<ProductDevice<WebGpu>>,
    builder: ProductFrameBuilder,
    scene_renderer: SceneRenderer,
    scene: DemoScene,
    viewport: ViewportMetrics,
    /// Monotonic revision of the retained product composition. It advances
    /// only when the scene content or the target extent changes.
    revision: u64,
    /// The executor is reopened at most once per page. A second failure is
    /// reported instead of retrying forever.
    reopen_requested: bool,
    last_render_ms: f64,
    last_frame_ms: f64,
    last_resize_check_ms: f64,
    fps: FpsCounter,
    surface_rebuilds: u32,
}

/// Start the browser application. Called once from the WASM entry point.
pub(crate) async fn run() -> Result<(), JsValue> {
    if app_initialized() {
        return Ok(());
    }

    let window =
        web_sys::window().ok_or_else(|| js_error("the browser did not provide a window object"))?;
    let document = window
        .document()
        .ok_or_else(|| js_error("the page has no document"))?;

    diagnostics::bind_overlay(&document);
    VISIBLE.with(|flag| flag.set(!document.hidden()));
    install_visibility_listener(&document)?;

    let app = WebApp::create(&window, &document).await?;
    diagnostics::set_running(true);
    STATE.with(|slot| *slot.borrow_mut() = Some(app));
    schedule_frame();
    Ok(())
}

impl WebApp {
    async fn create(window: &Window, document: &Document) -> Result<Self, JsValue> {
        let canvas = document
            .get_element_by_id(CANVAS_ID)
            .ok_or_else(|| {
                js_error(format!(
                    "canvas element #{CANVAS_ID} was not found in the page"
                ))
            })?
            .dyn_into::<HtmlCanvasElement>()
            .map_err(|_| js_error(format!("element #{CANVAS_ID} is not a canvas")))?;

        let viewport = measure_viewport(window);
        canvas.set_width(viewport.target[0]);
        canvas.set_height(viewport.target[1]);

        let backend = WebGpu::open(canvas.clone()).await.map_err(|error| {
            js_error(format!(
                "the AGB WebGPU executor could not open the canvas: {}",
                describe_js(&error)
            ))
        })?;
        let mut device = ProductDevice::new(backend, GraphicsMemoryBudget::potato()).map_err(
            |error| js_error(format!("the AGB WebGPU device was rejected: {error}")),
        )?;
        device
            .configure(viewport.target, Output::Present)
            .map_err(|error| {
                js_error(format!(
                    "the AGB WebGPU target rejected {}x{}: {error}",
                    viewport.target[0], viewport.target[1]
                ))
            })?;

        let scene = DemoScene::from_recipe(DEMO_SCENE_RECIPE)
            .map_err(|error| js_error(format!("the embedded demo scene is invalid: {error}")))?;
        let scene_renderer = SceneRenderer::new(viewport.target[0], viewport.target[1]);

        let instance_id = Uuid::new_v4().to_string();
        let backend_label = device.info().backend;
        let adapter_name = device.info().name.clone();
        diagnostics::configure(backend_label, &adapter_name, &instance_id);
        diagnostics::set_viewport(viewport.target, viewport.target, viewport.pixel_ratio);
        diagnostics::set_hidden(document.hidden());

        let now = js_sys::Date::now();
        Ok(Self {
            window: window.clone(),
            canvas,
            device: Some(device),
            builder: ProductFrameBuilder::default(),
            scene_renderer,
            scene,
            viewport,
            revision: 0,
            reopen_requested: false,
            last_render_ms: now,
            last_frame_ms: now,
            last_resize_check_ms: now,
            fps: FpsCounter::new(now),
            surface_rebuilds: 0,
        })
    }

    /// Render at most once per frame interval. Returns whether the host should
    /// schedule another animation frame.
    fn tick(&mut self) -> bool {
        if PAUSED.with(Cell::get) || !VISIBLE.with(Cell::get) || RECOVERING.with(Cell::get) {
            return false;
        }

        let now = js_sys::Date::now();
        if now < self.last_render_ms {
            // The page clock can jump; do not stall the loop after a backwards
            // adjustment.
            self.last_render_ms = now;
            self.last_frame_ms = now;
        }
        if now - self.last_render_ms < FRAME_INTERVAL_MS - FRAME_INTERVAL_TOLERANCE_MS {
            return true;
        }
        self.last_render_ms = now;

        if now - self.last_resize_check_ms >= RESIZE_CHECK_INTERVAL_MS {
            self.last_resize_check_ms = now;
            if !self.resize_if_needed() {
                return false;
            }
        }

        if PAUSED.with(Cell::get) || !VISIBLE.with(Cell::get) {
            return false;
        }

        let dt = ((now - self.last_frame_ms) / 1000.0).clamp(0.0, 0.1);
        self.last_frame_ms = now;
        if self.scene.update(dt as f32) {
            self.revision = self.revision.saturating_add(1).max(1);
        }

        let frame = self.scene.render_frame(
            &mut self.scene_renderer,
            self.viewport.target[0],
            self.viewport.target[1],
        );
        let composition = match self.builder.append_scene(&frame, self.revision) {
            Ok(metrics) => metrics,
            Err(cause) => {
                report_fatal(&js_error(format!(
                    "the scene composition was rejected: {cause}"
                )));
                return false;
            }
        };
        let Some(device) = self.device.as_mut() else {
            // The executor is being reopened by the recovery task.
            return false;
        };
        let handle = match prepare_product_scene(device, &mut self.builder) {
            Ok(handle) => handle,
            Err(cause) => {
                // The helper flattens `raf_agb::Error` into text, so budget and
                // device failures both stop the page here.
                report_fatal(&js_error(format!(
                    "the AGB WebGPU device rejected the scene packet: {cause}"
                )));
                return false;
            }
        };
        let drawn = device.draw(handle, &scene_frame_uniforms(&frame));
        let metrics = match drawn {
            Ok(metrics) => metrics,
            Err(error) => {
                self.handle_graphics_failure(error, "presenting the demo scene");
                return false;
            }
        };

        self.fps.record(now);
        diagnostics::record_frame(diagnostics::FrameSample {
            fps: self.fps.fps,
            frame_ms: composition.frame_cpu_ms,
            gpu_ms: metrics.gpu_ms.map(|milliseconds| milliseconds as f32),
            draw_calls: metrics.draw_calls,
            triangles: frame.stats.triangles_rendered,
            entities: frame.stats.visible_entities,
            cpu_fallback: false,
            surface_rebuilds: self.surface_rebuilds,
            presented: true,
        });
        true
    }

    /// Poll the CSS viewport and retarget the AGB device when it changed.
    /// Returns `false` when the target could not be configured and the frame
    /// loop must stop.
    fn resize_if_needed(&mut self) -> bool {
        let next = measure_viewport(&self.window);
        if next == self.viewport {
            return true;
        }

        let previous = self.viewport;
        self.viewport = next;
        // Pixel-space commands and the configured extent are baked into the
        // retained packet, so a resize always advances the revision.
        self.revision = self.revision.saturating_add(1).max(1);

        let Some(device) = self.device.as_mut() else {
            return false;
        };
        let configured = device.configure(next.target, Output::Present);
        match configured {
            Ok(()) => {
                self.surface_rebuilds = self.surface_rebuilds.saturating_add(1);
                diagnostics::set_viewport(next.target, next.target, next.pixel_ratio);
                true
            }
            Err(error) => {
                self.viewport = previous;
                self.handle_graphics_failure(error, "resizing the demo target");
                false
            }
        }
    }

    /// One reopen is allowed per page. Surface, outdated-target, and device
    /// loss all rebuild the executor because the AGB WebGPU backend owns the
    /// canvas context and its device together.
    fn handle_graphics_failure(&mut self, error: GraphicsError, context: &str) {
        match error {
            GraphicsError::SurfaceLost
            | GraphicsError::SurfaceOutdated
            | GraphicsError::DeviceLost => self.request_device_reopen(context),
            GraphicsError::Budget {
                requested,
                available,
            } => report_fatal(&js_error(format!(
                "the AGB WebGPU budget rejected the frame while {context}: \
                 {requested} bytes requested, {available} available"
            ))),
            error => report_fatal(&js_error(format!(
                "the AGB WebGPU device failed while {context}: {error}"
            ))),
        }
    }

    fn request_device_reopen(&mut self, context: &str) {
        if RECOVERING.with(Cell::get) || self.reopen_requested {
            report_fatal(&js_error(format!(
                "the AGB WebGPU device failed again while {context} after one reopen; \
                 reload the page"
            )));
            return;
        }
        self.reopen_requested = true;
        RECOVERING.with(|flag| flag.set(true));
        // Release the lost executor before the replacement opens. Its `Drop`
        // unconfigures the shared canvas context, so it must not run after the
        // new executor has configured that same context.
        self.device = None;

        let canvas = self.canvas.clone();
        wasm_bindgen_futures::spawn_local(async move {
            let backend = WebGpu::open(canvas).await;
            let outcome = STATE.with(|slot| {
                let mut slot = match slot.try_borrow_mut() {
                    Ok(slot) => slot,
                    // A re-entrant callback must never panic the page.
                    Err(_) => return Err("the page state was busy during recovery".to_string()),
                };
                match slot.as_mut() {
                    Some(app) => app.install_reopened(backend),
                    None => Ok(()),
                }
            });
            RECOVERING.with(|flag| flag.set(false));
            match outcome {
                Ok(()) => schedule_frame(),
                Err(message) => report_fatal(&js_error(message)),
            }
        });
    }

    fn install_reopened(
        &mut self,
        backend: Result<WebGpu, JsValue>,
    ) -> Result<(), String> {
        let backend = backend.map_err(|error| {
            format!(
                "the AGB WebGPU executor could not reopen the canvas: {}",
                describe_js(&error)
            )
        })?;
        let mut device = ProductDevice::new(backend, GraphicsMemoryBudget::potato())
            .map_err(|error| format!("the reopened AGB WebGPU device was rejected: {error}"))?;
        device
            .configure(self.viewport.target, Output::Present)
            .map_err(|error| {
                format!(
                    "the reopened AGB WebGPU target rejected {}x{}: {error}",
                    self.viewport.target[0], self.viewport.target[1]
                )
            })?;
        self.device = Some(device);
        // The builder keeps its retained source, so the next prepare rebuilds
        // the packet on the new device without re-recording the scene.
        self.surface_rebuilds = self.surface_rebuilds.saturating_add(1);
        diagnostics::set_viewport(
            self.viewport.target,
            self.viewport.target,
            self.viewport.pixel_ratio,
        );
        Ok(())
    }
}

/// Pause or resume the animation-frame chain.
pub(crate) fn set_paused(paused: bool) {
    PAUSED.with(|flag| flag.set(paused));
    diagnostics::set_paused(paused);
    if !paused && VISIBLE.with(Cell::get) && app_initialized() {
        schedule_frame();
    }
}

/// Report an unrecoverable initialization or present error.
pub(crate) fn report_fatal(error: &JsValue) {
    let message = error
        .as_string()
        .unwrap_or_else(|| describe_js(error));
    web_sys::console::error_1(&JsValue::from_str(&message));
    PAUSED.with(|flag| flag.set(true));
    diagnostics::report_error(&message);
}

fn install_visibility_listener(document: &Document) -> Result<(), JsValue> {
    let document = document.clone();
    let event_document = document.clone();
    let callback = Closure::wrap(Box::new(move |_event: web_sys::Event| {
        let visible = !event_document.hidden();
        VISIBLE.with(|flag| flag.set(visible));
        diagnostics::set_hidden(!visible);
        if visible && !PAUSED.with(Cell::get) && app_initialized() {
            schedule_frame();
        }
    }) as Box<dyn FnMut(web_sys::Event)>);

    let listener: &js_sys::Function = callback.as_ref().unchecked_ref();
    document.add_event_listener_with_callback("visibilitychange", listener)?;
    VISIBILITY_CALLBACK.with(|slot| *slot.borrow_mut() = Some(callback));
    Ok(())
}

fn schedule_frame() {
    if RECOVERING.with(Cell::get) {
        return;
    }
    if SCHEDULED.with(|flag| flag.replace(true)) {
        return;
    }
    let Some(window) = web_sys::window() else {
        SCHEDULED.with(|flag| flag.set(false));
        return;
    };

    FRAME_CALLBACK.with(|slot| {
        let mut slot = slot.borrow_mut();
        if slot.is_none() {
            *slot = Some(Closure::wrap(
                Box::new(on_animation_frame) as Box<dyn FnMut()>
            ));
        }
        let Some(callback) = slot.as_ref() else {
            SCHEDULED.with(|flag| flag.set(false));
            return;
        };
        let function: &js_sys::Function = callback.as_ref().unchecked_ref();
        if window.request_animation_frame(function).is_err() {
            SCHEDULED.with(|flag| flag.set(false));
            report_fatal(&js_error(
                "the browser rejected the animation frame request",
            ));
        }
    });
}

fn on_animation_frame() {
    SCHEDULED.with(|flag| flag.set(false));
    if PAUSED.with(Cell::get) || !VISIBLE.with(Cell::get) || RECOVERING.with(Cell::get) {
        return;
    }

    let keep_running = STATE.with(|slot| match slot.try_borrow_mut() {
        Ok(mut slot) => match slot.as_mut() {
            Some(app) => app.tick(),
            None => false,
        },
        // A re-entrant callback must never panic the page.
        Err(_) => false,
    });
    if keep_running {
        schedule_frame();
    }
}

fn app_initialized() -> bool {
    STATE.with(|slot| {
        slot.try_borrow()
            .map(|slot| slot.is_some())
            .unwrap_or(false)
    })
}

fn measure_viewport(window: &Window) -> ViewportMetrics {
    let pixel_ratio = finite_positive(window.device_pixel_ratio(), 1.0).clamp(1.0, MAX_PIXEL_RATIO);
    let css_width = window
        .inner_width()
        .ok()
        .and_then(|value| value.as_f64())
        .map(|value| finite_positive(value, DEFAULT_WINDOW_WIDTH))
        .unwrap_or(DEFAULT_WINDOW_WIDTH);
    let css_height = window
        .inner_height()
        .ok()
        .and_then(|value| value.as_f64())
        .map(|value| finite_positive(value, DEFAULT_WINDOW_HEIGHT))
        .unwrap_or(DEFAULT_WINDOW_HEIGHT);

    let physical_width = (css_width * pixel_ratio).round().max(1.0);
    let physical_height = (css_height * pixel_ratio).round().max(1.0);
    let fit = (MAX_CANVAS_WIDTH / physical_width)
        .min(MAX_CANVAS_HEIGHT / physical_height)
        .min(1.0);
    let canvas_width = ((physical_width * fit).round() as u32).clamp(1, MAX_CANVAS_WIDTH as u32);
    let canvas_height = ((physical_height * fit).round() as u32).clamp(1, MAX_CANVAS_HEIGHT as u32);

    let pixels = f64::from(canvas_width) * f64::from(canvas_height);
    let scale = if pixels > MAX_RENDER_PIXELS {
        (MAX_RENDER_PIXELS / pixels)
            .sqrt()
            .clamp(MIN_RENDER_SCALE, 1.0)
    } else {
        1.0
    };
    let target_width = ((f64::from(canvas_width) * scale).round() as u32).clamp(1, canvas_width);
    let target_height = ((f64::from(canvas_height) * scale).round() as u32).clamp(1, canvas_height);

    ViewportMetrics {
        target: [target_width, target_height],
        pixel_ratio,
    }
}

fn finite_positive(value: f64, fallback: f64) -> f64 {
    if value.is_finite() && value > 0.0 {
        value
    } else {
        fallback
    }
}

fn describe_js(error: &JsValue) -> String {
    js_sys::Reflect::get(error, &JsValue::from_str("message"))
        .ok()
        .and_then(|message| message.as_string())
        .or_else(|| error.as_string())
        .unwrap_or_else(|| format!("{error:?}"))
}

fn js_error(message: impl AsRef<str>) -> JsValue {
    JsValue::from_str(message.as_ref())
}
