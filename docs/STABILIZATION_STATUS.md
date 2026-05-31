# Estado De Estabilizacion

Fecha: 2026-05-26

Este documento resume que ya esta resuelto en codigo, que ya venia funcionando, que sigue pendiente y en que fase cae cada bloque. La idea es tener una sola fuente de verdad mientras cerramos la estabilizacion antes de testear a fondo.

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

- Confirmacion al salir con cambios.
- Autosave endurecido.
- Guardado con fallo visible y sin perdida silenciosa.

## Pendiente Real

- Undo/redo despues de varias operaciones encadenadas.
- Drag/drop de assets y hierarchy: falta preview visual mas claro, mejor hover y mas precision al detectar bloques.
- Persistencia de settings y layout: necesita pasada dedicada.
- Seleccion, duplicado, delete y shortcuts: falta auditoria funcional completa.
- Navegacion de camara: falta prueba manual prolongada y verificacion de ergonomia fina.
- Gizmos: afinar sensacion final del nuevo scale handle segun uso real.
- Multi-select: falta dejar claro el objeto principal, sobre todo en hierarchy y properties, y revisar el bajon de FPS.
- Resize de ventana: pendiente de verificacion seria.
- Cambios de modo 2D/3D: pendiente de verificacion seria.
- Play mode / runtime: no hay runtime real aun; hay que dejar esto honesto en UI.
- Crear schematic desde cero: la base existe, pero la experiencia aun es mala.
- Cablear en schematic: la cancelacion base ya esta mejor, pero falta mas feedback visual y menos friccion general.
- DRC: pendiente de verificacion seria.
- Simulacion DC: pendiente de verificacion seria.
- Librerias de componentes con datos y datasheets: pendiente de diseno de escalado.
- Export netlist/BOM/SVG: la UX base ya mejoro con botones clicables y copia al portapapeles, pero todavia falta salida a archivo y flujo mas serio.
- PCB core: ya hay una primera mejora de hover/tolerancia/preview, pero mover componentes, route, outline y la experiencia general siguen verdes.
- Guardar/reabrir sincronizando Schematic y PCB: base funcional, pero hay que probar mas la persistencia visual y airwires.
- Project settings de electronics: faltan settings y properties propios con nivel mas serio.
- Renderer activo: falta una verdad unica del path canonico entre docs y codigo para no seguir mezclando CPU-first, GPU-first y rutas "preparadas".
- Hot path grafico: el backend GPU activo todavia crea buffers por draw y por frame; eso merece optimizacion estructural antes de empezar una guerra de micro-optimizaciones por todo el engine.
- Medicion del hot path: falta una linea base reproducible con escenas de referencia para validar mejoras reales de renderer con antes/despues, no por intuicion.
- Contrato CPU fallback/GPU activo: falta dejar por escrito que paridad minima se mantiene mientras se consolida el path canonico, para no optimizar rompiendo la ruta potato.
- Optimizacion global del engine: no conviene abrirla aun; primero hay que congelar que renderer/runtime grafico es el camino oficial.
- Experiencia general de electronics: sigue necesitando una pasada fuerte de interfaz.

## Fases

### Fase 1: Seguridad de sesion y guardado

Estado: implementada en codigo, pendiente prueba manual final.

Incluye:

- Guardado real.
- Dirty state consistente.
- Autosave real.
- Confirmacion al salir.

Objetivo: cero perdida silenciosa.

### Fase 2: Schematic usable de verdad

Estado: en progreso.

Ya cubierto en esta fase:

- Final correcto al conectar a pin/endpoint/junction.
- Doble click izquierdo deja de crear branch extra.
- Rotacion correcta del preview al colocar componentes.
- Cancelacion limpia del wire mode con click derecho.
- Cancelacion limpia de cualquier placement activo con click derecho.
- Popup de export clicable en vez de solo visual.
- Export copia contenido al portapapeles ademas de dejarlo en log.

Falta en esta fase:

- Placement menos tosco.
- Export a archivo real desde la UI.

Objetivo: hacer un schematic sin pelearte con la interfaz.

### Fase 3: PCB core funcional

Estado: iniciada.

Ya cubierto en esta fase:

- Hover visual en componentes, trazos y airwires.
- Tolerancias de seleccion/ruteo mas amplias.
- Preview vivo al dibujar outline.
- Airwires reconstruidos durante el drag de componentes.

Falta en esta fase:

- Verificacion manual de que route y outline ya se sienten correctos en uso repetido.
- Mejoras de ruteo mas alla del auto-ruteo ortogonal base.

Objetivo: que PCB deje de ser "se ve pero no sirve".

### Fase 4: Viewport Game y hierarchy

Estado: implementada en codigo, pendiente prueba manual de rendimiento y tacto final.

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

Objetivo: arreglar control, feedback, multi-select y rendimiento al manipular.

### Fase 5: Polish de UI y accesibilidad

Estado: implementada en codigo, pendiente validacion visual final.

Ya cubierto en esta fase:

- Toggle persistente para mostrar/ocultar el contador FPS de la barra superior.
- HUD superior con ancho adaptativo para que no se corte tan facil.
- Toggles visuales rapidos para grid y labels dentro del viewport.
- Brujula XYZ interactiva con snap por eje y reset a vista isometrica.
- Mejor contraste visual en top bar y downbar.

Falta en esta fase:

- Validacion visual final en ventanas chicas y monitores distintos.

Objetivo: limpiar la experiencia sin tocar la logica base.

### Fase 6: Runtime truth pass

Estado: 6A documentada en MD temporal; implementacion de runtime aun no iniciada.

Objetivo: dejar claro que existe, que no existe y que botones prometen de mas.

Documento temporal de referencia creado en:

- `docs/TEMP_PHASE6_RUNTIME_TRUTH_PASS.md`

### Fase 7: Renderer canonico y hot path grafico

Estado: no empezada.

Ya justificado para esta fase:

- Scene, Schematic y PCB ya comparten una ruta moderna de runtime grafico.
- El viewport ya usa mediciones reales de render/upload y escala adaptativa.
- Sigue habiendo mezcla de verdad documental y tecnica sobre si el renderer debe leerse como CPU-first, GPU-first o ruta hibrida en transicion.
- El backend GPU activo todavia paga costo estructural creando buffers por draw/per-frame en el camino caliente.
- El fallback CPU sigue formando parte del contrato del engine y no conviene degradarlo mientras se congela el camino oficial.

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

- Una definicion escrita de "renderer canonico" que no choque entre README, ARCHITECTURE, RENDERER y CHANGELOG.
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