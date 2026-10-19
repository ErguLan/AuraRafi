# Electronics: correcciones de fiabilidad

Implementación del 11 de octubre de 2026. Se conserva el catálogo de componentes y la interfaz existente.

## Comportamiento corregido

- Los cables con varios segmentos y las uniones explícitas conservan continuidad. Un cruce sin unión sigue separado.
- Las referencias de componentes no se duplican al borrar y volver a colocar.
- Los nombres automáticos no colisionan con nombres explícitos. Las etiquetas repetidas en islas desconectadas reciben nombres eléctricos distintos y una advertencia; el lienzo muestra esos nombres reales.
- Compartir el retorno de dos fuentes no constituye un cortocircuito. Las fuentes incompatibles entre el mismo par de nets sí se reportan.
- Una resistencia en paralelo no demuestra protección de un LED. La comprobación busca un camino en serie sin bypass.
- El LED apagado no inyecta energía en un circuito pasivo. El cálculo y las corrientes publicadas usan la misma ley de rama.
- La conectividad PCB usa contacto geométrico de cobre en la misma capa. Una pista Bottom no conecta pads SMD Top.
- La sincronización remapea pistas mediante identidades de pines. Si una net se divide o desaparece, conserva las pistas en `removed_traces`; repetir la sincronización no vacía esa lista. La lista también participa en el historial.
- Girar una huella no gira el símbolo del esquema. Undo/redo conserva ambas orientaciones independientes.

## Editor, análisis y guardado

El panel de simulación conserva los mapas de resultados y muestra voltajes de nets y V/A/W por componente. Las filas permiten enfocar la pieza correspondiente. Una edición marca los resultados anteriores como obsoletos.

El DRC nativo comprueba el documento activo: ERC del esquema o geometría del PCB. Los marcadores pertenecen a su superficie. El comando nativo `electronics.drc` devuelve el mismo informe mostrado; `electronics.simulate` reutiliza el mismo resultado sin ejecutar dos veces el solver.

Cada editor mantiene un trabajador de análisis y como máximo una solicitud pendiente. Las solicitudes nuevas cancelan las anteriores. La extracción de netlist, el solver y los recorridos costosos de PCB consultan la cancelación durante el cálculo.

Los previews reutilizan la geometría de escena mientras la revisión del documento no cambia. Los overlays se regeneran sin volver a extraer la netlist.

Los archivos se reemplazan mediante temporales únicos, escritura sincronizada y rename. Un fallo de reemplazo conserva el destino anterior. El guardado del par esquema/PCB registra primero la operación completa en `.<archivo-esquema>.pending-save.ron`. Al abrir o guardar se intenta completar una operación interrumpida; si los archivos difieren de las versiones esperadas, se conservan junto con el registro y se muestra el conflicto. Esto es recuperación del par, no una transacción del sistema de archivos entre dos rutas ni coordinación de varios escritores simultáneos.

## Reglas y modelos actuales

- PCB: separación de cobre/borde de 0.2 unidades del lienzo; ancho mínimo y ancho del ruteo rápido de 0.25, según las constantes existentes en milímetros.
- El ruteo rápido prueba dos codos ortogonales en la capa de los pads. Rechaza pads ausentes o desplazados, cruces con otras nets, violaciones de borde y cambios de capa sin transición física.
- Simulación DC: LED lineal por tramos con resistencia directa de 100 ohm, fuga de 1e-12 S y fuentes con resistencia interna de 0.01 ohm. Límite de 64 iteraciones y 4096 incógnitas del solver denso.
- Convención: voltaje de rama = pin 0 menos pin 1; corriente positiva de pin 0 hacia pin 1. Potencia positiva = absorbida; negativa = entregada.
- Capacitores abiertos en DC estacionaria. No hay transitorios RC ni simulación de inducción magnética.

Las huellas actuales siguen siendo geometría simplificada del editor. Estas comprobaciones no certifican dimensiones comerciales, taladros, vías ni fabricación. La exportación Gerber continúa siendo un placeholder explícito.

## Verificación

`raf_electronics`: 67 pruebas unitarias y 22 regresiones de integración. Incluyen los ocho bugs reproducidos, balance de corriente/potencia, LED inverso, cruce con unión explícita, capas, clearance, ruteo bloqueado, renombrado/división de nets, persistencia y cancelación.

La suite completa del editor alcanzó 423/424: el fallo restante fue `panels::nodes_catalog::tests::camera_and_hierarchy_nodes_have_complete_bilingual_labels`, por la clave ausente `nodes.pin.entity-reference.entity` en el trabajo paralelo de Nodes/locales. No se modificaron esos archivos como parte de esta implementación. Las 164 pruebas focalizadas de Electronics pasaron tras la última corrección; verifican rotación/undo, resultados visibles, superficie del DRC, locks, cancelación del trabajador y recuperación de guardado.

Microprueba del mismo fixture de auditoría: una cadena de segmentos, tres muestras por tamaño, mediana, `raf_electronics` con opt-level=3 dentro de dev. Había trabajo paralelo en la máquina.

| Segmentos | Antes (ms) | Después (ms) |
|---:|---:|---:|
| 500 | 2.124 | 0.972 |
| 1000 | 5.066 | 1.541 |
| 2000 | 10.350 | 3.438 |
| 4000 | 44.822 | 6.840 |

La medición corresponde a extracción de netlist, no a FPS ni a un build release completo. La unión usa un índice espacial de extremos/pines en lugar de recorrer cada pareja de cables; sigue existiendo un peor caso denso cuando muchos puntos coinciden.

No se realizó aceptación visual en una ventana nativa ni validación de fabricación. La selección múltiple, edición de bloques, campos numéricos ampliados, un solver transitorio, huellas físicas y exportación Gerber/drill requieren trabajo posterior.

Archivos centrales: `raf_electronics/src/netlist.rs`, `simulation.rs`, `drc.rs`, `pcb/layout.rs`, `pcb/geometry.rs`, `pcb/drc.rs`, `storage.rs`; `raf_editor/src/electronics_controller.rs`, `electronics_analysis.rs`, `electronics_document.rs` y `commands/electronics.rs`.
