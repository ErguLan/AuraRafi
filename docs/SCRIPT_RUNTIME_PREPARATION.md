# Script Runtime Preparation

Scripting is intentionally prepared before the shipping game runtime is
connected. The editor can create, attach, list, validate, and compile script
assets now, while keeping the renderer and editor workflow independent from a
runtime process.

## Current Contract

- Rhai files and node-graph compilation produce project assets that can be
  attached to scene nodes.
- `script.run` executes `on_start` against a cloned scene only. It is a safe
  authoring check and never mutates the active editor scene.
- `GameRuntimeState` is an editor-side preparation harness for cloned-scene
  hook loading and input snapshots. It is not the shipped runtime.
- The editor build action keeps runtime launch disabled while the runtime
  boundary, renderer, and platform packaging are still being stabilized.
- WASM/native-module support remains a future adapter boundary. It does not
  run as an implicit editor fallback.

## Connection Rule

The future runtime should consume the same serialized scene, primitive asset
manifests, script attachment paths, and `EditorCameraBlock`-independent camera
components. It must own its own window/surface lifecycle and never reuse the
editor's RafUI shell as an in-game UI host.

This keeps today’s script assets useful without forcing runtime behavior into
the editor before its platform, rendering, and sandbox contracts are ready.
