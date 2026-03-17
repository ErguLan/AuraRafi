# AuraRafi — GEMINI.md

Contexto del proyecto para continuar desarrollo con Gemini / Antigravity.

## Que es esto

Motor hibrido open-source en Rust. Dos caracteristicas unicas combinadas: **videojuegos (2D/3D)** + **diseno electronico (esquematicos, PCB, simulacion)**. Corre en hardware barato. Objetivo futuro: que cualquier nino lo use sin necesidad de saber programar.

Creado vibe-coding con Antigravity + Yoll IDE. Branding de Yoll solo en pantalla de carga, sutil.

## Herramientas usadas en este proyecto

- **Antigravity** (Google Deepmind) — agente principal de coding
- **Yoll IDE** — IDE ligero del creador, yoll.site
- PowerShell en Windows
- Rust toolchain GNU (`stable-x86_64-pc-windows-gnu`) para no depender de Visual Studio

## Estado: v0.1.1

### Funciona
- UI completa: loading screen, project hub, main editor con todos los paneles
- Schematic editor full: DRC, simulacion MNA, export SVG/BOM/Gerber/Netlist, compartir circuitos
- Node editor con executor que corre grafos visuales
- Traducciones EN/ES en toda la UI
- Settings: simple mode, target platform, render quality
- raf_hardware: modelos de datos para serial/sensores/actuadores/robot/ML

### NO funciona aun
1. Rendering 3D (wgpu setup existe, sin shaders ni camara) — PRIORIDAD
2. AI Chat sin LLM conectado
3. Asset pipeline (browser existe, sin importacion real)
4. Serial I/O real (falta crate `serialport`)
5. Networking (stubs vacios)

## Workspace

```
editor/           # Binario principal, carga icon.png como icono de ventana
crates/
  raf_core/       # Base de todo: ECS, scene graph, CommandBus, EventBus, config
  raf_render/     # wgpu camera + pipeline + mesh.rs + projection.rs (sin rendering aun)
  raf_editor/     # UI: app.rs state machine, theme.rs, panels/
  raf_assets/     # Importador y browser de assets
  raf_electronics/# Electronica: componentes, esquematico, netlist, DRC, MNA, exports
  raf_nodes/      # Visual scripting: nodos, grafo, executor
  raf_ai/         # ToolRegistry + ChatPanel (sin LLM)
  raf_net/        # Protocolo de red (stubs)
  raf_hardware/   # Serial, sensores, actuadores, robot, ML data
```

## Como correr (Windows sin Visual Studio)

```powershell
# Setup unico:
rustup default stable-x86_64-pc-windows-gnu
winget install BrechtSanders.WinLibs.POSIX.UCRT

# Correr:
cargo run -p aura_rafi_editor
```

El `.cargo/config.toml` apunta a `target_gnu` para evitar conflictos con builds MSVC anteriores.

## Reglas criticas — nunca romper

| Regla | Razon |
|-------|-------|
| Sin dependencias pesadas | Debe correr en patatas |
| Colores de `theme.rs` always | No hardcodear colores |
| Todo mutacion por CommandBus | Undo/redo + AI tools |
| Sin emojis en codigo | Consistencia |
| Traducciones EN/ES | Publico hispanohablante es el target principal |
| Panels usan `fn show(&mut self, ui: &mut egui::Ui)` | Patron consistente |

## Patrones frecuentes

```rust
// Traduccion inline (no hay i18n framework aun):
let is_es = self.settings.language == Language::Spanish;
ui.label(if is_es { "Hola" } else { "Hello" });

// Agregar entidad a escena:
let id = self.scene.add_root_with_primitive("Cubo 1", Primitive::Cube);
self.hierarchy.selected_node = Some(id);
self.viewport.selected = Some(id);

// Cargo check rapido antes de cargo run:
cargo check
```

## Proximos pasos (orden sugerido)

1. **Viewport 3D**: integrar raf_render con el viewport usando la proyeccion ya hecha en `projection.rs` + shaders WGSL minimos para flat shading
2. **AI Chat funcional**: conectar `raf_ai` a OpenRouter (API key del usuario)
3. **Asset drag-drop**: `raf_assets` con watch de filesystem y thumbnails
4. **serialport real**: `raf_hardware` necesita I/O serial para conectar Arduino/ESP32
5. **Modo nino**: UI simplificada que oculta todos los paneles tecnicos

## Contexto importante para Antigravity

- Los cambios de permisos en Windows a veces resetean archivos — verificar con grep antes de asumir que algo fue editado
- `target_gnu` es el build dir, no `target/`
- El proceso del editor bloquea escritura a `target/` — cerrarlo antes de builds limpios
- `cargo check` es rapido, `cargo run` compila todo de nuevo si hay cambios en deps
- Los errores de `is_es` fuera de closures son comunes — definir la variable antes del bloque

## Links

- GitHub: https://github.com/ErguLan/AuraRafi
- Yoll: https://yoll.site
