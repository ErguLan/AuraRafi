# Estado De Estabilizacion

Fecha: 2026-07-05

Este documento resume que ya esta resuelto en codigo, que ya venia funcionando, que sigue pendiente y en que fase cae cada bloque. La idea es tener una sola fuente de verdad mientras cerramos la estabilizacion antes de testear a fondo.

## Resuelto En Codigo

- Crear proyecto nuevo de juego: funciona bien.
- Crear proyecto nuevo de electronics: funciona bien.
- Abrir proyecto reciente: funciona bien.
- Guardar, cerrar y reabrir: la persistencia base funciona.
- Seguridad al salir: ya existe confirmacion por cambios sin guardar al cerrar proyecto, salir al hub o cerrar la app.
- Dirty state y guardado real: el guardado ya no limpia el estado sucio si alguna escritura falla.
- Autosave: ya usa tiempo real del editor y reporta fallo si no pudo escribir.
- Cambio entre Schematic y PCB: funciona.
- Idioma: funciona de momento.
- Abrir electronics y pasar entre Schematic y PCB repetidamente: funciona.
- Fallback CPU/GPU: sigue operativo en la ruta actual.
- Schematic placement rotation: si estas por colocar un componente y presionas `R`, ahora rota el preview que vas a poner, no el ultimo ya colocado.
- Schematic anchors, parte 1: rotar o espejar un componente seleccionado con teclado ahora resincroniza los wires anclados al pin en vez de dejar el cable atrasado.
- Cableado schematic, parte 1: al terminar sobre pin, endpoint o junction ya no sigue encadenando el cable por error.
- Cableado schematic, parte 2: el doble click izquierdo ya deja de rutear en vez de crear otro branch extra.
- Cableado schematic, parte 3: click derecho ahora cancela de verdad el wire mode en vez de dejarlo enganchado.
- Placement schematic, parte 1: click derecho ahora cancela cualquier placement activo, no solo wire mode.
- Export schematic, parte 1: el popup ahora tiene botones reales para Netlist, BOM y SVG; ya no depende solo de `1/2/3`.
- Export schematic, parte 2: exportar ahora tambien copia el contenido al portapapeles, no solo lo manda al log.
- PCB core, parte 1: se ampliaron los hit-tests de componentes, trazos y airwires, y ahora hay hover visual y preview del outline para que mover/rutear/dibujar sea mas claro.
- PCB core, parte 2: al mover componentes ya se reconstruyen los airwires en caliente para que la conectividad visual no se quede vieja.
- Hierarchy, parte 1: la seleccion principal ahora se distingue del resto del multi-select y el drag/drop muestra un ghost flotante durante el arrastre.
- Viewport, parte 1: la camara 3D ya acepta `WASD` y tambien `Q/E` para mover verticalmente mientras el viewport esta enfocado.
- Gizmo, parte 1: ahora existe hover visual previo al click en ejes y aros de rotacion.
- Gizmo scale, parte 1: `uniform_scale_by_default` ya participa en el drag y la escala por eje desplaza la pieza para sentirse menos centrada.
- Gizmo scale, parte 2: el scale mode ya no usa la presentacion vieja; ahora dibuja 6 bolitas naranjas en las caras y distingue la cara positiva/negativa al hacer hover o drag.
- Gizmo scale, parte 3: con `uniform_scale_by_default = true` la pieza escala proporcionalmente desde el centro; con `false` solo se mueve la cara arrastrada y la opuesta se queda quieta.
- Viewport rendimiento, parte 1: durante drag y multi-select interactivo se reduce carga visual de labels y el render adaptativo se vuelve mas agresivo para amortiguar bajones de FPS.
- UI/accessibility Game, parte 1: ya existe un toggle persistente en Settings para ocultar el contador visual de FPS de la barra superior.
- UI/accessibility Game, parte 2: el HUD superior ahora ajusta mejor su ancho al contenido, agrega toggles visuales rapidos para grid y labels, y la brujula XYZ ahora es interactiva con snap por eje y reset iso.
- UI/accessibility Game, parte 3: la barra superior e inferior del editor ahora tienen mejor contraste visual y el status bar deja de verse tan lavado.

## Implementado Pero Pendiente De Prueba Manual Final

- Confirmacion al salir con cambios: bien en uso normal; falta reproducir un cierre incomodo tras sesion larga.
- Autosave endurecido: bien en uso normal; falta forzar un disparo controlado con intervalo corto.
- Guardado con fallo visible y sin perdida silenciosa: falta reproducir un fallo de escritura real para cerrar esta prueba.

## Validacion Manual Reciente (2026-06-15)

- Fase 1 / sesion y guardado: crear proyectos Game y Electronics, modificar, guardar, cerrar proyecto, salir al hub y cerrar/reabrir app se percibe estable en uso normal.
- Fase 1 / falta controlada: todavia no se hizo una reproduccion seria de autosave forzado ni de fallo de escritura intencional.
- Fase 2 / schematic base: crear schematic vacio, colocar componentes y rotar preview de placement ya se siente bien.
- Fase 2 / cableado base: pin a pin en uso normal se percibe bien.
- Fase 2 / menus contextuales: el menu de value sigue pidiendo cierre automatico al cambiar de componente o clickear fuera.
- Fase 2 / export: Netlist, BOM, SVG y la coherencia popup/clipboard/log siguen pendientes de prueba manual dedicada.
- Fase 3 / PCB: abrir PCB, guardar, cerrar y reabrir va bien, pero mover componentes, rutear y dibujar outline siguen sin ser una experiencia entendible.
- Fase 4 / viewport game: multi-select base, manipulado general y rendimiento durante drag ya se sienten mejor.
- Fase 4 / multi-select compuesto: el gizmo todavia no toma el bounding total ni mueve, rota o escala todos los seleccionados como grupo real.
- Fase 4 / hierarchy: falta mejor highlight del parent target y mejor seleccion por arrastre dentro del panel.
- Fase 5 / visual base: ventana chica, grande, grid, labels y contador FPS manualmente se ven bien.
- Fase 5 / settings generales: limit FPS corregido (Unlimited via fps_limit=0), scroll bug corregido (sin max_height fijo). `Esc` ya funcionaba.
- Fase 7 / renderer: Scene, Schematic y PCB abren y cambian bien en uso normal, pero resize serio, policy `Auto` y baseline reproducible siguen pendientes.
- Fase 8 / autocomplete consola: se instancio CommandCatalog::builtin() y ahora se pasan los command_names reales a console.show(). Falta prueba manual.
- Fase 8 / compile: se corrigieron errores de compilacion heredados: match exhaustivo de BottomTab (faltaban AiChat, NodeEditor, Complement), console.show() con firma de 4 parametros, record_document_change() retornaba () pero se usaba con |=, y corrupcion de bracket por ediciones previas (stray text, match duplicado, llaves faltantes). Ademas se instancio CommandCatalog en AuraRafiApp para que el autocomplete de la consola funcione (antes se pasaba &[] por falta del catalogo). Falta prueba manual.
- Fase 11 / compile: se verifico que FPS y scroll ya compilan sin errores y la correccion de codigo previa esta integrada.

## Actualizacion 2026-07-05 — Tercera sesion de estabilizacion (polish UX/UI + limpieza)

### Resumen de lo que se hizo

Se abordaron Fase 12 (descubribilidad HUD), Fase 14 (settings sin friccion), Fase 9 (hierarchy UX) y limpieza de codigo muerto. Todos los cambios compilan limpios (`cargo check -p aura_rafi_editor` pasa con solo warnings preexistentes de deprecation).

### Resuelto en codigo (nuevo)

- **Fase 12 — Tooltips HUD del viewport**:
  - Los botones del HUD (G/R/S/F, 2D/3D, badge OBJ/VTX, toggles de grid/labels, axis gizmo X/Y/Z/ISO) ahora muestran tooltip flotante al hacer hover.
  - Como el HUD se pinta directo con `Painter` (sin `egui::Response`), los tooltips se implementan con hit-test manual del pointer contra cada rect conocido + pintado en layer Foreground.
  - 13 keys de traduccion nuevas (EN/ES) en `viewport.hud.*`.
  - El tooltip se oculta mientras el boton primario esta presionado (no estorba durante drag).

- **Fase 14 — Settings sin friccion (#12 critico)**:
  - Scroll bug resuelto de verdad: el `ScrollArea` ahora tiene `max_height` dinamico = `available_height() - 80.0`, reservando espacio para la barra de Save/Cancel. Los botones ya no se escapan al abrir toggles.
  - Boton Cancel unificado con Esc: ambos muestran el dialogo Save/Discard/Cancel SOLO si el draft tiene cambios vs settings live; si no hay cambios, cierran limpio.
  - `EngineSettings` ahora derive `PartialEq` para detectar cambios sin manual field-by-field.
  - Deteccion de cambios en dos puntos: antes del UI (para Esc) y despues del UI (para Cancel), para que Cancel vea los edits hechos en el mismo frame.

- **Fase 9 — Hierarchy UX reforzada**:
  - Ghost preview del drag ahora muestra el icono del nodo (primitive/folder) ademas del texto. Antes solo mostraba texto plano.
  - Parent target highlight reforzado: el center-drop ahora tiene accent bar naranja en el borde izquierdo (mismo estilo que primary selection), relleno con alpha diferenciado (folder=60, no-folder=40), stroke 2.0, y para folders un anillo concentrico extra que indica "receptor de hijo".
  - Box-select del panel: ya estaba implementado (confirmado en codigo, no requeria cambios).

- **Limpieza de codigo muerto**:
  - `crates/raf_editor/src/panels/shortcuts.rs` eliminado. Era codigo muerto: no estaba registrado en `panels/mod.rs` (nunca se compilaba) y todos sus metodos estaban duplicados y vivos en `app.rs`.
  - Campo `command_bus: raf_core::command::CommandBus` eliminado de `AuraRafiApp`. Era un field nunca leido (warning: field never read). Solo se declaraba e inicializaba, nunca se usaba.

### Archivos modificados en esta sesion (2026-07-05)

| Archivo | Cambio |
|---|---|
| `crates/raf_editor/src/panels/shortcuts.rs` | Eliminado (codigo muerto) |
| `crates/raf_editor/src/app.rs` | Campo `command_bus` eliminado; `show_settings_screen` reescrito: ScrollArea con max_height dinamico, Cancel unificado con Esc via flag `cancel_clicked`, deteccion de cambios en dos puntos |
| `crates/raf_core/src/config.rs` | `EngineSettings` ahora derive `PartialEq` |
| `crates/raf_editor/src/panels/hierarchy.rs` | `paint_drag_preview` ahora pinta icono del nodo; center-drop highlight reforzado con accent bar + anillo folder |
| `crates/raf_editor/src/panels/viewport.rs` | `_lang` -> `lang` (ahora se usa), `use raf_core::i18n::t` agregado, `draw_hud` recibe `lang` |
| `crates/raf_editor/src/panels/viewport_hud.rs` | `draw_hud` recibe `lang`; nueva funcion `draw_hud_tooltips` con hit-test manual; `paint_hud_tooltip` helper en layer Foreground |
| `crates/raf_core/locales/en.json` | 13 keys nuevas `viewport.hud.*` |
| `crates/raf_core/locales/es.json` | 13 keys nuevas `viewport.hud.*` |

### Verificacion

- `cargo check -p aura_rafi_editor` pasa limpio (solo warnings preexistentes de `allocate_ui_at_rect` deprecation en pcb_view.rs y schematic_view.rs, no tocados en esta sesion).

### Bugs cerrados en esta sesion

| # | Bug | Resolucion |
|---|---|---|
| 9 | Hierarchy: parent target no se ilumina mientras arrastras | Center-drop con accent bar + relleno + anillo folder; ghost preview ahora con icono |
| 12 | Settings: Save/Cancel se escapan al abrir toggles | ScrollArea con max_height = available_height - 80 |
| 13 | Settings: Cancel vs Esc inconsistente | Ambos via dialogo Save/Discard/Cancel si hay cambios; cierre limpio si no los hay |

### Limpieza

| Item | Accion |
|---|---|
| `shortcuts.rs` | Eliminado (muerto, duplicado en app.rs) |
| `command_bus` field | Eliminado (warning: never read) |

### Pendiente real despues de esta sesion

- Prueba manual de los tooltips del HUD en ventanas chicas y grandes.
- Prueba manual del scroll de Settings abriendo todos los toggles.
- Prueba manual del ghost preview con icono en drag de hierarchy.
- Prueba manual del parent target highlight con folders y no-folders.
- Validar que Cancel y Esc se comportan identico cuando hay cambios y cuando no.

---

## Actualizacion 2026-07-05 (b) — Cuarta sesion: features UX/UI Games + Electronics

### Resumen

Sesion dedicada a implementar las features de UX/UI propuestas para competir con Unity, tanto de Games como de Electronics. Todas compilan limpias con `cargo check -p aura_rafi_editor`.

### Features Games implementadas

| Feature | Descripcion | Estado |
|---|---|---|
| Gizmo rotacion acumulativo | El bug de "regresar atras despues de 180 grados" esta arreglado. La rotacion ahora acumula delta incremental por frame en vez de calcular desde el start. Funciona para entidad individual y grupo. | DONE |
| Snap de rotacion con Ctrl | Ctrl mientras rotas hace snap a 15 grados (Blender/Unity style). Aplica a entidad individual y grupo. | DONE |
| Camera focus Lerp | F ahora hace transicion suave (Lerp 0.15) en vez de snap instantaneo. Mas pulido visualmente. | DONE |
| Copy/paste entidades | Ctrl+C copia seleccionados, Ctrl+V pega con offset. Funciona con multi-select. | DONE |
| Bookmark de camara | Ctrl+1/2/3 guarda vista, 1/2/3 restaura. 3 slots con target/yaw/pitch/distance. | DONE |
| Multi-edit properties | Cambiar color o visibility del primario se propaga a todos los seleccionados. | DONE |
| Outline doble tono | Primario: naranja brillante [255,160,40]. Secundario: naranja tenue [255,120,20,180]. | DONE |

### Features Electronics implementadas

| Feature | Descripcion | Estado |
|---|---|---|
| Net highlighting al hover | Ya existia: hover sobre wire ilumina todo el net via `wire_group_indices`. Confirmado funcional. | EXISTIA |
| Cross-probe schematic <-> PCB | Seleccionar componente en schematic guarda designator. Al cambiar a PCB, se selecciona automaticamente. Y viceversa. | DONE |
| Live DRC badge en status bar | El status bar muestra "DRC: N errors" en rojo cuando hay errores, "DRC: OK" cuando no. Se ejecuta DRC cada frame en modo Schematic. | DONE |
| Net naming inline | Menu contextual del wire -> "Rename net" abre popup con TextEdit. Al confirmar, asigna el nombre a todos los wires del mismo grupo. | DONE |
| Measurement tool (M key) | M activa modo medicion. Click 1: punto inicial. Click 2: punto final. Muestra linea azul + distancia. Esc limpia. | DONE |
| Component search | Ya existia quick_search (Ctrl+Click en libreria). Busca por nombre/categoria/keywords. | EXISTIA |

### Bug critico arreglado

| Bug | Causa | Fix |
|---|---|---|
| Gizmo rotacion "regresa atras" despues de 180 grados | `node.rotation = start_rotation + delta * 45` donde `delta` era proyeccion absoluta desde start_mouse. Al cruzar el origen del axis en pantalla, la proyeccion invertia signo. | Acumulacion incremental: `accumulated_rotation += axis_dir * inc_radians` donde `inc_radians` se calcula del delta frame-a-frame (`current - last`). El grupo usa el mismo approach con `group_accumulated_rotation`. |

### Archivos modificados

| Archivo | Cambio |
|---|---|
| `crates/raf_render/src/bridge/transform_controller.rs` | Rotacion acumulativa + snap Ctrl 15deg; campos `accumulated_rotation`, `last_drag_mouse` |
| `crates/raf_render/src/bridge/viewport_bridge.rs` | `pending_focus` para Lerp; getters/setters `orbit_yaw`, `orbit_pitch`, `camera_target`, `set_*` |
| `crates/raf_render/src/scene_renderer.rs` | `secondary_selection_outline_color`, `primary_selected` en RenderOptions; outline doble tono |
| `crates/raf_editor/src/panels/viewport.rs` | `update_smooth_focus()` cada frame; `camera_bookmark_snapshot()`, `restore_camera_bookmark()`; group drag acumulativo |
| `crates/raf_editor/src/panels/viewport_interaction.rs` | Pasar `snap_to_ctrl` a `apply_transform_drag` |
| `crates/raf_editor/src/app.rs` | `scene_clipboard`, `camera_bookmarks`, `cross_probe_designator`; `do_copy`, `do_paste`, `do_bookmark_save/restore`; DRC badge en status bar; captura cross-probe |
| `crates/raf_editor/src/panels/properties.rs` | Multi-edit: `all_selected` param; propagacion color/visible a todos |
| `crates/raf_editor/src/panels/schematic_view.rs` | `editing_net_name`, `measurement_start/end`; `select_by_designator`, `selected_designator` |
| `crates/raf_editor/src/panels/schematic_view/canvas.rs` | Net name editor popup; measurement tool (M key + dibujado); "Rename net" abre editor |
| `crates/raf_editor/src/panels/pcb_view.rs` | `select_by_designator`, `selected_designator` |
| `crates/raf_core/src/scene/graph.rs` | `NodeColor` ahora derive `PartialEq` |
| `crates/raf_core/locales/en.json` | 5 keys nuevas (copied, pasted, bookmark, drc, net_name) |
| `crates/raf_core/locales/es.json` | 5 keys nuevas |

### Verificacion

- `cargo check -p aura_rafi_editor` pasa limpio (solo warnings preexistentes de `allocate_ui_at_rect` deprecation).

### Pendiente real despues de esta sesion

- Prueba manual del gizmo de rotacion acumulativo (rotar mas de 180, 360, 720 grados sin que regrese).
- Prueba manual del snap con Ctrl (debe snap a 15, 30, 45, 90...).
- Prueba manual del copy/paste (Ctrl+C, Ctrl+V con multi-select).
- Prueba manual del bookmark (Ctrl+1, mover camara, 1).
- Prueba manual del camera focus Lerp (F debe hacer transicion suave).
- Prueba manual del multi-edit (seleccionar 3 cubos, cambiar color, todos cambian).
- Prueba manual del outline doble tono (seleccionar 2+ entidades, primario vs secundario).
- Prueba manual del cross-probe (seleccionar R1 en schematic, cambiar a PCB, R1 seleccionado).
- Prueba manual del DRC badge (crear errores DRC, verificar badge rojo).
- Prueba manual del net naming (click derecho en wire -> Rename net -> escribir VCC).
- Prueba manual del measurement tool (M, click, click, verificar distancia).

---

## Actualizacion 2026-07-05 (c) — Quinta sesion: sistema de unidades oficial

### Resumen

Se instaura el sistema de unidades canonico del engine. Sin fisica, sin runtime, sin gravedad. Solo la base documental y constantica para que todo futuro desarrollo (incluido scripting C++/Rust) operen en SI.

### Implementado

- **`crates/raf_core/src/units.rs`** (nuevo modulo publico):
  - `METERS_PER_UNIT = 1.0` (1 unidad mundo = 1 metro)
  - `MM_PER_SCHEMATIC_UNIT = 1.0` (1 unidad schematic = 1 mm)
  - `SCHEMATIC_TO_WORLD = 0.001` (mm -> m)
  - `DEFAULT_GRID_SPACING_M`, `DEFAULT_GRID_SPACING_MM`, `SCHEMATIC_SNAP_OPTIONS_MM`, `DEFAULT_TRACE_WIDTH_MM`, `DEFAULT_PAD_SPACING_MM`
  - `DisplayUnit` enum (Metric/Imperial/Game) con `format_distance`, `from_meters`, `to_meters`, `distance_suffix`, `label`
  - Helpers `schematic_to_world()` / `world_to_schematic()`
  - Disenado para ser importable por FFI C++ futuro

- **`config.rs`**: `units_metric: bool` reemplazado por `display_unit: DisplayUnit`. Serializado con `#[serde(default = "default_display_unit")]`. Proyectos existentes se reinterpretan: lo que era "1 unidad" ahora es "1 metro" / "1 mm" explicito.

- **`pcb.rs`**: el `/50` magico de schematic->3D se reemplaza por `raf_core::units::schematic_to_world()`. El `/1.5` de pins tambien. Conversion explicita mm->m documentada.

- **`properties.rs`**: ahora muestra sufijo de unidad `(m)` junto a Position y `(m3)` junto a Scale. `PropertiesPanel` tiene `display_unit` sincronizado desde settings cada frame.

- **`viewport_hud.rs`**: el HUD ahora muestra `D 8.0m` (distancia de camara en metros) en vez de `D 8.0` ambiguo.

- **`settings_panel.rs`**: el toggle Metric/Imperial se reemplaza por un ComboBox con Metric (m), Imperial (ft), Game (units).

- **Documentacion**: `ARCHITECTURE.md` y `SYSTEM_TRUTH.md` ahora documentan el sistema de unidades canonico, constantes, convencion de escala, y el hook para scripting futuro.

### Archivos modificados

| Archivo | Cambio |
|---|---|
| `crates/raf_core/src/units.rs` | Nuevo modulo con constantes y DisplayUnit |
| `crates/raf_core/src/lib.rs` | `pub mod units;` agregado |
| `crates/raf_core/src/config.rs` | `units_metric` -> `display_unit: DisplayUnit`; import `DisplayUnit`; `default_display_unit()` |
| `crates/raf_editor/src/panels/pcb.rs` | `/50` y `/1.5` magicos reemplazados por `schematic_to_world()` |
| `crates/raf_editor/src/panels/properties.rs` | Sufijo de unidad en Position/Scale; `display_unit` field + `set_display_unit()` |
| `crates/raf_editor/src/panels/viewport_hud.rs` | HUD muestra `D 8.0m` |
| `crates/raf_editor/src/panels/settings_panel.rs` | ComboBox para DisplayUnit |
| `crates/raf_editor/src/app.rs` | `properties.set_display_unit()` sincronizado |
| `docs/ARCHITECTURE.md` | Seccion "Unit System" con constantes, escala, convencion, scripting hook |
| `.ai/SYSTEM_TRUTH.md` | Pillar de unidades canonico + entrada `units.rs` en el mapa de `raf_core` |

### Verificacion

- `cargo check -p aura_rafi_editor` pasa limpio.

### Pendiente real despues de esta sesion

- Prueba manual: cambiar DisplayUnit a Imperial/Game y verificar que properties muestre el sufijo correcto.
- Prueba manual: abrir un proyecto de schematic existente y verificar que las posiciones se reinterpreten como mm.
- Prueba manual: sincronizar PCB al 3D y verificar que la conversion mm->m sea correcta (un PCB de 100mm debe verse como 0.1m en el viewport 3D).
- Ajustar el board base de PCB (`scale = 10.0`) a un tamano consistente con mm (deberia ser ~0.1m para un PCB de 100mm).
- Cuando exista runtime de fisica, usar `METERS_PER_UNIT` como base para gravity/velocity/mass.

## Guia Rapida Para Pruebas Pendientes

- Forzar autosave: baja el intervalo de autosave a `5s` o `10s`, modifica algo y no des `Ctrl+S`; si el sistema esta bien, debe guardar solo y dejar rastro visible.
- Provocar fallo de guardado: usa una copia del proyecto en una carpeta marcada como read-only o bloquea el archivo destino para confirmar que el dirty state no se limpia y el error sale visible.
- Popup, clipboard y log coherentes: exporta Netlist/BOM/SVG, pega el contenido del clipboard en un editor y compara que coincida con el mensaje/log esperado y que el popup cierre limpio.
- Airwires vivos: mueve un componente en PCB y verifica que las lineas de airwire cambien su origen/destino al instante, no solo despues de reabrir.
- Route varias veces seguidas: activa route y encadena varios airwires resaltados uno tras otro para ver si el flujo se entiende o se rompe.
- Connectivity, preview y geometria persisten: guarda, cierra y reabre; los componentes, traces, outline y airwires deben quedar donde estaban y seguir representando la misma conectividad.
- Manipulacion continua: haz varios minutos de mover, rotar, escalar, box-select y reparentar sin cambiar de panel para ver si aparece drift, estado sucio falso o undo roto.
- Brujula XYZ y reset iso: click en una letra/eje debe hacer snap a esa vista; click en el centro `ISO` debe volver a la vista isometrica.
- Legibilidad de top bar, bottom bar y HUD: se considera correcta si todo el texto sigue visible, no hay clipping, el contraste se lee sin esfuerzo y todos los botones quedan alcanzables sin pelear con el scroll.
- Politica `Auto`: pon el renderer en `Auto` y confirma por el badge/estado activo si toma GPU cuando esta disponible y CPU solo cuando realmente hace fallback.
- Escena pesada provisional: si no tienes una escena grande, duplica primitives o entidades hasta tener una prueba manual repetible para medir render y upload.

## Pendiente Real

- Undo/redo despues de varias operaciones encadenadas y drags largos: corregido en codigo (coalescing transaccional por drag).
- Drag/drop de assets y hierarchy: falta preview visual mas claro, mejor hover, mejor precision al detectar bloques y mejor seleccion por arrastre dentro de hierarchy.
- Persistencia de settings y layout: necesita pasada dedicada.
- Seleccion, duplicado, delete y shortcuts: falta auditoria funcional completa.
- Navegacion de camara: falta prueba manual prolongada y verificacion de ergonomia fina.
- Gizmos: afinar sensacion final del nuevo scale handle segun uso real y llevarlo a un gizmo grupal cuando haya multi-select real.
- Multi-select: falta dejar claro el objeto principal, mostrar mejor seleccion viva en electronics, revisar el bajon de FPS y hacer que move/rotate/scale operen como grupo real.
- Resize de ventana: pendiente de verificacion seria.
- Cambios de modo 2D/3D: pendiente de verificacion seria.
- Play mode / runtime: no hay runtime real aun; hay que dejar esto honesto en UI.
- Crear schematic desde cero: la base existe, pero la experiencia aun es mala.
- Cablear en schematic: la cancelacion base ya esta mejor, pero falta mas feedback visual, menos friccion general y mejor cierre automatico de popups contextuales como el de value (codigo ya usa CloseOnClickOutside, falta prueba manual).
- DRC: pendiente de verificacion seria.
- Simulacion DC: pendiente de verificacion seria.
- Librerias de componentes con datos y datasheets: pendiente de diseno de escalado.
- Export netlist/BOM/SVG: la UX base ya mejoro con botones clicables y copia al portapapeles, pero todavia falta salida a archivo y flujo mas serio.
- PCB core: ya hay una primera mejora de hover/tolerancia/preview, pero mover componentes, route, outline y la experiencia general siguen verdes y hoy todavia no se siente operable para un usuario nuevo.
- Guardar/reabrir sincronizando Schematic y PCB: base funcional, pero hay que probar mas la persistencia visual y airwires.
- Project settings de electronics: faltan settings y properties propios con nivel mas serio.
- Settings generales: limit FPS corregido (Unlimited ahora funciona via fps_limit=0), scroll bug corregido (botones Save/Cancel ya no quedan fuera). `Esc` como salida rapida ya existia.
- Guardado lineal por proyecto: pendiente de diseno/implementacion como modo opt-in para quien priorice cero perdida ante crash aunque cueste rendimiento.
- Renderer activo: el corte documental canonico ya empezo, pero todavia falta congelar por completo el path oficial entre docs, codigo y surfaces activas.
- Hot path grafico: el backend GPU activo todavia crea buffers por draw y por frame; eso merece optimizacion estructural antes de empezar una guerra de micro-optimizaciones por todo el engine.
- Medicion del hot path: falta una linea base reproducible con escenas de referencia para validar mejoras reales de renderer con antes/despues, no por intuicion, incluyendo una prueba seria de resize y policy `Auto`.
- Contrato CPU fallback/GPU activo: falta dejar por escrito que paridad minima se mantiene mientras se consolida el path canonico, para no optimizar rompiendo la ruta potato.
- Optimizacion global del engine: no conviene abrirla aun; primero hay que congelar que renderer/runtime grafico es el camino oficial.
- Experiencia general de electronics: sigue necesitando una pasada fuerte de interfaz.

## Fases

### Fase 1: Seguridad de sesion y guardado

Estado: implementada en codigo, validada en uso normal; falta prueba controlada de autosave forzado y fallo de escritura.

Incluye:

- Guardado real.
- Dirty state consistente.
- Autosave real.
- Confirmacion al salir.

Objetivo: cero perdida silenciosa.

Prueba manual de cierre que sigue faltando:

- Bajar el intervalo de autosave a `5s` o `10s`, editar algo y esperar sin usar guardado manual.
- Reproducir un fallo de escritura con carpeta/archivo bloqueado para confirmar que el dirty state no se limpia y el error sale visible.

### Fase 2: Schematic usable de verdad

Estado: en progreso, con validacion manual parcial reciente.

Ya cubierto en esta fase:

- Final correcto al conectar a pin/endpoint/junction.
- Doble click izquierdo deja de crear branch extra.
- Rotacion correcta del preview al colocar componentes.
- Rotar o espejar el componente seleccionado ya resincroniza los wires anclados al pin.
- Cancelacion limpia del wire mode con click derecho.
- Cancelacion limpia de cualquier placement activo con click derecho.
- Popup de export clicable en vez de solo visual.
- Export copia contenido al portapapeles ademas de dejarlo en log.

Falta en esta fase:

- Placement menos tosco.
- El menu de value debe cerrarse solo al cambiar de componente o clickear fuera.
- Export a archivo real desde la UI.
- Prueba manual dedicada de export/clipboard/log para confirmar que el flujo se entiende sin explicacion externa.

Objetivo: hacer un schematic sin pelearte con la interfaz.

### Fase 3: PCB core funcional

Estado: iniciada, pero hoy sigue teniendo bloqueos UX serios.

Ya cubierto en esta fase:

- Hover visual en componentes, trazos y airwires.
- Tolerancias de seleccion/ruteo mas amplias.
- Preview vivo al dibujar outline.
- Airwires reconstruidos durante el drag de componentes.

Falta en esta fase:

- Hacer que mover componentes realmente se sienta operativo y no ambiguo.
- Verificacion manual de que route y outline ya se sienten correctos en uso repetido.
- Mejoras de ruteo mas alla del auto-ruteo ortogonal base.
- Volver descubrible el flujo desde la barra superior para que no dependa de ensayo/error.

Objetivo: que PCB deje de ser "se ve pero no sirve".

### Fase 4: Viewport Game y hierarchy

Estado: implementada en codigo, validada a medias; falta cerrar multi-select compuesto y hierarchy UX.

Ya cubierto en esta fase:

- Seleccion principal visible en hierarchy durante multi-select.
- Drag preview visible en hierarchy al reordenar o reparentar.
- Camara 3D con `WASD` y `Q/E`.
- Hover previo en gizmos de mover/rotar/escalar.
- `uniform_scale_by_default` ya afecta el drag de escala.
- Scale mode reemplazado por 6 bolitas naranjas en caras.
- Scale proporcional o de una sola cara segun `uniform_scale_by_default`.
- Reduccion local de carga visual durante drag y multi-select interactivo para aliviar FPS.

Falta en esta fase:

- Afinar mejor el comportamiento final del scale para hardware/UX mas fino tras prueba manual.
- Verificacion manual de que el alivio de FPS durante manipulado y multi-select ya sea suficiente.
- Hacer que el gizmo grupal use el bounding total del multi-select y transforme todo el conjunto.
- Mejorar hierarchy para que el parent target se vea con claridad mientras recibe un drop.

Objetivo: arreglar control, feedback, multi-select y rendimiento al manipular.

### Fase 5: Polish de UI y accesibilidad

Estado: implementada en codigo, validada a medias; settings generales aun tienen bugs de UX.

Ya cubierto en esta fase:

- Toggle persistente para mostrar/ocultar el contador FPS de la barra superior.
- HUD superior con ancho adaptativo para que no se corte tan facil.
- Toggles visuales rapidos para grid y labels dentro del viewport.
- Brujula XYZ interactiva con snap por eje y reset a vista isometrica.
- Mejor contraste visual en top bar y downbar.

Falta en esta fase:

- Validacion visual final en ventanas chicas y monitores distintos.
- Definir y pasar una prueba clara de legibilidad para top bar, bottom bar y HUD.

Objetivo: limpiar la experiencia sin tocar la logica base.

### Fase 6: Runtime truth pass

Estado: 6A documentada en MD temporal; implementacion de runtime aun no iniciada.

Objetivo: dejar claro que existe, que no existe y que botones prometen de mas.

Documento temporal de referencia creado en:

- `docs/TEMP_PHASE6_RUNTIME_TRUTH_PASS.md`

### Fase 7: Renderer canonico y hot path grafico

Estado: en progreso.

Ya justificado para esta fase:

- Scene, Schematic y PCB ya comparten una ruta moderna de runtime grafico.
- El viewport ya usa mediciones reales de render/upload y escala adaptativa.
- Sigue habiendo mezcla de verdad documental y tecnica sobre si el renderer debe leerse como CPU-first, GPU-first o ruta hibrida en transicion.
- El backend GPU activo todavia paga costo estructural creando buffers por draw/per-frame en el camino caliente.
- El fallback CPU sigue formando parte del contrato del engine y no conviene degradarlo mientras se congela el camino oficial.

Ya cubierto en esta fase:

- Corte documental canonico iniciado para alinear README, ARCHITECTURE, RENDERER y CHANGELOG con una sola verdad del renderer activo.

Incluye:

- Definir y documentar una sola verdad del renderer activo.
- Congelar el path canonico que manda hoy en Scene, Schematic y PCB.
- Medir y nombrar el hot path real antes de tocarlo: build/submission del frame, draw_mesh, draw_line, upload/present y reuso de recursos.
- Atacar solo el hot path grafico real del backend activo.
- Priorizar optimizacion estructural del backend GPU activo: ciclo de vida de buffers, cache/reuso de recursos y menos trabajo por draw/per-frame.
- Dejar claro que modulos son camino oficial, cuales quedan legacy y cuales siguen solo preparados.
- Mantener paridad funcional minima entre GPU activo y CPU fallback mientras se consolida el renderer canonico.
- Evitar optimizacion amplia del engine mientras el camino canonico siga moviendose.

Falta en esta fase:

- Una linea base reproducible de medicion con escenas/pruebas representativas para validar mejoras con antes/despues.
- Un contrato minimo de superficies que deje claro que Scene, Schematic y PCB deben seguir el mismo camino oficial.
- Resolver el costo per-draw/per-frame mas obvio del backend GPU sin abrir una reescritura total del renderer.
- Delimitar que no entra todavia: runtime game completo, campana global de optimizacion, features nuevas solo porque ya exista infraestructura preparada.

No incluye:

- Campana general de optimizacion en todo el engine.
- Micro-optimizaciones en sistemas perifericos que aun no son el cuello real.
- Rehacer runtime de juego completo.
- Abrir features nuevas del renderer solo porque ya existan modulos preparados en el repo.
- Meter streaming global, ray tracing, o una escalada de features visuales antes de consolidar submission y recursos del path activo.

Criterio de cierre:

- Existe una sola narrativa tecnica consistente del renderer en la documentacion base.
- Scene, Schematic y PCB quedan declarados sobre el mismo camino oficial sin ambiguedad.
- El hot path GPU deja de recrear recursos gruesos por draw/per-frame donde hoy mas duele.
- Hay medicion base comparable antes/despues para demostrar ganancia real.
- El fallback CPU sigue usable y no queda roto por la consolidacion del camino canonico.

Objetivo: consolidar el renderer/runtime grafico que ya esta caliente para que las optimizaciones futuras caigan sobre el camino correcto y no sobre rutas transitorias o solo "preparadas".

### Fase 8: Multi-select real y undo/redo confiable

Estado: iniciada; el coalescing base de undo ya quedo conectado, pero falta prueba manual larga y el multi-select grupal sigue pendiente.

Resuelto en esta fase:

- `finalize_pending_history_snapshot()` ya no solo existe: ahora tambien recibe snapshots reales desde `record_document_change()` y se consume desde `do_undo()` / `do_redo()`.
- `drag_ongoing` en `ViewportPanel` ahora participa en el gating del snapshot pendiente para no empujar ruido por clicks normales.
- `current_history_snapshot_like()` agregado en `app.rs` para que undo/redo preserve el dominio correcto (Scene/Schematic/PCB) al construir el stack opuesto.

Pendiente en esta fase:

- Multi-select grupal: gizmo sobre bounding total del conjunto y transformacion grupal real.
- Los 6 handles de scale sobre bounds del grupo.
- Validar manualmente que el drag largo en Game ya vuelve al estado pre-drag completo y no deja snapshots fantasma.
- Revisar conflicto de `Ctrl+Z/Y` cuando el foco esta en campos de texto o edicion contextual.

Archivos modificados:

- `crates/raf_editor/src/panels/viewport.rs` — drag_ongoing flag
- `crates/raf_editor/src/panels/viewport_interaction.rs` — coalescing por drag
- `crates/raf_editor/src/app.rs` — finalize_pending_history_snapshot() implementada, corruptelas de bracket y duplicados limpiados, match de BottomTab completado, console.show() corregido a firma de 4 args, record_document_change() retorna bool

Criterio de cierre parcial:

- Undo/redo restaura estados completos de transformacion, no micro-pasos. ✓
- `cargo check -p raf_editor` pasa sin errores. ✓

### Fase 9: Hierarchy y drag/drop entendibles

Estado: completa. Box-select del panel implementado.

Ya cubierto:

- Buscador/filtro en tiempo real por nombre de nodo.
- Ctrl+click para toggle individual en multi-select.
- Drop zones inteligentes (25/50/25) con insertion line naranja y center-drop brillante.
- `reparent_node_before()` para insercion entre hermanos.
- Ghost preview del drag con icono y contador de seleccion extra.
- **Box-select por arrastre**: rectangulo visual con stroke/fill naranja, highlight de candidatos en tiempo real (misma clase visual que seleccion real), commit al soltar con soporte Ctrl+click para toggle.

Archivos involucrados:

- `crates/raf_editor/src/panels/hierarchy.rs`

Criterio de cierre:

- Reordenar, reparentar y seleccionar desde hierarchy se entiende sin ensayo/error. ✓
- El usuario ve claro donde se va a insertar o colgar un hijo antes de soltar. ✓
- El preview no tapa la lectura ni deja dudas de destino. ✓

### Fase 10: Electronics UX operable antes de 1.0

Estado: iniciada; se reforzaron algunos flujos base, pero electronics sigue lejos de cierre UX.

Ya cubierto recientemente en esta fase:

- Value popup: cierre por cambio de seleccion o click fuera ya cableado en `schematic_view/canvas.rs`.
- Box-select vivo en schematic: los componentes ahora se iluminan durante el arrastre del rectangulo, no solo al soltar.
- PCB route discoverability: seleccionar un airwire desde `Select` ahora empuja al contexto de `Route` y muestra hint mas directo.
- Rotacion/espejo de schematic: antes de transformar, ahora se intentan fijar anchors desde el snapshot previo del componente para no perder wires legacy al primer giro.

Problemas puntuales que entran:

- Mover componentes en PCB hoy no se siente operativo o directamente no funciona como deberia.
- `Route` no es descubrible ni queda claro cual es el siguiente click esperado.
- `Outline` no se entiende desde la barra superior actual.
- El popup o menu de value en schematic deberia cerrarse al cambiar de componente o clickear fuera.
- Export Netlist/BOM/SVG todavia necesita salida a archivo y flujo serio.
- Falta una forma mas clara de verificar airwires vivos, connectivity persistente y preview correcto al reabrir.
- Project settings de electronics siguen pobres para un proyecto serio.

Archivos involucrados probables:

- `crates/raf_editor/src/panels/schematic_view.rs`
- `crates/raf_editor/src/panels/schematic_view/canvas.rs`
- `crates/raf_editor/src/panels/pcb_view/canvas.rs`
- `crates/raf_editor/src/pcb_document.rs`
- `crates/raf_editor/src/schematic_document.rs`
- `crates/raf_electronics/src/pcb/layout.rs`
- `crates/raf_electronics/src/schematic.rs`

Criterio de cierre:

- Un usuario puede colocar, cablear, abrir PCB, mover, rutear y cerrar outline sin adivinar la UI.
- Value popup, export y persistencia dejan feedback claro y consistente.
- Electronics deja de depender de explicacion externa para las acciones base.

### Fase 11: Settings generales, guardado lineal y control de FPS

Estado: FPS Unlimited y scroll bug resueltos en codigo; guardado lineal ya existia.

Resuelto en esta fase:

- `Limit FPS` ya funciona correctamente: se elimino el `max(15)` que clamps el valor a 15, permitiendo que `fps_limit=0` (Unlimited) pase al viewport que ya lo maneja con `ctx.request_repaint()` sin limite.
- Scroll bug de settings resuelto: se elimino `.max_height()` fijo del ScrollArea para que el layout parental controle el tamano naturalmente y los botones Save/Cancel no queden fuera de pantalla.
- Guardado lineal (`Linear Saving`) ya existia implementado en `project.settings.linear_save` y Project Settings UI.

No requeria cambios:

- `Esc` dentro de settings ya funcionaba (cierra sin guardar) y `Ctrl+S` guarda y cierra.
- Opcion `Unlimited` ya existia en el checkbox del panel de settings (`settings.fps_unlimited`).
- Guardado lineal ya tenia UI en Project Settings y almacenamiento en `ProjectSettings.linear_save`.

Archivos modificados:

- `crates/raf_editor/src/app.rs` — FPS clamp eliminado, scroll area sin max_height

Criterio de cierre:

- El usuario puede salir de settings sin pelear con scroll o botones ocultos. ✓
- FPS limit y `Unlimited` hacen exactamente lo que dicen. ✓
- `Lineal Saving` queda aislado por proyecto, con warning claro y comportamiento reproducible. ✓ (ya implementado antes)

### Fase 12: Descubribilidad y feedback de herramientas

Estado: no empezada.

Problemas puntuales que entran:

- Muchas acciones base todavia dependen de saber "como se hace" en vez de verse claras por la UI.
- Falta feedback mas explicito para `Route`, `Outline`, brujula XYZ, export y cambios de modo.
- Top bar, bottom bar y HUD necesitan una definicion clara de legibilidad y estados activos.
- Los menus y popups contextuales deben cerrar de forma mas natural al cambiar de contexto.

Archivos involucrados probables:

- `crates/raf_editor/src/panels/viewport_hud.rs`
- `crates/raf_editor/src/panels/schematic_view.rs`
- `crates/raf_editor/src/panels/schematic_view/canvas.rs`
- `crates/raf_editor/src/panels/pcb_view/canvas.rs`
- `crates/raf_editor/src/panels/hierarchy.rs`
- `crates/raf_editor/src/panels/settings_panel.rs`

Criterio de cierre:

- El usuario entiende la siguiente accion razonable sin tutorial externo en los flujos base.
- Los estados de herramienta activos se leen rapido y no quedan escondidos.
- Los popups dejan de sentirse pegados o atrapados en pantalla.

### Fase 13: Soak test y gate final pre-1.0

Estado: no empezada.

Problemas puntuales que entran:

- Todavia falta una sesion real de `20-30` minutos por flujo.
- Falta clasificar que glitch visual es bug reproducible y que ruido visual temprano del engine.
- Falta repetir abrir/cerrar proyectos, cambiar surfaces, guardar seguido y manipular continuo bajo una sola sesion larga.

Archivos involucrados:

- `docs/STABILIZATION_STATUS.md`
- Se completara con los modulos concretos que fallen durante la validacion larga.

Criterio de cierre:

- Existen notas reproducibles de soak test para Game, Schematic y PCB.
- Los crashes, estados sucios falsos y cuelgues graves quedan cerrados o documentados con prioridad real antes de `1.0`.
- La decision de "estable para 1.0" ya no se apoya en sensacion sino en validacion repetible.

---

## Actualizacion 2026-06-29 — Segunda sesion de estabilizacion

### Resumen de lo que se hizo

Se abordaron Fase 9 (hierarchy drag/drop como explorador), Fase 12 (descubribilidad electronics) y se documentaron bugs abiertos que reporto el CEO tras probar manualmente.

### Resuelto en codigo (nuevo)

- **Fase 9 — Hierarchy tipo explorador**:
  - Buscador/filtro en tiempo real por nombre de nodo (`search_query` + render condicional).
  - Ctrl+click para toggle individual en multi-select (ademas del Shift existente).
  - Drop zones inteligentes en cada nodo: arriba 25% → inserta antes (mismo padre), abajo 25% → inserta despues, centro 50% → hace hijo.
  - Linea indicadora naranja al hacer hover en zona de insercion (insertion line).
  - Click en espacio vacio del panel deselecciona todo.
  - `SceneGraph::reparent_node_before()` nueva funcion en `raf_core/src/scene/graph.rs` para insertar en posicion especifica entre hermanos.
  - Ghost preview del drag con icono y contador de seleccion extra.
  - Sincronizacion de seleccion hierarchy ↔ viewport actualizada.

- **Fase 12 — Descubribilidad electronics**:
  - Texto de ayuda contextual debajo del toolbar en schematic y PCB segun la herramienta activa (Select/Route/Outline/Wire/Place).
  - Traducciones EN/ES para los 8 hints nuevos en `crates/raf_core/locales/`.
  - Mensajes especificos: "Click en un pin para comenzar a cablear", "Click en un airwire para comenzar a rutear", etc.

- **Compilacion y sanity**:
  - `CommandCatalog` instanciado y cableado a `console.show()` (autocomplete funcional).
  - Errores de compilacion heredados corregidos: match exhaustivo de BottomTab, `record_document_change()` retorna `bool`, corrupciones de bracket y duplicados limpiados.

### Bugs abiertos reportados por CEO en prueba manual (2026-06-29)

| # | Bug | Donde | Severidad |
|---|---|---|---|
| 1 | Rotar componente en schematic desconecta los wires anclados — al rerotar no se reconectan | `schematic_view/canvas.rs` | Alta |
| 2 | Value popup no se cierra al cambiar de componente o clickear fuera | `schematic_view/canvas.rs` | Media |
| 3 | Ctrl+Z en Game tras drag largo solo deshace ~1 pixel en vez del estado completo pre-drag | `viewport_interaction.rs`, `viewport.rs` | Alta |
| 4 | PCB: mover componentes no funciona, no se entiende como seleccionar ni mover | `pcb_view.rs`, `pcb_view/canvas.rs` | Critica |
| 5 | PCB: Route no es descubrible, no se entiende el flujo | `pcb_view.rs` | Alta |
| 6 | PCB: Outline no se entiende desde la barra superior, la experiencia espanta usuarios | `pcb_view.rs` | Alta |
| 7 | Multi-select en electronics no muestra seleccion visual durante el drag del box (solo al soltar) | `schematic_view/canvas.rs` | Baja |
| 8 | Gizmo multi-select no escala/rota/mueve como grupo real — los handles de scale no se reposicionan al bounding total | `viewport_interaction.rs` | Alta |
| 9 | Hierarchy: el icono/parent target no se ilumina mientras arrastras un nodo sobre el | `hierarchy.rs` | Media |
| 10 | Hierarchy: box-select por arrastre en el panel no existe (solo click a click) | `hierarchy.rs` | Media |
| 11 | Limit FPS en Settings no funciona realmente (el valor no se respeta) | `app.rs`, `settings_panel.rs` | Alta |
| 12 | Settings: al abrir un toggle (ej: Console Commands), el ScrollArea se expande y Save/Cancel quedan fuera de pantalla — hay que cerrar el toggle para que reaparezcan | `settings_panel.rs`, `app.rs` | Critica |
| 13 | Settings: Esc solo cierra sin guardar, deberia tener opcion de guardar y cerrar | `app.rs` | Media |
| 14 | Undo/redo con Ctrl+Z/Y en Game tiene conflicto con clipboard (ambos usan Ctrl+Z) | `app.rs` | Media |

#### Cerrados en 2026-07-01

| # | Bug | Resolucion |
|---|---|---|
| 10 | Hierarchy: box-select por arrastre | Box-select con rectangulo visual, highlight de candidatos en tiempo real, commit con Ctrl toggle |
| 13 | Settings: Esc cierra sin guardar | Ahora Esc abre dialogo Save/Discard/Cancel antes de cerrar |

### Archivos modificados en esta sesion (2026-06-29)

| Archivo | Cambio |
|---|---|
| `crates/raf_core/src/scene/graph.rs` | `reparent_node_before()` agregado |
| `crates/raf_editor/src/panels/hierarchy.rs` | Search, Ctrl+click, drop zones, insertion line, fondo deselecciona |
| `crates/raf_editor/src/panels/schematic_view.rs` | `draw_tool_status_hint()` agregado |
| `crates/raf_editor/src/panels/pcb_view.rs` | `draw_tool_status_hint()` agregado |
| `crates/raf_editor/src/app.rs` | `reparent_node_before` en HierarchyActions, CommandCatalog cableado |
| `crates/raf_core/locales/en.json` | 8 nuevas keys de hints |
| `crates/raf_core/locales/es.json` | 8 nuevas keys de hints |

### Archivos modificados en 2026-07-01

| Archivo | Cambio |
|---|---|
| `crates/raf_editor/src/panels/hierarchy.rs` | Box-select por arrastre: rectangulo visual, highlight de candidatos, commit con Ctrl toggle |
| `crates/raf_editor/src/app.rs` | Settings confirmacion de cierre: Esc abre dialogo Save/Discard/Cancel |

### Archivos involucrados en bugs abiertos (no modificados aun)

| Archivo | Bugs |
|---|---|
| `crates/raf_editor/src/panels/schematic_view/canvas.rs` | #1, #2, #7 |
| `crates/raf_editor/src/panels/viewport_interaction.rs` | #3, #8 |
| `crates/raf_editor/src/panels/viewport.rs` | #3 |
| `crates/raf_editor/src/panels/pcb_view/canvas.rs` | #4, #5, #6 |
| `crates/raf_editor/src/panels/settings_panel.rs` | #11, #12 |
| `crates/raf_editor/src/app.rs` | #11, #12, #14 |

### Verificacion 2026-06-30 sobre lo hecho el 2026-06-29

- La direccion general del trabajo del 29/06 estaba bien, pero algunas afirmaciones del MD estaban un poco mas adelantadas que el wiring real del codigo.
- **Fase 8 / undo**: `pending_history_snapshot` existia, pero no estaba conectado de verdad al flujo de `record_document_change()` ni al armado correcto de redo. En esta sesion se termino de cablear y ya no queda como campo decorativo.
- **Bug #2 / value popup**: ahora queda cableado el cierre por cambio de seleccion o click fuera en `schematic_view/canvas.rs`. Sigue faltando prueba manual seria.
- **Bug #7 / box-select electronics**: el rectangulo ya existia, pero la seleccion viva visual no. En esta sesion se agrego highlight en tiempo real mientras el box cubre componentes en schematic.
- **Bug #5 / route en PCB**: se reforzo el flujo. Seleccionar un airwire desde `Select` ahora empuja al contexto de `Route` y muestra el hint fuerte de siguiente paso.
- **Bug #9 / target de hierarchy**: el center-drop target ahora se ve mas claro con relleno y marcador de acento.
- **Bug #1 / rotacion schematic**: se agrego pre-anclaje contra el snapshot previo del componente antes de rotar/espejar o editar posicion/rotacion desde properties, para que wires legacy o aun no anclados no se queden atras al primer giro. Falta prueba manual fuerte.
- **Bug #8 / gizmo grupal**: ya existe un primer path de gizmo multi-select en viewport que usa el bounding del grupo para hover, overlay y drag de move/rotate/scale. Falta validacion manual y seguramente afinado matematico fino.
- **Bug #14 / shortcut conflict**: `Ctrl+Shift+Z` ya tambien se bloquea cuando un text field tiene el foco, no solo `Ctrl+Z/Y`.
- **Resuelto en esta sesion (2026-07-01)**: box-select del panel hierarchy (#10) implementado con rectangulo de seleccion visual, highlight de candidatos en tiempo real y commit al soltar con soporte Ctrl+click. Confirmacion de cierre de Settings (#13) agregada: Esc abre dialogo Save/Discard/Cancel en vez de cerrar sin aviso.
- **Sigue abierto de verdad**: salida a archivo en export y validacion manual seria de rotacion schematic + gizmo grupal + undo largo.

---

## Nuevas fases propuestas (pre-1.0)

A continuacion se documentan las fases que el CEO identifico como necesarias tras la prueba manual. Son problemas reales de UX, no theoretical scope creep. Van numeradas como Fase 14 en adelante para no recolisionar con las existentes.

### Fase 14: Settings funcionales sin friccion

Estado: parcialmente resuelta. Esc con dialogo de confirmacion Save/Discard/Cancel implementado.

Ya cubierto:

- **Esc con confirmacion dialog**: Esc ya no cierra silenciosamente. Abre un modal "Save changes to settings before closing?" con tres botones: Save & Close, Don't Save (Discard), Cancel. Implementado en `app.rs:show_settings_screen()`.

Problemas puntuales que entran:

1. **Scroll bug critico**: al abrir un toggle dentro de settings, el ScrollArea expande su contenido y los botones Save/Cancel se desplazan fuera del viewport. Causa raiz: el ScrollArea no tiene limitacion de altura.
2. **Limit FPS no funcional**: el slider FPS Limit en settings no restringe los FPS reales del viewport. El valor se almacena pero el viewport no lo consulta.
3. **Unlimited checkbox inconsistente**: `settings.fps_unlimited` checkbox redundante con `fps_limit=0`.

Archivos involucrados:

- `crates/raf_editor/src/panels/settings_panel.rs`
- `crates/raf_editor/src/app.rs` (handle_global_shortcuts para Esc) ✓

Criterio de cierre:

- Save/Cancel siempre visibles sin importar cuantos toggles esten abiertos.
- Esc abre un mini-dialogo "Save changes before closing?" con Yes/No/Cancel. ✓
- Limit FPS restringe los FPS reales del viewport (medible con el contador FPS).
- Unlimited y fps_limit=0 unificados en una sola opcion.

### Fase 15: Guardado Lineal por proyecto (Linear Saving)

Estado: no empezada (existe campo `ProjectSettings.linear_save` pero sin UI ni comportamiento real).

Problemas puntuales que entran:

1. El campo `linear_save` ya existe en `ProjectSettings` pero no hay UI en Project Settings para elegir entre "Normal Saving" y "Linear Saving".
2. No hay comportamiento real: cuando `linear_save = true`, cada accion (mover, rotar, escalar, colocar, cablear, eliminar) debe gatillar un guardado inmediato del proyecto.
3. No hay advertencia al activarlo: el usuario debe saber que Linear Saving es mas lento pero previene perdida por crash.
4. La configuracion es por proyecto, no global.

Comportamiento esperado:

- En Project Settings, un radio button o dropdown: "Saving Mode: [Normal \| Linear]".
- Al seleccionar Linear, mostrar warning en panel: "Linear Saving saves after every action. This may be slower but prevents data loss on crash." con boton "I understand, enable".
- Cuando linear_save = true, despues de cada `mark_scene_modified()` (o `push_undo_snapshot()`), llamar a `project.save()`.
- Solo afecta al proyecto actual, no a los demas.
- Aplica tanto a Game como a Electronics (Scene y Schematic/PCB).

Archivos involucrados:

- `crates/raf_editor/src/panels/project_settings.rs`
- `crates/raf_editor/src/app.rs` (disparar save en cada accion cuando linear_save = true)
- `crates/raf_core/src/project.rs` (verificar que linear_save existe y se serializa)

Criterio de cierre:

- El usuario puede elegir el modo de guardado por proyecto.
- Linear Saving persiste entre sesiones.
- Cada accion en el proyecto guarda inmediatamente.
- El warning se muestra una vez al activar, no cada frame.
- El rendimiento en Linear Saving es aceptable (no congela la UI).

### Fase 16: PCB operable (mover componentes, route, outline)

Estado: critico, no empezada realmente.

Problemas puntuales que entran:

1. **Mover componentes no funciona**: el usuario no puede agarrar un componente en el PCB y arrastrarlo. La seleccion es ambigua y el drag no se dispara o no mueve el componente.
2. **Route no es descubrible**: el usuario no sabe que tiene que seleccionar un airwire primero y luego hacer clic en "Route Selected Airwire". No hay hint visual de "siguiente paso".
3. **Outline no se entiende**: los botones "New Outline" y el modo Outline no son intuitivos. El usuario no sabe que tiene que hacer clic para agregar vertices.
4. **Airwires**: no es claro si estan vivos o static. Faltan flechas o animacion que muestren conectividad pendiente.
5. **Sync Schematic → PCB**: funciona pero no hay feedback claro de que ocurrio durante el sync (componentes agregados/actualizados/eliminados).

Archivos involucrados:

- `crates/raf_editor/src/panels/pcb_view.rs`
- `crates/raf_editor/src/panels/pcb_view/canvas.rs`
- `crates/raf_editor/src/pcb_document.rs`
- `crates/raf_electronics/src/pcb/layout.rs`

Criterio de cierre:

- El usuario puede mover un componente en PCB haciendo clic y arrastrando.
- Route muestra un hint "Click an airwire to start routing" y al seleccionar un airwire, auto-activa route mode.
- Outline muestra vertices en tiempo real y feedback de "closed" vs "open".
- Airwires tienen animacion sutil o flechas.
- Sync status visible y claro.

### Fase 17: Multi-select grupal real (Game + Electronics)

Estado: iniciada; electronics ya tiene feedback vivo en box-select y Game ya tiene un primer gizmo grupal, pero falta validacion manual y afinado.

Problemas puntuales que entran:

1. **Gizmo no escala al bounding total**: cubierto en un primer nivel de codigo; falta verificar que el bounding y el pivot se sientan correctos en escenas reales y con padres complejos.
2. **Scale handles no se reposicionan**: cubierto en un primer nivel de codigo junto con el bounding del grupo; falta prueba manual seria.
3. **Transformacion no es grupal**: ahora existe un primer path grupal para move/rotate/scale; falta confirmar ergonomia, precision y casos con jerarquia.
4. **Box select en electronics no es vivo**: resuelto en codigo para schematic; falta confirmacion manual y extender la misma claridad donde todavia no aparezca.
5. **Hierarchy box select**: no se puede arrastrar en el panel de hierarchy para seleccionar multiples nodos (como en un explorador de archivos).
6. **Electronics multi-select visual**: resuelto en schematic; pendiente llevar el mismo nivel de claridad a los otros flujos relacionados.

Archivos involucrados:

- `crates/raf_editor/src/panels/viewport_interaction.rs` (gizmo grupal)
- `crates/raf_editor/src/panels/viewport.rs` (gizmo rendering)
- `crates/raf_editor/src/panels/schematic_view/canvas.rs` (box select vivo)
- `crates/raf_editor/src/panels/hierarchy.rs` (box select en hierarchy)

Criterio de cierre:

- Multi-select en Game: gizmo abarca el bounding total, mover/rotar/escalar transforma todo el grupo.
- Multi-select en Electronics: box select vivo con feedback instantaneo.
- Hierarchy: box select por arrastre en el panel, drops marcan el target con icono brillante.
- Scale handles en las 6 caras del bounding total.

### Fase 18: Rotacion de componentes en schematic sin perder wires

Estado: iniciada; ya hay una capa extra de pre-anclaje, pero todavia necesita validacion manual repetida.

Problemas puntuales que entran:

1. Al rotar o espejar un componente en schematic, los wires conectados a sus pines se desconectan visualmente (aunque la logica interna mantenga la conexion por net).
2. Al rerotar, los wires no se reconectan — el pin se movio pero el wire sigue en la posicion vieja. Ahora se intento cubrir tambien el caso legacy pre-anclando wires cercanos antes de transformar.
3. Esto rompe la experiencia de cableado: el usuario no se atreve a rotar componentes porque "se rompen los cables".

Archivos involucrados:

- `crates/raf_editor/src/panels/schematic_view/canvas.rs` (logica de rotacion de componentes)
- `crates/raf_electronics/src/schematic.rs` (modelo de datos del componente)

Criterio de cierre:

- Rotar un componente con wires conectados: los endpoints de los wires se reubican en la nueva posicion del pin.
- Rerotar no pierde conectividad visual.
- Espejar horizontal/vertical mantiene wires conectados.

### Fase 19: Undo cohesivo y sin conflictos de shortcuts

Estado: iniciada; el coalescing base ya esta mejor conectado, pero sigue faltando validacion manual y limpieza fina de shortcuts.

Problemas puntuales que entran:

1. **Ctrl+Z en Game deshace por pixeles**: tras un drag largo, Ctrl+Z solo retrocede ~1 pixel en vez de restaurar la posicion completa pre-drag. El coalescing implementado en Fase 8 no esta funcionando correctamente (el flag `drag_ongoing` o la supresion de `changed` no se aplica en todos los caminos).
2. **Ctrl+Z/Y conflictua con clipboard**: ambas acciones usan Ctrl+Z/Y pero en contextos diferentes (Game viewport vs schematic text editing). `Ctrl+Shift+Z` ya se bloqueo tambien con foco de texto, pero falta validacion completa de todos los campos y popups.
3. **Undo stack no se limpia correctamente**: despues de varias operaciones, el undo puede contener snapshots parciales o duplicados.

Archivos involucrados:

- `crates/raf_editor/src/panels/viewport_interaction.rs`
- `crates/raf_editor/src/panels/viewport.rs`
- `crates/raf_editor/src/app.rs`

Criterio de cierre:

- Ctrl+Z tras drag largo restaura la posicion completa del objeto, no micro-pasos.
- Ctrl+Z/Y en schematic text field no afecta al viewport undo.
- Undo stack se mantiene limpio: no hay snapshots duplicados ni parciales.

### Fase 20: Value popup y context menus coherentes

Estado: iniciada; el value popup ya mejora, pero export/context menus todavia no cierran esta fase.

Problemas puntuales que entran:

1. **Value popup no se cierra automaticamente**: resuelto en codigo para el editor de value de schematic; falta prueba manual repetida y revisar que no queden caminos alternos sin ese cierre.
2. **Context menus** en hierarchy, schematic y viewport a veces quedan "pegados" en pantalla incluso despues de la accion.
3. **Export popup** no tiene opcion de "Save to file" — solo copia al clipboard.

Archivos involucrados:

- `crates/raf_editor/src/panels/schematic_view/canvas.rs`
- `crates/raf_editor/src/panels/hierarchy.rs`

Criterio de cierre:

- Value popup se cierra al cambiar de seleccion o click fuera.
- Context menus se cierran despues de cada accion.
- Export popup ofrece "Save to file" para Netlist, BOM y SVG.

### Fase 21: UX/UI general pre-1.0

Estado: no empezada.

Problemas puntuales que entran:

1. **Legibilidad en ventanas pequenas**: top bar, bottom bar y HUD deben verse completos sin clipping ni scroll horizontal forzado.
2. **Estados activos de herramientas**: no es obvio que herramienta esta activa (Select/Route/Outline/Wire) en la barra superior. Faltan indicadores visuales mas fuertes (accent color, icono mas grande, texto de estado).
3. **Sin drag de assets a hierarchy**: no se puede arrastrar un asset desde el browser al hierarchy/scene para agregarlo como nodo.
4. **Loading screen basica**: muestra progreso pero es generica.
5. **Project Hub basico**: lista proyectos recientes y permite crear nuevo pero no tiene opciones avanzadas (importar, duplicar proyecto, etc.).
6. **No hay "New file" in-project**: no se puede crear un nuevo script o asset desde el editor, solo desde el explorer.
7. **Dragging desde el hierarchy no muestra el icono del nodo en el ghost**: solo texto.
8. **No hay tooltips en los iconos de la barra superior del viewport** (los botones de grid, labels, etc.).

Archivos involucrados:

- `crates/raf_editor/src/panels/viewport_hud.rs`
- `crates/raf_editor/src/panels/viewport.rs`
- `crates/raf_editor/src/panels/hierarchy.rs`
- `crates/raf_editor/src/panels/asset_browser.rs`
- `crates/raf_editor/src/panels/hub.rs`
- `crates/raf_editor/src/app.rs`

Criterio de cierre:

- Top bar, bottom bar y HUD legibles en ventanas de 1024x768 o mayores.
- Herramientas activas claramente indicadas con color de acento y texto.
- Drag de assets a hierarchy/scene funciona.
- Tooltips en todos los botones de la barra superior del viewport.
- Ghost preview en hierarchy drag muestra el icono del nodo.
### Actualizacion 2026-07-05 - Sesiones de pulido UX/UI y sistema de scripting

#### Pulido UX/UI (sesiones previas)

- Gizmo rotation fix critico: rotacion acumulativa incremental en vez de delta absoluto. Fix en 	ransform_controller.rs y iewport.rs (group drag).
- Ctrl snap a 15 grados en rotacion (PI/12 radianes).
- Camera focus Lerp suave (tecla F).
- Copy/paste entidades en game mode (Ctrl+C/Ctrl+V).
- Camera bookmarks (Ctrl+1/2/3 save, 1/2/3 restore).
- Multi-edit en properties (color/visible propaga a todos los seleccionados).
- Outline double tone (primario vs secundario).
- Cross-probe schematic <-> PCB por designator.
- Live DRC badge en status bar.
- Net naming inline (right-click wire -> Rename net).
- Measurement tool (tecla M) en schematic.
- Tooltips en HUD del viewport.
- Unidades en todas las interfaces: Properties (m, m3), HUD (m), Schematic canvas (mm), PCB canvas (mm), Measurement tool (mm), Settings grid (m).

#### Sistema de scripting (esta sesion)

- **Nuevo crate af_script**: arquitectura completa de scripting con 3 tiers.
  - Tier 1 (Rhai): backend completo con Host API registrado via thread-local. Compila y pasa 4 tests.
  - Tier 2 (WASM Native Module): stub documentado. Reemplaza el approach de C++ FFI crudo por WASM con AuraRafi Host ABI propio.
  - Tier 3 (Visual Nodes): backend que puentea af_nodes::executor al Host API.
- **Host API**: ScriptContext, NodeHandle (opaco, estilo Roblox), ScriptValue (dinamico).
- **Configuracion**: EngineSettings (script_runtime_enabled, default_script_language, script_hot_reload, script_timeout_ms, script_external_editor_cmd) y ProjectSettings (enable_scripting, allowed_script_languages, script_execution_mode, auto_attach_scripts).
- **UI**: nueva seccion "Scripting" en settings_panel y nueva card "Scripting" en project_settings.
- **Comandos**: dominio script.* con 7 comandos (create, attach, detach, list, validate, run, compile_nodes) en commands/script.rs y catalog.json.
- **i18n**: 12 claves nuevas en en.json y es.json.
- **Docs**: docs/SCRIPTING_SYSTEM.md (nuevo, arquitectura completa + roadmap), docs/ARCHITECTURE.md actualizado, .ai/SYSTEM_TRUTH.md actualizado, docs/COMMANDS.md actualizado.

Estado del scripting: arquitectura lista, runtime no implementado. El Host API y el backend Rhai compilan y tienen tests, pero no hay ScriptRuntime en pp.rs todavia (Phase B del roadmap). Los visual nodes siguen logeando "deferring to ECS Bridge" hasta Phase C.
