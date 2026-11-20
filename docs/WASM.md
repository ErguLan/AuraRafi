# WebAssembly Build Target

Status: experimental, demo-scoped and buildable. This is the v0.11 WebAssembly
target for the renderer core, not the editor and not the full Player.

`cargo check -p raf_web --target wasm32-unknown-unknown` passes. The platform
gates that previously blocked a wasm build are closed and listed below. This
document still does not claim a full browser Player: project loading, scripts,
audio and the editor remain out of scope.

`raf_web` hosts an AuraRafi scene in a browser canvas. It reuses the same scene
graph, command recording, `SceneRenderer`, `BasicDevice`, and
`SceneFrameOutput` contract as the native viewport. The only browser-specific
graphics code is the canvas surface lifecycle and the final present blit.

## What This Target Does

- Creates a `wgpu::Instance` with the browser WebGPU backend and the WebGL2
  fallback (`Backends::BROWSER_WEBGPU | Backends::GL`).
- Requests a low-power adapter against the real canvas surface.
- Requests `Limits::downlevel_defaults()` on WebGPU and
  `Limits::downlevel_webgl2_defaults()` on the WebGL2 fallback. The WebGPU
  floor can exceed WebGL2's guaranteed buffer and texture limits, so using it
  unconditionally would reject some WebGL2 drivers.
- Builds a bounded, data-driven demo scene from an embedded RON recipe:
  a ground plane, four primitives with distinct colors, a sun, a gradient sky,
  and ambient light.
- Records the scene with `raf_render::scene_renderer::SceneRenderer::build_frame`.
- Executes that frame with `ApiGraphicBasic::BasicDevice` using a host-shared
  `SharedGraphicsContext` and `GraphicsMemoryBudget::potato()`.
- Presents `SceneFrameOutput::GpuTexture` to the canvas, with a CPU-pixel
  fallback path when `BasicDevice` chooses its recovery renderer.
- Bounds the canvas to 1920x1080 physical pixels, the device pixel ratio to 2,
  and the scene render target to 1.44 million pixels (1600x900 equivalent).
- Renders at most 30 frames per second for the demo.
- Stops the animation-frame chain entirely when paused or when the page is
  hidden. A paused or hidden page performs no scene, GPU, or present work.
- Reconfigures the surface on resize, surface loss, or outdated swap chains.
- Exposes `set_paused(bool)` and `stats()` to browser hosts.
- Shows a small diagnostics overlay with FPS, backend, resolution, frame time,
  draw calls, triangle count, and state.

## Build Requirements

- A Rust toolchain with the `wasm32-unknown-unknown` target:
  `rustup target add wasm32-unknown-unknown`
- A `wasm-bindgen` CLI that exactly matches the `wasm-bindgen` crate version in
  `Cargo.lock` (currently 0.2.113):
  `cargo install wasm-bindgen-cli --version 0.2.113`
- The first build resolves the web crates (`web-sys`, `js-sys`,
  `wasm-bindgen-futures`, `console_error_panic_hook`), so it needs registry
  access.

## Build

Windows PowerShell:

```powershell
.\crates\raf_web\build.ps1
```

POSIX shell:

```sh
./crates/raf_web/build.sh
```

The scripts run:

```sh
cargo build -p raf_web --target wasm32-unknown-unknown --release
wasm-bindgen --target web --out-dir crates/raf_web/pkg --out-name raf_web \
  <target-dir>/wasm32-unknown-unknown/release/raf_web.wasm
```

`<target-dir>` is read from `cargo metadata`, so the workspace `.cargo/config.toml`
target directory is respected. The scripts stop with an actionable message if
`cargo`, the wasm target, or a matching `wasm-bindgen` CLI is missing.
`crates/raf_web/pkg/` is generated output and should not be edited or committed.

## Run

Serve the crate directory over HTTP. A `file://` page cannot import the
generated module.

```sh
python -m http.server 8000 --directory crates/raf_web
```

Open `http://localhost:8000/index.html`. Press `P` to pause or resume.

## JavaScript API

`raf_web.js` (generated) exports:

- `default()` / `init()`: loads and initializes the module. The
  `#[wasm_bindgen(start)]` entry then creates the canvas application.
- `set_paused(paused: boolean)`: pauses or resumes the request-animation-frame
  chain.
- `stats(): object`: current diagnostics snapshot.

`stats()` returns:

```text
{
  instance: string,          // per-page UUID for correlating console logs
  running: boolean,
  paused: boolean,
  hidden: boolean,
  error: string | null,
  backend: { kind: "WebGPU" | "WebGL2" | string, adapter: string },
  canvas: { width, height },
  render: { width, height, pixel_ratio },
  frame: {
    fps, cpu_ms, gpu_ms: number | null,
    draw_calls, triangles, entities,
    cpu_fallback: boolean,
    frames,                  // presented frames only
    surface_rebuilds
  }
}
```

`gpu_ms` is `null` unless the browser adapter exposed timestamp queries through
the device actually requested here. The host requests no optional features, so
this is normally `null` on the web.

## Limits That Are Real

These are not implemented and must not be presented as working:

- **Project loading.** `RuntimeManifest::read`, project file access, script
  loading, and texture resolution by path use `std::fs` and native paths. A
  browser build needs an asynchronous virtual file system and a fetch/OPFS
  layer before the Player can run in a page. `raf_web` deliberately has no VFS
  placeholder that could be mistaken for one.
- **Scripts.** Rhai runtime files are loaded through the native project layer.
  The demo has no scripting host.
- **Audio.** `raf_player/src/audio.rs` is Windows-only. The web target has no
  audio path.
- **Threads.** `raf_player/src/session.rs` prepares work with `std::thread`.
  `wasm32-unknown-unknown` has no thread support here, and no wasm-threads
  configuration is enabled.
- **The editor and RafUI.** No editor shells, docks, inspectors, menus, input
  adapter, or retained UI surfaces are hosted by this crate.
- **Full project assets.** No texture, mesh, or audio project files are read.
  The scene is embedded RON data.
- **Saving and runtime services.** There is no project save, storage worker,
  progress slot, or runtime service host in the browser demo.
- **Streaming.** The page does not expose the native `--stream` transport and no
  WebRTC media adapter is wired into `raf_web`.
- **Device loss recovery.** A lost surface is reconfigured. A lost device is not
  re-created, because that would discard the `BasicDevice` and scene state; the
  page reports the failure instead.
- **Unbounded resolution and FPS.** The demo intentionally caps both. A future
  Player needs the shared dynamic-resolution policy, not a per-page loop.

## Platform Gates Closed

The wasm build previously stopped in shared crates. These gates are now closed
and verified with `cargo check -p raf_web --target wasm32-unknown-unknown`.

### 1. `arboard` is native-only

`raf_render` declares `arboard` under
`[target.'cfg(not(target_arch = "wasm32"))'.dependencies]`. `NativeUiInputBridge`
still compiles on wasm; `read_clipboard_rgba` returns `None`, the paste read is
cfg-gated, and `write_clipboard` is a no-op. Browser clipboard support can be
added later through `navigator.clipboard` without changing the module boundary.

### 2. Browser-safe clock

`crates/raf_render/src/engine_time.rs` selects `std::time::Instant` on native and
a small `js_sys::Date::now()` wrapper on wasm. `device.rs` and `shadow_gpu.rs`
use that clock, so the active GPU path no longer panics on
`wasm32-unknown-unknown`.

### 3. UUID randomness

The workspace `uuid` dependency enables the `js` feature so `Uuid::new_v4()`
works in the browser. On native the feature is a no-op.

`winit` remains an unconditional dependency of `raf_render`, but winit 0.30 has
a web backend and compiles for `wasm32-unknown-unknown`. It is not a blocker;
the native window host is compiled and never instantiated by `raf_web`.

## Acceptance Criteria For Promotion Beyond Demo

Do not call this a browser Player until all of the following are true:

1. A clean clone builds with the documented commands on Windows and POSIX, with
   a pinned `wasm-bindgen-cli` version and no local patches.
2. The same page renders on WebGPU and on WebGL2, with the reported backend and
   no console validation errors.
3. Pausing and hiding the tab stop all frame work; resuming restores the scene
   without a time jump or duplicate loop.
4. Resize, DPR change, tab restore, and surface loss are exercised repeatedly
   without panics or stale frames.
5. Memory and upload counters remain bounded across a ten-minute session; the
   canvas, render target, and cache budgets stay within the documented caps.
6. A wasm project VFS can load manifests, scripts, and textures through fetch or
   OPFS; path resolution no longer depends on `std::fs`.
7. Audio and Player preparation no longer require native threads, or the web
   page explicitly declares those features unavailable.
8. A rendered wasm frame is visually compared with the same scene and camera on
   desktop, using the same command list, before parity is claimed.
9. The diagnostics contract (`stats()`) is stable enough for browser smoke tests
   and does not change shape without a version note.
