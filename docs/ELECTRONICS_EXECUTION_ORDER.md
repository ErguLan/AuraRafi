# Electronics: orden de ejecución

Revisión del análisis recibido el 17 de octubre de 2026. Dirección de interfaz: `raf-creative-ui-craftsmanship`, por solicitud del usuario. Se conserva el catálogo de componentes.

## Decisión de producto

Priorizar una herramienta en la que sea fácil construir, inspeccionar y corregir un circuito existente. Después demostrar una placa pequeña con dimensiones y archivos físicos verificables. La integración específica de ensamblado de un fabricante viene después de ese resultado.

El análisis recibido identifica carencias reales, pero agrupa demasiados frentes como P0 y deja pérdidas de edición para el final. Generar Gerber antes de resolver unidades, pads y correspondencias puede producir archivos sintácticamente válidos que describan una placa equivocada.

## Hallazgos contrastados

| Hallazgo | Estado comprobado | Implicación |
|---|---|---|
| Gerber | Export explícitamente placeholder; no se cambió en esta entrega | Sigue pendiente una salida de fabricación real |
| Escala física | `MM_PER_SCHEMATIC_UNIT = 1.0`, pero la huella llamada `0805` tiene cuerpo `36 x 18` y pads separados 36 unidades | Fijar el contrato físico antes del exportador; no basta con asignar un nombre comercial |
| Inspector del esquema | `inspector_data` no enviaba `Editable`; la superficie lo interpretaba como solo lectura | Se corrigió antes de añadir más paneles o herramientas |
| Pérdida de borradores | El host conservaba un borrador ligado a una pieza, pero solo enviaba Enter y descartaba el borrador al cambiar selección | Aplicar al salir y transportar el UUID original, con cancelación explícita |
| Vías | No existe transición física entre capas | Necesaria para ampliar el ruteo entre capas; no bloquea una primera placa de una capa |
| Rendimiento PCB | `nearby_pairs` ordena AABBs y descarta pares por X/capa/Y; conserva peor caso cuadrático | Medir arrastre con documentos representativos antes de reemplazar el algoritmo |
| Highlight de nets | El controlador y compositor ya tienen `highlight_net_id` | Conectar la interacción con esa capacidad; no crear otro sistema de resaltado |

Dos correcciones del texto original: GTS/GBS representan máscara de soldadura; GTP/GBP corresponden a pasta. Una placa de una capa es un objetivo de fabricación válido. Referencias: [preparación de Gerber](https://jlcpcb.com/help/article/gerber-files-preparation), [generación de capas](https://jlcpcb.com/help/article/how-to-generate-gerber-files-in-different-software), [capas PCB](https://jlcpcb.com/resources/pcb-layers).

## Secuencia

### 1. Edición fiable y orientación dentro del circuito

Primera entrega implementada:

- Habilitar valor editable en componentes desbloqueados del esquema.
- Aplicar con Enter, Tab o clic fuera; Escape descarta el borrador.
- Enviar el UUID de la pieza editada para tolerar que la selección cambie antes de aplicar.
- Refrescar el valor del documento cuando el campo no está en edición, incluyendo undo/redo.
- Conservar el historial y los bloqueos del controlador.

Continuación de esta etapa: HUD compacto con zoom, paso de snap y conexiones pendientes; elección del paso; resaltado de una net al inspeccionar sus conexiones; navegación a hallazgos. La lectura de unidades debe corresponder al contrato real del documento. Las mediciones y los informes existentes se reutilizan.

Aceptación: editar R1, seleccionar R2, comprobar que solo cambió R1, deshacer, rehacer, cancelar otra edición y guardar/cargar. Repetir con una pieza bloqueada y un destino eliminado.

### 2. Contrato físico mínimo de las piezas existentes

Definir unidades PCB, origen, rotación y cara; identidad de cada pad, correspondencia pin-pad, tipo SMD/THT y taladro cuando corresponda. Versionar la representación y mantener legibles los documentos actuales, sin reescalar diseños silenciosamente.

Asignar huellas verificadas a las piezas que ya existen y ofrecer su elección en el inspector. Ampliar encapsulados solo cuando un diseño de referencia lo necesite. Un identificador no reconocido puede servir como representación ilustrativa, pero no debe superar la validación de fabricación mediante un genérico silencioso.

Aceptación: una huella con dimensiones comprobadas conserva pin, net, posición y orientación tras sync, rotación, undo y guardado/carga.

### 3. Una placa pequeña completa hasta archivos verificables

Comenzar con una sola capa y las piezas ya soportadas. Emitir cobre, contorno, máscaras y taladros reales que correspondan al documento. Separar las comprobaciones de placa desnuda de las que exige el ensamblado automático.

Exponer el mismo exportador mediante UI y comandos. Validar primero geometría, continuidad y archivos con un lector independiente; empaquetar después. Los nombres de archivos aceptados por un fabricante no sustituyen la verificación del contenido.

Aceptación: un diseño válido y otro con errores intencionales; el inválido queda bloqueado con una explicación accionable y el válido coincide dimensionalmente en otro lector.

### 4. Ruteo guiado y segunda capa

Introducir vías con diámetro, taladro y net reales; transición de capa coherente; DRC de anillo/taladro/cobre; ruta incremental, ancho por pista, vértices y rip-up con historial. El selector de capa vive junto al contexto del PCB.

Aceptación: una conexión entre caras solo se considera resuelta al existir una transición conductora válida, y undo restaura tanto el ruteo como el ratsnest.

### 5. Producción, escala y ensamblado

Clases de net, reglas configurables, perfiles de fabricación, serigrafía y metadatos de componentes. BOM/CPL para ensamblado cuando encapsulado, origen y convención de rotación estén comprobados. Medir latencia de arrastre, memoria del historial y duración de análisis antes de priorizar reemplazos del motor. Vista 3D y visor Gerber integrado son posteriores a un exportador verificado externamente.

## Dirección de interfaz

Aplicar la skill Creative como instrumento de precisión: lienzo protagonista, herramientas agrupadas por tarea, estados tranquilos y respuesta clara al acercarse a pines, seleccionar una net o bloquearse una ruta. Los avisos y colores explican una acción o una conexión concreta.

El contexto cambia con la tarea: esquema para construir y entender; PCB para huellas, cobre y capas; revisión para errores y salida. Evitar llenar el shell de paneles que solo son útiles en otra tarea. La animación puede reforzar hover/snap, pero nunca cambiar conectividad ni retrasar una edición.

## Límite de esta entrega

Se implementó el primer tramo de edición del inspector. Las demás etapas son un orden de trabajo, no capacidades entregadas. La validación de lógica y superficies no reemplaza probar gestos en una ventana nativa; no se certificó fabricación ni se implementó Gerber en esta entrega.
