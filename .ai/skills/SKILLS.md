# Skills — Flujos de trabajo para IA

## skill: agregar-feature-electronica

**Cuando usar**: El usuario pide agregar un componente, regla DRC, o funcion de exportacion al schematic editor.

**Archivos clave**:
- `crates/raf_electronics/src/component.rs` — tipos de componentes y SimModel
- `crates/raf_electronics/src/library.rs` — biblioteca de partes incorporadas
- `crates/raf_electronics/src/schematic.rs` — estructura del esquematico
- `crates/raf_electronics/src/netlist.rs` — generacion de netlist
- `crates/raf_editor/src/native_electronics.rs` — adaptador de canvas CAD nativo

**Pasos tipo**:
1. Leer `component.rs` para entender SimModel
2. Agregar variante, implementar parse
3. Agregar a library.rs
4. Agregar la presentación al canvas CAD nativo sólo después de definir el
   contrato de interacción; durante la estabilización la frontera sigue verde
5. `cargo check`

---

## skill: agregar-nodo-visual

**Cuando usar**: El usuario pide un nuevo tipo de nodo en el visual scripting.

**Archivos clave**:
- `crates/raf_nodes/src/node.rs` — definiciones de nodos y pins
- `crates/raf_nodes/src/executor.rs` — logica de ejecucion
- `crates/raf_editor/src/native_workbench.rs` — punto de integración RafUI

---

## skill: mejorar-viewport

**Cuando usar**: El usuario pide mejoras visuales al viewport de escena.

**Archivos clave**: `crates/raf_editor/src/panels/viewport_controller.rs`,
`crates/raf_render/src/bridge/`, y `crates/raf_editor/src/native_workbench.rs`

**Importante**: Mantener el canvas bajo ApiGraphicBasic: GPU WGPU como adapter
privado cuando esté disponible y CPU como recuperación. RafUI posee el chrome y
la interacción retenida; no reintroducir el host retirado ni crear un segundo renderer.

---

## skill: traducir-ui

**Cuando usar**: El usuario pide agregar espanol a textos nuevos.

**Patron**:
```rust
let is_es = self.settings.language == Language::Spanish;
// o dentro de closures donde no hay acceso a self:
let is_es = lang == Language::Spanish;
```

---

## skill: debug-compilacion

**Cuando usar**: Hay errores de compilacion.

**Checklist**:
1. `cargo check` primero (mas rapido que `cargo run`)
2. Leer TODOS los errores antes de editar — muchos son consecuencia de uno solo
3. Verificar que imports esten correctos (`use raf_core::scene::graph::Primitive;`)
4. Si `is_es` da error de scope: esta definido dentro de un closure — redefinirlo fuera
5. Si hay warnings de unused: prefijar con `_` el nombre de la variable
