//! Browser application lifecycle for the WASM canvas host.
//!
//! The host owns one `BasicDevice`, one `SceneRenderer`, one surface, and one
//! self-scheduling animation-frame callback. The callback renders at a bounded
//! cadence and stops entirely when the page is paused or hidden, so an idle
//! browser tab performs no scene, GPU, or present work.

use std::cell::{Cell, RefCell};
use std::sync::Arc;

use raf_render::api_graphic_basic::capabilities::{
    GraphicsAdapterPreference, GraphicsMemoryBudget,
};
use raf_render::api_graphic_basic::device::{
    BasicDevice, BasicDeviceConfig, SceneFrameOutput, SharedGraphicsContext,
};
use raf_render::scene_renderer::SceneRenderer;
use uuid::Uuid;
use wasm_bindgen::prelude::*;
use wasm_bindgen::JsCast;
use web_sys::{Document, HtmlCanvasElement, Window};

use crate::diagnostics;
use crate::present::CanvasBlit;
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
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct ViewportMetrics {
    canvas: [u32; 2],
    render: [u32; 2],
    pixel_ratio: f64,
}

struct FpsCounter {
    window_start_ms: f64,
    frames: u32,
    fps: f32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PresentOutcome {
    Presented,
    Skipped,
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
            // A very long gap means the page was paused or the surface was
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
    surface: wgpu::Surface<'static>,
    surface_config: wgpu::SurfaceConfiguration,
    device: Arc<wgpu::Device>,
    queue: Arc<wgpu::Queue>,
    basic_device: BasicDevice,
    scene_renderer: SceneRenderer,
    scene: DemoScene,
    blit: CanvasBlit,
    viewport: ViewportMetrics,
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
        canvas.set_width(viewport.canvas[0]);
        canvas.set_height(viewport.canvas[1]);

        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor {
            // On wasm this descriptor only selects between the browser WebGPU
            // backend and the WebGL2 fallback.
            backends: wgpu::Backends::BROWSER_WEBGPU | wgpu::Backends::GL,
            ..wgpu::InstanceDescriptor::default()
        });

        let surface = instance
            .create_surface(wgpu::SurfaceTarget::Canvas(canvas.clone()))
            .map_err(|error| {
                js_error(format!(
                    "the browser could not create a render surface: {error}"
                ))
            })?;

        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::LowPower,
                compatible_surface: Some(&surface),
                force_fallback_adapter: false,
            })
            .await
            .ok_or_else(|| {
                js_error(
                    "neither WebGPU nor a WebGL2 fallback adapter was available; \
                     enable hardware acceleration or a modern browser",
                )
            })?;

        let adapter_info = adapter.get_info();
        let backend_label = match adapter_info.backend {
            wgpu::Backend::BrowserWebGpu => "WebGPU",
            wgpu::Backend::Gl => "WebGL2",
            other => other.to_str(),
        };
        let adapter_name = adapter_info.name.trim();

        // WebGL2 has a lower mandatory floor than `downlevel_defaults()`;
        // requesting the WebGPU floor there can fail on constrained drivers.
        let limits = match adapter_info.backend {
            wgpu::Backend::BrowserWebGpu => wgpu::Limits::downlevel_defaults(),
            _ => wgpu::Limits::downlevel_webgl2_defaults(),
        };

        let (device, queue) = adapter
            .request_device(
                &wgpu::DeviceDescriptor {
                    label: Some("AuraRafi_WebDevice"),
                    required_features: wgpu::Features::empty(),
                    required_limits: limits,
                    memory_hints: wgpu::MemoryHints::Performance,
                },
                None,
            )
            .await
            .map_err(|error| {
                js_error(format!(
                    "the browser refused the GPU device request: {error}"
                ))
            })?;
        let device = Arc::new(device);
        let queue = Arc::new(queue);

        let surface_capabilities = surface.get_capabilities(&adapter);
        let surface_format = surface_capabilities
            .formats
            .iter()
            .copied()
            .find(wgpu::TextureFormat::is_srgb)
            .or_else(|| surface_capabilities.formats.first().copied())
            .ok_or_else(|| js_error("the browser surface reported no color formats"))?;
        let present_mode = surface_capabilities
            .present_modes
            .iter()
            .copied()
            .find(|mode| *mode == wgpu::PresentMode::Fifo)
            .or_else(|| surface_capabilities.present_modes.first().copied())
            .unwrap_or(wgpu::PresentMode::Fifo);
        let alpha_mode = surface_capabilities
            .alpha_modes
            .first()
            .copied()
            .unwrap_or(wgpu::CompositeAlphaMode::Auto);

        let surface_config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format: surface_format,
            width: viewport.canvas[0],
            height: viewport.canvas[1],
            present_mode,
            alpha_mode,
            view_formats: Vec::new(),
            desired_maximum_frame_latency: 2,
        };
        surface.configure(&device, &surface_config);

        let scene = DemoScene::from_recipe(DEMO_SCENE_RECIPE)
            .map_err(|error| js_error(format!("the embedded demo scene is invalid: {error}")))?;
        let scene_renderer = SceneRenderer::new(viewport.render[0], viewport.render[1]);
        let blit = CanvasBlit::new(&device, surface_format);

        let basic_device = BasicDevice::new(BasicDeviceConfig {
            allow_gpu: true,
            force_cpu: false,
            shared_graphics_context: Some(SharedGraphicsContext::from_host(
                device.clone(),
                queue.clone(),
            )),
            memory_budget: GraphicsMemoryBudget::potato(),
            adapter_preference: GraphicsAdapterPreference::LowPower,
        });

        let instance_id = Uuid::new_v4().to_string();
        diagnostics::configure(backend_label, adapter_name, &instance_id);
        diagnostics::set_viewport(viewport.canvas, viewport.render, viewport.pixel_ratio);
        diagnostics::set_hidden(document.hidden());

        let now = js_sys::Date::now();
        Ok(Self {
            window: window.clone(),
            canvas,
            surface,
            surface_config,
            device,
            queue,
            basic_device,
            scene_renderer,
            scene,
            blit,
            viewport,
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
        if PAUSED.with(Cell::get) || !VISIBLE.with(Cell::get) {
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
            self.resize_if_needed();
        }

        if PAUSED.with(Cell::get) || !VISIBLE.with(Cell::get) {
            return false;
        }

        let dt = ((now - self.last_frame_ms) / 1000.0).clamp(0.0, 0.1);
        self.last_frame_ms = now;
        self.scene.update(dt as f32);

        let frame = self.scene.render_frame(
            &mut self.scene_renderer,
            self.viewport.render[0],
            self.viewport.render[1],
        );
        let output = self.basic_device.execute_scene_frame(&frame);
        let metrics = self.basic_device.last_frame_metrics();
        let cpu_fallback = matches!(&output, SceneFrameOutput::CpuPixels(_));

        let presented = match self.present_scene(&output) {
            Ok(outcome) => outcome,
            Err(error) => {
                report_fatal(&js_error(error));
                return false;
            }
        };

        let presented = presented == PresentOutcome::Presented;
        if presented {
            self.fps.record(now);
        }
        diagnostics::record_frame(diagnostics::FrameSample {
            fps: self.fps.fps,
            frame_ms: metrics.frame_cpu_ms,
            gpu_ms: metrics.gpu_timing_sampled.then_some(metrics.gpu_frame_ms),
            draw_calls: metrics.total_draw_calls(),
            triangles: frame.stats.triangles_rendered,
            entities: frame.stats.visible_entities,
            cpu_fallback,
            surface_rebuilds: self.surface_rebuilds,
            presented,
        });
        true
    }

    fn resize_if_needed(&mut self) {
        let next = measure_viewport(&self.window);
        if next == self.viewport {
            return;
        }

        self.canvas.set_width(next.canvas[0]);
        self.canvas.set_height(next.canvas[1]);
        self.surface_config.width = next.canvas[0];
        self.surface_config.height = next.canvas[1];
        self.surface.configure(&self.device, &self.surface_config);
        self.viewport = next;
        self.surface_rebuilds = self.surface_rebuilds.saturating_add(1);
        diagnostics::set_viewport(next.canvas, next.render, next.pixel_ratio);
    }

    fn present_scene(&mut self, output: &SceneFrameOutput) -> Result<PresentOutcome, String> {
        let surface_texture = match self.surface.get_current_texture() {
            Ok(texture) => texture,
            Err(wgpu::SurfaceError::Lost | wgpu::SurfaceError::Outdated) => {
                self.surface.configure(&self.device, &self.surface_config);
                self.surface_rebuilds = self.surface_rebuilds.saturating_add(1);
                return Ok(PresentOutcome::Skipped);
            }
            Err(wgpu::SurfaceError::Timeout) => return Ok(PresentOutcome::Skipped),
            Err(wgpu::SurfaceError::OutOfMemory) => {
                return Err(
                    "the browser surface ran out of memory; reduce the window size and reload"
                        .to_string(),
                );
            }
        };

        let target = surface_texture
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());
        self.blit.present(
            &self.device,
            &self.queue,
            &target,
            output,
            self.viewport.render,
        )?;
        surface_texture.present();
        Ok(PresentOutcome::Presented)
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
    let message = error.as_string().unwrap_or_else(|| format!("{error:?}"));
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
    if PAUSED.with(Cell::get) || !VISIBLE.with(Cell::get) {
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
    let render_width = ((f64::from(canvas_width) * scale).round() as u32).clamp(1, canvas_width);
    let render_height = ((f64::from(canvas_height) * scale).round() as u32).clamp(1, canvas_height);

    ViewportMetrics {
        canvas: [canvas_width, canvas_height],
        render: [render_width, render_height],
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

fn js_error(message: impl AsRef<str>) -> JsValue {
    JsValue::from_str(message.as_ref())
}
