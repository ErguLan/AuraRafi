# GPU-First Graphics API Design: ApiGraphicBasic

This document details the architecture and design of `ApiGraphicBasic`, a lightweight, unified graphics API abstraction layer for AuraRafi. It sits between the editor panels (game scene viewport and electronics workspace) and the underlying execution hardware (GPU or CPU software fallback).

---

## 1. Core Principles

1. **GPU-First, CPU-Fallback**: The engine prioritizes GPU hardware execution via a private `wgpu` instance. If hardware drivers are missing or initialization fails, the engine falls back to CPU-based software rasterization.
2. **Generous GPU Compatibility Margins**: When requesting the GPU device, the engine prioritizes low-power adapters (integrated laptop GPUs) and uses downlevel default limits (WebGL2 compatible) to run on older or lower-spec graphics hardware.
3. **No Raw API Pollution**: The rest of the engine does not interact with Vulkan, DirectX, Metal, or raw `wgpu` pipeline and bind group boilerplate. All drawing operations flow through the simplified interfaces in `ApiGraphicBasic`.
4. **Reactive Rendering**: To prevent the editor from acting as a resource hog, rendering is reactive. A frame is only drawn when camera state, entity transforms, or user inputs modify the viewport. Idle editor state consumes 0% GPU/CPU rendering resources.

---

## 2. Abstraction Interfaces

The graphics abstraction is composed of four main modules in `crates/raf_render/src/ApiGraphicBasic/`:

### A. Device Driver (`BasicDevice`)
Manages the hardware execution context. Attempts GPU initialization first, fallback to CPU software rasterization on failure.

```rust
pub struct BasicDevice {
    backend: BasicBackendType,
    wgpu_instance: Option<wgpu::Instance>,
    wgpu_adapter: Option<wgpu::Adapter>,
    wgpu_device: Option<wgpu::Device>,
    wgpu_queue: Option<wgpu::Queue>,
}
```

### B. Pipelines (`BasicPipelineKind`)
Exposes pre-configured shaders and render states instead of dynamic custom shader compiling:
* `FlatColor`: Rapid solid geometry rendering with flat face shading.
* `UnlitWireframe`: Grid lines, selection boundaries, and debug overlays.
* `PbrLit`: Full metallic/roughness PBR lighting (fully active on GPU, simplified/unlit fallback on CPU).
* `XRay`: Solid/wireframe rendering that bypasses the depth test (for gizmos and transformation alignment).

### C. Geometry (`BasicMesh` & `BasicVertex`)
A unified vertex and index container representing 3D geometry. Vertex attributes include:
* `position`: 3D coordinate vector.
* `normal`: Surface orientation vector.
* `uv`: Texture map coordinate coordinates.

### D. Command List (`BasicCommandList`)
Recorded commands issued by editor views for a frame:
* `Clear`: Resets the rendering target color buffer.
* `SetPipeline`: Binds a pipeline state.
* `DrawMesh`: Draws a registered indexed mesh with transform matrices and tints.
* `DrawLine`: Draws a 3D line with custom thickness and depth-test flags.
* `DrawGrid`: Renders the coordinate grid.

---

## 3. GPU Integration for Electronics (Schematics & PCB Views)

To unify game rendering and electronics design, the Schematic and PCB views are migrated to draw directly via the GPU-first pipeline:

```
Schematic View / PCB View
  --> Emits BasicCommandList (DrawMesh for footprints, DrawLine for wires/routes)
  --> Executed by BasicDevice
  --> GPU Hardware (60+ FPS) or CPU Software fallback
```

* **Footprints & Symbols**: Schematic symbols and PCB footprints are decomposed into simple GPU primitive recipes (Cubes, Planes, Cylinders).
* **Traces and Wires**: Rendered as fast 3D/2D line buffers on the GPU, allowing smooth zooming, panning, and instant visual routing updates.
* **Synchronized Render Loop**: Both the game viewport and the electronics editor canvas use the identical graphics backend. This maintains low memory usage and high responsiveness across the entire workspace.
