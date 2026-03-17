# Skills — Flujos de trabajo para IA

## skill: agregar-feature-electronica

**Cuando usar**: El usuario pide agregar un componente, regla DRC, o funcion de exportacion al schematic editor.

**Archivos clave**:
- `crates/raf_electronics/src/component.rs` — tipos de componentes y SimModel
- `crates/raf_electronics/src/library.rs` — biblioteca de partes incorporadas
- `crates/raf_electronics/src/schematic.rs` — estructura del esquematico
- `crates/raf_electronics/src/netlist.rs` — generacion de netlist
- `crates/raf_editor/src/panels/schematic_view.rs` — UI del editor

**Pasos tipo**:
1. Leer `component.rs` para entender SimModel
2. Agregar variante, implementar parse
3. Agregar a library.rs
4. Agregar dibujo en schematic_view.rs
5. `cargo check`

---

## skill: agregar-nodo-visual

**Cuando usar**: El usuario pide un nuevo tipo de nodo en el visual scripting.

**Archivos clave**:
- `crates/raf_nodes/src/node.rs` — definiciones de nodos y pins
- `crates/raf_nodes/src/executor.rs` — logica de ejecucion
- `crates/raf_editor/src/panels/node_editor.rs` — UI de la paleta

---

## skill: mejorar-viewport

**Cuando usar**: El usuario pide mejoras visuales al viewport de escena.

**Archivo clave**: `crates/raf_editor/src/panels/viewport.rs`

**Importante**: Mantener CPU rendering (egui painter + proyeccion matematica). NO agregar pipeline GPU sin discutirlo primero.

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
