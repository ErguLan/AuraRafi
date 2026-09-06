# TEMP — opciones de sombreado retiradas del viewport

Este archivo es documentación temporal para conservar el contexto de las dos
opciones retiradas en agosto de 2026. Se puede eliminar cuando ya no haga falta
recuperar o rediseñar estos modos.

## Estado actual

El viewport del editor conserva únicamente `Lit`. `Wireframe shading` y
`Preview shading` ya no se pueden seleccionar desde el toolbar ni desde
Settings.

Los valores `Wireframe` y `Preview` del enum persistido
`raf_core::config::ViewportRenderMode` se mantienen sólo para poder leer un
archivo RON antiguo. Al cargarlo, ambos se normalizan a `Solid` (`Lit`) para no
resetear el resto de la configuración ni volver a activar un modo retirado.

## Wireframe shading

Qué mostraba:

- Omitía el relleno de los triángulos de las mallas.
- Dibujaba las aristas de los objetos, por lo que la escena se veía como una
  estructura de líneas.
- Era útil para inspeccionar topología, geometría y visibilidad de caras.

Cómo estaba integrado:

1. `viewport_toolbar_surface.rs` agregaba la opción `wireframe` al
   `UiSelect` de `viewport.render_style` y mostraba el texto localizado
   `app.viewport_wireframe`.
2. `parse_viewport_toolbar_select_action` y
   `parse_viewport_toolbar_action` convertían la selección o el comando en
   `ViewportToolbarAction::Wireframe`.
3. `native_workbench.rs` guardaba el estado como
   `ViewportRenderStyle::Wireframe` y persistía
   `ViewportRenderMode::Wireframe`.
4. `viewport_controller.rs` lo convertía a `RenderMode::Wireframe`.
5. `scene_renderer.rs` omitía el draw de triángulos y forzaba el overlay de
   aristas tanto en la ruta de rasterizado como en la lista de comandos de
   ApiGraphicBasic.

## Preview shading

Qué mostraba:

- Conservaba el relleno sólido de la escena.
- Forzaba las aristas de superficie en todos los objetos, incluso cuando la
  preferencia de aristas sólidas estaba apagada.
- Visualmente era una mezcla de sólido más wireframe, no un preview separado
  de materiales o iluminación.

Cómo estaba integrado:

1. El mismo `UiSelect` de `viewport_toolbar_surface.rs` emitía el valor
   `preview` y usaba `app.viewport_preview` como etiqueta.
2. Los parsers del toolbar lo convertían en
   `ViewportToolbarAction::Preview`.
3. `native_workbench.rs` y `viewport_controller.rs` transportaban el valor
   mediante `ViewportRenderStyle::Preview`,
   `NativeViewportRenderStyle::Preview` y `RenderMode::Preview`.
4. `scene_renderer.rs` seguía rasterizando los triángulos, pero trataba el
   modo como una variante con aristas forzadas en las rutas CPU y
   ApiGraphicBasic.

## Settings y Project Settings

- `settings_surface.rs` sí tenía un selector de `viewport_render_mode` con
  `solid`, `wireframe` y `preview`; ahora sólo declara `solid` y lo presenta
  como `Lit`.
- `native_workbench_settings.rs` ya no acepta valores ni claves de selección
  para Wireframe/Preview.
- `project_settings_surface.rs` no tenía un selector de sombreado del
  viewport. Su selector de `Render preset` (`Potato`, `Low`, `Medium`, `High`)
  controla calidad/rendimiento del proyecto y no era una de estas dos
  opciones, por lo que permanece intacto.

## Archivos que componían el flujo

- `crates/raf_editor/src/panels/viewport_toolbar_surface.rs`: dropdown y
  acciones del toolbar.
- `crates/raf_editor/src/settings_surface.rs`: selector global de viewport.
- `crates/raf_editor/src/native_workbench_settings.rs`: adaptación de eventos
  de Settings al modelo.
- `crates/raf_editor/src/native_workbench.rs` y
  `crates/raf_editor/src/native_editor_commands.rs`: aplicación de acciones.
- `crates/raf_editor/src/panels/viewport_controller.rs`: puente entre editor
  y renderer.
- `crates/raf_render/src/scene_renderer.rs`: relleno y aristas de la escena.
- `crates/raf_core/locales/en.json` y `es.json`: etiquetas localizadas.

