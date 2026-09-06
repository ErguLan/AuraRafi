# Skills — Flujos de trabajo autorizados para IA

Este archivo es el índice operativo principal de `.ai`. No inventa una
arquitectura paralela: cada flujo debe respetar `.ai/SYSTEM_TRUTH.md`,
`.ai/instructions.md` y las fronteras del código activo.

Antes de implementar, inspecciona el código real y confirma que la ruta sigue
vigente. Las rutas retiradas o históricas no deben reactivarse por copiar una
guía vieja.

## skill: frontend-design-ui-ux

**Cuando usar**: diseñar, revisar o implementar una interfaz a partir de un
brief, screenshot o problema de UX.

La guía visual activa es `.ai/STUDIO_GRADE_UI.md`. La skill externa
`frontend-design-ui-ux` define el método de trabajo, pero no reemplaza las
fronteras nativas de RafUI ni obliga a usar React, CSS o una delegación externa.
Un brief visual explícito puede cambiar los defaults de Studio Grade UI; nunca
puede saltarse ownership, i18n, accesibilidad o rendimiento.

Para RafUI, conservar la división `*_surface.rs` / host / dominio y validar la
ventana real además de los checks de código.

## skill: agregar-feature-electronica

**Cuando usar**: agregar un componente, una regla DRC/ERC, una exportación o
una extensión del schematic/PCB.

**Archivos clave**:

- `crates/raf_electronics/src/component.rs` — componentes y `SimModel`.
- `crates/raf_electronics/src/library.rs` — built-ins en
  `ComponentLibrary::default_library()` y assets externos.
- `crates/raf_electronics/src/schematic.rs` — documento esquemático.
- `crates/raf_electronics/src/pcb/layout.rs` — documento PCB 2D.
- `crates/raf_electronics/src/drc.rs` y `extensions.rs` — reglas base y hooks
  de extensión.
- `crates/raf_editor/src/electronics_controller.rs` y
  `electronics_controller_interaction.rs` — estado, interacción, historial y
  persistencia del editor.
- `crates/raf_editor/src/native_electronics.rs`,
  `native_workbench_electronics.rs` y `panels/electronics_*_surface.rs` — CAD
  nativo y superficies RafUI.

**Flujo**:

1. Leer el modelo de datos y el contrato de interacción antes de tocar la UI.
2. Implementar la lógica en `raf_electronics`; no ocultarla dentro de un
   builder de superficie.
3. Elegir conscientemente entre built-in, asset `.ron` o hook de extensión.
4. Conectar la mutación al historial/CommandGateway y a la persistencia
   correspondiente.
5. Agregar la presentación CAD sólo cuando el modelo y sus hit-tests estén
   definidos.
6. Ejecutar la validación relevante al final y leer todos los errores antes de
   hacer correcciones adicionales.

## skill: agregar-nodo-visual

**Cuando usar**: agregar un tipo de nodo o cambiar su ejecución.

**Archivos clave**:

- `crates/raf_nodes/src/node.rs` — tipos, categorías y pins.
- `crates/raf_nodes/src/executor.rs` — ejecución.
- `crates/raf_editor/src/panels/nodes_surface.rs` — paleta y superficie
  visual nativa.
- `crates/raf_editor/src/native_workbench.rs` y los hosts asociados —
  integración del editor.

**Flujo**:

1. Definir el dato y los pins en `raf_nodes`.
2. Registrar la creación en la superficie de nodos actual.
3. Implementar el match de ejecución y la serialización que corresponda.
4. Verificar que el grafo se conserve en `nodes.ron` y que las mutaciones
   respeten la frontera de comandos/historial.

## skill: mejorar-viewport

**Cuando usar**: cambiar la interacción, proyección o presentación del
viewport de escena o CAD.

**Archivos clave**: `crates/raf_editor/src/panels/viewport_controller.rs`,
`crates/raf_editor/src/native_workbench.rs`,
`crates/raf_editor/src/native_electronics.rs`,
`crates/raf_render/src/bridge/` y `.ai/APIGRAPHICBASIC.md`.

**Reglas**:

- Mantener ApiGraphicBasic como dueño público de gráficos.
- GPU/WGPU privado es el camino normal cuando está disponible; CPU es
  recuperación, headless, pruebas o incompatibilidad.
- Proyección y contratos compartidos deben servir al viewport de juego y al
  CAD. No crear un segundo renderer.
- RafUI posee chrome, superficies retenidas e interacción semántica; el
  compositor resuelve la presentación.
- Validar render invalidation, hit-test, escalado y recuperación CPU según el
  cambio; una compilación exitosa no prueba una ventana visual.

## skill: traducir-ui

**Cuando usar**: agregar o cambiar texto visible.

**Reglas**:

- No usar localización inline con `if is_es` ni duplicar strings en Rust.
- Crear una clave semántica y agregarla en ambos archivos:
  `crates/raf_core/locales/en.json` y `crates/raf_core/locales/es.json`.
- Resolver el texto con `t("namespace.key", lang)` desde la superficie o
  modelo correspondiente.
- Revisar longitud, fallback, foco y contraste en ambos idiomas.

## skill: debug-compilacion

**Cuando usar**: hay errores de compilación o una validación falla.

**Checklist**:

1. Ejecutar el check apropiado al final del cambio completo.
2. Leer todos los errores y separar la primera causa de sus consecuencias.
3. Verificar imports y rutas contra `rg --files`, no contra una guía antigua.
4. No corregir errores no relacionados ni usar reparaciones automáticas sobre
   trabajo ajeno.
5. Reportar claramente qué se verificó y qué quedó sin probar.

## skill: agregar-panel-nativo

**Cuando usar**: agregar un panel, dock, menú, overlay o inspector del editor.

**Flujo**:

1. Leer `docs/RAF_UI.md`, `docs/EDITOR_RAFUI.md`,
   `.ai/APIGRAPHICBASIC.md` y `.ai/STUDIO_GRADE_UI.md`.
2. Crear una superficie `*_surface.rs` enfocada bajo el módulo dueño.
3. Crear un host sólo si hacen falta estado temporal, input, texturas,
   persistencia o traducción de acciones.
4. Emitir `UiAction` tipadas y resolverlas en el controlador/gateway, nunca
   mutar el modelo desde el builder.
5. Verificar layout, foco, teclado, GPU y recuperación CPU.
