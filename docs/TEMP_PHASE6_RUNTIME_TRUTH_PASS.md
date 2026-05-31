# Fase 6A - Runtime Truth Pass Temporal

Fecha: 2026-05-27

Este documento NO implementa runtime. Este documento define la verdad actual del runtime, responde dudas de producto/arquitectura y deja una base tecnica para decidir cuando construirlo, rebotearlo o reformularlo.

La intencion de Fase 6A es simple:

- dejar de confundir renderer compartido con runtime jugable
- dejar claro que el runtime Game actual esta cortado
- decidir que partes SI deben existir antes de prometer Play/Run real
- definir una arquitectura modular para que el runtime futuro no nazca pegado a `app.rs`

---

## 1. Respuesta corta a la duda principal

### El runtime actual esta listo para desarrollarse ya mismo?

No como sistema serio.

Si se quiere un runtime liviano pero potente, tipo Roblox Studio pero aun mas ligero, primero hace falta cerrar contratos de arquitectura y herramientas. Hoy hay piezas utiles, pero el sistema de ejecucion como producto todavia no existe de forma honesta.

### Yo lo implementaria completo en `1.12` antes del release?

No completo.

Mi recomendacion seria esta:

- `Si` meteria `6A` antes de `1.12`: documentacion, honestidad de UI, definicion de capacidades, corte de promesas falsas.
- `Tal vez` meteria un `6B` minimo antes de `1.12` solo si el release necesita una demo real de Play Mode: start, stop, sandbox, tick, logs y input basico.
- `No` meteria un runtime completo antes de `1.12` si eso retrasa estabilidad del editor. Un runtime mal definido te rompe scene editing, scripting, physics, audio y debugging al mismo tiempo.

Conclusion: antes de `1.12`, si. Pero solo la parte honesta y el contrato tecnico. No intentaria vender runtime completo antes de que exista una base modular real.

---

## 2. Verdad actual del runtime en este repo

### 2.1. Donde esta hoy el runtime cortado

Los puntos mas importantes hoy son estos:

- `crates/raf_editor/src/game_runtime.rs`
- `crates/raf_editor/src/app.rs`
- `crates/raf_core/src/project.rs`
- `crates/raf_editor/src/script_support.rs`
- `crates/raf_core/locales/en.json`
- `crates/raf_core/locales/es.json`

### 2.2. Que hace realmente hoy

#### `crates/raf_editor/src/game_runtime.rs`

Hoy `GameRuntimeState` es practicamente un stub:

- `RuntimeInputState::from_egui(...)` no traduce input real
- `GameRuntimeState::start(...)` solo clona `SceneGraph`
- `GameRuntimeState::update(...)` devuelve `RuntimeReport::default()`
- no hay loop real de runtime
- no hay scheduler de sistemas
- no hay capa de servicios
- no hay estados de play session
- no hay integracion real con audio, physics, scripts o nodos en ese archivo actual

Eso significa que existe una forma de nombrar el runtime, pero no una ejecucion jugable.

#### `crates/raf_editor/src/app.rs`

En `handle_build()` la rama `ProjectType::Game` no ejecuta play mode real. Lo que hace es:

- poner `self.runtime = None`
- escribir el mensaje `app.runtime_temporarily_disabled`

Eso es importante porque confirma que el runtime de juego hoy esta intencionalmente desconectado.

#### Locales actuales

Todavia existen mensajes como:

- `app.runtime_started`
- `app.runtime_stopped`
- `app.runtime_scene_locked`

O sea: la UI ya habla el idioma de un runtime real, pero el flujo principal de `Game` hoy no lo sostiene.

### 2.3. Que piezas SI existen y sirven para el futuro runtime

Aunque el runtime real este cortado, el repo SI tiene piezas reutilizables:

- `ProjectSettings` ya tiene toggles de runtime como `enable_audio`, `enable_physics`, `pause_when_unfocused` y `runtime_render_preset`
- `SceneGraph` ya existe como documento editable y tambien como candidata a snapshot de sandbox
- `nodes.ron` ya existe como persistencia del grafo visual
- el repo memory confirma que el flujo actual apunta a Rhai + nodos como base runtime ligera
- `script_support.rs` ya escanea scripts y detecta `on_start` / `on_update`
- el renderer compartido ya existe para viewport y superficies editoriales

La lectura correcta no es "no hay nada".

La lectura correcta es:

> hay infraestructura dispersa, pero no hay todavia un runtime modular, coherente y verdadero

---

## 3. Contradiccion actual que Fase 6A tiene que dejar por escrito

El `CHANGELOG` historico dice que hubo flujo integrado de Play/Stop, sandbox, nodos, Rhai, physics y audio en `0.8.0`.

Pero el estado actual del repo muestra otra realidad:

- el runtime Game esta temporalmente deshabilitado
- `game_runtime.rs` no representa un sistema de simulacion completo
- la UI y las traducciones todavia conservan huellas del flujo anterior

Esto no significa que el changelog este "mintiendo" necesariamente. Puede significar cualquiera de estas tres cosas:

1. existio un runtime previo y luego se recorto mientras se rehacia renderer/editor
2. parte del flujo existio, pero fue simplificado o desconectado
3. la arquitectura del runtime se adelanto en comunicacion respecto al estado actual del codigo

Para Fase 6A eso no importa tanto como una regla practica:

> desde ahora, el editor no debe prometer runtime real si el flujo actual no puede sostenerlo extremo a extremo

---

## 4. Como deberia pensar el engine el runtime futuro

Si quieres algo "liviano y complejo como Roblox Studio, pero aun mas liviano", yo NO intentaria copiar Roblox Studio. Yo intentaria copiar solo sus virtudes utiles:

- sandbox separado del documento editable
- servicios claros
- play/stop instantaneo
- logs y errores entendibles
- capacidades expandibles sin inflar el core

Y evitaria sus costos:

- sistemas gigantes acoplados entre si
- APIs demasiado magicas
- demasiada deuda interna por soportar mil modos a la vez

La idea correcta aqui seria:

> un runtime pequeno en el core, y complejo por capas

Eso significa:

- core runtime pequeno
- servicios opcionales encima
- tooling de editor separado
- diagnostico fuerte
- scripting primero simple y confiable
- expansion gradual a physics/audio/AI/networking sin romper el nucleo

---

## 5. Que deberia existir antes de trabajar un runtime serio

Esta es la parte mas importante del documento.

Antes de programar runtime serio, yo tendria lista esta base:

### 5.1. Contrato de estados del runtime

El runtime no puede ser solo `Some(GameRuntimeState)` o `None`.

Necesita un estado explicito. Ejemplo:

- `Disabled`
- `Unavailable { reason }`
- `Ready`
- `Starting`
- `Running`
- `Paused`
- `Stopping`
- `Crashed { error }`

Si eso no existe, la UI va a seguir mintiendo o mostrando cosas ambiguas.

### 5.2. Sandbox claro

El runtime debe correr sobre una copia de trabajo, no sobre el documento editable principal.

Minimo necesita:

- snapshot de escena de entrada
- world/runtime scene separado
- forma de descartar el sandbox al parar
- opcion futura de inspeccionar diferencias si algun dia quieres debugging avanzado

### 5.3. Input real

Hoy `RuntimeInputState` no representa nada serio. Antes de runtime hay que decidir:

- teclado
- mouse
- wheel
- botones por frame
- estados hold/pressed/released
- input actions mapeables a futuro
- foco de viewport vs foco de editor

Si esto no esta definido, scripts, nodos y cameras van a comportarse distinto cada vez.

### 5.4. Tiempo y tick

El runtime necesita un contrato de tiempo real:

- delta time
- fixed step opcional para physics
- max delta clamp
- pause/unfocus policy
- tick order estable

Sin eso no hay simulacion repetible ni debug util.

### 5.5. Consola y logs de runtime

Tu pregunta sobre consola/terminal/debug es clave.

Yo prepararia DOS capas, no una sola:

#### Consola in-editor

Debe existir siempre. Es la fuente principal para:

- logs de gameplay
- warnings de assets faltantes
- errores de script/nodos
- mensajes de physics/audio/input
- trazas de play/stop/pause/reload

Y debe tener:

- severidades (`info`, `warn`, `error`, `debug`)
- categoria (`runtime`, `script`, `audio`, `physics`, `nodes`, `assets`, `camera`)
- timestamp o frame index
- filtro por origen
- boton para limpiar

#### Terminal externa

No la haria obligatoria para el runtime basico.

La terminal es util para:

- procesos externos
- herramientas de build
- compilacion futura de lenguajes pesados
- export pipelines
- bots o toolchains auxiliares

Pero un runtime liviano tipo editor debe poder diagnosticarse primero desde su propia consola interna.

Conclusion:

- consola interna: obligatoria
- terminal externa: opcional/auxiliar

### 5.6. Sistema de reportes y errores

`RuntimeReport` hoy solo tiene `logs` y `errors`, pero eso es poco.

Yo prepararia:

- `RuntimeEvent`
- `RuntimeLogEntry`
- `RuntimeError`
- `RuntimeWarning`
- `RuntimeStatus`
- `RuntimeDiagnosticsSnapshot`

Y algo clave: codigos de error.

Ejemplo:

- `RT-BOOT-001` - fallo al crear sandbox
- `RT-SCRIPT-001` - script no encontrado
- `RT-SCRIPT-002` - lenguaje no soportado en runtime actual
- `RT-NODE-001` - `nodes.ron` invalido
- `RT-PHYS-001` - physics deshabilitado pero requerido
- `RT-ASSET-001` - asset faltante
- `RT-CAM-001` - ninguna camera principal valida

Eso te evita logs vagos tipo "algo fallo".

### 5.7. Cameras principales del proyecto

Esto debe definirse ANTES del runtime, no despues.

El runtime necesita saber:

- cual camera es principal
- que pasa si no hay ninguna
- que pasa si hay varias marcadas como principales
- si la editor camera se reutiliza al iniciar play mode o no
- como se hace fallback

Yo haria esta regla:

- si hay una `MainCamera` valida, el runtime usa esa
- si no hay ninguna, fallback controlado a una camera runtime temporal y warning visible
- nunca usar la camera del editor como camera de gameplay real salvo modo debug explicito

Tambien prepararia desde ya:

- camera stack simple
- free debug camera opcional
- camera cut / blend como futura capa, no como requisito del primer runtime

### 5.8. Audio y physics como servicios, no como flags sueltos

Hoy `ProjectSettings` ya tiene `enable_audio` y `enable_physics`.

Eso esta bien, pero para runtime serio hace falta traducirlos a servicios concretos:

- `AudioService`
- `PhysicsService`

Si el flag existe sin servicio, la UI promete mas de lo que corre.

### 5.9. Scripts

`script_support.rs` acepta muchos lenguajes a nivel catalogo, pero eso NO significa que deban entrar al runtime al mismo tiempo.

Mi recomendacion para runtime liviano:

- arrancar con `Rhai` como primer lenguaje real de runtime
- tratar Rust/C++ como experimentales o fuera de runtime live hasta que exista pipeline serio
- dejar Lua/Python/JS/TS solo como catalogo o tooling hasta que haya arquitectura clara

Ademas, el repo memory deja una restriccion importante:

- en este workspace Windows GNU no conviene apostar a runtimes C-backed para el primer corte

Entonces el camino limpio es:

- `Rhai first`
- todo lo demas despues

### 5.10. Nodos

Los nodos no deberian vivir "dentro" de `app.rs` ni de `game_runtime.rs` como parche.

El runtime necesita un `NodeRuntimeBridge` o equivalente que haga esto:

- cargar `NodeEditorDocument`
- validar grafo
- registrar eventos `On Start` / `On Update`
- ejecutar sin depender del editor UI
- reportar errores al sistema de diagnostico

### 5.11. Assets y rutas

Antes de runtime real hace falta decidir:

- resolucion de rutas de assets desde sandbox
- que pasa si assets faltan
- politica de hot reload en play mode
- si scripts se recargan al vuelo o no
- que assets son solo editor y cuales son runtime

### 5.12. Herramientas de debug

Si quieres un runtime "ligero pero complejo", el secreto no es tener mil features, sino tener debug suficiente.

Yo tendria listos estos paneles o overlays antes de escalar demasiado:

- runtime state
- frame time / fixed step
- camera activa
- entidades runtime activas
- errores recientes
- scripts activos
- grafo de nodos cargado
- servicios activos/desactivados

No necesitas un profiler gigante al principio. Necesitas visibilidad minima correcta.

---

## 6. Que NO intentaria meter en la primera version real

Para mantenerlo liviano, yo NO meteria esto en el primer runtime serio:

- multiplayer
- replicacion de red
- varios lenguajes runtime a la vez
- editor camera y gameplay camera hiper mezcladas
- inspector en caliente tipo AAA
- asset hot reload universal para todo
- debugger visual gigante estilo engine enterprise

El error clasico seria querer construir Roblox Studio completo antes de tener un Play Mode verdadero y estable.

---

## 7. Como deberia ser la arquitectura modular futura

### 7.1. Regla principal

El runtime no deberia quedarse resumido en `crates/raf_editor/src/game_runtime.rs`.

Ese archivo hoy sirve como stub o puerta de entrada, pero el runtime serio debe dividirse en multiples archivos y, en una etapa posterior, probablemente en un crate propio.

### 7.2. Recomendacion de corto plazo

Si quieres avanzar sin una migracion brutal, yo haria primero una modularizacion local dentro de `raf_editor`.

Ejemplo:

- `crates/raf_editor/src/runtime/mod.rs`
- `crates/raf_editor/src/runtime/session.rs`
- `crates/raf_editor/src/runtime/state.rs`
- `crates/raf_editor/src/runtime/input.rs`
- `crates/raf_editor/src/runtime/report.rs`
- `crates/raf_editor/src/runtime/errors.rs`
- `crates/raf_editor/src/runtime/sandbox.rs`
- `crates/raf_editor/src/runtime/camera.rs`
- `crates/raf_editor/src/runtime/diagnostics.rs`
- `crates/raf_editor/src/runtime/capabilities.rs`
- `crates/raf_editor/src/runtime/services/mod.rs`
- `crates/raf_editor/src/runtime/services/audio.rs`
- `crates/raf_editor/src/runtime/services/physics.rs`
- `crates/raf_editor/src/runtime/services/scripts.rs`
- `crates/raf_editor/src/runtime/services/nodes.rs`

Y `game_runtime.rs` quedaria como compat layer temporal o facade.

### 7.3. Recomendacion de mediano plazo

Cuando el runtime ya sea real, yo si evaluaria moverlo a crate propio:

- `crates/raf_runtime/`

Motivo:

- separa editor host de runtime host
- reduce acoplamiento con egui
- hace mas facil testing
- permite pensar export/build futuro sin llevarte el editor entero

---

## 8. Como deberia manejar el engine el runtime

Si lo resumiera en una sola frase:

> el engine debe tratar el runtime como un host de simulacion con servicios, no como una extension improvisada del viewport

Eso cambia mucho el enfoque.

### 8.1. Flujo ideal

1. El editor prepara snapshot del proyecto.
2. Se construye un sandbox runtime.
3. Se validan capacidades requeridas.
4. Se inicializan servicios segun `ProjectSettings`.
5. Se resuelven cameras y assets.
6. Se disparan eventos `On Start`.
7. Comienza loop `Update`.
8. Los diagnosticos alimentan consola y overlays.
9. `Stop` destruye sandbox y devuelve control limpio al editor.

### 8.2. Servicios que yo modelaria desde el principio

- `TimeService`
- `InputService`
- `SceneService`
- `CameraService`
- `ScriptService`
- `NodeService`
- `DiagnosticsService`
- `AssetResolver`

Y como opcionales por feature gate:

- `AudioService`
- `PhysicsService`
- `AnimationService`
- `NetworkService`

---

## 9. Lista de cosas que deberian estar listas antes de desarrollar runtime real

Checklist concreta:

- contrato de estados del runtime
- matriz de capacidades
- sandbox scene separado
- input model real
- time/fixed-step policy
- camera principal/fallback policy
- consola runtime con categorias
- codigos de error
- service boundaries claras
- Rhai como primer runtime real
- puente limpio para nodos
- asset resolution policy
- logs de boot/start/stop/crash
- smoke tests basicos

Si faltan mas de 3 o 4 de esas cosas, todavia no conviene prometer runtime serio.

---

## 10. Que deberia contener Fase 6B si algun dia se hace

Solo como referencia futura:

- `Play` real para Game
- `Stop` real
- scene clone real
- input basico real
- logs reales de runtime
- `Rhai on_start/on_update` real
- status visible de runtime
- bloqueo de edicion solo cuando de verdad corra el sandbox

Eso ya seria un runtime minimo verdadero.

---

## 11. Cuando conviene rebotear o reformular el runtime

Yo reformularia el plan si pasa cualquiera de estas cosas:

- el runtime sigue dependiendo demasiado de `app.rs`
- editor y runtime se pisan el mismo `SceneGraph` vivo
- la UI necesita demasiadas excepciones para explicar estados raros
- scripts y nodos se ejecutan por caminos paralelos incompatibles
- audio/physics entran como hacks en vez de servicios
- para arrancar Play Mode hacen falta demasiadas condiciones invisibles

Si aparece alguno de esos sintomas, no conviene "seguir parchando". Conviene frenar y mover el runtime a un diseño mas modular.

---

## 12. Respuesta final y posicion tecnica

Mi posicion, basada en el repo actual, es esta:

- `6A` si debe existir ya, antes de `1.12`, porque evita seguir construyendo sobre una promesa falsa.
- runtime completo antes de `1.12`: no lo recomendaria.
- runtime minimo, liviano y verdadero: si podria entrar despues de esta fase, pero solo si se hace modular y con Rhai/nodos como primer eje real.
- el runtime actual hoy debe considerarse un punto de arranque conceptual, no una base suficiente para escalar.

Si algun dia se construye bien, no deberia ser un solo archivo. Deberia ser un sistema modular con varios archivos, y probablemente despues un crate propio.

Ese es el punto real de esta Fase 6A:

> no hacer mas grande el runtime actual, sino dejar claro que partes estan cortadas, que partes sirven y que condiciones deben cumplirse antes de reactivarlo o redisenarlo.
