# Electronics: escritorio CAD

La composición conserva RafUI y el controlador CAD nativo. Games comparte el
estilo nuevo de la downbar y la consola; sus paneles y su viewport conservan su
distribución.

## Distribución

- Barra de herramientas de Electronics a todo el ancho, debajo del menú.
  Los comandos conservan selección, colocación, ruteo, contorno, navegación,
  rotación y deshacer existentes. En ventanas estrechas se usa la barra compacta.
- Proyecto y biblioteca permanecen visibles a la izquierda. El árbol separa
  documentos, inventario y catálogo; sus categorías se pueden plegar y la
  búsqueda revela coincidencias dentro de grupos plegados. Componentes y Huellas
  tienen accesos explícitos. La pestaña Huellas muestra el catálogo físico en la
  columna izquierda y sus detalles, importación y asignación en el inspector.
  El canvas permanece visible y admite selección mientras se explora. La
  biblioteca sigue la pieza seleccionada; asignar conserva proyecto y UUID de
  destino explícitos. También se puede explorar sin selección.
- El centro tiene una pestaña de documento y menús explícitos de capa activa,
  ancho de pista, cuadrícula y zoom. El porcentaje usa 96 px/pulgada como escala
  de referencia, y se actualiza al mover la cámara sin reconstruir el escritorio.
  Una paleta vertical mantiene accesibles las herramientas de dibujo. El minimapa se presenta arriba a la derecha y conserva su misma
  geometría de navegación e hit testing.
- El inspector ocupa toda la altura disponible a la derecha. X, Y y rotación
  permiten entrada numérica: Enter aplica, Escape descarta. La identidad del
  componente y la superficie se conservan en cada edición. Los bloqueos,
  anclas de cables y el historial siguen pasando por el documento. Propiedades,
  Apariencia y Redes pertenecen solo a Electronics. El bloqueo y los giros de
  90 grados son acciones reales y reversibles. Redes usa los nombres y números
  físicos de pads y permite resaltar una red; no deduce identidad del índice.
  Los interruptores de referencia y valor aparecen en Apariencia bajo
  «Toda la placa · Vista», también sin selección. Su alcance es global y no
  altera el esquema, el cobre ni los archivos de fabricación. Los controles
  propios de la selección, como bloqueo y colocación, conservan su alcance.
- La downbar de Electronics termina antes del inspector. Sus pestañas usan
  una línea superior de selección, encabezados de columnas y filas legibles.
  En grupos estrechos conserva iconos, tooltips y acceso a las pestañas.
  Los filtros de consola se abren desde la cabecera: no ocupan una segunda
  barra permanente. Se mantienen limpiar, autoscroll y filtros reales de nivel.
  La tabla muestra únicamente datos disponibles: severidad, hora, origen y
  mensaje. No se inventan componentes, capas o ubicaciones para llenar columnas.

## Contratos de geometría y coste

`EditorFrameLayout` reserva la barra global, la pestaña, los controles, las
reglas y la paleta. Render, cámara, encuadre, etiquetas y entrada CAD deben usar
`electronics_canvas_for_pcb`; no deben reconstruir offsets por separado.

Las reglas ocupan espacio fuera del rectángulo de entrada CAD. Se componen
en el overlay que depende de la cámara, con un límite de marcas. Pan y zoom
no necesitan reconstruir el escritorio completo ni rasterizar otra vez el
minimapa cuando su geometría no cambió. La cuadrícula de puntos de PCB reutiliza
el batch de líneas existente y reduce densidad según el tamaño en pantalla.

El minimapa tiene un botón de plegado con entrada retenida propia. Plegarlo
libera el área de la imagen para dibujar; su cabecera conserva el acceso para
desplegarlo. El encuadre de PCB deja libre la franja ocupada por el minimapa
expandido en ventanas de al menos 600 puntos de canvas. Pan y zoom siguen
usando el rectángulo completo y no modifican la geometría del documento.

## Beta y límites

- Vista 3D: acceso y panel informativo, explícitamente en desarrollo. No se
  presenta una imagen 2D como si fuese un render 3D.
- El inspector de reglas permite parámetros físicos de placa y excepciones por
  red. Los selectores de ancho activo y cuadrícula mantienen sus presets.
- Texto libre sobre la placa, cotas persistentes y fillets interactivos
  requieren herramientas de dominio adicionales. No se muestran controles
  inactivos simulando estas funciones.
- DRC, simulación y exportación muestran su estado real. Un cambio visual no
  certifica la placa ni sustituye la comprobación del paquete de fabricación.
- Los layouts personalizados guardados de la downbar se conservan. La acción
  de restablecer paneles aplica la disposición por defecto nueva de Electronics.

## Verificación

`electronics_workspace_qa` combina las superficies reales con geometría CAD
ejecutada en CPU. Sus imágenes opcionales se guardan con
`RAF_ELECTRONICS_WORKSPACE_QA_DIR`. Las pruebas del controlador cubren edición
numérica, bloqueo, identidad, anclas y undo. Las de presentación cubren barras
en distintos anchos, idiomas, pestañas y columnas de consola.

Las capturas CPU permiten revisar composición. La aceptación de la ventana
nativa, DPI, respuesta del mouse y FPS se debe comprobar con el ejecutable
actualizado; no se deduce de esas capturas.

## Pulido de la comparación visual

Las capturas de referencia y producto se comparan por regiones y con ampliación
sin suavizado para distinguir tipografía, márgenes y trazos. La configuración
observada tenía fuente global de 10 px; Electronics usa una base de 14 px sin
reescribir esa preferencia ni cambiar el entorno de Games. Las fuentes mayores
siguen siendo respetadas.

Los campos de capa/ancho/cuadrícula/zoom separan etiqueta y valor. Sus menús
se anclan a la geometría efectiva del control. El área de clic exterior usa
un panel transparente, evitando el hover/focus de un botón que cubría la ventana.
Las herramientas CAD tienen SVG y PNG propios, con una misma escala óptica.

La esquina entre documento, regla y paleta se pinta explícitamente: era una
reserva de layout sin superficie y dejaba ver el clear del compositor. El
minimapa conserva su geometría con un marco de cámara neutro, sin teñirla de
naranja. El inspector de una pista muestra su red, ancho, longitud, capa y
vértices; no ofrece huella, previsualización de componente ni un bloqueo
inventado a partir de un campo de solo lectura.

La identidad de componente usa su símbolo real cuando está disponible. Huella,
valor y colocación comparten filas compactas; los títulos de sección no muestran
flechas de acordeón sin una acción asociada. La previsualización 2D conserva los
pads y trazos reales, añade ejes y margen técnico, y evita sugerir un modelo 3D.
Su caché depende del documento, paleta y alcance de etiquetas, no del hover.

La revisión nativa de cierre está registrada en
`artifacts/electronics-desktop-redesign/finish-2026-11-06/QA.md`: 240 pruebas de
Electronics aprobadas, compilación final reabierta y verificación del catálogo,
encuadre e inspector sin modificar el corazón del usuario. La comprobación de
descarga oficial requiere red y quedó ignorada; no se infieren FPS ni aceptación
de fabricación a partir de estas pruebas.
