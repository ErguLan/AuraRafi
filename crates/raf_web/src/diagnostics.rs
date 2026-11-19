//! Diagnostics for the WebAssembly canvas host.
//!
//! The frame path publishes a small snapshot; the page overlay reads it at a
//! bounded rate and `stats()` exposes the same data to browser tooling. This
//! module never drives rendering and never allocates in the scene path.

use std::cell::{Cell, RefCell};

use js_sys::{Object, Reflect};
use wasm_bindgen::JsValue;
use web_sys::{Document, Element};

const OVERLAY_INTERVAL_MS: f64 = 250.0;

#[derive(Clone, Debug)]
pub(crate) struct DiagnosticsSnapshot {
    pub instance: String,
    pub running: bool,
    pub paused: bool,
    pub hidden: bool,
    pub error: Option<String>,
    pub backend: String,
    pub adapter: String,
    pub canvas: [u32; 2],
    pub render: [u32; 2],
    pub pixel_ratio: f64,
    pub fps: f32,
    pub frame_ms: f32,
    pub gpu_ms: Option<f32>,
    pub draw_calls: u32,
    pub triangles: u32,
    pub entities: u32,
    pub cpu_fallback: bool,
    pub frames: u64,
    pub surface_rebuilds: u32,
}

impl Default for DiagnosticsSnapshot {
    fn default() -> Self {
        Self {
            instance: String::new(),
            running: false,
            paused: false,
            hidden: false,
            error: None,
            backend: "unknown".to_string(),
            adapter: String::new(),
            canvas: [0, 0],
            render: [0, 0],
            pixel_ratio: 1.0,
            fps: 0.0,
            frame_ms: 0.0,
            gpu_ms: None,
            draw_calls: 0,
            triangles: 0,
            entities: 0,
            cpu_fallback: false,
            frames: 0,
            surface_rebuilds: 0,
        }
    }
}

pub(crate) struct FrameSample {
    pub fps: f32,
    pub frame_ms: f32,
    pub gpu_ms: Option<f32>,
    pub draw_calls: u32,
    pub triangles: u32,
    pub entities: u32,
    pub cpu_fallback: bool,
    pub surface_rebuilds: u32,
    pub presented: bool,
}

#[derive(Clone)]
struct OverlayHandles {
    fps: Option<Element>,
    backend: Option<Element>,
    resolution: Option<Element>,
    frame: Option<Element>,
    state: Option<Element>,
    error: Option<Element>,
}

thread_local! {
    static SNAPSHOT: RefCell<DiagnosticsSnapshot> = RefCell::new(DiagnosticsSnapshot::default());
    static OVERLAY: RefCell<Option<OverlayHandles>> = RefCell::new(None);
    static LAST_OVERLAY_MS: Cell<f64> = Cell::new(0.0);
}

pub(crate) fn bind_overlay(document: &Document) {
    let handles = OverlayHandles {
        fps: element(document, "rafi-fps"),
        backend: element(document, "rafi-backend"),
        resolution: element(document, "rafi-resolution"),
        frame: element(document, "rafi-frame"),
        state: element(document, "rafi-state"),
        error: element(document, "rafi-error"),
    };
    OVERLAY.with(|slot| *slot.borrow_mut() = Some(handles));
}

pub(crate) fn configure(backend: &str, adapter: &str, instance: &str) {
    SNAPSHOT.with(|slot| {
        let mut snapshot = slot.borrow_mut();
        snapshot.backend = backend.to_string();
        snapshot.adapter = adapter.to_string();
        snapshot.instance = instance.to_string();
    });
    write_overlay(true);
}

pub(crate) fn set_viewport(canvas: [u32; 2], render: [u32; 2], pixel_ratio: f64) {
    SNAPSHOT.with(|slot| {
        let mut snapshot = slot.borrow_mut();
        snapshot.canvas = canvas;
        snapshot.render = render;
        snapshot.pixel_ratio = pixel_ratio;
    });
    write_overlay(true);
}

pub(crate) fn set_running(running: bool) {
    SNAPSHOT.with(|slot| slot.borrow_mut().running = running);
    write_overlay(true);
}

pub(crate) fn set_paused(paused: bool) {
    SNAPSHOT.with(|slot| {
        let mut snapshot = slot.borrow_mut();
        snapshot.paused = paused;
        if paused {
            snapshot.fps = 0.0;
        }
    });
    write_overlay(true);
}

pub(crate) fn set_hidden(hidden: bool) {
    SNAPSHOT.with(|slot| {
        let mut snapshot = slot.borrow_mut();
        snapshot.hidden = hidden;
        if hidden {
            snapshot.fps = 0.0;
        }
    });
    write_overlay(true);
}

pub(crate) fn record_frame(sample: FrameSample) {
    SNAPSHOT.with(|slot| {
        let mut snapshot = slot.borrow_mut();
        snapshot.fps = sample.fps;
        snapshot.frame_ms = sample.frame_ms;
        snapshot.gpu_ms = sample.gpu_ms;
        snapshot.draw_calls = sample.draw_calls;
        snapshot.triangles = sample.triangles;
        snapshot.entities = sample.entities;
        snapshot.cpu_fallback = sample.cpu_fallback;
        snapshot.surface_rebuilds = sample.surface_rebuilds;
        if sample.presented {
            snapshot.frames = snapshot.frames.saturating_add(1);
        }
    });
    write_overlay(false);
}

pub(crate) fn report_error(message: &str) {
    SNAPSHOT.with(|slot| {
        let mut snapshot = slot.borrow_mut();
        snapshot.error = Some(message.to_string());
        snapshot.running = false;
        snapshot.paused = true;
        snapshot.fps = 0.0;
    });
    write_overlay(true);
}

pub(crate) fn snapshot_js() -> JsValue {
    let snapshot = SNAPSHOT.with(|slot| slot.borrow().clone());

    let root = Object::new();
    set(&root, "instance", JsValue::from_str(&snapshot.instance));
    set(&root, "running", JsValue::from_bool(snapshot.running));
    set(&root, "paused", JsValue::from_bool(snapshot.paused));
    set(&root, "hidden", JsValue::from_bool(snapshot.hidden));
    set(
        &root,
        "error",
        snapshot
            .error
            .as_deref()
            .map(JsValue::from_str)
            .unwrap_or(JsValue::NULL),
    );

    let backend = Object::new();
    set(&backend, "kind", JsValue::from_str(&snapshot.backend));
    set(&backend, "adapter", JsValue::from_str(&snapshot.adapter));
    set(&root, "backend", backend.into());

    let canvas = Object::new();
    set(&canvas, "width", number(snapshot.canvas[0]));
    set(&canvas, "height", number(snapshot.canvas[1]));
    set(&root, "canvas", canvas.into());

    let render = Object::new();
    set(&render, "width", number(snapshot.render[0]));
    set(&render, "height", number(snapshot.render[1]));
    set(
        &render,
        "pixel_ratio",
        JsValue::from_f64(snapshot.pixel_ratio),
    );
    set(&root, "render", render.into());

    let frame = Object::new();
    set(&frame, "fps", JsValue::from_f64(f64::from(snapshot.fps)));
    set(
        &frame,
        "cpu_ms",
        JsValue::from_f64(f64::from(snapshot.frame_ms)),
    );
    set(
        &frame,
        "gpu_ms",
        snapshot
            .gpu_ms
            .map(|value| JsValue::from_f64(f64::from(value)))
            .unwrap_or(JsValue::NULL),
    );
    set(&frame, "draw_calls", number(snapshot.draw_calls));
    set(&frame, "triangles", number(snapshot.triangles));
    set(&frame, "entities", number(snapshot.entities));
    set(
        &frame,
        "cpu_fallback",
        JsValue::from_bool(snapshot.cpu_fallback),
    );
    set(&frame, "frames", number64(snapshot.frames));
    set(
        &frame,
        "surface_rebuilds",
        number(snapshot.surface_rebuilds),
    );
    set(&root, "frame", frame.into());

    root.into()
}

fn write_overlay(force: bool) {
    let now = js_sys::Date::now();
    let last = LAST_OVERLAY_MS.with(Cell::get);
    if !force && now >= last && now - last < OVERLAY_INTERVAL_MS {
        return;
    }
    LAST_OVERLAY_MS.with(|slot| slot.set(now));

    let snapshot = SNAPSHOT.with(|slot| slot.borrow().clone());
    let overlay = OVERLAY.with(|slot| slot.borrow().clone());
    let Some(overlay) = overlay else {
        return;
    };

    set_text(
        overlay.fps.as_ref(),
        &format!("{:.0}", snapshot.fps.max(0.0)),
    );
    set_text(overlay.backend.as_ref(), &snapshot.backend);
    if let Some(backend) = overlay.backend.as_ref() {
        if !snapshot.adapter.is_empty() {
            let _ = backend.set_attribute("title", &snapshot.adapter);
        }
    }
    set_text(
        overlay.resolution.as_ref(),
        &format!(
            "{}x{} render / {}x{} canvas",
            snapshot.render[0], snapshot.render[1], snapshot.canvas[0], snapshot.canvas[1]
        ),
    );

    let frame_text = match snapshot.gpu_ms {
        Some(gpu_ms) => format!(
            "{:.1} ms | gpu {:.1} ms | {} draws | {} tris",
            snapshot.frame_ms, gpu_ms, snapshot.draw_calls, snapshot.triangles
        ),
        None => format!(
            "{:.1} ms | {} draws | {} tris",
            snapshot.frame_ms, snapshot.draw_calls, snapshot.triangles
        ),
    };
    set_text(overlay.frame.as_ref(), &frame_text);

    let state = state_label(&snapshot);
    set_text(overlay.state.as_ref(), state);
    if let Some(state_element) = overlay.state.as_ref() {
        let _ = state_element.set_attribute("data-state", state);
    }

    if let Some(error_element) = overlay.error.as_ref() {
        match snapshot.error.as_deref() {
            Some(message) => {
                error_element.set_text_content(Some(message));
                let _ = error_element.remove_attribute("hidden");
            }
            None => {
                let _ = error_element.set_attribute("hidden", "");
            }
        }
    }
}

fn state_label(snapshot: &DiagnosticsSnapshot) -> &'static str {
    if snapshot.error.is_some() {
        "error"
    } else if snapshot.paused {
        "paused"
    } else if snapshot.hidden {
        "hidden"
    } else if snapshot.running {
        "running"
    } else {
        "booting"
    }
}

fn element(document: &Document, id: &str) -> Option<Element> {
    document.get_element_by_id(id)
}

fn set_text(element: Option<&Element>, text: &str) {
    if let Some(element) = element {
        element.set_text_content(Some(text));
    }
}

fn set(object: &Object, key: &str, value: JsValue) {
    let _ = Reflect::set(object.as_ref(), &JsValue::from_str(key), &value);
}

fn number(value: u32) -> JsValue {
    JsValue::from_f64(f64::from(value))
}

fn number64(value: u64) -> JsValue {
    JsValue::from_f64(value as f64)
}
