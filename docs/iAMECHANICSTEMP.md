# Agent: mecánicas y flujo del frontend

Este documento describe cómo funcionaba la experiencia visual del Agent dentro
del editor. Está escrito como una especificación para que otra inteligencia
pueda reconstruirla desde cero sin conocer el proyecto original.

## Idea general

Agent es un chat integrado en el editor. No es una ventana separada: vive como
una pestaña del bottom dock. La pestaña se llama `agent`, tiene una etiqueta
localizada y usa el icono de Agent.

El frontend muestra tres zonas principales:

- Una barra lateral con conversaciones.
- Un área principal con el estado, el historial y el compositor.
- Una zona inferior de entrada y envío.

La información visual llega desde un controlador de estado. El frontend no
debe hablar directamente con HTTP ni ejecutar herramientas. Solo presenta el
estado, emite acciones y muestra el resultado de esas acciones.

## Lugar dentro del editor

El bottom dock contiene grupos de pestañas. En un proyecto Game, uno de los
grupos puede contener:

```text
console
agent
nodes
project-settings
```

En un proyecto Electronics puede contener:

```text
console
agent
simulation
```

El Agent comparte el dock con esas pestañas y su ancho depende del grupo. El
usuario puede:

- Seleccionar la pestaña.
- Reordenarla dentro del grupo.
- Arrastrarla a otro grupo.
- Crear un grupo nuevo soltándola en un borde.
- Abrir el menú contextual del tab.
- Redimensionar la separación entre grupos.
- Colapsar el dock.

La pestaña se conserva aunque el contenido del Agent no esté activo. Cuando
está seleccionada, el frontend obtiene el rectángulo de contenido del grupo y
presenta allí la superficie Agent.

## Estructura visual

La superficie tiene una raíz horizontal equivalente a:

```text
agent.workbench
  agent.root
    agent.sidebar
    agent.main
```

La sidebar se coloca a la izquierda. El contenido principal ocupa el espacio
restante. La superficie usa los colores del tema industrial oscuro o del tema
claro del editor.

La sidebar puede abrirse y cerrarse con una animación corta. Su ancho máximo es
aproximadamente `224 px`. El contenido del panel puede desplazarse
verticalmente.

## Sidebar de conversaciones

La sidebar contiene:

1. Un encabezado.
2. Un botón de nueva conversación.
3. Una lista de sesiones.

### Encabezado

El encabezado muestra el título `Chat sessions` y un botón para cerrar la
sidebar. El cierre solo cambia el estado visual; no elimina conversaciones.

### Nueva conversación

El botón `New chat` crea una sesión nueva. Al activarlo:

- Se crea o selecciona una sesión vacía.
- Se limpia el runtime de la conversación anterior.
- Se limpia el campo de entrada.
- Se actualiza el título activo.
- Se actualiza el historial persistido cuando corresponde.

### Lista de sesiones

Las sesiones se muestran con la más reciente primero. Cada fila tiene:

- Un botón con el título de la conversación.
- Un botón para eliminarla.
- Un estado visual distinto para la sesión activa.

Si no hay sesiones se muestra un texto equivalente a `No conversations yet`.

El título se acorta visualmente para no desbordar la fila. La identidad de la
sesión no depende de su texto, sino de su identificador.

## Encabezado principal

El encabezado del área principal contiene:

- El título de la sesión actual.
- Un selector de modelo.
- Un detalle corto del proveedor y modelo.
- Un selector de modo.
- Un botón para abrir o cerrar la sidebar.
- Un botón para abrir Settings de AI.

### Título

Si existe una sesión, se muestra su título. Si todavía no existe una sesión
válida, se usa un título equivalente a `New chat`.

### Selector de modelo

El selector presenta el shortcut activo. El valor especial
`provider_default` significa que se usará el modelo configurado directamente
en el proveedor por defecto.

Al abrirlo, aparece un popup con una opción por cada shortcut configurado. Cada
opción muestra:

```text
{provider} | {model_id}
```

La opción seleccionada se marca visualmente. Si el shortcut pertenece a un
proveedor que no puede ser usado por el transporte actual, se muestra
deshabilitada y no responde al click.

### Crear un shortcut

El popup de modelos incluye `Add model`. Al abrirlo aparece un formulario con:

- Label del shortcut.
- ID del modelo.
- Provider actual.
- Botón para abrir Settings.
- Botón para confirmar.
- Botón para cancelar.
- Mensaje de error de validación.

El label y el model ID no pueden estar vacíos. El label tampoco puede
duplicarse. Al confirmar correctamente se agrega un shortcut y puede quedar
seleccionado.

Al abrir el formulario, el focus comienza en el campo del label. Al pulsar
Enter en cualquiera de los dos campos se intenta confirmar.

### Selector de modo

El selector tiene dos opciones:

- `Passive`.
- `Active`.

El modo queda guardado en la configuración del Agent. El popup de modo y el
popup de modelos no deben estar abiertos al mismo tiempo.

### Settings

El botón Settings abre la sección AI de Settings. Desde allí se configuran los
proveedores y los parámetros generales del Agent.

## Readiness y estados

Antes de permitir un envío, el frontend muestra si el Agent está listo.

Los estados de preparación son:

- Provider disabled: el proveedor no está habilitado.
- Model missing: no hay modelo efectivo.
- Adapter required: la URL o el transporte no son compatibles.
- Ready: se puede enviar una solicitud.

Cuando el modo es `Active` y el Agent está listo, se muestra una advertencia
explicando que las herramientas pueden ejecutarse sin pedir confirmación
individual.

El runtime puede estar en estos estados:

- `Done`: no está realizando trabajo.
- `Thinking`: espera o procesa la respuesta del proveedor.
- `ExecutingTools`: está ejecutando herramientas.
- `AwaitingApproval`: necesita una decisión del usuario.
- `Error`: ocurrió un error.

Durante `Thinking` se presenta un mensaje de actividad parecido a `Thinking
while waiting for the provider...`.

Durante `ExecutingTools` se presenta un mensaje parecido a `Applying tool
changes...`.

Durante `AwaitingApproval` se presenta una tarjeta de aprobación. La tarjeta
contiene un texto general, un botón `Approve` y un botón `Deny`. No muestra el
nombre ni los argumentos de cada tool call.

Los errores se agregan normalmente como un mensaje del historial con el texto
`Error: {error}`.

## Historial visual

El historial es una lista vertical con scroll. Los mensajes del sistema se
usan para el contexto interno, pero no se muestran como tarjetas normales.

Los roles visuales son:

- User, con un estilo de usuario.
- Assistant, con un estilo del Agent.
- Tool, con un estilo de resultado de herramienta.

Cada mensaje tiene una tarjeta con:

- Una etiqueta de rol.
- El contenido en texto plano.

No hay renderer Markdown, botones inline, bloques de código especiales ni
acciones embebidas en una tarjeta.

El contenido visible se limita a aproximadamente `16,384` caracteres por
mensaje. Si el contenido es más largo, se trunca para la presentación.

La identidad visual del mensaje debe usar un UUID estable. No se debe usar la
posición actual de la lista como identidad porque el scroll virtualizado puede
cambiar el rango mostrado.

## Scroll y virtualización

El historial completo sigue siendo una conversación continua. El usuario no
elige una página manualmente.

Para no construir todas las tarjetas de una conversación larga, el frontend
calcula una ventana visible usando:

- El desplazamiento vertical.
- La altura disponible.
- La altura conocida o estimada de cada mensaje.
- Un overscan de tres mensajes alrededor del viewport.

Los mensajes que quedan fuera de la ventana se representan mediante un spacer
superior y otro inferior. Los spacers conservan el tamaño total de la lista y
permiten que la barra de scroll siga representando todo el historial.

Cada mensaje puede comenzar con una altura estimada a partir de la cantidad de
caracteres y el ancho disponible. Cuando se presenta, su altura real puede
guardarse para siguientes cálculos.

Si cambia el ancho de la superficie, las estimaciones deben invalidarse porque
la cantidad de caracteres por línea cambia.

Si cambia la sesión activa, deben limpiarse las alturas, el rango virtual y el
estado de interacción asociado al historial anterior.

## Streaming

El proveedor puede enviar una respuesta por partes. Durante ese flujo:

1. Se crea un mensaje Assistant activo.
2. Cada fragmento se añade al mismo mensaje.
3. El contenido de la tarjeta se actualiza sin crear una tarjeta nueva por
   token.
4. El resumen del historial puede actualizarse.
5. Cuando termina la respuesta, el runtime pasa a `Done`.

La UI debe distinguir entre cambiar el texto de un mensaje existente y cambiar
la estructura de la lista. Si el contenido puede cambiar la altura de la
tarjeta, la medida debe actualizarse; si solo cambia un valor de tamaño fijo,
puede actualizarse la pintura del nodo sin reconstruir el árbol entero.

Mientras existe salida viva, el coordinador solicita frames continuos con una
frecuencia limitada. Cuando el streaming termina, la UI vuelve al modo
event-driven.

## Resumen del historial

Debajo del encabezado o antes del historial se muestra un resumen. Cuando no
hay mensajes puede decir:

```text
No messages yet | max response {tokens}
```

Cuando hay mensajes puede decir:

```text
Messages {start}-{end} of {total} |
context approx. {tokens} tokens |
max response {max_tokens}
```

Los tokens son una estimación basada aproximadamente en cuatro caracteres por
token. El valor `max_tokens` proviene de la configuración del Agent.

## Suggestions

Si el proveedor y el modelo están listos, el frontend puede mostrar
sugerencias debajo del historial o antes del compositor.

Las sugerencias originales eran parecidas a:

- `Inspect {project_name}`.
- `Create a starter blockout`.
- `Explain the current scene`.

Una sugerencia no envía el mensaje automáticamente. Solo copia su texto al
campo de entrada para que el usuario pueda editarlo o enviarlo.

Si ya hay mensajes, el bloque de sugerencias puede ocultarse para dar más
espacio al historial.

## Compositor

La parte inferior de la superficie es el compositor del usuario.

Contiene:

- Un campo de texto multilinea.
- Un botón de envío o detención.

El campo admite aproximadamente `4,096` caracteres. Tiene placeholder
localizado y conserva el texto mientras el usuario edita.

El botón muestra `Send` cuando el runtime está inactivo y `Stop` cuando existe
una operación activa.

El botón Send debe estar habilitado solo cuando:

- El Agent está listo.
- El texto, después de quitar espacios exteriores, no está vacío.
- El runtime no está en un estado que bloquee una nueva solicitud.

El botón Stop cancela la operación en curso. La cancelación no debe borrar
automáticamente el historial ni el texto salvo que el controlador lo decida.

El envío por teclado debe usar la misma acción que el botón Send. No debe
existir un segundo camino con reglas distintas.

## Flujo de input

El Agent tiene un owner de input propio, distinto al owner del workbench.

Cuando la pestaña Agent está activa:

1. El workbench entrega primero el input a la superficie Agent.
2. La superficie ejecuta hit-test y focus sobre sus controles.
3. Los cambios de texto se convierten en acciones `SetInput`.
4. Los clicks se convierten en comandos de la superficie.
5. El coordinador transforma esos comandos en acciones del controlador.
6. El backend procesa las acciones y cambia el estado.
7. En el siguiente ciclo, la superficie vuelve a sincronizarse.

Mientras el puntero está dentro de la superficie Agent, el workbench inferior
no debe consumir pasivamente el wheel, hover o click-away que pertenezca al
Agent. Si otra superficie tiene pointer capture, esa superficie conserva la
prioridad.

Si Search está abierto, Search recibe el teclado antes que Agent.

## Acciones visuales

Las acciones principales del frontend son:

```text
SetInput(text)
ToggleSidebar
CloseSidebar
NewChat
SelectSession(index)
DeleteSession(index)
SelectModel(label)
SetMode(Passive | Active)
SetNewModelLabel(text)
SetNewModelId(text)
AddModel
OpenSettings
Submit
Stop
Approve
Deny
UseSuggestion(text)
```

Approve y Deny no deben ejecutar herramientas directamente desde la superficie.
El coordinador guarda la decisión y el runtime la consume en su siguiente
paso.

## Comandos de UI

Los comandos que puede emitir la superficie son equivalentes a:

```text
agent.sidebar.toggle
agent.sidebar.close
agent.new-chat
agent.model.menu
agent.mode.menu
agent.add-model.toggle
agent.add-model.cancel
agent.add-model.confirm
agent.add-model.settings
agent.settings
agent.submit
agent.stop
agent.approve
agent.deny
agent.session:{index}
agent.session.delete:{index}
agent.model.select:{label}
agent.mode.select:passive
agent.mode.select:active
agent.suggestion:{index}
```

Los comandos describen intención. La capa coordinadora decide cómo modificar
el controlador y qué frame o superficie debe invalidarse.

## Modos de ejecución

### Passive

En Passive, el proveedor puede proponer tools, pero el flujo se detiene en
`AwaitingApproval`. La UI muestra la tarjeta de aprobación.

Si el usuario pulsa Approve, el coordinador pasa la decisión al runtime y las
tools se ejecutan una por una.

Si pulsa Deny, el runtime recibe un resultado de rechazo y continúa con el
turno correspondiente.

### Active

En Active, las tool calls aprobadas por la política de este modo comienzan sin
mostrar la tarjeta de aprobación. La UI muestra una advertencia persistente o
visible mientras el modo está seleccionado.

El modo no cambia el proveedor ni el modelo. Solo cambia la política de
confirmación antes de ejecutar herramientas.

## Configuración visible

La sección AI de Settings reúne los valores del Agent y sus proveedores.

### Provider por defecto

Los proveedores visibles soportados por el editor son normalmente OpenRouter y
OpenAI. El usuario puede seleccionar cuál será el provider por defecto.

### Datos de cada provider

Cada tarjeta de proveedor puede contener:

- Enabled.
- Base URL.
- Model.
- API key.
- Mostrar u ocultar API key.
- Limpiar API key.
- Usar ese proveedor como default.

La API key debe tratarse como secreto. El frontend puede mostrarla como campo
password y solo revelarla bajo una acción explícita.

### Parámetros del Agent

La configuración general incluye:

- Modo `Passive` o `Active`.
- Streaming habilitado o deshabilitado.
- Máximo de tokens de respuesta.
- Persistencia de credenciales.
- Shortcuts de modelos.

Los valores de referencia eran:

```text
modo por defecto: Passive
streaming por defecto: true
max response tokens: 4096
rango de max response tokens: 1024 a 32768
```

El valor de tamaño de página del historial existía como configuración
heredada, pero el frontend visual usaba una lista continua con scroll y una
ventana automática.

## Modelo efectivo

Para crear una solicitud, el frontend necesita obtener el modelo efectivo en
este orden:

1. Si el usuario eligió el sentinel `provider_default`, usar el modelo de la
   configuración del provider por defecto.
2. Si eligió un shortcut, usar el provider y model ID de ese shortcut.
3. Si el provider está deshabilitado, mostrar `ProviderDisabled`.
4. Si el model ID está vacío, mostrar `ModelMissing`.
5. Si la URL no puede usarse con el adapter esperado, mostrar
   `AdapterRequired`.

La superficie solo presenta ese resultado. No debe intentar resolver secretos,
construir requests HTTP ni comprobar conectividad por su cuenta.

## Sesiones e historial

Cada conversación tiene:

- Un ID.
- Un título.
- Una lista de mensajes.
- Una fecha de actualización.

El historial se asocia al proyecto actual y se guarda en una ubicación como:

```text
<project>/.ai/agent_history.ron
```

La escritura puede hacerse de forma asíncrona para no bloquear el ciclo visual.

La primera pregunta del usuario puede convertirse en el título de la sesión,
con un límite visual aproximado de 42 caracteres.

Cambiar de proyecto debe:

- Cerrar o suspender la sesión visual anterior.
- Cargar las sesiones del nuevo proyecto.
- Seleccionar la sesión activa guardada.
- Limpiar el focus y el scroll de la superficie anterior.

## Contrato con el controlador

El frontend espera que el controlador pueda proporcionar:

- La sesión activa.
- La lista de sesiones.
- Los mensajes actuales.
- Readiness.
- Modelo y provider efectivos.
- Modo actual.
- Estado del runtime.
- Texto de error.
- Si existe una aprobación pendiente.
- Si existe streaming activo.
- Si existen sugerencias.

El frontend devuelve:

- Cambios de texto.
- Comandos de botones.
- Selección de sesión.
- Selección de modelo.
- Decisiones de aprobación.
- Solicitud de Settings.

El controlador devuelve el nuevo estado en la siguiente sincronización.

## Herramientas desde la perspectiva de la UI

La UI no necesita conocer la implementación de una tool. Solo necesita
representar:

- Que el Agent está pensando.
- Que está ejecutando cambios.
- Que necesita aprobación.
- Que terminó.
- Que falló.

El executor del editor traduce las tool calls a comandos del catálogo compartido
por el editor, CLI y MCP. Si una tool cambia la escena, el coordinador solicita
una actualización del canvas. Si solo cambia el texto del Agent, se actualiza
la superficie del Agent.

## Ciclo de sincronización

Un ciclo normal tiene esta forma:

1. El event loop recibe input o una señal del runtime.
2. El coordinador actualiza el tiempo y el snapshot de input.
3. El controlador Agent se prepara con Settings y el proyecto actual.
4. El runtime procesa una pequeña parte del trabajo pendiente.
5. La UI lee el estado actualizado.
6. La UI calcula el rango visible del historial.
7. La UI construye o reutiliza la superficie retenida.
8. El compositor presenta el Agent dentro del rect del bottom dock.
9. Se conserva focus, scroll, caret y selección para el siguiente frame.

Un frame sin cambios visuales no necesita reconstruir la superficie. Un cambio
de texto o de estado debe invalidar solo la parte necesaria.

## Focus y teclado

El campo `agent.input` puede recibir focus de teclado. El frontend debe:

- Mantener el caret.
- Soportar selección y edición multilinea.
- Permitir composición IME.
- Informar al host nativo el rectángulo del campo para ubicar el panel IME.
- Liberar el focus cuando el usuario hace click en otra zona.
- Restaurar el focus anterior cuando se cierra una búsqueda superpuesta.

Los popups deben capturar teclado mientras están abiertos. Un click fuera de un
popup puede cerrarlo, pero el click interno debe conservar la oportunidad de
emitir su acción antes de cerrar el popup.

## Localización

Los textos visibles deben usar claves de traducción. Las familias principales
de claves son:

```text
app.agent_tab
app.agent_title
app.agent_new_chat
app.agent_delete
app.agent_empty_title
app.agent_empty_subtitle
app.agent_input_placeholder
app.agent_send
app.agent_stop
app.agent_thinking
app.agent_executing_tools
app.agent_awaiting_approval
app.agent_error_label
app.agent_model_missing
app.agent_provider_disabled
app.agent_provider_adapter_required
app.agent_add_model
app.agent_mode_passive
app.agent_mode_active
```

Las etiquetas de rol también deben localizarse. El idioma de la superficie
debe coincidir con el idioma de Settings y del proyecto.

## Accesibilidad

Cada botón debe tener:

- Un nombre accesible.
- Focus por teclado.
- Estado visible cuando está seleccionado.
- Estado disabled cuando no puede ejecutarse.
- Tooltip solo cuando aporta información que no está visible.

La tarjeta de aprobación debe comunicar que necesita una decisión. El estado
de modo Active debe ser distinguible sin depender únicamente del color.

El contraste alto puede cambiar bordes y texto de controles interactivos. El
modo de movimiento reducido debe saltar las animaciones de sidebar y popup.

## Qué debe ser estable al recrearlo

Una recreación fiel debe mantener estas propiedades:

- El Agent vive dentro del bottom dock.
- La pestaña conserva el ID `agent`.
- Las conversaciones son sesiones separadas.
- El historial es continuo y tiene scroll.
- El streaming actualiza un mismo mensaje Assistant.
- Passive solicita aprobación.
- Active permite el flujo directo definido por el controlador.
- El modelo puede elegirse por shortcut.
- Se pueden crear shortcuts con label y model ID.
- Settings controla provider, modelo, credenciales y parámetros del Agent.
- El frontend emite intenciones y el controlador decide el efecto real.
- La ejecución de tools nunca debe estar implementada dentro del árbol visual.
