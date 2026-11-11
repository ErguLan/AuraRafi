# Electronics: biblioteca física y flujo PCB

Implementación del 22–24 de octubre de 2026. Interfaz nativa RafUI; catálogo eléctrico conservado.

## Uso

1. Seleccionar una pieza y abrir **Huella PCB → Cambiar** en el inspector. El catálogo aparece a la izquierda y sus detalles a la derecha; el lienzo sigue visible e interactivo.
2. Buscar entre las huellas del proyecto. Importar un archivo `.kicad_mod` por su ruta o pegar una URL directa del repositorio oficial de KiCad y pulsar **Importar**. La descarga es asíncrona, limitada a 4 MiB y conserva el origen y la licencia. No hay un buscador remoto de partes ni integraciones comerciales fingidas.
3. Revisar dimensiones, geometría y correspondencia de pines. Cada botón de pad recorre los números disponibles; por ejemplo, A/K pueden asignarse explícitamente a 1/2. No se deduce la correspondencia por cantidad de terminales.
4. Asignar. La definición física se incorpora al PCB y funciona sin acceso a la biblioteca original. La asignación y el mapeo se deshacen juntos. Las pistas que tocaban la huella anterior se estacionan para revisión; no se consideran conexiones válidas después de cambiar sus pads. El aviso del PCB muestra su cantidad y permite **Descartar** las pistas retiradas antes de rerutear; Deshacer las recupera. No se borran automáticamente al cambiar la huella.
5. En PCB, dos filas permanentes muestran **F.Cu / Superior**, **B.Cu / Inferior**, **Capas**, ancho, snap, revisión y fabricación. Estas filas quedan fuera del área de dibujo y de sus coordenadas de entrada. **Ruta** comienza en un pad, admite puntos intermedios a 45° y termina en otro pad de la misma red. Pulsar una conexión aérea inicia un borrador desde su extremo más cercano; no crea cobre automáticamente. El menú contextual conserva el ruteo rápido explícito. Esc cancela el borrador y Ctrl+Z retrocede; una ruta terminada ocupa una entrada del historial.
   La guía del canvas muestra referencia, pad y red. Los aros verdes identifican destinos de esa red; un pad de otra cara o red explica por qué no se puede conectar. Los conectores PTH admiten inicio desde ambas caras, incluido el centro del taladro. El DRC conserva sus marcadores y su informe completo, con los textos largos fuera de los pads.
6. **Exportar fabricación** comprueba la representación física y el DRC, y crea un trabajo nuevo en `ElectricalAssets/Manufacturing/board-<id>`. Cada trabajo conserva los anteriores. `COMPLETE` solo se escribe después de terminar todos sus archivos.

## Espacio de trabajo CAD

- La biblioteca del esquema parte de 260 puntos y se puede plegar a una barra de 52. PCB comienza con esa barra compacta; Biblioteca, Proyecto y Capas siguen accesibles desde ella.
- El inspector aparece con la selección, admite fijarse y muestra la geometría física de la huella, sus dimensiones y la correspondencia pin–pad. Las vistas previas no resuelven ni certifican huellas heredadas por ilustración.
- Los proyectos nuevos comienzan con el dock inferior cerrado; pulsar una pestaña lo despliega. Se conserva el layout de dock ya guardado en proyectos existentes.
- Los anchos elegidos sobreviven a plegar paneles. Al abrir el inspector o el dock, la cámara mantiene la geometría en las mismas coordenadas de ventana. En ventanas estrechas se prioriza un panel lateral a la vez; abrir la biblioteca permite recuperarla sin botones recortados.
- Los símbolos esquemáticos son trazos del renderizador CAD. Una red muestra una sola etiqueta; los rectángulos de lectura ya no se pintan como bloques sobre cada segmento.
- El minimapa mide 184 puntos de ancho, usa los mismos límites para imagen e interacción y desaparece en documentos vacíos.
- Los proyectos nuevos usan una cuadrícula PCB de 0.25 mm; el esquema conserva su espaciado de 20 unidades y los valores explícitos de proyectos existentes se respetan. Encuadrar PCB calcula el margen según las dimensiones físicas de la placa.
- El material visual del PCB sigue su contorno cerrado, incluidos entrantes. Los contornos abiertos o cruzados no reciben relleno. La triangulación opcional se limita a 512 vértices y conserva los bordes editables por encima de ese límite; no cambia contorno, cobre, DRC ni exportación.

## Contratos físicos

- PCB en milímetros; origen de huella, giro de pieza y giro de pad independientes. La cara inferior refleja X antes de aplicar la rotación de la pieza.
- Pads rectangulares, circulares, ovalados y redondeados; SMD, taladro metalizado y NPTH; taladros circulares o ranurados con offset. Los PTH conducen entre las dos caras; los NPTH no generan cobre.
- La vista previa y el PCB utilizan las formas físicas. No se coloca una ilustración PNG genérica encima de una huella importada. El ancho visual de las pistas procede de su ancho físico.
- La comprobación de holgura usa un límite exterior conservador y la continuidad se demuestra con un límite interior. Un pequeño hueco físico no se convierte en conexión por aproximar una curva.
- Perfil de edición actual: holgura de 0.20 mm y ancho mínimo de 0.10 mm. No constituye un perfil certificado de un fabricante.
- Los documentos antiguos siguen siendo legibles. Las huellas ilustrativas heredadas no superan la comprobación de fabricación como si fueran geometría resuelta.

## Archivos de salida

Gerber métrico absoluto 4.6: cobre superior/inferior, máscaras superior/inferior, serigrafía vectorial superior/inferior y contorno. Excellon decimal métrico separado para taladros PTH y NPTH, incluidas ranuras. La inversión de Y es la misma en todas las capas y taladros. La anchura de dibujo del contorno es 0.05 mm; la dimensión de placa se toma de su línea central.

El escritor devuelve advertencias explícitas sobre texto de serigrafía omitido y trazos decorativos discontinuos emitidos como continuos. No exporta pasta, datos de ensamblado o modelos 3D. Los gráficos libres en cobre/máscara/pasta y los recortes definidos dentro de una huella quedan bloqueados hasta que exista un contrato físico completo para ellos.

## Límites reales

El MVP de fabricación usa dos caras de cobre. Las vías son taladros metalizados
independientes del catálogo de componentes. Las reglas base y las excepciones por
red se guardan en la PCB y se comparten entre ruteo y DRC. La importación remota
admite archivos oficiales directos, no catálogos web completos. Planos de cobre,
apilados de más de dos capas y autoenrutado siguen fuera de este MVP.

## Cierre funcional de dos capas: 8 de noviembre de 2026

- **Vía / Shift+V**, durante una ruta: el siguiente clic fija una vía pasante y
  continúa por la otra cara. Cobre y taladros del borrador se confirman juntos al
  terminar la ruta; Esc descarta el borrador completo. Deshacer vuelve por puntos
  y cambios de cara antes de tocar el historial del documento.
  Enter confirma un recorrido parcial, incluso al acabar en una vía; el DRC
  conserva sus conexiones pendientes hasta completar la red.
- Se puede iniciar y terminar sobre cobre existente; la unión agrega vértices a
  la pista original y mantiene sus identificadores. Las vías se pueden seleccionar,
  mover con sus vértices unidos o eliminar. El DRC vuelve a comprobar la geometría.
- **Reglas de diseño** abre un inspector contextual: ancho mínimo, separación,
  borde, taladro, anillo y tamaño de vía. Enter aplica; Esc descarta. Las redes
  permiten excepciones de ancho y separación; un par de redes usa la separación
  más exigente. Son parámetros propios, no un perfil certificado de fabricación.
- El paquete físico mantiene `board-fabrication.zip` con los CAM en la raíz.
  Añade por separado `board-assembly.zip`, `BOM.csv`, `CPL.csv`, informe eléctrico
  y resultados DC. Los códigos de compra quedan vacíos; los centroides y giros
  necesitan cotejarse con las piezas reales antes de encargar ensamblado.
- La simulación DC verifica balance de corriente y potencia y conserva ambos
  residuos en sus resultados. El modelo de LED sigue siendo lineal por tramos,
  no un motor de transitorios ni un modelo obtenido de un fabricante.
- En un circuito con fuente, exportar bloquea errores ERC, terminales flotantes,
  LEDs sin limitador verificado y un resultado DC que no converge. Una placa de
  prueba pasiva puede exportarse con su informe eléctrico para revisión.
- Vista 3D permanece en beta, sin ampliar su implementación en esta tanda.

La evidencia de pruebas, aceptación nativa y fabricación del corazón se registra
por separado en `artifacts/electronics-completion-2026-11-08`; estas instrucciones
describen el flujo implementado, no una certificación del diseño del usuario.

La comprobación de superficies CPU y la captura CAD fuera de pantalla se documentan como tales: no sustituyen probar gestos, foco, DPI y rendimiento en una ventana de escritorio. La lectura CAM independiente valida sintaxis y medidas del diseño de prueba; no certifica cualquier diseño futuro ni un pedido de fabricación.

## Verificación del 23 de octubre de 2026

- `raf_electronics`: 92 pruebas unitarias y 22 regresiones de integración aprobadas.
- `raf_render`, filtro `cad_surface`: 18 pruebas aprobadas. La captura CPU comprueba píxeles de cobre dentro del pad y fondo dentro del taladro, además de anchos físicos y densidad.
- `raf_editor`, filtro `electronics`: 191 pruebas aprobadas, incluidas asignación con historial, rutas manuales, exportación sin sobrescribir trabajos y rechazo de un esquema desactualizado.
- `raf_editor`, filtro `footprint`: 14 aprobadas. Este grupo se solapa con el anterior; no son 14 casos adicionales. La prueba de descarga, ignorada por defecto por necesitar red, se ejecutó por separado contra el repositorio oficial y pasó.
- Barra PCB: dos pruebas aprobadas, con controles en ambas densidades y descarte de pistas retiradas.
- Gerbonara 1.6.3 abrió los siete Gerber y dos Excellon del diseño de prueba sin advertencias: contorno de 12 × 10 mm, PTH circular, ranura rotada con offset y un NPTH de 0.6 mm. El contorno tiene una anchura gráfica de 0.05 mm, separada de sus medidas por línea central.
- Catálogos español/inglés válidos como JSON y comprobación de espacios del diff sin errores. La compilación conserva advertencias de código sin uso; no se afirma una compilación sin advertencias.

Las pruebas del editor se compilaron con optimización de prueba desactivada para `raf_editor`, sin alterar el perfil del repositorio. Las capturas son de superficies CPU y del renderizador CAD ejecutado fuera de pantalla; sigue pendiente la aceptación en una ventana nativa.

## Verificación de interfaz del 24 de octubre de 2026

- `raf_editor`, filtro `electronics`: 209 pruebas aprobadas. Incluye biblioteca/inspector, navegación, conservación de anchos y cámara, encuadre físico y controles de la barra superior en 900/1280/1600 puntos.
- `raf_editor`, filtro `pcb_tool_options_surface`: cuatro pruebas aprobadas. Las opciones conservan sus parámetros físicos y caben en las presentaciones de una y dos filas.
- `raf_render`, filtro `cad_`: 21 pruebas aprobadas. El material PCB se verifica con píxeles ejecutados dentro de un contorno cóncavo y fuera de su entrante; los contornos abiertos o cruzados no reciben relleno.
- `raf_electronics`: 92 unitarias y 22 regresiones de integración aprobadas después del cambio a símbolos vectoriales.
- Capturas CPU integradas del esquema a 1600 × 900 y PCB a 1280 × 800, con una placa de prueba explícita de 20 × 12 mm y huellas importadas. Se componen los mismos builders de paneles y el render CAD, incluyendo minimapa real. Son fixtures de revisión y no capturas de una ventana nativa.
- Las pruebas de gestos, foco, DPI y rendimiento en ventana siguen pendientes. Las advertencias de código sin uso del checkout se conservan; no se afirma un build sin advertencias.

## Cierre de compilación del 25 de octubre de 2026

- La regresión `electronics_pcb_grid_defaults` de `raf_core` aprobó la cuadrícula inicial de 0.25 mm y la conservación de valores explícitos antiguos.
- `cargo build -p aura_rafi_editor --locked --offline` terminó correctamente con el perfil `dev` del repositorio y el target aislado `target_electronics_production_20261023`. El ejecutable está en `x86_64-pc-windows-gnu/debug/aura_rafi_editor.exe` dentro de ese target. La compilación conserva advertencias de código sin uso.
- El ejecutable se abrió para revisión. La automatización no consiguió activar su ventana: `failed to activate captured window`, también después de volver a seleccionar la ventana y reintentar una vez. No se certifican selección, arrastre, foco o DPI nativos a partir de esa sesión; las capturas integradas anteriores siguen siendo evidencia CPU.

## Rendimiento del lienzo: 25 de octubre de 2026

Se encontró una rasterización CPU de 880 × 560 del minimapa, seguida de una subida de imagen, en cada revisión de cámara o hover. Ahora la imagen conserva la geometría del documento y la selección en caché; RafUI mueve el rectángulo de cámara por separado. Pan, zoom y hover no vuelven a generar la imagen. El mapa del mouse usa la misma transformación y el mismo margen que el raster, sin desplazar sus coordenadas al mover la cámara. La salida CAD ejecutada también se reutiliza en redibujos de UI cuando su frame, dispositivo y superficie siguen vigentes.

La resolución del minimapa y la calidad CAD se conservan. El raster recorta líneas fuera de pantalla y usa un camino directo para rectángulos alineados; las curvas y taladros mantienen la evaluación geométrica.

- `raf_editor`, filtro `electronics`: 213 pruebas aprobadas. Incluye invalidación de caché, conservación del mapeo durante pan y reutilización de la salida CAD.
- `cargo run -p raf_editor --example electronics_perf --locked --offline`: benchmark CPU con lienzo 1280 × 720, minimapa 880 × 560, 8 iteraciones de calentamiento y 30 muestras por caso. No mide presentación GPU ni FPS de ventana.
- Antes, el minimapa con dos componentes costaba una mediana de 14.380 ms en esquema y 10.152 ms en PCB. Durante pan con la caché caliente, el camino actualizado cuesta 0.001 ms en ambos casos y solicita cero subidas de imagen. Con 100 componentes, su percentil 95 es 0.397 ms en esquema y 0.022 ms en PCB. El repintado estático cuando cambia el documento queda entre 1.207 y 2.103 ms de mediana en los cuatro fixtures.
- Las capturas CPU integradas conservan PCB, huellas y minimapa. Estos datos no certifican todavía fluidez, foco, DPI o gestos en la ventana del usuario.
- La compilación del ejecutable corregido terminó con código 0 mediante `cargo rustc -p aura_rafi_editor --bin aura_rafi_editor --locked --offline`, con el mismo target y salida alternativa `aura_rafi_editor_perf_20261025.exe` para conservar el ejecutable abierto. Mantiene advertencias de código sin uso.
- El ejecutable nuevo abrió correctamente el Hub en una ventana independiente. La prueba de interacción se detuvo al detectar entrada del usuario en esa ventana; no se midieron FPS nativos ni se certificó todavía pan/zoom del editor abierto.

## Correcciones de autoría del 28 de octubre de 2026

- Contorno ahora marca vértices y se cierra pulsando el primero. Admite formas cóncavas como un corazón; rechaza cruces y áreas degeneradas con el mismo validador geométrico del DRC. Esc cancela y el cierre completo ocupa una operación de historial. La prueba de corazón es una regresión sintética; no modifica el proyecto Soak del usuario.
- Las nuevas ubicaciones iniciales consideran los límites rotados de cada huella y conservan las posiciones existentes. No constituyen un colocador automático del circuito ni repaquetan una placa ya editada.
- El selector ofrece patrones genéricos Raf para resistencia y LED 0805 y entrada de alimentación por dos taladros a 2.54 mm. Requieren asignación explícita y comprobar la pieza elegida; la entrada de dos cables no se presenta como un soporte 18650. Las geometrías asignadas quedan incorporadas al PCB y su historial. Los archivos KiCad importados siguen disponibles.
- El límite Winit procesa las transiciones de botones antes de acumularlas con eventos posteriores. Los gestos capturados por el lienzo siguen recibiendo la liberación al pasar sobre UI. La adquisición de terminales eléctricos funciona con la cuadrícula desactivada.
- Los caracteres de control de teclado no se insertan como texto; el Hub rechaza nombres con caracteres de control antes de crear el proyecto. Se mantiene el mensaje localizado de validación.
- El estado Guardado/Modificado ocupa una entrada independiente del aviso de fabricación. Los detalles completos permanecen en inspector y análisis.
- Exportar agrega `board-fabrication.zip` dentro de cada trabajo, con los CAM en la raíz. El marcador COMPLETE se escribe después del ZIP. Se mantienen los bloqueos físicos y DRC; no se realiza una subida ni un pedido a JLCPCB.

La primera compilación completa de regresiones del editor quedó bloqueada por referencias en curso a APIs de modelos IA eliminadas en `agent_surface.rs` y `settings_surface.rs`. No se modificaron esos archivos para sortear el bloqueo. La aceptación manual del editor y del proyecto Soak queda pendiente de autorización del usuario.

Validación independiente: 95 pruebas unitarias y 22 regresiones de `raf_electronics` aprobadas, incluidas geometría de corazón, rechazo de contorno cruzado, patrones físicos y separación de batería. El escritor ZIP se compiló como harness aislado de la función de producción y `zipfile` de Python verificó nombres, contenidos y CRC. Esto valida el archivo de prueba, sin afirmar aceptación de JLCPCB ni ejecución de la exportación integrada bloqueada por la compilación del editor. Los catálogos de idioma se analizaron como JSON y el diff acotado pasó la comprobación de espacios.

## Cierre de interacción del 30 de octubre de 2026

La revisión sin automatización de escritorio encontró dos errores adicionales de interacción: el árbitro global retiraba capturas antes de entregar mouse-up, y el límite de presentación podía aplazar la lectura de las transiciones CAD. Ahora una liberación explícita conserva su dueño hasta que el consumidor la procesa; una captura obsoleta sin liberación sigue cancelándose. Electronics procesa las transiciones de botones aunque la siguiente presentación aún no tenga permiso, conservando el límite de frames del renderizador. Los arrastres aplican la coordenada final al liberar, incluso sin muestras intermedias.

Cerrar un contorno usa ocho píxeles de radio en pantalla, independiente de la cuadrícula de fabricación. Guardar ambos documentos notifica la revisión UI para refrescar Guardado/Modificado; los avisos de fabricación conservan su entrada separada. Las regresiones ejercitan el árbitro antes y después del controlador, el movimiento sin frames intermedios, el cierre con cuadrícula fina y la persistencia/estado tras éxito o fallo.

Las cuatro llamadas nuevas del diseñador RafUI a row/column se ajustaron a su firma existente para permitir compilar el editor. Las cuatro variantes del receptor de sombras especifican explícitamente el modo de escena (ui_mode = None) de la firma actual. Son ajustes de compatibilidad de llamadas, sin cambiar las decisiones de autoría o de sombreado. La PCB del usuario no se abrió ni se modificó en esta revisión.

## Verificación de cierre del 31 de octubre de 2026

La cancelación de un borrador CAD recibe Esc también al pasar sobre paneles pasivos; un campo de texto, búsqueda o selector modal mantiene su prioridad. El atajo Ctrl+S se despacha al pulsar la tecla mientras el modificador sigue activo, incluso si la presentación está limitada por FPS. Mantener la combinación pulsada no repite el guardado sin otra pulsación.

El ZIP producido por la exportación integrada de prueba fue leído con zipfile de Python: CRC correctos, nombres únicos en la raíz, siete Gerber métricos y dos Excellon métricos, además de README.txt. No es una exportación del proyecto Soak ni una aceptación por un fabricante.

La compilación y ejecución del editor de pruebas terminó correctamente con `cargo test -p raf_editor --lib electronics --locked --offline`, usando el target aislado y la optimización de pruebas de `raf_editor` desactivada mediante configuración de la invocación. El bloqueo de compilación registrado el 28 de octubre ya no impide esta validación.

- Electronics: 222 pruebas aprobadas, incluidas liberación capturada, cableado entre pines con el árbitro real, posición final de arrastre, contorno de corazón y cierre con cuadrícula fina, prioridad de Esc, persistencia/estado de guardado y despacho de Ctrl+S sin repetición por mantener las teclas.
- Huellas, sobre el mismo ejecutable de pruebas: 17 aprobadas y una ignorada por requerir acceso al repositorio oficial en red. Este filtro se solapa con Electronics; los conteos no se suman como casos únicos.
- Límites compartidos: cuatro pruebas de input de `raf_core`, cinco del puente de input nativo de `raf_render`, una de nombres de proyecto y tres de `NativeEditorRuntime` aprobadas. También hay solapamiento con el filtro de Electronics.
- Catálogos español/inglés válidos como JSON y comprobación de espacios del diff acotado sin errores. Se conservan advertencias de código sin uso.

Esta revisión no utilizó Computer Use, no abrió el engine ni modificó la PCB del usuario. La aceptación de gestos, foco, DPI y FPS en ventana continúa pendiente. La lectura ZIP verifica el contenedor y marcadores de formato; no sustituye revisar la geometría en un lector CAM independiente ni la aceptación de fabricación.

## Referencias de formato

- [Formato de huellas KiCad](https://dev-docs.kicad.org/en/file-formats/sexpr-footprint/).
- [Licencia oficial de las bibliotecas KiCad](https://www.kicad.org/libraries/license/).
- [Especificación Gerber de Ucamco](https://www.ucamco.com/en/gerber).
- [Gerbonara, lector CAM independiente usado en la validación](https://gerbolyze.gitlab.io/gerbonara/).
