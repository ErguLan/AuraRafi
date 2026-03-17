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

## ADR-003: Traducciones inline, no framework i18n

**Decision**: `if is_es { "Texto ES" } else { "Text EN" }` en cada lugar.

**Razon**: Agregar `fluent` o similar agrega complejidad de archivos de traduccion, hot-reload, fallbacks. Para 2 idiomas, inline es mas simple y facil de mantener.

**Futuro**: Migrar a `fluent` cuando haya 3+ idiomas o contribuidores de comunidad.

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
