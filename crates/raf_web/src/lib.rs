//! WebAssembly canvas host for the AuraRafi renderer core.
//!
//! This crate is the browser platform boundary for `raf_render` and
//! `ApiGraphicBasic`. It owns the canvas element and one AGB
//! `ProductDevice<WebGpu>`; canvas presentation happens inside that executor,
//! not through a foreign presenter or a second blit path. The scene itself is
//! still built and recorded by the same `raf_core::SceneGraph` and
//! `raf_render::scene_renderer::SceneRenderer` used by the desktop viewport.
//!
//! It does not load projects, scripts, textures, or audio. Those systems still
//! depend on `std::fs`, native threads, and the Windows audio backend; the
//! browser needs a virtual file system and a web audio path first. The current
//! limits are documented in `docs/WASM.md`.

#![cfg(target_arch = "wasm32")]

mod app;
mod diagnostics;
mod scene;

use wasm_bindgen::prelude::*;

/// Browser entry point.
///
/// Installs the panic hook so Rust panics reach the browser console, then
/// starts the asynchronous canvas application. Initialization errors are
/// reported to the page instead of aborting silently.
#[wasm_bindgen(start)]
pub fn start() -> Result<(), JsValue> {
    console_error_panic_hook::set_once();
    wasm_bindgen_futures::spawn_local(async {
        if let Err(error) = app::run().await {
            app::report_fatal(&error);
        }
    });
    Ok(())
}

/// Pause or resume the demo frame schedule.
///
/// Pausing cancels the `requestAnimationFrame` chain, so a paused page performs
/// no scene, GPU, or present work until a host calls this function again with
/// `false`.
#[wasm_bindgen]
pub fn set_paused(paused: bool) {
    app::set_paused(paused);
}

/// Returns the current diagnostics as a plain JavaScript object.
///
/// The object is rebuilt on demand and is intended for browser tooling,
/// automated smoke checks, and the page overlay. It never drives rendering.
#[wasm_bindgen]
pub fn stats() -> JsValue {
    diagnostics::snapshot_js()
}
