# AuraRafi — CLAUDE.md

Contexto completo del proyecto para continuar desarrollo con Claude.

## Que es esto

Motor hibrido open-source construido en Rust. Sirve para dos cosas que ningun otro engine hace juntas: **desarrollo de videojuegos (2D/3D)** y **diseno electronico (schematics, PCB, simulacion)**. El objetivo es que corra en hardware barato (laptops de gama baja, "patatas") y que eventualmente cualquier nino pueda usarlo sin complicarse.

Creado por Yoll (yoll.site). Branding sutil — solo aparece en la pantalla de carga.

## Estado actual: v0.1.1

### Lo que funciona
- Editor completo: pantalla de carga, project hub, editor principal con todos los paneles
- Sistema de temas dark/light con acento naranja (#D4771A)
- Schematic editor: colocar componentes, cablear, rotacion, duplicar, eliminar, menu contextual, edicion de valores, DRC, simulacion MNA, export SVG/BOM/Netlist/Gerber, compartir circuitos
- Node editor: canvas bezier, conectar nodos, ejecutor que corre grafos de flujo
- Viewport: grid 2D, pan/zoom, seleccion de herramientas (Q/W/E/R)
- Hierarchy + Properties panels
- Console con filtros
- Asset browser
- Settings: tema, idioma, calidad, modo simple, plataforma destino
- Traducciones EN/ES en toda la UI
- Icono personalizado (R metalica naranja)
- raf_hardware: abstraccion serial, sensores, actuadores, estado robot, ML training data

### Lo que NO funciona (prioridades)
1. **Rendering 3D real** — viewport es 2D con grid. raf_render tiene setup de wgpu pero cero shaders/meshes/camara 3D. PRIORIDAD #1.
2. **AI Chat** — panel existe, pero sin conexion a ningun LLM
3. **Asset pipeline real** — browser existe pero sin importacion, thumbnails, hot-reload
4. **Networking** — raf_net son stubs, vacio
5. **Serial I/O real** — raf_hardware tiene los modelos pero sin la crate `serialport`

## Estructura de crates

```
editor/              # Binario: lanza la ventana, carga icono, configura eframe
crates/
  raf_core/          # ECS (hecs), scene graph plano, command bus, event bus, config, project
  raf_render/        # wgpu: Camera (ortho/persp), RenderPipeline, mesh.rs, projection.rs
  raf_editor/        # UI egui: app.rs (state machine), theme.rs, panels/
    panels/
      viewport.rs    # Grid 2D, herramientas, pan/zoom — NECESITA rendering 3D
      node_editor.rs # Canvas visual scripting con bezier connections
      schematic_view.rs # Editor electronico completo
      hierarchy.rs   # Arbol de escena
      properties.rs  # Transform editor
      console.rs     # Log output
      asset_browser.rs
      ai_chat.rs     # Placeholder — sin LLM conectado
      settings_panel.rs
  raf_assets/        # Importador, browser, primitivos 3D
  raf_electronics/   # Componentes, schematic, netlist (union-find), DRC, MNA sim, exports
  raf_nodes/         # Nodos, grafo, executor, NodeValue runtime
  raf_ai/            # ToolRegistry, ChatPanel, AiProvider structs
  raf_net/           # Stubs de protocolo
  raf_hardware/      # SerialPort, SensorData, ActuatorCommand, RobotState, ML training
```

## Como correr

```bash
# Una vez:
rustup default stable-x86_64-pc-windows-gnu
winget install BrechtSanders.WinLibs.POSIX.UCRT

# Siempre:
cargo run -p aura_rafi_editor
```

Target dir: `target_gnu` (configurado en `.cargo/config.toml`)

## Principios de diseno — NO romper estos

- **Ligero primero**: corre en Intel UHD + 4GB RAM. Sin dependencias pesadas sin justificacion
- **Modular**: cada crate es independiente, `raf_core` es la base de todos
- **Command bus**: TODA mutacion de estado va por `CommandBus` para undo/redo
- **egui immediate mode**: no hay estado retenido de widgets, cada frame redibuja todo
- **Sin emojis en codigo**
- **Traducciones inline**: `if is_es { "Espanol" } else { "English" }` — no hay framework i18n aun
- **Colores del theme.rs**: nunca hardcodear colores, usar las constantes `DARK_BG`, `ACCENT`, etc.

## Convenciones de codigo

- Comentarios en ingles
- `pub fn show(&mut self, ui: &mut egui::Ui)` — patron de todos los panels
- Structs de panels implementan `Default`
- No funciones de +300 lineas si se puede evitar

## Proximos pasos sugeridos (en orden)

1. Viewport 3D: shaders WGSL minimos, cubo con flat shading, camara orbital en wgpu
2. Conectar AI Chat a un proveedor (OpenRouter es el mas flexible)
3. Asset pipeline: drag-drop de imagenes, thumbnails
4. Serial I/O real con la crate `serialport`
5. Modo Simple: ocultar paneles avanzados para el target de ninos

## Links

- Repo: https://github.com/ErguLan/AuraRafi
- Branding: yoll.site (solo en pantalla de carga, sutil)
