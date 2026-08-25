# Electronics Workbench RafUI

## Estado de producto

Electronics usa una composicion hibrida ya decidida. La base es un workbench
CAD tecnico y sobrio; la toolbar flotante, el minimapa y los grupos movibles del
downbar aportan flexibilidad sin convertir todo el editor en tarjetas flotantes.

No se debe reiniciar este diseno desde cero en cada iteracion. Los cambios
futuros deben pulir esta composicion y conservar sus contratos.

## Contratos visuales

- El canvas es la superficie dominante.
- El naranja comunica seleccion, estado activo o una accion primaria. No se usa
  como relleno permanente de filas, categorias o controles secundarios.
- La biblioteca es compacta y plegable. Una busqueda abre temporalmente las
  categorias necesarias para mostrar resultados.
- Toolbar y tabs usan controles neutros; el activo se distingue con borde y
  velo de seleccion, no con bloques naranjas repetidos.
- Inspector evita encabezados duplicados y mantiene Properties y Sessions como
  tabs del mismo panel.
- El minimapa es una representacion derivada del documento. No inventa rutas ni
  sustituye la geometria CAD.

## Contratos de layout

- Electronics usa un downbar dividido por defecto:
  - `workspace`: Console, Assets, Project y Agent.
  - `analysis`: DRC y Simulation.
- Las tabs se pueden mover, dividir y restaurar. El menu de clic derecho de una
  tab pertenece a RafUI; el shell transicional solo posiciona el popup.
- Game conserva un unico downbar a todo el ancho. Ninguna regla de composicion
  exclusiva de Electronics debe filtrarse a Game.
- Los paneles laterales son redimensionables, pero sus rangos deben proteger el
  canvas y evitar columnas vacias sobredimensionadas.

## Contratos de interaccion

- Clic derecho en canvas abre un menu RafUI contextual para lienzo, componente,
  seleccion multiple o cable.
- El menu se cierra con Escape, clic exterior o una accion ejecutada.
- Acciones de documento siguen usando los snapshots y el historial existentes.
- DRC y Simulation conservan ejecucion fuera del hilo UI y publican estados de
  progreso y resultado.

## Contratos graficos y rendimiento

- ApiGraphicBasic es la autoridad publica del canvas CAD y de RafUI. WGPU solo
  se usa detras de su adapter privado.
- No se agregan fallbacks visuales duplicados fuera de RafUI para ocultar
  errores del renderer.
- `GpuLineVertex` alinea `vec3` a 16 bytes. Su layout debe conservar offsets
  `0, 16, 32, 48, 52`; volver a empaquetarlo implicitamente hace desaparecer
  grid, cables y simbolos.
- Los listados largos permanecen virtualizados y solo invalidan su superficie
  ante cambios reales de documento, seleccion, filtro, scroll o layout.
- Hover, drag y animaciones se limitan a su superficie; no fuerzan recompilar
  todo el workbench.

## Validacion obligatoria

Antes de considerar terminada una iteracion de Electronics:

1. `cargo check -p raf_editor`.
2. Pruebas de RafUI, AGB, CAD y layout afectadas.
3. Ejecutable debug actualizado.
4. Revision visual en una escena real: cables pasivos, simbolos, zoom/pan,
   toolbar, categorias, Inspector, menus de clic derecho, downbar y DPI.
5. Abrir Game y confirmar que su downbar sigue ocupando todo el ancho.

Compilar no sustituye la revision visual. Si el entorno no permite capturar o
controlar la aplicacion, ese limite debe quedar reportado de forma explicita.
