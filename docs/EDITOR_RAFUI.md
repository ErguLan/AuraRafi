# Editor RafUI

Estado: activo. Este documento es la autoridad funcional del frontend del
editor construido con RafUI. No es una especificacion de backend ni sustituye
`docs/RENDERER.md`, que conserva el contrato del renderer.

## Autoridad y precedencia

Para cualquier cambio de interfaz, consultar en este orden:

1. `Agent.md` para las reglas del repositorio.
2. `.ai/SYSTEM_TRUTH.md` para los limites arquitectonicos actuales.
3. `docs/RAF_UI.md` para el contrato tecnico de RafUI.
4. Este documento para el shell, workbenches y paneles del editor.
5. `.ulpi/design/DESIGN.md` para tokens, tipografia, color y lenguaje visual.
6. `docs/APIGRAPHICBASIC.md` y `docs/RENDERER.md` para la ruta grafica.

Los documentos dentro de `docs/archive/`, `.ai/archive/` y
`.ulpi/design/archive/` son contexto historico o propuestas. No son
autoridad de implementacion.

## Alcance del editor

El editor tiene dos contextos de producto:

- Game: escena, viewport 2D/3D, Hierarchy, Inspector, Assets, Project,
  Console, Nodes y Agent.
- Electronics: schematic, PCB, navigator, inspector, DRC y Simulation.

Game y Electronics comparten shell, RafUI, toolbar general, dock y Settings,
pero no deben mezclar paneles especificos de su dominio. DRC y Simulation
pertenecen a Electronics. Si un panel aun no tiene implementacion completa,
debe mostrar `Work in progress` de forma intencional y no inventar datos.

## Limites de ownership

### Surface

Las superficies declarativas construyen el documento RafUI: texto, layout,
controles, estados visuales y acciones semanticas. No deben ejecutar mutaciones
de escena, escribir persistencia por frame ni dibujar directamente con una
API grafica.

### Host

Los hosts conservan estado temporal y coordinan foco, docking, resize,
animaciones, scroll, menus contextuales, drag and drop, invalidacion y
persistencia de layout. Un host puede solicitar una nueva revision de surface,
pero no debe reconstruir todo el editor por cada movimiento del puntero.

### Dominio y comandos

La escena, documentos de proyecto, historial, undo/redo, runtime y acciones de
editor son propiedad de sus modulos de dominio o del Command Gateway. La UI
emite comandos tipados y refleja el estado confirmado. No crea un segundo
modelo paralelo del mundo.

### Graficos

RafUI entrega sus documentos al pipeline de ApiGraphicBasic. WGPU puede ser el
backend privado actual cuando AGB lo requiera. El toolkit retirado no es una
ruta de implementacion nueva y ningun puente heredado debe recibir funciones.

## Shell y layout

La composicion base del workbench es:

- barra global superior;
- panel lateral izquierdo para navegacion y Hierarchy;
- viewport central;
- Inspector a la derecha;
- downbar inferior con paneles del contexto activo;
- status bar inferior.

Los paneles son docks reales: pueden abrirse, cerrarse, redimensionarse,
reordenarse y restaurar su layout persistido. El layout debe responder al
tamano de ventana y a la escala DPI sin cortar textos ni convertir cada tab en
un icono sin etiqueta cuando hay espacio suficiente.

Reglas de responsive:

- ancho normal: etiqueta y controles principales visibles;
- ancho compacto: reducir padding y ocultar solo acciones secundarias;
- ancho minimo: mantener icono, etiqueta esencial y tooltip;
- nunca ocultar nombres importantes si existe espacio razonable para ellos;
- usar overflow o scroll horizontal antes de truncar una etiqueta critica;
- calcular tamano por contenido intrinseco y areas seguras de texto.

## Game workbench

### Hierarchy

Hierarchy muestra la estructura real que expone el modelo de escena. Debe
conservar World, padres, hijos, chevrons, indentacion, seleccion, visibilidad,
bloqueo y menu contextual cuando esos datos existan en el modelo actual.

La fila debe separar semanticamente:

- expandir o contraer;
- icono de tipo;
- nombre y seleccion;
- visibilidad;
- bloqueo;
- acciones adicionales.

La busqueda y los filtros deben conservar la relacion padre-hijo necesaria para
entender por que un resultado aparece. La seleccion de Hierarchy debe
sincronizarse con viewport e Inspector usando el estado de escena existente.

### Viewport

El viewport tiene dos toolbars independientes:

- toolbar horizontal de contexto: perspectiva, modo de visualizacion, grid,
  gizmos, camara y opciones Show;
- toolbar vertical de herramientas: seleccionar, mover, rotar, escalar,
  encuadrar y las acciones que ya exponga el editor.

Cada control debe tener etiqueta o tooltip, estado normal, hover, activo,
deshabilitado y foco. El naranja comunica seleccion, foco o accion primaria;
no se usa para pintar todos los botones a la vez. Los overlays flotantes no
deben modificar el layout del panel central ni bloquear innecesariamente la
interaccion con la escena.

Los labels del viewport deben seguir la politica existente del renderer:
mostrar prioridad a seleccion y hover, evitar acumulacion y ofrecer un modo
explicito para mostrar todos cuando exista soporte.

### Inspector

Inspector es un panel de propiedades del objeto seleccionado, no un panel de
creacion de primitivas. La UI solo debe mostrar campos que el backend actual
pueda leer o modificar.

La organizacion visual preferida es por secciones colapsables:

- Identity;
- Transform;
- Appearance;
- Components;
- Metadata;
- Debug.

Transform debe priorizar campos numericos editables para precision y puede
ofrecer sliders como control complementario. Position, Rotation y Scale deben
mantener alineacion X/Y/Z, unidades consistentes y reset contextual.

Valores como color deben usar un control de color real o un selector de color
entendible, no tres sliders RGB como unica interfaz. Dropdowns son preferibles
cuando el valor pertenece a un conjunto cerrado, por ejemplo tipo de forma,
modo, layer o fisica, siempre que esa opcion exista realmente en el modelo.

Los botones de creacion de Empty, Cube, Sphere, Plane o Cylinder no deben
parecer propiedades del objeto seleccionado. Deben vivir en la accion de
creacion, el menu contextual o el boton `+`, segun lo que ya soporte el
backend.

El estado vacio del Inspector debe explicar que hacer sin simular propiedades.
Los toggles Visible y Lock deben comunicar claramente su estado actual.

### Assets, Project, Console, Nodes y Agent

Estos paneles forman parte del workbench Game y conservan sus propios
contenidos. Deben compartir cabeceras, espaciado, iconografia y estados vacios,
pero no forzar una implementacion de dominio que aun no exista.

Agent se integra como panel RafUI del downbar. Su historial, streaming,
herramientas y aprobaciones siguen siendo responsabilidad de `raf_ai` y sus
hosts; el shell solo presenta estado y acciones.

## Electronics workbench

Electronics usa el mismo shell visual, pero su navegacion y downbar deben
priorizar schematic, PCB, navigator, inspector, DRC y Simulation. DRC y
Simulation no deben aparecer como paneles activos del workbench Game.

Las tareas pesadas de DRC y Simulation deben conservar su ejecucion fuera del
hilo de UI, con estado explicito de progreso, resultado, error y cancelacion.
La UI no debe bloquear el viewport mientras espera resultados.

## Settings

Settings es compartido cuando una opcion afecta al editor completo: tema,
escala, reduced motion, atajos globales, idioma, providers y preferencias de
renderizado que ya existan.

Las opciones exclusivas de Hierarchy, Inspector, Game o Electronics deben
permanecer en su contexto o en una subseccion claramente etiquetada. No se
duplican opciones antiguas: se adapta la opcion existente si su comportamiento
tambien aplica al panel nuevo y se documenta la migracion.

Toda opcion debe tener estado draft, aplicar/cancelar y persistencia coherente
con el host actual. No se agrega una opcion solo para ocultar un bug de layout
que debe corregirse en la superficie o el host.

## Interaccion y foco

- Un campo de texto captura teclado mientras esta activo.
- Ctrl+A, doble clic para palabra y seleccion por arrastre pertenecen al
  control de texto RafUI.
- Mientras un campo captura teclado, WASD y atajos del viewport no deben mover
  la camara ni ejecutar acciones de otra superficie.
- Menus contextuales, drag and drop y overlays deben tener captura y
  liberacion de puntero deterministas.
- Escape cancela renombrado, menu o drag cuando el control es propietario del
  gesto.
- Las animaciones de apertura, cierre, reveal y drag viven en el host y no
  deben invalidar todo el editor cuando solo cambia un overlay.

## Rendimiento e invalidacion

El editor debe permanecer utilizable en hardware de bajo consumo.

- No clonar el SceneGraph ni el documento completo en cada frame.
- Cachear superficies y datos derivados por revision.
- Virtualizar listas grandes y construir solo filas visibles mas overscan.
- Invalidar por cambios reales: datos, foco, hover, scroll, animacion o
  ventana; no por ruido de movimiento global que no afecte a la superficie.
- No escribir persistencia por frame.
- Mantener overlays y menus fuera del arbol pesado cuando sea posible.
- Medir tiempo de compilacion, cache hit, draw calls, atlas, target y repaint
  antes de atribuir una caida de FPS a un panel.

## Iconos y lenguaje visual

Los iconos de editor son recursos semanticos compartidos, preferentemente
vectoriales o rasterizados desde una fuente de alta resolucion y empaquetados
en atlas. No se generan manualmente por fila ni se cargan PNG independientes
para cada entidad.

Iconos normales usan blanco o gris lineal. Naranja queda reservado para
seleccion, foco, accion primaria y estado activo. Todos los paneles deben
compartir peso, tamano optico, padding y contraste.

## Estados y documentos auxiliares

Usar estos estados al crear documentacion o funciones:

- `current`: contrato activo;
- `transitional`: puente o migracion vigente;
- `work_in_progress`: visible pero incompleto;
- `planned`: aprobado, aun no implementado;
- `historical`: contexto que no debe guiar implementacion;
- `deprecated`: retirado o reemplazado.

Los documentos auxiliares de una funcion concreta deben enlazar aqui y no
redefinir la arquitectura. Las propuestas visuales pueden permanecer en
`.ulpi/design/archive/` para referencia, pero no deben estar en la cadena de
lectura automatica de una IA.

## Puerta de validacion

Antes de declarar una superficie terminada:

1. Confirmar ownership y que no se introdujo una segunda UI toolkit.
2. Revisar overflow, texto, foco, teclado, scroll, estados y escala DPI.
3. Verificar Game y Electronics en layouts separados.
4. Ejecutar `cargo fmt --check` y los checks focalizados disponibles despues
   de terminar el pase de codigo.
5. Abrir el ejecutable y revisar visualmente las interacciones principales.
6. Registrar cualquier fallo preexistente o cambio concurrente por separado.

> Developed by Yoll. More info: [yoll.site](https://yoll.site).
