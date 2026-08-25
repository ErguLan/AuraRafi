# Runbooks — Como hacer cosas comunes

## Agregar un nuevo panel al editor

1. Crear un `*_surface.rs` declarativo en `crates/raf_editor/src/panels/`.
2. Implementar un `*_surface.rs` declarativo y un host nativo sólo si requiere
   estado temporal, input, composición o persistencia.
3. Registrar el módulo en `panels/mod.rs` si es una superficie reutilizable.
4. Componer la superficie desde `native_workbench.rs` o el host nativo dueño
   de esa región; no crear un host Egui/Eframe.
5. Emitir comandos semánticos RafUI y resolverlos en el controlador o gateway
   de dominio correspondiente.

---

## Agregar un componente electronico nuevo

1. Agregar variante al enum `SimModel` en `raf_electronics/src/component.rs`
2. Implementar parsing en `SimModel::parse()`
3. Agregar a `ComponentLibrary::default()` en `library.rs`
4. Agregar su representación a `raf_electronics/src/cad_scene.rs` y validar
   sus líneas/pines en la escena nativa.
5. Agregarla al DRC si tiene reglas especiales (`raf_electronics/src/drc.rs`)

---

## Agregar un nodo nuevo al visual scripting

1. Agregar variante en `NodeCategory` si es nueva categoria (`raf_nodes/src/node.rs`)
2. Crear el nodo via `Node::new()` con sus pins definidos
3. Registrar en la paleta del node editor (`panels/node_editor.rs`)
4. Implementar logica de ejecucion en `raf_nodes/src/executor.rs` -> match en `execute_node()`

---

## Agregar traduccion a texto nuevo

```rust
// Al inicio del bloque UI:
let is_es = self.settings.language == Language::Spanish;

// Luego en cada string:
ui.label(if is_es { "Mi texto en espanol" } else { "My text in English" });
```

---

## Correr el editor (primera vez)

```powershell
rustup default stable-x86_64-pc-windows-gnu
winget install BrechtSanders.WinLibs.POSIX.UCRT
# Cerrar y reabrir terminal
cargo run -p aura_rafi_editor
```

## Correr el editor (normal)

```powershell
cargo run -p aura_rafi_editor
```

## Check rapido sin compilar binario

```powershell
cargo check
```

---

## Debug de builds rotos

1. Si falla con `dlltool not found`: reiniciar terminal (PATH de MinGW no cargo)
2. Si falla con `file locked`: cerrar el proceso `aura_rafi_editor.exe` primero
3. Si los cambios no se ven: verificar con grep que el archivo realmente cambio
4. Si falla en `target/` vs `target_gnu/`: verificar `.cargo/config.toml` existe y tiene `target-dir = "target_gnu"`

---

## Push a GitHub

```powershell
git add -A
git commit -m "descripcion del cambio"
git push origin main
```
