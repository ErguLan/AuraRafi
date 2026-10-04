# Script Runtime Preparation

The local runtime implementation now connects manual native Play through
`raf_runtime` (isolated simulation) and `raf_player` (Winit/RafUI/ApiGraphicBasic
host). See [Local Runtime](LOCAL_RUNTIME.md) for acceptance evidence and
remaining live-validation gates. Historical preparation notes below do not
override this active ownership map.

## Current Contract

- Rhai assets attach to scene nodes. Supported session graphs compile into
  a global Rhai attachment in the Play snapshot; the authoring Compile action
  returns source without writing files. See CAMERA_RUNTIME.md.
- `script.run` executes `on_start` against a cloned scene only. It is a safe
  authoring check and never mutates the active editor scene.
- `GameRuntimeState` is an editor-side preparation harness for cloned-scene
  hook loading and input snapshots. It is not the shipped runtime.
- The native Play controls launch an isolated snapshot, not the editor build
  action. Attached Agent/CLI/MCP launch remains disabled.
- WASM/native-module support remains a future adapter boundary. It does not
  run as an implicit editor fallback.

## Connection Rule

The future runtime should consume the same serialized scene, primitive asset
manifests, script attachment paths, and `EditorCameraBlock`-independent camera
components. It must own its own window/surface lifecycle and never reuse the
editor's RafUI shell as an in-game UI host.

This keeps today’s script assets useful without forcing runtime behavior into
the editor before its platform, rendering, and sandbox contracts are ready.
