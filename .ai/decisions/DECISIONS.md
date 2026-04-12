# Decisiones de Arquitectura

Registro de decisiones importantes y por que se tomaron.

## ADR-001: CPU rendering en viewport (no GPU inmediato)

**Decision**: El viewport usa egui painter con proyeccion matematica, NO un pipeline wgpu completo.

**Razon**: Setup de wgpu requiere shaders WGSL, render passes, buffers de GPU — todo eso agrega tiempo de compilacion y puede fallar en hardware sin drivers modernos. Con egui painter, un cubo proyectado en 2D se ve igual de bien para el editor y corre en cualquier cosa.

**Consecuencia**: El rendering final del juego (no el editor) si usara wgpu. El viewport del editor es preview ligero.

---

## ADR-002: GNU toolchain, no MSVC

**Decision**: `stable-x86_64-pc-windows-gnu` + MinGW, no Visual Studio.

**Razon**: Visual Studio Build Tools pesa ~6GB. MinGW pesa ~200MB. El objetivo es que cualquier persona pueda clonar y correr en 10 minutos.

**Consecuencia**: `.cargo/config.toml` fija el target y `target_gnu` como build dir.

---

## ADR-003: [DEPRECATED] Traducciones inline, no framework i18n

**Decision original**: `if is_es { "Texto ES" } else { "Text EN" }` en cada lugar.

**Razon de deprecacion**: Escalo excesivamente mal en las vistas de editor. Reemplazado por ADR-007.

---

## ADR-004: RON para serializacion de proyectos

**Decision**: RON (Rusty Object Notation) en vez de JSON o TOML para guardar proyectos.

**Razon**: RON soporta structs de Rust directamente con serde, es legible por humanos, y mas expresivo que TOML para estructuras anidadas.

---

## ADR-005: CommandBus como capa universal de mutacion

**Decision**: Toda mutacion de estado del engine pasa por `raf_core::CommandBus`.

**Razon**: Undo/redo, replay, y AI tool-calling funcionan TODOS con el mismo mecanismo. Un agente de IA emite comandos identicos a los que emite el usuario.

---

## ADR-006: SceneGraph plano (Vec) en vez de arbol de punteros

**Decision**: Todos los nodos viven en un `Vec<SceneNode>` contiguo con indices como referencias.

**Razon**: Cache-friendly para iteracion. O(1) lookup. Sin allocations fragmentadas en el heap. El arbol se reconstruye via indices parent/children.

---

## ADR-007: Unified JSON i18n Translation System

**Decision**: Implement a custom lightweight JSON i18n engine (`raf_core::i18n::t()`) embedding `en.json` and `es.json` inside the binary via `include_str!`.

**Razon**: Inline translations (ADR-003) cluttered the UI rendering logic severely. A zero-dependency JSON parser embedded inside the engine provides O(1) string lookups via HashMaps with 0 compile-time build script complexitiy (avoiding Fluent overhead).

---

## ADR-008: 3D PCB Layout powered by Game Engine Viewport

**Decision**: Electronics projects use the exact same 3D `SceneGraph` and CPU Viewport Painter used by Game projects to generate PCB layouts at runtime, instead of constructing a separate 2D PCB module.

**Razon**: Prevents code duplication. A PCB is inherently a 3D construct. Mapping schematic X/Y coordinates directly into X/Z 3D primitives (Cubes for FR4/Chips, Cylinders for pads) unifies the Game and Electronics domains naturally and gives users a CAD experience instantly.

---

## ADR-009: Data-driven Moddable Electronic Components

**Decision**: Electronic components (`SimModel`, `Pin`, `Footprint`) are serialized into `.ron` files inside an external `ElectricalAssets/` directory. The engine loads them dynamically at runtime rather than relying on Rust function pointers.

**Razon**: Extensibility. Modders or hardware engineers can add new components (like sensors, ICs, or batteries) by just dragging a `.ron` file into the folder without needing to recompile the Rust engine source code.
