# Estado De Estabilizacion

Fecha: 2026-08-23

## Estado actual de la migracion (2026-08-23)

Este documento conserva el historial de fases y hallazgos de estabilizacion.
Sus marcas `DONE`, rutas y nombres de hosts describen el estado de la fecha en
que fueron escritos, no una garantia del checkout actual. La ruta vigente del
editor es Winit + RafUI + ApiGraphicBasic: `native_application.rs` compone el
ciclo de ventana, `native_workbench.rs` y `native_studio.rs` contienen las
superficies retenidas, `native_editor_runtime.rs` concentra input/historial/
pacing, y `NativeEditorCompositor` compone canvas y UI.

Los hosts y adaptadores antiguos ya no forman parte de los manifiestos ni del
runtime activo. Las secciones posteriores que mencionan esos adaptadores,
`ViewportSurfaceHost` u otros hosts antiguos son evidencia historica para
comparar bugs, no archivos que deban reintroducirse. El canvas de Game y el
canvas de Electronics usan la frontera nativa. Electronics ya tiene edición,
historial, selección y comandos conectados; la aceptación que falta es visual
en el editor y la ampliación progresiva de authoring PCB/analysis.

La prioridad actual sigue siendo estabilizar input/foco, comandos, historial,
compositor, caches y consumo de recursos antes de activar Play/Runtime. En
Electronics, la composición retained ya está separada de la lógica del shell:
`native_workbench_surface.rs` compone las superficies y
`native_workbench_input.rs` enruta input, `native_workbench_surface.rs` compone
las superficies y `native_workbench_electronics.rs` prepara proyecciones del
documento. No se
deben reintroducir los archivos widget eliminados ni traer implementaciones
desde Git para resolver regresiones; cualquier recuperación debe reconstruirse
contra las APIs actuales y comprobarse en el checkout vivo.

DRC y simulación muestran estados explícitos (`not run`, `running`,
`completed`, `cancelled` y `failed`). El cálculo nativo se ejecuta fuera del
hilo de UI mediante `electronics_analysis.rs`; el dock hace polling del
resultado, permite cancelar y los cambios del documento invalidan los
resultados anteriores y sus marcadores DRC.

## Actualizacion 2026-08-20 — Fronteras nativas y alcance vigente

- El editor inicia por la ruta nativa `Winit -> RafUI -> ApiGraphicBasic`.
  `raf_editor` ya no importa tipos `wgpu::*` ni declara WGPU como dependencia
  directa; WGPU permanece encapsulado dentro de `raf_render` como adapter.
- La ventana, cierre, resize, DPI, minimizar, restauracion y presentacion son
  responsabilidades del host nativo Winit/ApiGraphicBasic. El renderer no
  intenta reemplazar el shell del sistema operativo ni recrear controles de
  ventana por su cuenta.
 - Electronics ya no es una frontera pasiva: `NativeElectronicsEditor` es el
  controlador activo de schematic/PCB, historial, seleccion, input, DRC,
  simulacion y comandos. La aceptacion pendiente es visual en el editor y la
  ampliacion progresiva de authoring PCB/analysis; no se deben reintroducir
  los hosts de widgets retirados.
- La superficie Game de Nodes ya fue reconstruida como documento RafUI nativo:
  carga y guarda el `NodeGraph` de la sesion y sus comandos de alta, baja,
  seleccion y validacion pasan por el boundary nativo. Las demas superficies
  Game historicas se recuperaran con la misma regla; no se restauran cuerpos
  de la interfaz retirada.
- Loading, Hub, New Project, descubrimiento de proyectos y apertura/retorno al
  Hub usan ahora superficies nativas directas; la lista se reconstruye desde
  `project.ron` y la creacion conserva la ruta por defecto y el selector de
  carpeta.
- Agent/AI Chat no se eliminó: su superficie RafUI nativa conserva sesiones,
  paginacion, modelos, modos, aprobaciones y composer. Si el proveedor no esta
  configurado muestra su estado real, sin fingir que el backend esta activo.
- Los modulos renderer sin consumidores del camino nativo quedan clasificados
  como `prepared` o `compatibility` hasta una auditoria de API publica. No se
  eliminan por conteo de archivos ni se consideran basura automaticamente.

## Actualizacion 2026-08-13 - Beta de authoring Agent/CLI/MCP

- El editor abierto es el modo beta recomendado para crear escenarios 3D y
  trabajar scripting: `raf attach` y `raf mcp serve --attach` llegan al mismo
  endpoint local que conserva la escena, la sesion, el guardado y el historial
  real de `SceneHistory`.
- Las mutaciones adjuntas devuelven revision, diff estructurado (creados,
  modificados y eliminados), verificaciones, metricas y un `undo_token` real.
  `transaction.undo` solo acepta el token en el mismo proyecto, sesion y
  revision; cualquier edicion posterior lo invalida.
- La CLI headless cerrada permanece deliberadamente limitada a inspeccion,
  capacidades, proyecto y workspace. No simula mutaciones de escena que no
  hayan pasado por un executor de documentos.
- Queda documentado para v0.12 un host core opcional y manual para trabajar con
  el editor cerrado: sin RafUI ni renderer, con lock de proyecto,
  presupuestos de recursos, endpoint local efimero y apagado explicito. No se
  convertira en un servicio residente por defecto para proteger equipos
  potato.
- Onboarding: `docs/CLI_MCP_QUICKSTART.md` para humanos y
  `.ai/skills/raf-game-authoring/` para agentes. Play, Stop y Runtime siguen
  fuera de esta estabilizacion.

## Actualizacion 2026-08-06 - Agent FPS, historial retained y paginacion

- Se identifico la causa raiz del lag severo al abrir Agent: la ruta GPU de
  `RafUiSurfaceBridge` no guardaba el tamano logico/fisico despues de renderizar
  una presentacion. `render_needed` quedaba verdadero en todos los frames,
  incluyendo idle. Al cambiar de tab el bridge dejaba de ejecutarse y por eso
  el engine parecia estabilizarse.
- La ruta GPU ahora marca ambos tamanos despues de una presentacion exitosa.
  Resize, cambio de superficie, scroll e interaccion real siguen invalidando;
  un Agent quieto ya no debe recomponerse continuamente.
- El historial visible continua paginado: el runtime conserva los mensajes
  para contexto, pero la superficie retained construye solo una pagina de
  mensajes no-system. `Load older messages` y `Back to latest` son navegacion
  local, no descargas del backend ni una segunda persistencia.
- `Settings > AI` expone `Agent messages per page`, con rango 4..32 y default 8.
  Valores pequenos priorizan FPS al abrir chats largos; valores grandes reducen
  clics pero aumentan el costo de layout/atlas/pintura de cada pagina.
- Al cambiar de chat o pagina se limpian scroll, foco, hover y pointer capture
  retenidos para que el documento anterior no contamine la nueva superficie.
- Regla para futuras regresiones: cualquier bridge retained debe actualizar sus
  marcas de ultimo target presentado en las rutas GPU y CPU, y toda animacion o
  cambio de documento debe medirse con `render_needed`, cache hits, uploads del
  atlas y tiempo de presentacion. Nunca usar un contador de FPS aislado como
  prueba de idle-zero-work.
- Se corrigio la fuga de input entre superficies: un RMB iniciado en el
  viewport 3D ya no se entrega al Agent al cruzar su rectangulo durante el
  orbitado. El bridge conserva el gesto solo si nacio dentro de la superficie
  o si RafUI ya tenia pointer capture.
- La huella del ultimo mensaje quedo acotada a los 900 caracteres visibles de
  la tarjeta. Tool results largos siguen en runtime, pero no se recorren ni se
  serializan completos para decidir si una superficie retained cambio.
- No se movio la carga de historial a un hilo artificial: la sesion observada
  mas grande tenia 101 mensajes y aproximadamente 64 KB de texto serializado;
  el costo dominante era la presentacion repetida del bridge, no la lectura de
  ese archivo. Si una futura medicion demuestra una sesion mucho mayor, se
  debe amortizar por paginas antes de agregar concurrencia.

## Actualizacion 2026-08-04 - Shell retained, menus, dock motion y controles de ventana

- La barra superior recupero el modelo compartido historico de
  `File | Edit | View | Project | Help`. Cada menu abre debajo de su trigger
  como una superficie retained elevada, cierra por Escape/click externo o
  comando aceptado y conserva comandos estables; las acciones Edit que aun no
  tienen backend permanecen visibles pero deshabilitadas.
- El downbar ya no comparte una sola textura mutable entre grupos. Cada
  `group_id` tiene su propio `RafUiSurfaceBridge`, por lo que al crear un split
  las tiras de los grupos anteriores conservan sus iconos, etiquetas y estado.
- El drag del downbar actualiza el destino con el puntero global, mantiene el
  nodo fuente capturable y anima el slot de insercion con `UiTween`. La creacion
  o eliminacion de tracks interpola la geometria con `UiMotionSpec::layout()`;
  el resize manual sigue inmediato para conservar precision del puntero.
- Hub y editor exponen minimizar, maximizar, cerrar y arrastre mediante
  `UiWindowCommand`; el host ejecuta la accion y la superficie solo emite el
  intent.
- Regla vigente para agentes: las transiciones funcionales son prioridad en
  menus, seleccion, drag/reorder, docking, creacion/remocion de paneles y
  cambios de layout. Se deben usar tweens compartidos, respetar reduced motion
  y evitar animacion decorativa permanente.
- Pendiente de estabilizacion: revisar el rendimiento de estas transiciones en
  GPU y fallback CPU. Hay que medir frame time durante idle/hover/drag/split,
  allocations, uploads de textura/atlas, repaints solicitados y comportamiento
  en DPI alto y hardware integrado. Con esas mediciones se decide si el costo
  actual ya es aceptable o si se optimizan invalidacion, cache, frecuencia o
  composicion antes de marcar motion como cerrado.
- Verificacion de esta actualizacion: `cargo fmt --all -- --check`,
  `git diff --check`, `cargo test -p raf_ui --lib` (57),
  `cargo test -p raf_editor --lib` (58), `cargo check -p raf_editor` y
  `cargo build -p aura_rafi_editor` pasan. La captura visual compartida del
  ejecutable confirma la barra completa, el downbar por grupos y el shell de ventana;
  la medicion comparativa de rendimiento queda abierta como tarea explicita.

## Actualizacion 2026-07-27 - RafUI Foundation Integrity y command bridge

- Se cerro el primer bloque de integridad del nucleo retained sin reconstruir
  interfaces visuales: RowWrap/Grid, medicion intrinsic localizada con
  reflow, scroll con limites reales, rango virtualizable, drag con umbral,
  caret/seleccion basica, Shift+Tab, modificadores y validacion/migracion de
  documentos.
- El atlas de texto deja de depender del ID del nodo para compartir cadenas,
  recupera el espacio cuando se llena y reporta una region sucia para que el
  GPU no suba toda la textura en cada edicion. El compositor tambien libera
  texturas de imagen que abandonan el draw list.
- Iconos con texto usan una composicion leading/label y tintado semantico;
  estilos agregan estados Selected/Open/Invalid y pueden heredar color del
  tema. Esto queda en el nucleo, no en una pantalla de UI concreta.
- `ViewportPanel::navigation_status()` prepara el modelo de la futura franja
  `Move | World | Snap | Camera | Focus Lock`. No se agrego una interfaz ni se
  duplico el manejo del viewport.
- Se definio una fase futura de Settings para `selection_mode`,
  `free_drag_enabled`, `drag_threshold_px`, `gizmo_only_transform`, snap y
  confirmacion de transformaciones masivas. No se monta ahora porque el shell
  visual esta decommissioned.
- `raf_core::command_protocol` y `raf_editor::commands::gateway` separan el
  kernel de comandos de cualquier consola o ventana. El mismo JSONL acotado
  puede viajar por stdio, TCP, Unix socket o named pipe; queda preparado para
  CLI y MCP de agentes sin activar un runtime jugable. El protocolo valida
  version y nombre antes de ejecutar.
- TextField agrega seleccion total Ctrl/Cmd+A, preedit IME en el contrato
  nativo y libera capturas al perder foco de ventana. El host directo puede
  aplicar `prefers_reduced_motion` sin acoplar RafUI a Winit.
- Se preserva la regla critica: no tocar Play, Stop, Runtime ni simulacion en
  esta fase. Tampoco se agregan sombras, PBR, particulas o animacion
  esqueletica.
- Verificacion de esta actualizacion: `raf_ui` 42 pruebas, `raf_render` 155
  pruebas, `raf_core` 37 pruebas, `raf_editor` 39 pruebas y `cargo check`
  del workspace pasan; `cargo fmt --all -- --check` tambien pasa. La QA visual
  manual en 100%, 125%, 150% y 200% DPI sigue pendiente hasta que exista una
  superficie nuevamente montada.

## Actualizacion 2026-07-22 - RafUI physical-density correction

- Se corrigio la mezcla de coordenadas logicas y fisicas del compositor GPU:
  solids, texto e imagenes ahora convierten sus vertices al target fisico antes
  de NDC, y el target fisico forma parte de la clave de geometria.
- `UiEnvironment::raster_scale()` ahora coincide con `pixels_per_point`; no
  supersamplea un atlas 2x dentro de una textura 1x.
- Las superficies retained usan muestreo nearest cuando las dimensiones fisicas
  de origen y destino coinciden, incluso con DPI fraccional redondeado; linear
  queda reservado para un reescalado real. Viewport y CAD conservan su politica
  linear.
- Los iconos retained se cargan con mipmaps CPU alpha-correctos y reduccion
  premultiplicada; el sampler elige un mip completo en vez de mezclar dos,
  evitando texture shimmering, bleeding y el aspecto lavado en iconos 64px
  mostrados en celdas pequenas.
- Verificacion: `raf_ui` 38 pruebas, `raf_render` 147 pruebas, `raf_editor`
  74 pruebas y `cargo check -p raf_ui -p raf_render -p raf_editor` pasan.

## Actualizacion 2026-07-22 - Games workbench visual pass

- El toolbar de Game conserva solo herramientas existentes de edicion. Scene
  y FPS siguen siendo un unico estado dinamico al extremo derecho, sin
  duplicados ni controles globales inventados.
- El viewport queda compuesto por dos overlays retained RafUI independientes:
  arriba a la izquierda, 2D/3D y el modo de render real; abajo a la derecha,
  foco, grid, reset de vista y el historial real de Undo/Redo. No se agrego
  Play/Run porque ese runtime no pertenece a esta superficie.
- Cada overlay usa su propio `RafUiSurfaceBridge`; barra superior, overlay
  superior y overlay inferior no comparten textura, atlas, input ni tooltip.
  Sus dos rectangulos se registran como zonas bloqueadas para que orbit, pan
  o manipulacion del mundo nunca empiecen bajo un control flotante.
- `undo.png` y `redo.png` son assets separados de alta densidad, generados
  como flechas direccionales opuestas. La accion deshace/rehace los snapshots
  existentes de la app; no son iconos decorativos ni comandos vacios.
- El texto retained tiene un contrato separado de la geometria: el target de
  paneles e iconos sigue 1:1 con el output fisico, mientras que el atlas de
  texto usa un piso de 1.25x y aplica peso semantico (regular/medium/bold) a
  la cobertura vectorial. Esto evita volver a suavizar todo el surface para
  mejorar solo etiquetas pequenas.
- No se agrego Play/Run al boceto porque el runtime de Game sigue fuera del
  contrato disponible en esta superficie.
- Hierarchy reorganizo su header en dos niveles: titulo/conteo y controles de
  busqueda/carpeta. Las filas conservan seleccion, expansion, visibilidad,
  menu contextual y tooltips, pero reducen peso visual e iconos saturados.
- Inspector tabs, Properties, Sessions y bottom tabs adoptan el mismo peso de
  icono: activo enfatizado, inactivo atenuado, sin cambiar sus acciones.

## Actualizacion 2026-07-21 - RafUI Frontier Core

- `raf_ui::overlays` establece el contrato de colocacion global para tooltips,
  menus, popovers, modales y previews de drag. El overlay conserva el dueño
  semantico, pero ya no queda atrapado por el clip de la superficie origen.
- `UiSizeMode` agrega `FitContent`, `MinContent`, `MaxContent`, `Fixed`, `Fill`
  y `Auto`. ApiGraphicBasic mide texto localizado desde el atlas antes de
  generar el draw list final.
- `UiInteractionState` conserva tiempo monotono de hover, transiciones enter/
  leave y progreso de hover intent. `UiTween` sustituye pasos de opacidad por
  motion dependiente de tiempo y compatible con reduced motion.
- `raf_ui::components` centraliza recipes para icon buttons, panel headers,
  tree rows y tooltips. El tooltip del editor vive en
  `raf_ui_tooltip.rs`, separado del bridge de colocacion.
- `UiEnvironment` centraliza layout logico, target fisico y raster scale. GPU y
  CPU reciben la misma geometria retenida.
- `UiSurfaceDiagnostics` expone conteos de cajas, clipping, hit regions,
  text requests, nodos de tamaño cero y z-order para inspector y golden tests.
- La migracion conserva un compositor de texturas como frontera de presentacion.
  No se agrego dibujo de rectangulos, texto ni interaccion retenida fuera de
  RafUI.
- Verificacion estructural de esta actualizacion: `cargo check -p raf_ui
  -p raf_render -p raf_editor` pasa. La validacion visual manual queda abierta
  hasta reiniciar el engine y revisar dark/light, compacto, HiDPI, GPU y CPU.

## Actualizacion 2026-07-19 - Integridad del viewport y batching ligero

- El viewport ahora registra la entrada de gizmo/vertex antes de construir el
  frame. La vista deja de mostrar un estado atrasado un frame durante un drag.
- La clave de cache ya no se invalida en cada frame de Vertex Edit: solo cambia
  cuando una edicion modifica la malla. La camara y la escena siguen siendo
  entradas explicitas de la clave.
- El target de escena respeta la escala DPI del host nativo, por lo que el
  render usa pixeles fisicos y los overlays conservan geometria logica.
- Las lineas contiguas se registran como `DrawLineBatch`; GPU y CPU comparten
  ancho, color y depth bias. El GPU expande cada linea a un quad de seis
  vertices en una sola llamada por batch; el CPU usa el mismo ancho visible.
- Los meshes persistentes se deduplican por frame y el cache GPU tiene limite
  de entradas. Los overrides de Vertex Edit son transitorios y no contaminan
  el cache persistente.
- El culling y el foco usan escala del transform mundial, corrigiendo hijos
  escalados por un padre. La geometria visible ya no depende de que el nodo
  tenga nombre.
- `Sprite2D` deja de ser una primitiva activa. Escenas RON y comandos antiguos
  se aceptan como alias de `Plane`; el 2D de juego queda definido como escena
  3D con camara ortografica. UI/overlays siguen siendo RafUI.
- No se habilitaron sombras, postprocesado, PBR, particulas ni animacion
  esqueletica; siguen fuera del alcance de estabilizacion.
- Crash de arranque corregido: el shader de lineas ya no asigna swizzles WGSL
  (`clip.xy`/`clip.z`), una forma rechazada por wgpu 23 antes de abrir proyectos.
  `shaders::tests::basic_scene_shader_parses_with_wgpu_naga` protege el contrato.
- Verificacion: `cargo check -p raf_render -p raf_editor` pasa;
  `cargo test -p raf_render --lib` (139), `cargo test -p raf_editor --lib`
  (67) y `cargo test -p raf_core --lib` (34) pasan.

## Actualizacion 2026-07-18 - RafUI compilation and native menu boundary

- RafUI no longer creates a UI `BasicCommandList` plus a separate direct draw
  list. `UiSurfaceFrame` now carries only layout, hit regions, and text
  requests; `UiSurfaceDrawList` is the single retained paint payload consumed
  by GPU presentation and CPU recovery.
- `UiSurfaceCompilationCache` reuses unchanged layout, hit testing, resolved
  text, and paint data. The GPU compositor retains vertex buffers, avoids
  duplicate image uploads, and merges compatible adjacent paint runs without
  changing the visual stacking order. Metrics expose cache hits, upload bytes,
  paint runs, and draw calls for real before/after measurements.
- `UiApplicationMenu` is the canonical File/Edit/View/Project/Help command
  tree. The native application bar and `NativeApplicationMenuAdapter` consume
  that shared model. Platform adapters return activations only; they never
  mutate scene, CAD, project, or persistence state.
- The native Winit shell owns the window and event loop. Menu presentation has
  an explicit native-host boundary instead of a hidden platform-specific fork.

## Actualizacion 2026-07-18 - Contratos RafUI y ApiGraphicBasic

- Documentacion RafUI: `docs/RAF_UI.md` y `docs/EDITOR_RAFUI.md` definen la propiedad de cada
  capa, el ciclo de una superficie retained, layout responsivo, docking, foco,
  scroll, accesibilidad, checklist y el patron exacto de menus con trigger,
  overlay, cierre por Escape/click externo y acciones tipadas.
- Regla visual: `DESIGN.md` y las reglas de `.ai` son obligatorias para toda
  nueva superficie. Tokens semanticos, i18n, contraste, escala logica/fisica,
  no inventar datos, no usar decoracion costosa y no crear un segundo sistema
  de widgets son requisitos, no sugerencias.
- CAD: Schematic y PCB siguen siendo superficies ApiGraphicBasic. El overlay
  de componentes solo consume assets PNG y el mismo documento CAD para sus
  bounds; el minimapa decorativo anterior fue retirado hasta tener navegación
  real sobre la cámara.
- ApiGraphicBasic: `docs/APIGRAPHICBASIC.md` fija que Scene, CAD, RafUI y
  assets pasan por contratos y handles propios. `raf_assets` descubre y
  decodifica; ApiGraphicBasic asigna, sube, presupuesta y libera recursos GPU.
  WGPU continua como backend privado actual y no puede filtrarse hacia
  documentos, paneles o modelos de dominio.
- Estado de migracion: Hub, Settings y las superficies principales del
  shell/CAD ejercen RafUI; cualquier adaptador de presentacion legado restante
  es transitorio y no debe recibir nuevas superficies. La documentacion no
  sustituye la validacion manual completa del editor.

Este documento resume que ya esta resuelto en codigo, que ya venia funcionando, que sigue pendiente y en que fase cae cada bloque. La idea es tener una sola fuente de verdad mientras cerramos la estabilizacion antes de testear a fondo.

## Actualizacion 2026-07-30 - Downbar y controles RafUI estabilizados

- El downbar inicia con un solo grupo ordenado. Las pestanas se pueden mover
  entre grupos o dividir por los bordes hasta un maximo de tres; al sacar la
  ultima pestana, el grupo fuente desaparece y las columnas se reacomodan.
- El acomodo del downbar se guarda por proyecto en
  `.aura_rafi/editor_downbar.ron`, separado de `project.ron`. El cargador
  normaliza versiones, grupos vacios, duplicados y pestanas no soportadas.
- Console, Assets, Project y Project Settings son las pestanas activas. El
  Project consume el filesystem real; no agrega escenas, sesiones, Node Editor
  ni Agent simulados.
- Project Settings vive dentro del downbar. Settings global permanece en la
  barra superior. Toggles y ranges ya tienen geometria retenida en RafUI y
  los ranges tienen campo numerico editable con parseo y clamp en el host.
- La busqueda Ctrl+K ahora es un textbox real; su ejecucion queda fuera hasta
  conectar el backend de comandos. No se agregaron Build, Play ni estados de
  runtime.
- Verificacion: `cargo fmt --all -- --check`, `git diff --check`,
  `cargo check -p raf_editor -p aura_rafi_editor`, `raf_ui` 54 pruebas,
  `raf_render` 161 pruebas y `raf_editor` 55 pruebas pasan. La QA visual de
  arrastre, recarga por proyecto y DPI aun requiere abrir el ejecutable.

## Actualizacion 2026-07-18 - Hibrido Controlado ApiGraphicBasic/WGPU

- Decision: `ApiGraphicBasic` es el dueño permanente del sistema grafico. WGPU
  sigue siendo el adaptador GPU activo y backend de compatibilidad mientras la
  API propia gana responsabilidades. No se hara un reemplazo total de golpe.
- Organizacion: la migracion no se manejara como niveles desechables. Cada
  update puede mover una responsabilidad completa hacia contratos propios:
  handles, capabilities, device/queue/surface, recursos persistentes, uploads,
  memoria y residencia, comandos, pipelines, sincronizacion, frame graph,
  assets, diagnostico o recuperacion.
- Estado hibrido: viewport, CAD y RafUI conservan una sola ruta superior. WGPU
  y los futuros backends DX12/Vulkan/Metal existen debajo de ApiGraphicBasic;
  no se duplican documentos, escenas, paneles ni command models.
- Fundacion inmediata cuando se autorice programacion: ocultar tipos `wgpu::*`
  de las capas superiores, introducir handles propios, definir un solo dueño de
  device, capabilities y presupuestos, medir el hot path y mejorar batching y
  reuso sin retirar todavia el backend WGPU.
- Retiro lento: un backend nativo puede coexistir con WGPU, convertirse en
  default al pasar paridad y mediciones, y dejar WGPU como fallback. WGPU solo
  sale del producto cuando pasan las pruebas de Scene, Schematic, PCB, RafUI,
  memoria, idle, pacing, resize, perdida de device y hardware integrado/dedicado.
- Alcance de esta actualizacion: solo documentacion y reglas. No se modifico el
  renderer, no se inicio DX12/Vulkan/Metal y no se elimino ninguna dependencia.

Regla autoritativa: [ApiGraphicBasic Native Ownership Rule](../.ai/APIGRAPHICBASIC.md).

## Actualizacion 2026-07-18 - Fundacion 1 Implementada

- ApiGraphicBasic ahora expone handles generacionales propios para buffers,
  texturas, samplers, pipelines, meshes, materiales y surfaces. Estos ids no
  contienen punteros ni tipos de WGPU.
- Se agregaron `GraphicsCapabilities`, `GraphicsMemoryBudget` y preferencia de
  adaptador. `Auto` conserva presupuesto potato y preferencia low-power;
  `GpuPreferred` conserva la ruta de alto rendimiento.
- `BasicDevice` y `RenderRuntimeSnapshot` reportan backend, capabilities y
  presupuesto sin obligar a viewport, CAD o RafUI a conocer la API nativa.
- `SharedGraphicsContext` reemplaza el nombre WGPU en la frontera del runtime.
  `SharedWgpuContext` queda solamente como alias transitorio para compatibilidad.
- `SceneFrameOutput` encapsula la vista GPU en `GpuTextureView` con handle Rafi.
  `from_wgpu` y `as_wgpu` sobreviven solo como aliases de compatibilidad para
  la frontera nativa de presentacion.
- Verificacion: `cargo test -p raf_render --lib` (134) y
  `cargo test -p raf_editor --lib` (66) pasan; `cargo check -p aura_rafi_editor`
  tambien termina correctamente.
- Todavia pendiente: registry real con eviction, enforcement de budgets,
  batching estructural, unificacion completa de DeviceHub entre hosts nativos,
  frame graph y backends DX12/Vulkan/Metal. WGPU sigue siendo el ejecutor GPU.

## Actualizacion 2026-07-17 - Shell CAD Electronics

- Electronics: Schematic y PCB ahora usan el shell compartido con selector
  contextual RafUI, navegador CAD a la izquierda, inspector a la derecha y
  dock inferior fijo. Los documentos, seleccion, cross-probe y sincronizacion
  PCB siguen en sus modulos existentes; no se creo una segunda escena CAD.
- Canvas: la presentación actual pertenece a `native_electronics.rs` y al
  host directo de ApiGraphicBasic; esta entrada conserva el nombre del host
  anterior sólo como evidencia de la migración.
- Biblioteca: el catalogo de schematic conserva busqueda y colocacion, ahora
  con clipping, scroll de contenido correcto y barra de posicion para escalar
  a bibliotecas mas grandes.
- Analisis: DRC y Simulacion dejaron de reenviar al usuario a Console. Cada
  una abre una pestana propia del dock con el reporte estructurado real y una
  accion para ejecutar de nuevo. Cualquier edicion invalida esos resultados.
- Recursos: `tools/generate_electronics_ui_icons.ps1` genera assets PNG locales
  de alta resolucion solo durante desarrollo. No forma parte del loop ni del
  paquete en ejecucion.

## Actualizacion 2026-07-12

- FPS visible: el contador deja de derivarse de un reloj de UI inestable o solo
  del tiempo de CPU del viewport. Usa un reloj monotono suavizado del editor, por
  lo que un frame ocioso no se reporta como un valor astronomico. El limitador
  reutiliza la ultima presentacion valida hasta el siguiente intervalo, de modo
  que limita trabajo real de CPU/GPU y no solo solicitudes de repaint.
- Gizmo: `gizmo_growth_scale` ahora escala la geometria compartida de Move y
  Rotate, incluido el hit-test. El grosor se mantiene estable y Scale queda
  completamente fuera de esa opcion.
- Focus: F con focus lock apagado solo encuadra la seleccion. Con lock activo,
  WASD aplica una inspeccion temporal relativa a la camara y vuelve al punto de
  foco unicamente al soltar las teclas, sin el rebote de seguimiento.
- Sessions: Properties incluye una pestana Sessions para activar o crear
  mundos, interfaces y documentos de electronica aislados.
- Agent: `electronics.diagnose` une topologia pin-red, DRC y simulacion para
  analisis de conexiones antes de que el agente sugiera cambios.
- Agent history: los cambios de documentos hechos por herramientas del agente
  ahora entran a Undo/Redo; las peticiones Undo/Redo del propio agente se
  encolan al editor sin acoplar el callback de herramientas a la interfaz.
- UI: `raf_ui` suma reglas de estilo por clase/id/tipo, texto atlas reutilizable
  y entrada retained de puntero, foco, drag y menu contextual para la futura
  superficie nativa.
- Assets IA: la ruta remota usa `gpt-image-2` por defecto; la nueva herramienta
  `asset.generate_local_png` crea iconos, badges, sprites temporales y texturas
  de referencia de forma determinista, offline y bajo demanda. Ninguna ruta
  mantiene Python activo dentro del loop del editor.

Pendiente de cierre: compilacion completa, pruebas de crates y recorrido manual
del viewport, chat, Sessions y CAD sobre la app nativa.

## Actualizacion 2026-07-13 - Hub RafUI

- Hub: la ruta activa despues de Loading sigue siendo RafUI/ApiGraphicBasic. La
  primera presentacion bitmap se reemplazo por un atlas de fuente vectorial
  cacheado; deja de mostrar el texto pixelado provisional.
- Jerarquia del Hub: bienvenida, proyecto reciente, proyectos reales,
  creacion de Game/Electronics, actividad reciente y selector Dark/Light/System
  son datos reales. No se agregaron templates, marketplace, IA ni estados de
  build ficticios.
- Proyecto reciente: clic izquierdo abre. Clic derecho abre un menu retained en
  la posicion del cursor con Abrir, Duplicar y Quitar de recientes.
- Presentacion: el Hub ya no llama `request_repaint()` en reposo. Solo solicita
  un frame de seguimiento al cambiar estado por una interaccion, eliminando un
  bucle de presentacion evitable que podia amplificar avisos de frame
  `Suboptimal` durante el launcher.
- Verificacion automatica: `cargo check -p raf_editor`, `cargo test -p raf_ui`
  (24), `cargo test -p raf_render` (126) y `cargo test -p raf_editor` (32)
  pasan en esta actualizacion.

Pendiente manual: relanzar el editor, revisar legibilidad de la nueva fuente,
probar el menu secundario, los tres temas, resize de ventana y confirmar que
los avisos `Suboptimal` ya no se repiten en reposo. Si quedara alguno tras un
resize real, debe rastrearse como evento de swapchain de la ventana, no
silenciarse en logs.

## Actualizacion 2026-07-14 - Fundacion Del Shell Compartido

- RafUI: `DockLayout` ahora guarda politica de panel (`Movable` o `Fixed`) y
  lados permitidos. Un dock no puede escapar a una zona que no acepta su tipo.
- Shell: cada proyecto puede persistir `editor_shell.ron`. Hierarchy,
  Properties y Sessions son paneles auxiliares movibles; Bottom es fijo y
  Center es el slot exclusivo para viewport o CAD.
- Compatibilidad: archivos inexistentes, antiguos o incompletos se reparan al
  abrir el proyecto. Se conserva toda dimension valida del usuario y se
  restaura infraestructura fija si se hubiera guardado en una posicion invalida.
- Adaptador temporal: el editor actual ya usa esas dimensiones persistidas para
  sus paneles laterales e inferior y las guarda solo despues de terminar un
  arrastre sobre el borde. Un resize de ventana no pisa la preferencia del
  usuario. La fuente de verdad de layout es RafUI y los hosts nativos; los
  adaptadores historicos no deben volver a recibir cuerpos de panel.
- Documento retained: `editor_shell_surface.rs` define el shell comun de Game
  y Electronics con intents tipados. Viewport, Schematic y PCB quedan como
  superficies de renderer, no como widgets genericos.
- Migracion visible inicial: los tabs inferiores y el selector
  Properties/Sessions ya se componen con RafUI/ApiGraphicBasic. Console,
  Assets, Agent, Properties y Sessions conservan sus cuerpos actuales mientras
  se mueve cada uno a una superficie propia.
- Electronics: el selector Schematic/PCB tambien es retained. El cambio de
  modo sigue ejecutando sincronizacion PCB y cross-probe en la frontera de la
  aplicacion, no dentro del renderer ni del documento de UI.

## Actualizacion 2026-07-14 - Composicion Del Hub RafUI

- Layout: el Hub deja de meter el contenido principal y el rail derecho dentro
  del mismo `ScrollView`. Ahora tiene tres regiones retained independientes:
  navegacion, `hub.content` y `hub.side-scroll`. El rail derecho conserva un
  ancho de escritorio real hasta que el contenido ya no cabe y entonces apila
  de forma responsiva.
- Paneles: Create new y Recent activity tienen tracks de altura declarados.
  Antes un panel `auto` en columna se resolvia a altura cero y sus controles
  se sobreponian; ahora cada accion y actividad ocupa su propia region.
- Tema: Dark, Light y System siguen usando los tokens semanticos de RafUI. La
  direccion por defecto en oscuro es negro, blancos/neutros y naranja; no se
  agrega azul de marca.
- Recursos: se reutilizan los PNG existentes de marca, settings, game y
  electronics. No se agrego generacion decorativa ni una dependencia de
  imagenes en el loop del editor.
- QA visual: se reviso el Hub en ventana nativa con escalado DPI 120. La
  captura correcta debe usar coordenadas fisicas de ventana en Windows; una
  captura virtualizada puede recortar el rail derecho y dar un falso positivo.
- Pruebas locales focalizadas: `studio_surface::tests` verifica el rail a un
  ancho de escritorio compacto y que Create/Activity no compartan una altura
  cero. `hub_surface_host` verifica que una solicitud contextual conserve la
  ruta del proyecto.

## Actualizacion 2026-07-14 - Nitidez Y Scroll Del Hub RafUI

- Scroll: la adaptacion del shell anterior al contrato RafUI invierte el signo
  de la rueda al entrar al contrato de offsets retained. Wheel-down ahora muestra contenido mas bajo
  y wheel-up regresa hacia el inicio. Se comprobo en la ventana nativa.
- HiDPI: `HubSurfaceHost` conserva layout e input en puntos logicos, pero crea
  el target de ApiGraphicBasic usando `pixels_per_point`. El atlas de texto se
  rasteriza a esa escala, el compositor GPU transforma clips al viewport fisico
  y el fallback CPU usa la misma geometria fisica.
- Recursos: `tools/ui_assets/generate_editor_icons.py` genera previews PNG de
  640x360 para Game y Electronics. Son visuales neutrales, sin texto incrustado
  para no romper i18n, y reemplazan iconos de 64 px estirados dentro de cards.
- Muestreo: atlas e imagenes usan filtrado lineal para no amplificar aliasing
  al redimensionar. El color y la composicion siguen usando el mismo renderer
  GPU-first y el mismo limite de recursos.
- Verificacion: `cargo test -p raf_ui` (25), `cargo test -p raf_render` (129),
  `cargo test -p raf_editor` (37), `cargo fmt --all -- --check` y
  `cargo build -p aura_rafi_editor` terminaron correctamente. La ventana
  nativa se reviso a escala fisica y la rueda se probo en ambos sentidos.

## Actualizacion 2026-07-14 - Refinamiento Visual Del Hub RafUI

- Composicion: el rail de proyecto y la columna de acciones se compactaron
  para priorizar el area de trabajo. La tarjeta destacada usa un preview mas
  amplio, contenido centrado y una accion primaria alineada al borde inferior.
- Jerarquia: el rail derecho ahora agrupa las acciones reales bajo Quick
  actions. Recent activity incluye el tipo de proyecto como icono y las cards
  separan nombre, tipo y ultima apertura para lectura rapida.
- Contexto: cada card muestra un control de tres puntos que abre el mismo menu
  retained de Abrir, Duplicar y Quitar de recientes que ya exponia el clic
  derecho. No se agregaron acciones ficticias.
- QA: se verifico el layout en la ventana nativa con DPI 120 y los estados
  neutrales y hover de la tarjeta destacada. `cargo test -p raf_editor` (37),
  `cargo fmt --all -- --check` y `cargo build -p aura_rafi_editor` terminaron
  correctamente.

## Direccion Grafica Propia Antes De Runtime

La ruta de trabajo definida para el editor y el engine es GPU nativa. CPU queda
para recuperacion, pruebas, ejecucion sin adaptador valido y equipos
incompatibles; no es el modo visual objetivo de un proyecto normal. Un perfil
ligero reduce resolucion, draw calls y memoria en una GPU integrada, no degrada
automaticamente al rasterizador CPU.

`ApiGraphicBasic` pasa a ser la tecnologia grafica dueña de Rafi: define los
frames, recursos, pipelines, capacidades, superficies, importacion de assets,
residencia en GPU y presentacion. WGPU es el adaptador GPU actual, no el techo
ni el propietario de esa arquitectura. La meta es que Rafi pueda seleccionar
backends propios por plataforma o area de trabajo, incluyendo rutas directas
cuando aporten mejor control, sin reescribir viewport, CAD, RafUI, escenas,
documentos o comandos.

RafUI no depende de WGPU en su modelo: arbol retained, layout, temas, foco,
input y documentos viven en Rust puro. Hoy el compositor de ApiGraphicBasic usa
WGPU para dibujar la lista de quads y texto, pero ese compositor esta debajo de
la interfaz y se puede sustituir por un backend propio sin repetir la migracion
de cada pantalla.

### Proceso Continuo Del Hibrido

No se interpreta como una secuencia de niveles que obligue a parar el engine.
Se interpreta como una lista de responsabilidades que iran faltando hasta que
ApiGraphicBasic pueda vivir sin WGPU:

| Responsabilidad | Estado durante el hibrido | Destino |
| --- | --- | --- |
| API publica | ApiGraphicBasic existe, pero aun hay tipos/ownership WGPU en la implementacion | Solo handles y descriptores Rafi visibles arriba |
| Ejecucion GPU | WGPU ejecuta la ruta activa | Backend propio elegido por plataforma |
| Recursos | Hay reuso parcial, caches y slots, pero falta un registry con presupuesto completo | Recursos persistentes, generaciones, residency y eviction propios |
| Comandos | Scene/CAD/RafUI ya producen datos de render compartidos, con granularidad aun mejorable | Encoder Rafi con batching, render/copy y compute futuro |
| Surface/device | Existen hosts y contextos activos, todavia ligados a WGPU/host transitorio | Un DeviceHub y lifecycle propio por adaptador/surface |
| Shaders/pipelines | WGPU/WGSL sostienen la ruta actual y hay features preparadas | Paquetes y caches por backend bajo layouts Rafi |
| Sincronizacion | La resuelve principalmente el adaptador actual | Estados, fences, recovery y diagnostico propiedad de Rafi |
| Features futuras | PBR, shadows, post, particles y skeletal siguen preparadas/inactivas | Frame graph y presupuestos comunes antes de activarlas |
| Compatibilidad | WGPU y CPU preservan cobertura mientras nace lo nativo | WGPU opcional y finalmente removible; CPU queda recovery |

Durante la coexistencia se selecciona un backend por ruta de device/surface. No
se mezclan recursos WGPU y DX12/Vulkan/Metal dentro del mismo frame salvo que
exista un contrato de interop deliberado y medido; las copias cross-API
accidentales violan el objetivo potato-first.

Antes de runtime se debe decidir con evidencia:

- El workspace usa WGPU 23 como adaptador actual. ApiGraphicBasic necesita
  completar su capa de backend propia con contratos de device, cola, swapchain,
  shaders, recursos, sincronizacion y diagnostico por plataforma.
- El nuevo asset ingress de ApiGraphicBasic sera responsable de importar y
  preparar assets para el backend activo: clasificacion, decode/transcode,
  thumbnails, cache y residencia GPU. `raf_assets` conserva el catalogo y los
  archivos de proyecto; ApiGraphicBasic decide como los recursos llegan a GPU.
- Vulkan, DX12 y Metal se exponen detras de la misma API de Rafi. Los tiers
  ligeros siguen siendo una eleccion de presupuesto, no una limitacion de la
  arquitectura ni una dependencia de una sola capa externa.
- ApiGraphicBasic ya evita el piso WebGL2 en su inicializacion propia. Faltan
  mediciones de adaptador, memoria y frame pacing para ordenar el trabajo de
  los backends propios y priorizar cada plataforma.
- PBR, sombras, postprocesado, particulas, animacion esqueletica y picking GPU
  no se activan en esta estabilizacion. Solo se preservan sus fronteras limpias.
- La evolucion busca que ninguna dependencia externa sea obligatoria para el
  producto final: WGPU puede coexistir como backend mientras ApiGraphicBasic
  gana rutas propias. Ninguna de esas decisiones obliga a reescribir UI,
  escenas, documentos ni comandos.

Validacion obligatoria antes de runtime: GPU integrada y dedicada, resize
sostenido, perdida/restauracion de device, Linux, macOS, frame pacing y memoria.

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
  - Como el HUD se pinta directo con `Painter` y no depende de respuestas de un
    toolkit externo, los tooltips se implementan con hit-test manual del
    pointer contra cada rect conocido + pintado en layer Foreground.
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

- `cargo check -p aura_rafi_editor` pasa limpio en la pasada registrada; las llamadas `allocate_ui_at_rect` de electronics fueron migradas a `allocate_new_ui` despues.

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

- `cargo check -p aura_rafi_editor` pasa limpio en la pasada registrada; las llamadas `allocate_ui_at_rect` de electronics fueron migradas a `allocate_new_ui` despues.

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
- Hot path grafico: el backend GPU activo ya reutiliza buffers de meshes
  persistentes y slots de lineas; aun paga uniforms por draw y uploads de
  overrides transitorios, por lo que falta una medicion estructural completa.
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

- `docs/archive/TEMP_PHASE6_RUNTIME_TRUTH_PASS.md`

### Fase 7: Renderer canonico y hot path grafico

Estado: en progreso.

Ya justificado para esta fase:

- Scene, Schematic y PCB ya comparten una ruta moderna de runtime grafico.
- El viewport ya usa mediciones reales de render/upload y escala adaptativa.
- Sigue habiendo mezcla de verdad documental y tecnica sobre si el renderer debe leerse como CPU-first, GPU-first o ruta hibrida en transicion.
- El backend GPU activo ya tiene cache de meshes persistentes, slots de lineas
  reutilizables y batches contiguos; quedan uniforms por draw y batching
  cross-surface para otra iteracion medida.
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

---

## Actualizacion 2026-07-01 — UX / Movimiento / Settings

### Resumen

Sesion enfocada en bugs de movimiento reportados por el CEO y mejoras de UX chicas pero de alto impacto.

### Resuelto en codigo

| Feature | Descripcion | Archivos |
|---|---|---|
| **WS invertido** | W ahora mueve hacia adelante (direccion de la camara), S hacia atras. `settings.invert_ws` checkbox en Settings > Editor > Mouse para restaurar comportamiento viejo. | `viewport.rs`, `viewport_interaction.rs`, `config.rs`, `settings_panel.rs`, `en.json`, `es.json` |
| **Focus Lock (tecla F)** | F togglea bloqueo de enfoque (si `focus_lock_enabled` en settings). Cuando activo, la camara sigue al objeto seleccionado cada frame. Icono "F" en HUD (junto a grid/labels). Toggle ON/OFF con click en el icono o tecla F. | `viewport.rs`, `viewport_interaction.rs`, `viewport_hud.rs`, `config.rs`, `settings_panel.rs`, `app.rs` |
| **Gizmo auto-scaling** | Manijas de scale (circulos), rotation rings y flechas de translate crecen con la distancia de camara. `settings.gizmo_growth_scale` slider (0-100%) en Settings > Editor > Gizmo Controls. | `viewport_overlay.rs`, `config.rs`, `settings_panel.rs`, `en.json`, `es.json` |
| **Ctrl bloquea movimiento** | Cuando Ctrl esta presionado, WASD y F no se procesan. Soluciona conflicto Ctrl+D (duplicate) moviendo la camara. | `viewport.rs`, `viewport_interaction.rs` |
| **Free drag (click y mover)** | Click en un objeto (sin tocar gizmo) inicia un drag libre. El objeto se mueve en el plano horizontal (Y constante) siguiendo el mouse, como en Roblox Studio. | `viewport.rs`, `viewport_interaction.rs` |
| **Camera boundary bounce fix** | Orbit pitch ahora usa soft-clamp cerca de los limites (-1.4, 1.4) para evitar rebote cuando se llega al tope. | `viewport_bridge.rs` |
| **Part selector bug fix** | El radio de picking usaba `node.scale` local ignorando escala de padres. Corregido a world-space usando `world.x_axis/y_axis/z_axis.length()`. | `input_handler.rs` |

### Nuevas settings agregadas

| Setting | Tipo | Default | Ubicacion |
|---|---|---|---|
| `invert_ws` | bool | false | Settings > Editor > Mouse |
| `focus_lock_enabled` | bool | true | Settings > Editor |
| `gizmo_growth_scale` | f32 (0-100) | 0.0 | Settings > Editor > Gizmo Controls |

### Archivos modificados en total

| Archivo | Cambios |
|---|---|
| `crates/raf_core/src/config.rs` | `invert_ws`, `focus_lock_enabled`, `gizmo_growth_scale` fields + defaults |
| `crates/raf_editor/src/panels/settings_panel.rs` | UI: invert_ws checkbox, focus_lock checkbox+desc, gizmo_growth slider |
| `crates/raf_core/locales/en.json` | 4 nuevas keys i18n |
| `crates/raf_core/locales/es.json` | 4 nuevas keys i18n |
| `crates/raf_editor/src/panels/viewport.rs` | fields: invert_ws, focus_lock_enabled, focus_locked, gizmo_growth_scale, free_drag_*; WS fix con invert; Ctrl guard; focus lock update |
| `crates/raf_editor/src/panels/viewport_interaction.rs` | F key toggle focus lock; Ctrl guard; free drag logic en start/drag/stop |
| `crates/raf_editor/src/panels/viewport_hud.rs` | HudAction::ToggleFocusLock; draw como tercer toggle; tooltip; click handler |
| `crates/raf_editor/src/panels/viewport_overlay.rs` | gizmo_size_factor(); sizes multiplicados por factor |
| `crates/raf_editor/src/app.rs` | Sync de nuevas settings al viewport |
| `crates/raf_render/src/bridge/viewport_bridge.rs` | Soft-clamp de pitch orbit |
| `crates/raf_render/src/bridge/input_handler.rs` | World-space radius para picking |

### Bugs cerrados

| # | Bug | Estado |
|---|---|---|
| (nuevo) | WS invertido (W=backward, S=forward) | Fixeado: W=forward, S=backward por defecto |
| (nuevo) | Ctrl+D mueve camara | Fixeado: Ctrl bloquea WASD/F |
| (nuevo) | Part selector no detecta objetos grandes/gigantes | Fixeado: radius en world-space |
| (nuevo) | Gizmo handles chicos cuando camara lejana | Fixeado: auto-scaling con growth offset |
| (nuevo) | F no togglea correctamente | Fixeado: F ahora togglea focus lock |

### Sigue abierto

- Salida a archivo en export (#20)
- Validacion manual de rotacion schematic (#1), gizmo grupal (#8), undo largo (#3)
- PCB mover componentes (#4, #5, #6)
- Limit FPS cableado real (#11)
- Settings scroll bug (#12)

## Actualizacion 2026-07-07 - Superficies CAD, render tiers y runtime Rhai

- `raf_electronics::cad_scene` ahora deriva escenas CAD retenidas desde schematic y PCB: componentes, pins, wires, traces, pads, airwires, labels, board outline y DRC markers.
- `ApiGraphicBasic::cad_surface` convierte esas escenas CAD a
  `BasicCommandList`; `native_electronics.rs` presenta Schematic/PCB por el
  runtime gráfico compartido con fallback CPU.
- `RenderConfig::for_preset` y `RenderConfig::resource_profile` formalizan los presupuestos por tier: triangulos, textura, escala de superficie, sombras, post-proceso, luces y budgets futuros.
- `ViewportSurfaceHost` ahora deriva un `ViewportSurfacePlan` del tier activo: escala de superficie, frame budget, sombras preparadas, post-proceso, PBR, particulas y skeletal animation quedan como contrato de superficie, no como logica desperdigada en el shell de UI.
- `raf_script::runtime::RhaiScriptRuntime` queda como harness preparado: carga scripts `.rhai` adjuntos, ejecuta `on_start` y `on_update(dt)` contra una escena clonada y reporta errores reales del Host API.
- `raf_editor::game_runtime` sigue como fachada preparada, no como Play mode activo. Convierte el `InputSnapshot` nativo a la entrada del runtime de scripts y mantiene separada la escena editable de la escena clonada para futuras conexiones.
- `/script.run` usa la misma sesion Rhai para ejecutar `on_start` una vez contra una escena clonada, sin mutar el documento editable.
- `SelectionIdBuffer` define el contrato de seleccion pixel-perfect: ID por pixel, prioridad por capa y desempate por profundidad. `IdBufferSpec::scaled_extent` deja listo el sizing por tier.
- Play mode del producto sigue guardado en `app.rs`; el harness Rhai ya compila y tiene tests, pero falta reactivar el flujo completo con consola, estado visible, nodes, physics y scene locking validados juntos.

## Actualizacion 2026-08-12 - Electronics hybrid workbench

- Corregido el layout de vertices de lineas de ApiGraphicBasic. El padding de
  `GpuLineVertex` no coincidia con los offsets declarados y el shader recibia el
  alpha como depth bias; por eso grid, cables y simbolos desaparecian hasta que
  otra ruta visual los resaltaba.
- Electronics reduce el peso visual: biblioteca plegable, tarjetas y gaps mas
  compactos, botones secundarios neutros, naranja reservado para estado/accion
  y paneles laterales con rangos que protegen el canvas.
- Inspector elimina el titulo duplicado y estabiliza Properties/Sessions como
  tabs RafUI.
- El menu contextual del schematic fue migrado del popup historico a una
  superficie RafUI contextual con cierre por Escape, clic exterior o accion.
- El downbar de Electronics migra a `workspace + analysis`; Game conserva un
  solo downbar a todo el ancho. Las tabs tienen menu RafUI de clic derecho para
  dividir o restaurar paneles.
- Contrato completo documentado en `docs/EDITOR_RAFUI.md`.
- Validacion tecnica: `cargo check -p raf_editor`, build del ejecutable, 68 tests
  de `raf_ui` y pruebas focalizadas de AGB, shader, menu contextual y aislamiento
  de layout Game/Electronics.
- Revision visual automatizada pendiente: el servicio nativo de Computer Use no
  estuvo disponible en dos intentos; no se declara aceptacion visual final.
