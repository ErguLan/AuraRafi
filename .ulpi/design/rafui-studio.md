---
project: ProyectRaf
register: product
aesthetic_direction: technical / utilitarian
design_system: RafUI retained primitives
feature: RafUI Studio
---

# RafUI Studio

## Design Read

RafUI Studio debe sentirse como un banco de calibración del editor: preciso,
silencioso y verificable. Su apuesta es quitar decisiones repetidas al autor
de una surface sin esconder la geometría, el texto, los iconos ni las acciones
que realmente se van a ejecutar.

El lenguaje queda ligado a [DESIGN.md](DESIGN.md): fondo oscuro contenido,
tipografía Ubuntu vectorial, densidad alta, naranja sólo para agencia activa y
bordes de un píxel. No hay gradientes, brillo, glassmorphism ni paneles
decorativos.

## Objetivo de producto

Diseñar una interfaz RafUI no debe requerir volver a investigar el renderer,
adivinar el tamaño de un tooltip, corregir cada DPI manualmente o pintar una
segunda versión desde Egui. El autor debe poder:

1. elegir una receta existente;
2. inspeccionar el árbol, layout, estilo, tipografía y acciones;
3. cambiar sólo propiedades permitidas mediante una edición serializable;
4. validar que la surface usa acciones y claves reales;
5. ejecutar la misma revisión en GPU, CPU, dark/light y 100/125/150/200%;
6. comparar un snapshot estructural o RGBA antes de entregar.

## Separación de render

Un autor continúa escribiendo un `UiNode`, pero RafUI Studio conserva tres
políticas internas separadas:

| Capa | Responsabilidad | Regla de calidad |
| --- | --- | --- |
| Geometría | rectángulos, bordes, radios, clipping y hitboxes | se calcula en puntos lógicos y se asigna al target físico; no hereda el supersampling de texto |
| Texto | roles, pesos, line-height, localización y atlas vectorial | usa la densidad de texto del environment; no cambia el tamaño del layout |
| Iconos | imágenes pequeñas, atlas/PNG, tint y sampling | conserva su densidad física; no se estira un asset de baja resolución |

La API pública de esta separación es `UiDensityContract`. El renderer puede
usar `UiGeometrySnap`, `UiSamplingMode`, `geometry_scale`, `text_scale` e
`icon_scale` sin que una decisión de texto deforme una frontera o un icono.

## Los siete bloques

### 1. Recetas reutilizables

`UiStudioRecipeCatalog` mantiene el vocabulario oficial:

- `technical-toolbar`
- `icon-button`
- `segmented-control`
- `floating-action-rail`
- `tree-row`
- `inspector-field`
- `editor-tab`
- `tooltip`
- `panel-header`
- `empty-state`

Las funciones de `raf_ui::components` producen nodos ordinarios. No existe un
widget paralelo ni una capa de estilos privada por panel.

### 2. Inspector estructurado

`RafUiStudio::inspect` devuelve `UiStudioNodeInspection` con path estable,
clases, claves, `UiLayout`, `UiStyle`, tipografía, interacción, eventos y
cantidad de hijos. Un inspector visual futuro puede consumir esta estructura
sin volver a recorrer el documento ni acceder a estado del dominio.

### 3. Edición controlada

`UiStudioEdit` permite cambiar texto, tooltip, label accesible, clases, layout,
parches de estilo y tipografía. Cada edición identifica un `node_id`, se puede
serializar y se aplica a `UiDocument::find_node_mut`. No permite ejecutar una
acción de proyecto ni mover la lógica de negocio al árbol visual.

### 4. Matriz DPI

`UiStudioDpiMatrix` genera dark/light en 100%, 125%, 150% y 200%. Cada caso
expone tamaño físico y contrato de densidad. La matriz es una entrada estable
para pruebas GPU/CPU y revisión visual.

### 5. Diagnósticos

`UiStudioDocumentReport` y los diagnósticos de `ApiGraphicBasic` son datos, no
una UI obligatoria. Detectan IDs duplicados, nodos interactivos sin label o
acción, icon buttons sin tooltip, claves vacías, tamaños inválidos, clipping,
boxes de tamaño cero, requests de texto y z-order inesperado.

### 6. Golden snapshots

`UiStudioGoldenSnapshot` valida buffers RGBA con tamaño exacto y reporta píxeles
diferentes, delta máximo y delta acumulado. `UiStudioSnapshotCase` guarda la
huella estructural de un documento junto con tamaño, escala y tema. El host
puede conservar los golden fuera del binario y ejecutarlos en CI.

### 7. Validador de verdad

`UiStudioCommandRegistry` recibe los comandos reales de la aplicación.
`UiStudioValidationOptions` comprueba que los bindings no inventen acciones,
que iconos tengan tooltip, que interactivos tengan accesibilidad y que las
claves de localización existan cuando se entrega un catálogo. RafUI Studio
reporta el problema; la aplicación sigue siendo la única que ejecuta comandos.

## Flujo de autoría

```text
view model existente
  -> UiDocument / UiNode
  -> receta RafUI Studio
  -> UiStudioEdit (si hace falta)
  -> diagnóstico + validación de acciones
  -> matriz DPI / GPU / CPU
  -> snapshot estructural o RGBA
  -> surface host y bridge de presentación
```

El bridge sólo presenta el frame terminado. No crea tooltips con widgets
externos, no resuelve acciones y no sustituye al viewport, CAD o escena.

## Pre-Flight

- [x] Identidad visual enlazada a `DESIGN.md`.
- [x] Recetas con clases semánticas y obligaciones explícitas.
- [x] Estados hover, focus, active y reduced-motion siguen siendo del host.
- [x] Ediciones limitadas al documento retenido.
- [x] Accesibilidad y tooltips verificables.
- [x] Dark/light y 100/125/150/200% representados por una matriz estable.
- [x] GPU y CPU consumen el mismo contrato de densidad y draw list.
- [x] Snapshot estructural y comparación RGBA disponibles.
- [x] Las acciones se validan contra un registro real.
- [x] No se agregan features de escena/CAD al documento visual.

## Criterio de éxito

Una nueva surface de editor debe necesitar principalmente un `*_surface.rs`,
un view model existente y sus claves de idioma. Si el autor tiene que tocar el
muestreo del renderer, duplicar un tooltip en Egui, inventar un command ID o
corregir tamaños a mano para cada DPI, RafUI Studio todavía no cumplió su
objetivo.
