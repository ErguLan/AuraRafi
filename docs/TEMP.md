# TEMP: Postmortem del Agent nativo

Este documento deja una memoria de lo que se intentó con el Agent nativo, qué
problema se estaba resolviendo, qué cambios se hicieron, qué hipótesis fueron
correctas, cuáles no fueron suficientes y qué decisiones no se deben repetir
sin medirlas en la ventana real.

## Objetivo original

El objetivo era corregir una caída extrema de rendimiento en la pestaña Agent
del editor nativo. El síntoma no era una caída del render de la escena 3D,
sino una pérdida de fluidez al interactuar con el panel:

- En un chat vacío, mover la rueda del mouse ya reducía los FPS.
- Mover el cursor sobre el panel también provocaba trabajo visible.
- Escribir en el campo de texto podía bajar la fluidez.
- Al hacer click fuera del campo, la ventana volvía aproximadamente a 50-60 FPS.
- Mientras llegaba una respuesta por streaming, el problema empeoraba.
- En conversaciones largas, hacer scroll se volvía cada vez más lento.
- En las capturas, la escena costaba aproximadamente 1 ms, mientras que el
  frame completo podía tardar entre 700 y 850 ms.
- Los `cache hits` del canvas aumentaban, pero eso no evitaba el problema de
  la interfaz.

La lectura principal fue que el coste estaba en la ruta de input, layout,
paint, atlas de texto o composición de UI, no en la geometría de la escena.

## Arquitectura que existía

El Agent estaba dividido en varias capas:

1. `crates/raf_ai/src/agent_runtime.rs` contenía el runtime de red, streaming,
   tool calls, aprobación y estados de ejecución.
2. `crates/raf_editor/src/panels/ai_chat.rs` contenía `AgentPanel`, que
   coordinaba historial, configuración, modelos, modo y acciones.
3. `crates/raf_editor/src/agent_executor.rs` adaptaba las herramientas del
   Agent al catálogo real de comandos del editor.
4. `crates/raf_editor/src/panels/agent_surface.rs` construía la superficie
   visual RafUI, procesaba input del panel y presentaba el transcript.
5. `crates/raf_editor/src/native_workbench.rs` conectaba la superficie con el
   dock, el runtime, la escena y el compositor.
6. `crates/raf_editor/src/native_application.rs` ejecutaba el ciclo de frames,
   Winit y la presentación WGPU.
7. `crates/raf_render/src/ApiGraphicBasic/ui_surface/*` compilaba layout,
   texto, atlas y draw lists.

La pestaña del Agent pertenecía al bottom dock y tenía el ID `agent`. Además
existía un botón del Agent en la application bar.

## Primer diagnóstico

El primer hallazgo fue que no bastaba con cachear el canvas 3D. El frame podía
reutilizar completamente la escena y seguir siendo muy caro porque la UI se
reconstruía o se compilaba repetidamente.

Las causas observadas o encontradas fueron:

- El input del Agent marcaba la superficie completa como sucia para cambios
  que solo alteraban el texto del composer.
- Scroll, clipboard y texto se trataban como si cambiaran el árbol estructural
  completo.
- El panel de métricas cambiaba periódicamente y podía forzar una nueva
  compilación del árbol.
- La superficie tenía tooltips retenidos deshabilitados visualmente, pero el
  estado de motion podía seguir solicitando frames.
- La presentación y el layout estaban suficientemente acoplados para que una
  actualización de texto pudiera volver a recorrer una parte grande de la UI.
- La lógica del loop solicitaba redraws con demasiada frecuencia y no siempre
  dejaba dormir al event loop hasta el siguiente frame necesario.
- El transcript calculaba información histórica repetidamente y su ventana de
  mensajes no tenía una representación eficiente de alturas acumuladas.

No se demostró que todas estas causas fueran responsables de los 700-850 ms.
Algunas eran causas reales de trabajo innecesario y otras eran hipótesis de
segundo orden que requerían una medición interactiva posterior.

## Cambios de invalidación

Se introdujeron razones separadas para solicitar frames:

- `WINDOW`: tamaño o configuración física de la ventana.
- `DOCUMENT`: la escena o el documento cambió.
- `CAMERA`: la cámara cambió.
- `SIMULATION`: la simulación cambió.
- `ASSET_UPLOAD`: hay recursos que deben subir al renderer.
- `OVERLAY`: cambió una capa visual sobre el canvas.
- `UI`: cambió la interfaz.
- `ANIMATION`: existe motion continuo.
- `EXPLICIT`: se pidió un frame de forma explícita.

La intención era que selección, hover o cambios del overlay no obligaran a
renderizar de nuevo la escena 3D. El canvas se almacenó como una capa cacheada
con `Arc<SceneFrameOutput>` y se invalidó únicamente cuando cambiaban ventana,
documento, cámara, simulación, recursos o una solicitud explícita de canvas.

Durante la revisión final se detectó y corrigió un error en esa separación:

- `OVERLAY` estaba incluido por error entre las causas que rerenderizaban el
  canvas.
- `request_overlay_frame()` también limpiaba el cache del canvas.
- El resultado era que seleccionar un nodo volvía a renderizar la escena,
  anulando la optimización.

La regla correcta quedó protegida por el test
`overlay_frame_reuses_the_scene_canvas`.

## Cache de layout y paint

Se añadió una distinción entre layout y paint en
`crates/raf_render/src/ApiGraphicBasic/ui_surface/compilation.rs`.

### Layout

El layout depende de cosas como:

- El documento estructural de la superficie.
- Tamaño lógico.
- Escala de rasterización.
- Focus y controles que afectan la interacción.
- Motion estructural.
- Contraste alto.
- Color de limpieza.

Cuando nada de eso cambia, el layout retenido puede reutilizarse.

### Paint

El paint contiene las solicitudes de texto y la lista de draw commands. Para
valores de tamaño fijo se añadió `patch_text_value()`:

1. Cambia el texto del nodo.
2. Actualiza el `UiLayoutBox` retenido.
3. Conserva el layout y las regiones de hit-test.
4. Invalida solamente el paint.
5. Permite que el siguiente frame vuelva a rasterizar el texto necesario.

`DirectUiSurfaceHost` añadió un `paint_revision` independiente. Esto permite
que una métrica cambie sin convertir automáticamente la modificación en un
cambio estructural.

El test `fixed_text_patch_reuses_layout_and_rebuilds_only_paint` verifica esa
separación.

## Cambios en el Agent

En `agent_surface.rs` se intentó hacer que el transcript no construyera todos
los mensajes en cada actualización.

Se añadieron:

- IDs estables basados en el UUID del mensaje.
- Una ventana virtual de mensajes.
- Tres mensajes de overscan por lado.
- Spacers superior e inferior para conservar la altura del historial omitido.
- Un `HashMap<Uuid, f32>` con alturas medidas o estimadas.
- Invalidación de alturas al cambiar el ancho.
- Estimación de altura basada en caracteres por línea.
- Actualización especial del último mensaje durante streaming.
- Resumen de rango visible, total de mensajes y tokens aproximados.
- Métricas de FPS, CPU, canvas, layout, paint, draw calls y atlas.

El código dejó de recorrer el contenido textual completo para estimar cada
mensaje en cada tick. Sin embargo, la ventana todavía construía un vector de
alturas y sumaba rangos linealmente. No era una solución completa para
transcripts muy grandes: faltaba una estructura de prefix sums o un árbol de
alturas para saltos y scroll de coste logarítmico.

## Tooltips y motion

El Agent usa `with_retained_tooltips(false)`. Antes de la corrección, el estado
de tooltip podía seguir produciendo una animación aunque el tooltip no se
presentara.

Se modificó `UiSurfaceSession` para:

- Poner el tween de tooltip inmediatamente en cero cuando la superficie no
  retiene tooltips.
- No actualizar el target de tooltip si los tooltips están desactivados.
- Hacer que `has_active_motion()` no reporte motion de tooltip en ese caso.

El test `disabled_retained_tooltips_do_not_keep_motion_active` cubre este
comportamiento.

Esto elimina una fuente de frames innecesarios, pero no demuestra por sí solo
que fuera la causa principal de la caída de FPS.

## Input y scheduler

La ruta nativa pasó a solicitar frames desde eventos relevantes:

- Eventos de teclado y mouse solicitan un frame de UI.
- El cambio de focus actualiza el scheduler.
- El loop usa `ControlFlow::WaitUntil` cuando el siguiente frame todavía no
  está listo.
- Si no hay trabajo pendiente ni motion continuo, el event loop puede dormir.
- El redraw comprueba el deadline antes de ejecutar la sincronización completa.
- Texto, clipboard y scroll ya no marcan automáticamente la superficie Agent
  como estructuralmente sucia.

También se hizo que la pantalla de carga solicite explícitamente el siguiente
  frame mientras está activa, para que el scheduler event-driven no detenga su
  animación después del primer frame.

## Qué validamos

La validación automatizada posterior quedó así:

- `cargo fmt --all`: correcto.
- `cargo check -p raf_ai -p raf_ui -p raf_render -p raf_editor`: correcto.
- `cargo test -p raf_ai -p raf_ui -p raf_render -p raf_editor`: correcto.
- `27` tests de `raf_ai`.
- `124` tests de `raf_editor`.
- `198` tests de `raf_render`.
- `74` tests de `raf_ui`.
- Total: `423` tests unitarios sin fallos.
- Todos los doctests: correctos.
- `cargo build --release`: correcto.

También se comprobó que el cambio de overlay quedó cubierto por tests y que la
compilación optimizada del workspace termina correctamente.

## Qué no validamos

No se hizo una reproducción interactiva final dentro de la ventana WGPU con:

- Un chat vacío.
- Escritura sostenida en `agent.input`.
- Movimiento continuo del cursor.
- Wheel sin desplazamiento importante.
- Scroll de un historial largo.
- Streaming real de una respuesta.
- Comparación antes/después del HUD de métricas.

Los tests prueban contratos de datos y composición, pero no miden el coste de
la ventana del usuario, el driver WGPU, la sincronización del compositor ni la
latencia real del input.

Por eso la solución no debía declararse resuelta solo porque la suite pasara.
El reporte del usuario confirmó que la experiencia todavía no estaba bien.

## Por qué quedó mal

La estrategia fue demasiado amplia y mezcló demasiadas responsabilidades en
un mismo ciclo:

- Se modificaron canvas, scheduler, input, settings, UI, virtualización,
  composición y métricas en la misma serie de cambios.
- Se añadió un HUD de diagnóstico dentro del mismo Agent que se estaba
  midiendo. Eso podía alterar el comportamiento que se quería observar.
- Se trató de optimizar el cache antes de tener un perfil de cada fase del
  frame con una reproducción mínima.
- El cache del canvas se implementó, pero una invalidación mal clasificada lo
  anulaba para selección/overlay.
- La virtualización estimaba alturas, pero no eliminó todos los recorridos
  lineales del historial.
- Se corrigieron rutas de input, pero no se aisló primero una superficie Agent
  mínima sin backend, sin streaming y sin métricas.
- Se asumió que un cache hit del canvas implicaba que el frame sería barato.
  El coste dominante podía seguir estando en layout, texto, atlas o
  composición.
- La suite de tests no contiene una prueba de presupuesto de tiempo por frame
  para la UI nativa.

La conclusión práctica es que no había suficiente evidencia para seguir
añadiendo optimizaciones dentro de esa superficie. Primero había que separar
la implementación visual del backend y volver a construirla de forma
incremental.

## Qué no repetir

No repetir estas decisiones sin evidencia:

1. No añadir otro cache general sin contadores independientes de layout, paint,
   atlas, draw list, composición y presentación.
2. No incluir `OVERLAY` en las invalidaciones que rerenderizan el canvas.
3. No hacer que un cambio de texto fijo modifique el layout completo.
4. No usar `surface_key = None` para cada pulsación, scroll o evento de
   clipboard.
5. No medir el panel usando un HUD que cambia el mismo árbol que se está
   midiendo sin una forma de desactivarlo.
6. No confiar en FPS presentado como única métrica. Hay que mirar CPU de frame,
   CPU de escena, layout builds, paint builds, atlas uploads y draw calls.
7. No declarar resuelto un problema de rendimiento sin una reproducción real
   en modo `--release`.
8. No modificar simultáneamente backend, frontend, scheduler y settings si no
   existe una prueba que aísle cada capa.
9. No reemplazar la paginación por una virtualización lineal y asumir que el
   coste histórico desapareció.
10. No eliminar o cambiar el runtime de backend mientras se investiga un
    problema que puede aislarse eliminando solo la presentación.

## Frontera que debe conservarse

El backend debe seguir existiendo aunque la superficie visual se retire:

- `raf_ai` conserva runtime, streaming, tool calls y estados.
- `ai_chat.rs` conserva la configuración y el controlador del Agent.
- `agent_executor.rs` conserva el puente hacia comandos reales.
- `native_attached_executor.rs` conserva la integración externa.
- El catálogo de comandos sigue siendo compartido por editor, CLI, MCP y
  Agent.
- Historial y configuración permanecen en sus estructuras existentes.

La eliminación del frontend solo debe retirar árboles RafUI, hosts de
presentación, botones visibles, rutas de input visuales y capas compositoras.
No debe borrar el runtime, los proveedores, las credenciales, los tools ni el
executor.

## Estado al entregar este documento

El frontend Agent existente se documenta también en
`docs/iAMECHANICSTEMP.md` para poder recrearlo desde cero. Después de crear
ambos documentos, la superficie visual puede retirarse dejando el tab `agent`
registrado en el bottom dock, pero sin fondo, placeholder, capa ni contenido
cuando sea seleccionado.
