# Electronics native architecture

Este es el mapa activo del editor Electronics después de la migración. La
fuente de verdad es:

`RafUI nativo -> CommandGateway -> NativeElectronicsEditor -> raf_electronics`

El render sigue esta frontera:

`NativeElectronicsEditor.scene -> ApiGraphicBasic CadSurface -> RenderRuntime`

AGB es la autoridad de composición y dibujo. WGPU sólo ejecuta el backend
privado del renderer. RafUI construye superficies retenidas, hit testing y
comandos semánticos; no muta documentos.

## Módulos activos

- `crates/raf_editor/src/electronics_controller.rs`: documento vivo, cámara,
  selección, historial, escena y estado de análisis.
- `crates/raf_editor/src/electronics_controller_interaction.rs`: input CAD,
  gestos, placement, routing, edición, undo/redo y persistencia sobre el
  controlador vivo.
- `crates/raf_editor/src/native_electronics.rs`: adaptación de la escena CAD al
  host directo de ApiGraphicBasic.
- `crates/raf_editor/src/native_workbench.rs`: estado del shell, ciclo de
  sincronización e input general.
- `crates/raf_editor/src/native_workbench_surface.rs`: composición retained del
  shell, toolbar, navegador, inspector y dock; no contiene estado de documento.
- `crates/raf_editor/src/panels/electronics_canvas_overlay_surface.rs`: capa
  RafUI local al viewport CAD para artwork PNG nativo, designadores y ayuda;
  se presenta con clipping en el rectangulo fisico del canvas.
- `crates/raf_editor/src/native_workbench_input.rs`: dispatch de input retenido
  e intenciones semánticas del shell.
- `crates/raf_editor/src/native_workbench_electronics.rs`: proyecciones de
  navigator, inspector y líneas de análisis Electronics.
- `crates/raf_editor/src/electronics_analysis.rs`: tareas de DRC y Simulation
  fuera del hilo de UI, con resultado, cancelación y estado observable.
- `crates/raf_editor/src/electronics_minimap.rs`: resumen rasterizado del viewport;
  no redibuja símbolos y no recibe mutaciones de documento.
- `crates/raf_editor/src/panels/electronics_*_surface.rs`: superficies RafUI
  enfocadas; sólo emiten intenciones semánticas.
- `crates/raf_editor/src/native_attached_executor.rs`: adaptadores de
  transporte. UI, Agent y CLI/MCP adjunto llegan al mismo documento; no se
  mantienen copias paralelas.
- `crates/raf_editor/src/commands/electronics.rs`: catálogo de operaciones
  Electronics/PCB, sin dependencias de renderer o RafUI.
- `crates/raf_electronics/src/{schematic,cad_scene,pcb,drc,simulation}.rs`:
  modelo eléctrico, escena de presentación, layout físico y análisis.

## Frontera de composicion del viewport

Electronics tiene dos rectangulos nativos relacionados pero no mezclados:

1. `EditorFrameLayout::canvas`: espacio reservado para la toolbar contextual de
   RafUI.
2. `EditorFrameLayout::electronics_canvas()`: viewport efectivo debajo de esa
   toolbar. ApiGraphicBasic renderiza aqui el grid, wires, pines y geometria;
   el overlay RafUI de assets usa el mismo rectangulo y coordenadas locales.

El overlay nunca se presenta como una capa de ventana completa. Su `target_rect`
fisico coincide con el viewport CAD, su raiz usa `UiOverflow::Clip` y sus
indices solo ordenan componentes, labels y ayuda dentro del viewport. La
toolbar, navigator, inspector, dock, Settings y cualquier modal siguen siendo
chrome RafUI fuera de esa superficie. Games conserva su `layout.canvas` y no
usa esta politica de Electronics.

La navegacion CAD usa click izquierdo para seleccionar o editar, boton central
para pan, Space + arrastre izquierdo para pan temporal, rueda para zoom y
click derecho/Escape para cancelar herramientas temporales. Un click derecho
no puede dejar geometria provisional en la escena.

Los controles RafUI de Electronics exponen tooltips localizados mediante
hover intent y etiquetas de accesibilidad. Los tooltips describen la accion y,
cuando aplica, su atajo. El grid visual se adapta al zoom sin cambiar el snap
del documento; su visibilidad y opacidad son configurables por Electronics.

## Reglas de mantenimiento

1. Una mutación de documento pasa por `NativeElectronicsEditor` para registrar
   historial, invalidar análisis, reconstruir la escena y marcar dirty.
2. Una superficie RafUI emite un comando; nunca edita `Schematic` o `PcbLayout`
   directamente.
3. CLI, MCP, Agent y UI usan el mismo catálogo y gateway. Las intenciones
   transitorias de herramienta también pasan por el gateway UI nativo.
4. El DRC conserva el reporte estructurado y sus ubicaciones para dibujar
   marcadores en el canvas; el dock sólo muestra la lectura humana.
   Editar el documento invalida tanto los marcadores como las líneas visibles
   del dock; no se presenta un resultado anterior como si fuera actual.
5. Los módulos eliminados `SchematicGraph`, `SchematicViewPanel`, `PcbViewPanel`
   y `ElectronicsCadSurfaceHost` no forman parte de la arquitectura actual.
   Las menciones que permanezcan en `docs/archive/` son sólo historia.

## Estado actualizado de la migración

La migración de superficie está cerrada en código: el canvas y el chrome son
AGB/RafUI nativos, sin puente con Egui. DRC y simulación se ejecutan como
tareas de análisis fuera del hilo de UI; el dock expone running, completed,
cancelled y failed, y la UI continúa realizando polling mientras el trabajo
está activo. La validación obligatoria pendiente es visual dentro del editor:
geometría de símbolos y wires, cards de biblioteca, selección/inspector,
marcadores DRC, minimapa y authoring de board outline.

## Contrato UX final

Electronics conserva su leftbar vertical largo y sus paneles especializados;
Games conserva su tamaño y jerarquía propios. La interfaz RafUI está fuera del
rectángulo de render CAD y el grid no invade toolbar, inspector o docks. Los
assets de componentes provienen del catálogo nativo SVG/PNG; el renderer solo
compone artwork, pines, etiquetas, wires y estados de interacción.

El minimapa es una superficie de resumen del viewport: representa el contenido
simplificado, muestra el rectángulo visible y se actualiza con la cámara y la
escena. La navegación primaria sigue perteneciendo al canvas para conservar un
único dueño de input y evitar que el overview se convierta en otro editor.
