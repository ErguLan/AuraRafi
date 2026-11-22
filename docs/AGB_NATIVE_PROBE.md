# ApiGraphicBasic native probe

The probe opens its own device and presents independently of the editor and player. `raf_agb` owns the shared handles, budgets, graph, scheduler and registries. `raf_render` re-exports the extracted modules at their existing public paths.

The probe uploads immutable geometry, instances and textures once, submits the scene and overlays together in painter order, and updates the camera uniform only when it changes. The selected backend owns its native allocations and synchronization. Ordinary desktop and browser presentation is event driven. Captures and the benchmark wait for completion explicitly.

| Route | Probe output |
| --- | --- |
| Vulkan | Offscreen RGBA capture and native window with resize, minimize/restore and device reconstruction |
| Direct3D 12 | Offscreen RGBA capture with a native device and queue |
| Metal | Offscreen RGBA capture on Apple targets |
| WebGPU | Canvas presentation from WASM with browser device and queue |
| Recorder | CPU validation of resource admission, generations and pass order |

## Windows and desktop commands

```powershell
cargo build -p raf_agb --release --features desktop,dx12
$probe = 'target_gnu/x86_64-pc-windows-gnu/release/raf-agb-probe.exe'
$env:RAF_GRAPHICS_BACKEND = 'vulkan'
& $probe --window --shadows
& $probe --smoke --shadows
& $probe --backend dx12 --capture artifacts/dx12-scene.png --shadows
```

The standalone executable has no WGPU dependency. Pass `--backend` to override the environment. Use `desktop,metal` with `--backend metal` for the Apple offscreen probe. The workspace defaults to a Windows GNU target, so pass the target explicitly on other platforms.

Export the retained fixtures through the same RafUI compilation and CAD commands used by the product. The export step is separate from execution.

```powershell
cargo run -p raf_render --features agb-probe-fixtures --example agb_workload -- artifacts/agb-fixture.json 1280x720 es 100
& $probe --window --workload artifacts/agb-fixture.json
./tools/agb_compare.ps1 -Probe $probe -Output artifacts/agb-ab -Workload artifacts/agb-fixture.json
```

The last exporter argument selects 100, 125, 150 or 200 percent raster density. Keep the same physical viewport size, fixture, camera motion and driver when comparing lanes. The comparison script runs each lane in a separate process, alternates order and saves JSON and the first capture for each fixture. `frame_complete_ms` includes submission and the completion wait. `cpu_working_set_bytes` is sampled before capture. GPU process memory is reported on Windows when the driver provides the performance counters. Logical resident bytes represent the admitted workload and attachments and are measured separately from driver memory.

Use `--soak-seconds 600` to run a separate 30 FPS orbit in the same process after the timed benchmark. This records CPU and GPU memory at ten-second intervals and at the start and end.

## Browser commands

```powershell
cargo rustc -p raf_agb --lib --target wasm32-unknown-unknown --features webgpu --release --crate-type cdylib
wasm-bindgen --target web --out-dir crates/raf_agb/web/pkg target_gnu/wasm32-unknown-unknown/release/raf_agb.wasm
```

Use the `wasm-bindgen` CLI version pinned in `Cargo.lock`. Serve `crates/raf_agb/web` on localhost or HTTPS and open `index.html`. Place a retained fixture beside the page and set `?fixture=agb-fixture.json` to draw the same input as the native probe.

The current shader package covers instanced textured geometry, depth, planar fixture shadows and alpha compositing. The retained export covers clipped strokes, text atlas, images, rounded borders, gradients and bounded edge softness. A fixed probe fixture stays resident across camera moves. The live editor/player surfaces now use the same product contract; platform certification, mobile runtime validation and broader effect parity remain separate from a working probe.

## Verification on 2026-11-20

- Windows on Intel UHD Graphics, driver 31.0.101.4502. The standalone Vulkan window presented before and after resize, minimize/restore, and device reconstruction. A native build with `desktop,dx12` had no WGPU packages in its normal dependency tree.
- The same retained data rendered with Vulkan, Direct3D 12 and Chrome WebGPU. Vulkan captures cover 100, 125, 150 and 200 percent raster density. The Direct3D 12 static capture matched the corresponding reference PNG byte-for-byte.
- Linux desktop Vulkan, Android Vulkan, macOS Metal and iOS Metal passed cross-target Rust checks. Runtime evidence in this run covers Windows and Chrome on Windows.
- The A/B run uses identical workload geometry, shader source, frame size and adapter within each API pair. It measures the independent probe. The active editor and player remain outside this comparison.

### Retained static workload at 1280 × 720

The figures are medians across three Vulkan trials or five Direct3D 12 trials. Each trial measured 480 frames after warmup. The WGPU rows are historical evidence from before 0.11.5; the reference executor is no longer part of the workspace.

| Executor | Process working set | Shared graphics memory | Frame completion p95 |
| --- | ---: | ---: | ---: |
| Vulkan | 52.39 MiB | 23.73 MiB | 0.58 ms |
| WGPU/Vulkan (historical) | 78.93 MiB | 47.84 MiB | 1.11 ms |
| Direct3D 12 | 45.20 MiB | 15.06 MiB | 0.91 ms |
| WGPU/Direct3D 12 (historical) | 120.02 MiB | 88.26 MiB | 1.11 ms |

Direct3D 12 completed the orbit and repeated-mesh fixtures with a lower median p95 after caching command interfaces and configuring an optimized depth clear. Vulkan finished the repeated-mesh fixture at 1.15 ms p95. Both native executors used less process RAM and shared graphics memory in all three fixtures than the historical WGPU lanes.

In a separate Vulkan run, a 600-second orbit delivered 17,855 frames against a 30 FPS target. Shared GPU memory stayed at 23.74 MiB across 61 samples. Process working set ranged from 67.14 to 67.56 MiB. Captures, raw comparisons and the soak report are under `artifacts/agb-foundation/` in the measured checkout.

The AGB unit suite passed 33 of 33 tests. The existing renderer suite passed 344 of 345 tests. The `crowded_flow_compresses_without_sibling_overlap` failure was reproduced on the source snapshot of `621319bf07`.
