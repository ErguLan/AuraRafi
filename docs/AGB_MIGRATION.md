# ApiGraphicBasic: corte de producto

El contrato retenido, su matriz de cobertura y el arranque nativo viven en `raf_agb`. El editor, el player, el streaming y el host web ya abren y componen por AGB. WGPU fue retirado por completo en 0.11.5: no hay feature `reference`, ni dependencia, ni ejecutor de referencia.

## Decisiones de arquitectura

- ApiGraphicBasic posee los comandos, los recursos, la planificación y el ciclo de presentación. Los ejecutores de Vulkan, DX12, Metal y WebGPU implementan ese contrato bajo AGB. Cada dispositivo y sus superficies usan un solo ejecutor.
- Reusar los productores y hosts que existen. En el corte, escena, RafUI y CAD emiten o consumen el contrato común directamente. No introducir las capas intermedias `AgbSceneBackend`, `AgbUiBackend`, `AgbCadBackend`, `AgbCompositor` o `AgbWindowHost`, ni mantener dos renderers por superficie. La lógica por API queda en los ejecutores nativos.
- `NativeRequest::Auto` elige una API nativa compilada y capaz de servir la salida solicitada. Cada API detecta su GPU. `NativeRequest::Exact` exige la API elegida. Se resuelve una vez al abrir y una recuperación conserva la API activa. Sin fallback a WGPU. `RAF_GRAPHICS_BACKEND` sigue siendo una opción del banco.
- El producto compila los ejecutores nativos directamente: Vulkan en todas las plataformas de escritorio, DX12 en Windows y Metal en Apple. En Windows, `Auto` prioriza Vulkan para ventana y DX12 para captura offscreen. WGPU no existe en la build: el A/B se hace entre ejecutores nativos (Vulkan contra DX12).

## P0. Contrato de producto

| Área | Base del banco original | Condición de integración al producto |
| --- | --- | --- |
| Geometría y ejecución | `Workload` retiene vértices con posición, UV y color, instancias, texturas y rangos. El producto ya tiene `BasicVertex`, mallas indexadas y `GraphicCommand`. | Reutilizar los tipos neutrales que ya expresan el trabajo. Separar layouts de malla iluminada, línea y UI. Incluir normales en mallas iluminadas, índices e instancias. Asociar material y texturas al draw o la instancia según corresponda. El paquete serializable `Workload` queda para fixtures. |
| Estado y shaders | El banco usa estados fijos de escena u overlay. El producto usa `BasicPipelineKind`. | Definir pipeline, topología, blend, profundidad de lectura y escritura, culling, formatos y samples. Acordar coordenadas de viewport y scissor. Empaquetar shaders por ejecutor y reutilizar pipelines por descripción y revisión. |
| Datos del frame | El banco pasa `view_projection` y clear. Vulkan y WebGPU ya detectan cambios de cámara. | Pasar cámara, viewport, datos de luz, entorno, sombras y parámetros temporales de los pases que los consumen. Conservar profundidad, sentido de caras, origen de texturas, color lineal/sRGB y convención de alfa. |
| Identidad y actualizaciones | AGB tiene arena con handles generacionales por slot, ledger y presupuestos. `Device::upload` sube un paquete completo y `release` espera al backend. | Vincular los handles al dispositivo y su generación. Resolver recursos persistentes por identidad y revisión del origen. Actualizar rangos de buffer y rectángulos de textura o atlas. Presupuestar staging y recursos antes de reservar. Retirar asignaciones tras la señal de finalización del último frame que las usa y acotar los frames en vuelo. Si falla un reemplazo, conservar el recurso vigente y el saldo del ledger. |
| Ingreso de assets | El producto ya descubre y decodifica texturas mediante asset ingress. El banco sube los bytes de su fixture. | Conservar el ingreso de assets y transferir a AGB la subida y residencia por recurso. Probar recarga, evicción bajo presión, miniaturas y rehidratación con presupuestos separados para CPU y GPU. |
| Grafo de pases | `FrameGraph` compila declaraciones, orden, estados y aliasing. `FramePlan::new` y los ejecutores del banco implementan escena, overlays y salida. | Usar el grafo compilado para ejecutar los pases del producto con sus recursos y barreras. Cubrir profundidad/sombras, escena, filtro, CAD, UI y captura o presentación. Reutilizar recursos transitorios compatibles. Los pases apagados no reservan ni ejecutan trabajo. |
| Superficie y recuperación | `Device::configure` conoce tamaño y tipo de salida. Existen errores de superficie y dispositivo. El banco Vulkan ya reconfigura y reconstruye desde su host. | Exponer un ciclo común para adjuntar superficie, adquirir, presentar, redimensionar, suspender y restaurar. Recrear solo la superficie cuando el dispositivo sigue válido. Si se pierde el dispositivo, invalidar sus handles, rehidratar recursos desde sus fuentes y diagnosticar la recuperación. |
| Captura y métricas | El banco captura RGBA y mide submits, draws, bytes subidos y GPU ms cuando están disponibles. | Definir formato, orientación y dimensiones de captura. Separar envío, espera y readback para mantener libre el hilo de interacción. Conservar el diagnóstico del runtime y distinguir presupuesto lógico de memoria física observada. |

`BasicCommandList` ya no contiene vistas de textura de la ruta anterior. Los consumidores de `SharedGraphicsContext` y `GpuTextureView` desaparecieron: las texturas de escena llegan como RGBA de CPU y el paquete de producto las sube. `SceneRenderer` conserva visibilidad, LOD, orden y armado de escena. RafUI conserva layout e interacción. CAD conserva operaciones y picking. Sus pases comparten dispositivo y se componen mediante handles.

[Comandos actuales](../crates/raf_render/src/ApiGraphicBasic/command_list.rs) · [Malla](../crates/raf_render/src/ApiGraphicBasic/mesh.rs) · [Pipelines](../crates/raf_render/src/ApiGraphicBasic/pipeline.rs) · [Device del banco](../crates/raf_agb/src/device.rs) · [Arena](../crates/raf_agb/src/resource_registry.rs) · [Grafo](../crates/raf_agb/src/frame_graph.rs)

**Ejemplo del contrato esperado.** Orbitar invalida cámara y visibilidad. Las mallas y texturas conservan sus handles y siguen residentes. Si RafUI añade un glifo, sube solo el rectángulo cambiado del atlas. Si se reconstruye el dispositivo, los handles anteriores se rechazan y las identidades de origen permiten reconstruir los recursos vigentes.

### Uso del contrato implementado

- Construir un `ProductPacket` con comandos en orden, recursos y materiales. `ProductDevice::prepare` recibe una clave y revisión del origen. La clausura se ejecuta solo cuando cambia la revisión. Mantener una sola composición por dispositivo. Conservar las fuentes para reconstrucción.
- Pasar `FrameUniforms` al dibujar. Contiene cámara, luz, ambiente RGB, exposición, sombras, filtro y el viewport del canvas. Para el fondo usar `DrawSky` o `SceneBackground` antes de la geometría. Sombras y filtro están implementados en Vulkan y DX12 y se consultan mediante `supports_shadows` y `supports_scene_filter`; el compositor las desactiva si el ejecutor no las soporta.
- Usar `mesh_span` y `instance_span` para localizar rangos de vertex/instance en el paquete preparado. El tint de instancia en GPU incluye el multiplicador del material. Para modificar un atlas o imagen, usar `ResourcePatch::Texture` con rectángulo y RGBA ajustado al ancho del rectángulo. `patch` acepta la revisión esperada y avanza la revisión solo si la operación se acepta. Los cambios de topología o tamaño pasan por `prepare`.
- Conservar el mismo `PacketHandle` al actualizar parcialmente y renovar el handle al reemplazar el paquete. Mantener un presupuesto suficiente para admitir ambas revisiones durante el reemplazo. Vulkan y DX12 esperan el fence de la copia parcial, Metal espera el blit o escribe en almacenamiento compartido al terminar el trabajo anterior, y WebGPU encola la escritura. Una cola de staging asíncrona y captura de producto quedan pendientes.
- `Output::Present` y `Output::Capture` usan el mismo contrato. `capture` sigue siendo una lectura bloqueante para herramientas. Instanciar un dispositivo nuevo invalida los handles anteriores. El arranque Vulkan vuelve a preparar su pequeña pantalla al reconstruir.

[API del producto](../crates/raf_agb/src/product.rs) · [Comandos y recursos](../crates/raf_agb/src/product/contract.rs) · [Matriz consultable](../crates/raf_agb/src/product/coverage.rs) · [Selección nativa](../crates/raf_agb/src/backends/native.rs)

## P0. Matriz de cobertura

**Implementado** significa ejecución a través de `ProductDevice`. **Parcial** significa que se ejecuta un subconjunto explícito. **Falta** significa que AGB rechaza la operación con `Error::Unsupported`. Al verificar cada fila, registrar backend, target, fixture, resultado y artefactos. Una capacidad obligatoria ausente bloquea el corte de esa plataforma. Una capacidad opcional devuelve un rechazo explícito al perfil o al consumidor.

Antes de integrar, comprobar el contrato con entradas retenidas y casos de prueba equivalentes a las filas de la matriz. Durante el corte, repetir con los productores en vivo y la ruta real de inicio. El mismo caso puede estar cubierto en un target y pendiente en otro.

### Escena

| Comando de BasicCommandList | Estado en AGB hoy | Prueba requerida |
| --- | --- | --- |
| `Clear` | Parcial. Clear de inicio. Un clear posterior se rechaza. | Color, alfa y comportamiento al preservar o limpiar color y profundidad entre pases. |
| `SetPipeline` | Implementado. Variantes `SCENE`, `LIT` y `OVERLAY` con alfa recto. `UiSurface`/`UiSurfaceOverlay` se asignan a `OVERLAY` para paneles de UI en el mundo. Los estados sin topología de producto se rechazan con su nombre. | Variantes de `BasicPipelineKind`, caras dobles, oclusión, profundidad, transparencia y UI en escena. |
| `SetShadowReceiver` | Implementado por instancia en Vulkan y DX12. Cuando el pase está activo, la malla marcada recibe sombras. | La misma malla recibe o ignora la sombra según el estado. |
| `DrawMesh` | Implementado. Malla indexada con normales, material y luz difusa. | Malla indexada con normales, material, textura, transformaciones y luz del perfil. |
| `DrawMeshBatch` | Implementado. Rangos e instancias residentes, agrupación de draws adyacentes. | Rangos e instancias correctos. Cambiar cámara sin reupload de datos estáticos. |
| `DrawLine` | Implementado. Grosor en píxeles, clipping de plano cercano, bias y profundidad opcional. | Ancho, depth test y depth bias en línea ocluida y gizmo superpuesto. |
| `DrawLineBatch` | Implementado. Grosor en píxeles, clipping de plano cercano, bias y profundidad opcional. | Mismas semánticas de línea con batching y orden preservado. |
| `DrawScreenTriangleBatch` | Implementado en clip space, con pipeline overlay y recorte al viewport de escena. | Coordenadas, clipping y orden respecto a transparencias y UI. |
| `DrawGrid` | Implementado. Cuadrícula acotada alrededor de la cámara, con líneas principales y secundarias. | Grid a distintos zoom y ángulos, incluida la variante superpuesta. |
| `SceneBackground` | Implementado. Triángulo fullscreen de color lineal, solo en el pase de escena y recortado al viewport. | Fondo de canvas dentro de un rectángulo sin tocar el chrome del editor. |
| Viewport de escena | Implementado. `FrameUniforms.viewport` normalizado; el pase de escena lo usa y los overlays conservan el target completo. | Canvas en sub-rectángulo, resize y DPI. |
| Entorno, luces, sombras y filtro | Parcial. Cielo degradado o equirectangular, ambiente RGB y exposición. Vulkan y DX12 agregan shadow map direccional y filtro espacial de escena. Metal y WebGPU aún no tienen esos pases; mapa de radiancia para IBL pendiente. | Modo neutro y escena con entorno, sombras y filtro. Registrar pases y coste por perfil. |

[Frame de escena](../crates/raf_render/src/scene_renderer.rs) · [Ejecución actual](../crates/raf_render/src/ApiGraphicBasic/device/frame.rs) · [Exportador](../crates/raf_render/src/ApiGraphicBasic/ui_surface/native_workload.rs)

### RafUI

| Pintura | Estado en AGB hoy | Prueba requerida |
| --- | --- | --- |
| `Solid` | Implementado. Quad sólido y `StyledPaint` con radios por esquina, borde por lado con color propio, gradiente lineal de dos colores y feather acotado. | Rellenos y bordes al DPI físico. Reutilizar geometría y estado al repintar. |
| `Stroke` | Implementado. Trazo recto con ancho por eje, gradiente y feather; acepta clips redondeados anidados. Radios de esquina y bordes por lado se rechazan con `Error::Unsupported`. | Marcadores finos y ancho consistente a 100, 125, 150 y 200% de escala. |
| `Text` | Implementado. Atlas RGBA, tint, clips redondeados y actualización por rectángulo mediante `TexturePatch`; el productor resuelve los glifos. | Glifos nítidos, clipping y tint. Medir bytes subidos al aparecer un glifo nuevo. |
| `Image` | Implementado. Texture, UV, tint, contain/cover/stretch, recorte al quad y nine-slice. Borde y gradiente se emiten alrededor de la imagen; feather en una imagen con fit o nine-slice se rechaza. | `fit`, crop, sampling, tint y nine-slice. Mantener las imágenes persistentes al repintar. |
| Efectos y orden | Implementado. Composición retenida en el orden recibido. Gradiente lineal de dos colores, feather con kernel acotado y hasta nueve clips redondeados, incluido el borde propio de una imagen. | Orden por `z_index` y secuencia, clips anidados, alfa, gradientes, menús, tooltips y foco. Comprobar el perfil opaco de bajo consumo cuando corresponda. |

[Comandos de pintura](../crates/raf_render/src/ApiGraphicBasic/ui_surface/presentation.rs) · [Efectos](../crates/raf_render/src/ApiGraphicBasic/ui_surface/effects.rs) · [Contrato de RafUI](RAF_UI.md)

### Esquemático y PCB

Las siete capas aceptan pintura 2D plana y triángulos de pantalla vía `Command::Cad` y `CadTriangles`. El productor conserva zoom, densidad, visibilidad y orden. La aceptación final requiere las siete capas con el frame vivo de CAD.

| CadLayerKind | Estado en AGB hoy | Prueba requerida |
| --- | --- | --- |
| `Schematic` | Parcial. Primitivas 2D con etiqueta de capa. | Símbolos, pines, cables y etiquetas conservan posición y legibilidad. |
| `PcbTopCopper` | Parcial. Primitivas 2D con etiqueta de capa. | Anchos, rellenos, formas y perforaciones en pads y trazas superiores. |
| `PcbBottomCopper` | Parcial. Primitivas 2D con etiqueta de capa. | Geometría, visibilidad y orden frente a cobre superior y serigrafía. |
| `PcbSilkscreen` | Parcial. Primitivas 2D con etiqueta de capa. | Texto y contornos de serigrafía conservan forma y escala. |
| `Airwire` | Parcial. Primitivas 2D con etiqueta de capa. | Conexiones cambian con el documento y respetan zoom y orden. |
| `Overlay` | Parcial. Primitivas 2D con etiqueta de capa. | Selección, resaltado, guías y contornos donde los emite CAD. |
| `Drc` | Parcial. Primitivas 2D con etiqueta de capa. | Los marcadores siguen la geometría y conservan su prioridad. |

Mantener IDs, coordenadas, ancho físico y orden del frame. Comprobar que un mismo punto elige el mismo objeto mediante `cad_interaction::pick_editable` donde ya se usa. El dominio conserva el picking y AGB ejecuta su representación. [Capas CAD](../crates/raf_electronics/src/cad_scene.rs) · [Frame CAD](../crates/raf_render/src/ApiGraphicBasic/cad_surface.rs) · [Host CAD](../crates/raf_render/src/ApiGraphicBasic/cad_surface_host.rs)

### Composición, salida y plataformas

| Ruta | Evidencia actual | Qué falta para calificar |
| --- | --- | --- |
| Escena/CAD + RafUI | Un paquete compone escena iluminada, líneas, grid, fondo de canvas, CAD y pintura retenida. | Medir el recoste de UI en frames activos y añadir actualizaciones parciales si el perfil lo exige. |
| Vulkan | Arranque con `ProductDevice`, ventana, sub-viewport, captura, sombras y filtro. | Verificar el producto en los hosts Vulkan previstos. |
| DX12 | Swapchain Win32, captura offscreen, sub-viewport, sombras y filtro. | Verificar el producto en Windows con una captura real de sombras. |
| Metal | Presentación por CAMetalLayer en AppKit/UIKit, captura offscreen y sub-viewport. | Integrar el producto Apple y portar shadow map y filtro. |
| WebGPU | Host web de producto sobre canvas y `ProductDevice<WebGpu>` con sub-viewport. | Portar shadow map y filtro; el host ya dibuja el juego exportado. |
| Player, captura y streaming | Headless y ventana de producto por AGB; captura RGBA después del draw. | Repetir la evidencia de simulación e input con el proyecto real y medir readback fuera del hilo interactivo. |
| Recorder y referencia | Validación de comandos y A/B del banco. | Recorder cubre el contrato y sus errores sin GPU. El A/B compara Vulkan contra DX12 en procesos separados. |

Vulkan es el ejecutor principal de ventana en Linux y Windows. DX12 cubre ventana y captura en Windows; `Auto` prefiere Vulkan para presentar y DX12 para capturar. Metal cubre Apple y WebGPU el navegador. El contrato separa capacidades y presentación para poder incorporar otros targets. Las consolas quedan fuera de la cobertura verificada.

[Pruebas y hardware del banco](AGB_NATIVE_PROBE.md) · [Host web actual](../crates/raf_web/src/app.rs) · [Player headless](../crates/raf_player/src/headless.rs) · [Streaming](CLOUD_STREAMING.md)

### Capacidades del corte

- Los productores emiten el orden de escena, CAD y UI directamente. El fondo se dibuja primero, las líneas y el grid pertenecen a la escena y el canvas de CAD y RafUI se compone al final. En RafUI, conservar el orden del `UiSurfaceDrawList` al construir `StyledPaint`. Las medidas de borde, radio y feather se expresan normalizadas por eje. Nine-slice lleva insets de origen en texels e insets de destino normalizados.
- El cielo de escena usa un triángulo recortado por el viewport. El shader obtiene la dirección del cielo de la proyección y evalúa gradiente, disco solar, halo y estrellas. El cielo procedural no necesita un destino intermedio. La posición de cámara para efectos dependientes de distancia viene del productor de escena.
- El mapa direccional en Vulkan y DX12 admite 256, 512 o 1024 por lado. El producto usa la lista de proyectores opacos que prepara la escena, con un sesgo de profundidad dependiente del ángulo y la resolución. Los receptores conservan su ambiente. Cambiar color o filtro mantiene el mapa mientras su proyección y proyectores tengan la misma identidad. El filtro espacial de cinco muestras sigue disponible como pase opcional del ejecutor. Si el dispositivo no admite un efecto o excede el presupuesto, el producto puede continuar dibujando el frame.
- Una sesión renderiza y presenta con su dispositivo activo. El editor compone escena, CAD y RafUI en un packet y dibuja con un solo `ProductDevice`. El player headless captura después del draw; el streaming y el host web usan la misma ruta. Metal y WebGPU aún no implementan sombras ni filtro de escena.

[Estilos retenidos](../crates/raf_agb/src/product/paint_style.rs) · [Shadow map](../crates/raf_agb/src/backends/vulkan/shadow.rs) · [Filtro](../crates/raf_agb/src/backends/vulkan/filter.rs)

## P1. Arranque nativo aislado

El banco presenta una pantalla pequeña con un fondo, tarjeta, atlas e imagen mediante el contrato nuevo. El código de arranque ofrece Vulkan, DX12 con Win32 y Metal con AppKit/UIKit para ventana, además de captura offscreen y el canvas WebGPU del banco. En Windows la selección automática prioriza Vulkan para ventana y DX12 para captura. Una build con DX12 puede pedir `--startup --window --backend dx12`.

1. El host crea ventana o canvas y entrega superficie, tamaño físico y escala. Abre el ejecutor nativo elegido y valida límites y presupuesto.
2. AGB configura el destino y presenta una pantalla ligera con clear, UI, texto de atlas e imagen. Medir la inicialización y el primer frame por separado. La carga adicional avanza después del primer frame con presupuesto acotado.
3. El scheduler espera eventos en reposo. Resize y DPI invalidan solo lo afectado. Minimizar, suspender u ocultar el canvas pausa la presentación. Al restaurar, se redibuja con los recursos vigentes.
4. Una superficie vencida se reconfigura. Tras pérdida del dispositivo se reconstruye con la misma API y se vuelve a presentar. Probar error de inicialización, reintentos acotados y rechazo de handles antiguos. Un fallo persistente termina con etapa, API y causa.
5. Aceptar cada arranque con evidencia de ventana o canvas, captura, tiempo al primer frame, RAM/GPU y contadores de reposo. El editor y el player ya abren por esta ruta; el arranque de producto se documenta abajo.

Desde la raíz del repo en Windows:

```powershell
cargo run -p raf_agb --features desktop,dx12 --bin raf-agb-probe -- --startup --smoke
cargo run -p raf_agb --features desktop,dx12 --bin raf-agb-probe -- --startup --backend auto --size 640x360
```

[Ventana del banco](../crates/raf_agb/src/probe/window.rs) · [Arranque del editor](../crates/raf_editor/src/native_application/window.rs) · [Compositor actual](../crates/raf_render/src/ApiGraphicBasic/editor_compositor/compose.rs)

En la revisión anterior a esta ampliación, el banco se verificó en Windows con Intel UHD. Vulkan presentó cuatro frames en la prueba de arranque que cambia tamaño, minimiza, restaura y reconstruye el dispositivo. DX12 fue seleccionado automáticamente para captura. Las pruebas de ambos ejecutores comprobaron píxeles de malla iluminada y cambios visibles tras modificar una textura y una instancia. Metal macOS y WebGPU WASM pasaron compilación cruzada. La build nativa del banco excluye WGPU del grafo normal.

## P2. Verificación y corte de plataforma

- **Cobertura y calidad.** Validar las filas de la matriz con resolución, perfil, exposición, DPI y formato de color equivalentes. Capturar diferencias y verificar interacción y escalado en ventana o canvas. Antialiasing, normales, LOD, sombras y perfiles altos tienen presupuestos y pruebas propios. Priorizar contornos legibles y tipografía nítida en el perfil ligero.
- **Rendimiento primero.** Comparar producto contra producto en builds release y procesos separados con la misma máquina, driver, proyecto, cámara, input, FPS objetivo y calidad. Medir tiempo al primer frame, p95/p99, RAM, memoria gráfica compartida/dedicada donde se exponga, uploads, draws, submits, reposo y presión de memoria. Repetir para conocer la variación. La candidata debe igualar o mejorar consumo y tiempos a calidad equivalente dentro del ruido medido. Probar primero en equipo modesto y repetir en dedicada donde aplique. Comprobar reposo sin envíos ni uploads recurrentes y una sesión sostenida de al menos diez minutos. [Métricas del programa gráfico](GRAPHICS_PROGRAM.md#4-quality-target)
- **Evidencia por etapa.** `cargo check` valida compilación. Recorder valida protocolo y errores. Captura offscreen valida imagen. Ventana o canvas en hardware valida presentación. El A/B del probe solo mide su carga aislada. La promoción depende de las pruebas de producto por plataforma.
- **Corte completado.** El contrato, la matriz y el arranque nativo están conectados. El editor abre `NativeUiWindowHost` con `NativeRequest::Auto`, compone escena/CAD/RafUI en un `ProductFrameBuilder` y dibuja con `ProductDevice<NativeBackend>`. El player usa el mismo host para ventana, headless y streaming. El host web usa `ProductDevice<WebGpu>`. `cargo tree -e normal -p aura_rafi_editor` no muestra WGPU.

## P3. Corte de producto completado

- **Escena y resources.** `SceneRenderFrame` se convierte a `ProductPacket` con mallas indexadas, líneas, grid, triángulos de pantalla, entorno y estado de sombras. Una revisión de cámara no reescribe geometría.
- **RafUI.** `DirectUiSurfaceHost::append_product_paint` emite `Paint`/`StyledPaint` con radios por esquina, bordes por lado, gradientes, feather, clips anidados, nine-slice, texto de atlas e imágenes. El renderer CPU queda como recuperación.
- **CAD.** `CadSurfaceFrame` sigue siendo un `SceneRenderFrame`; el compositor lo dibuja por la misma ruta y el picking permanece en el dominio.
- **Ventana y composición.** `NativeUiWindowHost` abre el ejecutor nativo, configura el target, recompone tras resize y recupera superficie o dispositivo. `NativeEditorCompositor` compone el canvas en un sub-viewport y la UI a target completo.
- **Player y web.** El player headless captura después del draw; la ventana y el streaming presentan por AGB. `raf_web` abre `WebGpu` directamente y no enlaza WGPU.
- **WGPU.** `raf_render`, `raf_editor`, `raf_player` y `raf_web` ya no declaran `wgpu`. En 0.11.5 se retiró también el ejecutor `reference`, la variante `GraphicsBackendId::Wgpu` y los carriles `wgpu` del probe: el workspace completo compila sin WGPU.

### Evidencia del corte

```powershell
cargo check -p aura_rafi_editor
cargo tree -p aura_rafi_editor -e normal    # sin wgpu
cargo run -p aura_rafi_editor               # ventana AGB, backend=vulkan
cargo run -p raf_player --bin aurarafi-player -- --headless --manifest examples/headless/project/runtime.ron --frames 2 --size 1280x720 --fixed-hz 60 --out artifacts/agb-migration/player-agb --report artifacts/agb-migration/player-agb.json
```

En la verificación de este corte, el editor abrió en Vulkan en 630 ms de inicialización gráfica y 1.07 s de arranque total. El player headless capturó dos frames idénticos a 1280×720 con backend DX12 (captura) y el device retenido anunció Vulkan para presentación. Los frames quedaron en artifacts locales y no se versionan.

### Pendientes reales

- Metal y WebGPU no tienen todavía shadow map ni filtro de escena.
- El recoste de un frame con UI cambiada reconstruye el paquete completo. Un perfil con UI animada debe medir el upload y, si hace falta, separar escena y UI en dos capas con actualizaciones parciales.
- La preferencia VSync del editor se conserva, pero AGB presenta con FIFO; el modo inmediato requiere una API de presentación por dispositivo.
- La codificación sRGB de la presentación WebGPU y la captura de canvas siguen pendientes.

## Ejemplos con las interfaces actuales

El comando del player captura la referencia del producto. Los demás reproducen el A/B del banco. Guardar el backend efectivo del player y los datos de la máquina. Ejecutar desde la raíz del repo en Windows.

```powershell
New-Item -ItemType Directory -Force artifacts/agb-migration | Out-Null
cargo run -p raf_player --release --bin aurarafi-player -- --headless --manifest examples/headless/project/runtime.ron --frames 3 --size 1280x720 --fixed-hz 60 --out artifacts/agb-migration/player-current --report artifacts/agb-migration/player-current.json
cargo run -p raf_render --features agb-probe-fixtures --example agb_workload -- artifacts/agb-migration/retained.json 1280x720 es 100
cargo build -p raf_agb --release --features desktop,dx12
$probe = 'target_gnu/x86_64-pc-windows-gnu/release/raf-agb-probe.exe'
& $probe --backend vulkan --workload artifacts/agb-migration/retained.json --smoke
./tools/agb_compare.ps1 -Probe $probe -Output artifacts/agb-migration/ab-vulkan -Workload artifacts/agb-migration/retained.json
```

Este ejemplo consume el contrato de producto sin inicializar el editor. El destino es de captura, pero la preparación y los comandos son los mismos que usa la ventana. La segunda revisión modifica cuatro bytes de la textura y conserva el handle, la geometría y el asignador del destino.

```rust
use raf_agb::{
    backends::native::{NativeApi, NativeBackend, NativeRequest},
    capabilities::GraphicsMemoryBudget,
    product::{Command, FrameUniforms, PacketKey, Paint, PaintQuad, ProductDevice, ProductPacket},
    Output, ResourcePatch, Texture,
};

fn main() -> raf_agb::Result<()> {
    let size = [640, 360];
    let backend = NativeBackend::headless(NativeRequest::Exact(NativeApi::Vulkan))?;
    let mut device = ProductDevice::new(backend, GraphicsMemoryBudget::potato())?;
    device.configure(size, Output::Capture)?;

    let handle = device.prepare(PacketKey(1), 1, || ProductPacket {
        textures: vec![Texture { size: [1, 1], rgba: vec![255, 0, 0, 255] }],
        commands: vec![
            Command::Clear([0.0, 0.0, 0.0, 1.0]),
            Command::Paint(Paint::Image {
                quad: PaintQuad {
                    rect: [0.25, 0.25, 0.5, 0.5],
                    clip: [0.0, 0.0, 1.0, 1.0],
                    uv_rect: [0.0, 0.0, 1.0, 1.0],
                    color: [1.0; 4],
                },
                texture: 0,
            }),
        ],
        ..ProductPacket::default()
    })?;
    let frame = FrameUniforms::default();
    device.draw(handle, &frame)?;
    device.patch(handle, 1, 2, ResourcePatch::Texture {
        index: 0,
        origin: [0, 0],
        size: [1, 1],
        rgba: &[0, 255, 0, 255],
    })?;
    let updated = device.draw(handle, &frame)?;
    assert_eq!(updated.upload_bytes, 4);

    let rgba = device.capture()?;
    assert_eq!(rgba.len(), size[0] as usize * size[1] as usize * 4);
    let center = ((size[1] / 2 * size[0] + size[0] / 2) * 4) as usize;
    assert_eq!(&rgba[center..center + 4], &[0, 255, 0, 255]);
    device.release(handle)?;
    Ok(())
}
```

Las pruebas enfocadas, cuando se programe, son `cargo test -p raf_agb --lib`, `cargo test -p raf_render --lib` y `cargo check -p raf_web --target wasm32-unknown-unknown`, según el crate afectado. La referencia registrada de `raf_render` reproduce en `621319bf07` el fallo `crowded_flow_compresses_without_sibling_overlap`. Registrar resultados nuevos por separado. [Evidencia previa](AGB_NATIVE_PROBE.md#verification-on-2026-11-20)
