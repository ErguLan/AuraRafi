# Primitive Asset Manifests

AuraRafi treats base 3D primitives as internal asset manifests instead of opaque generated code.

The goal is simple: creating a cube, sphere, cylinder, or plane should feel like importing a small built-in model. The editor can still render those shapes through the lightweight primitive mesh path, but the scene keeps a source marker so tools, AI actions, serialization, and future importers know where the object came from.

## Rules

- Built-in primitives live as JSON manifests under `crates/raf_assets/src/prefabs/`.
- `/game.add primitive=cube` imports `builtin://primitive/cube`.
- The created node stores `source_asset` and `source_schema_version`.
- The renderer may use procedural GPU mesh recipes internally for performance, but editor commands and AI tools should talk in asset/import terms.
- Compound prefabs use the same manifest schema and can create a folder plus child parts.
- The system must remain lightweight: no Chromium, no heavyweight model pipeline for simple blocks, and no forced GPU-only path.

## Runtime Direction

The manifest path is prepared for future runtime/export work, but it does not activate product runtime behavior by itself. Runtime integration should read the same `source_asset` metadata later and resolve it through the asset registry.

## CPU/GPU Policy

Primitive manifests are data. They should be cheap to parse on CPU, cheap to instantiate in the scene graph, and cheap to render on GPU through shared mesh recipes or cached buffers. On low-resource devices, the manifest data stays the same while the render tier decides whether to use CPU fallback, reduced scale, or full GPU presentation.

## UI Direction

UI labels should describe these actions as import/asset operations, not code generation. This keeps the editor aligned with a professional tool workflow and avoids suggesting that scene geometry is being created by one-off generated source code.
