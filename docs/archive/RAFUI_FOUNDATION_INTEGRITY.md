# RafUI Foundation Integrity

Estado: en implementacion, sin reconstruir interfaces del editor.
Fecha: 2026-07-27

Este documento es la lista de invariantes que RafUI debe cerrar antes de
volver a montar una superficie grande. No autoriza Play, Runtime ni una nueva
pantalla de RafUI Studio. El contrato retained se valida por pruebas y hosts
headless; no se debe reintroducir un puente visual legado.

## Invariantes cerradas en esta pasada

- `RowWrap` no coloca los hijos fuera del viewport sobre el rectangulo padre.
- Las columnas explicitas de `Grid` tienen prioridad sobre el calculo
  automatico.
- El layout puede repetir una medicion de texto localizada y reacomodar el
  arbol completo antes de pintar; no depende solamente de corregir una caja
  despues del layout.
- El atlas de texto comparte cadenas iguales entre nodos, se recupera cuando
  se llena y expone region sucia para uploads parciales.
- El scroll usa maximos derivados del contenido y expone
  `UiVirtualRange` para listas y arboles que no deben materializar miles de
  filas fuera de pantalla.
- Drag tiene umbral de 4 px; el clic no se convierte en drag por jitter.
- TextField conserva caret, seleccion, flechas, Home, End, Delete y
  Backspace en estado de sesion; Ctrl/Cmd+A selecciona todo y el puente
  nativo conserva el preedit de IME sin insertarlo antes del commit.
- Shift+Tab, modificadores y composicion de input ya tienen lugar en el
  contrato de entrada; el host nativo puede alimentarlos.
- Perder el foco de la ventana libera botones, teclas y modificadores para que
  Alt+Tab o un dialogo modal no deje un drag fantasma.
- `UiSurfaceSession::set_reduced_motion` permite que el host aplique la
  preferencia del entorno sin meter APIs de ventana en el documento retained.
- Los estilos pueden reaccionar a `Selected`, `Open` e `Invalid`, y los
  componentes pueden heredar el color semantico del tema.
- Documentos UI se validan antes de entrar al renderer y pueden migrarse al
  formato actual.
- CLI, consola, RafUI, MCP y plugins comparten un protocolo JSONL acotado a
  1 MiB. El loop stdio no crea ventanas y funciona tambien como puente para
  sockets locales, TCP o named pipes.
- El compositor GPU descarta texturas de imagen que ya no estan en el draw
  list, evitando que thumbnails y previews acumulen memoria durante la sesion.

## Navegacion del viewport preparada

`ViewportPanel::navigation_status()` expone un modelo de solo datos para una
franja futura:

```text
tool | space | snap | camera | focus lock
```

La franja debe mostrar estado real del viewport, traducido por i18n, y nunca
duplicar la logica de `ViewportBridge`. Durante la decommission de interfaces
no se agrega una barra visual; el modelo se puede probar sin una ventana.

## Fase futura de Settings: seguridad de transformacion

Cuando Settings vuelva a tener superficie retained, agregara una seccion de
interaccion del viewport. No se implementa ahora porque no existe esa
interfaz. La configuracion propuesta es:

- `selection_mode`: seleccionar, rectangulo de seleccion o herramienta activa;
- `free_drag_enabled`: desactivado por defecto para evitar mover objetos por
  accidente;
- `drag_threshold_px`: minimo 4 px, configurable con limite seguro;
- `gizmo_only_transform`: exigir el gizmo para mover, rotar o escalar;
- `snap_enabled` y `snap_step`: usar la misma unidad que la grilla;
- `confirm_destructive_transform`: pedir confirmacion solo para operaciones
  masivas, nunca en cada click.

El flujo esperado es: Settings modifica un draft, la aplicacion valida los
rangos, guarda la preferencia y `ViewportPanel` la consume. El renderer no
lee Settings ni decide UX.

## Kernel de comandos para agentes

El endpoint no depende de una superficie activa. Una interfaz, una consola,
un CLI o un MCP solo producen `EngineCommandRequest`; un executor de dominio
devuelve `EngineCommandResponse` con `changed`, datos, warnings, diff y
estado de undo.

```text
RafUI  ─┐
Console ├─> CommandGateway -> domain executor -> response/diff/warnings
CLI    ─┤
MCP    ─┘
```

El transporte actual es JSONL sobre stdio. `IpcEndpoint` deja preparado el
mismo contrato para TCP, Unix sockets y Windows named pipes sin meter codigo
de plataforma en RafUI ni en el command model. Ejecutar comandos de escena,
assets o CAD sigue requiriendo un executor con proyecto; el protocolo no
finge un backend que aun no este montado.

## No tocar en esta fase

- Play, Stop, Runtime, simulacion jugable o loop de runtime.
- Sombras, PBR, postprocesado, particulas o animacion esqueletica.
- Una nueva pantalla RafUI Studio.
- Un segundo sistema de widgets fuera de RafUI.

## Gate antes de reconstruir el shell

La siguiente vuelta debe aportar pruebas de layout, atlas, scroll virtual,
teclado/IME, validacion documental, CPU/GPU golden y memoria acotada. La
validacion visual manual en 100%, 125%, 150% y 200% DPI sigue siendo
obligatoria cuando exista una superficie nuevamente montada.
