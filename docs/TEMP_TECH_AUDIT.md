# Auditoría técnica temporal (post-0.12)

Estado: **working tree de `v0.12.0` con cambios sin commit**. Las cifras de la
columna "worktree" incluyen trabajo en curso de otras áreas; la columna "HEAD"
corresponde al commit `0a3ffd8`. No es una auditoría de comportamiento: no se
ejecutaron builds ni tests para este documento, salvo los checks indicados.

## 1. Cifras actuales vs. informe anterior

| Métrica | Informe | Worktree | HEAD |
|---|---:|---:|---:|
| Archivos `.rs` | 406 | **834** | 808 |
| Líneas `.rs` | 196,174 | **326,945** | 306,376 |
| Miembros del workspace | 14 | **17** | 17 |
| `#[test]` raw | 1,147 | **1,869** | 1,742 |
| `unwrap` + `expect` raw | 1,376 | **2,994** | 2,667 |
| `panic!` + `unreachable!` raw | 52 | **95** | 92 |
| `unsafe` (token) | 43 | **262** (257 de código) | 261 |
| `raf_editor/src` | ≈98k / 134 | **156,512 / 329** | 145,992 / 320 |
| Comandos de catálogo | 114 | **142** | 131 |
| Paquetes `Cargo.lock` | 465 | **461** | 461 |

El informe describe un estado anterior incluso a HEAD. Solo `Cargo.lock` queda
cerca. La concentración de `raf_editor` sigue siendo ~48% del código, pero los
archivos de 4k líneas que citaba el informe ya no son los mayores: hoy el top es
`panels/viewport_controller.rs` (2,652), `native_attached_executor.rs` (2,253) y
`panels/electronics_inspector_surface.rs` (2,065).

### Método

- `.rs` recursivos, excluyendo rutas con `target*`, `.tmp-rustc`,
  `.codex-run-logs`, `artifacts` y `.git`.
- Líneas con `ReadAllLines`; se incluyen tests, ejemplos y `build.rs`.
- Patrones: `#[test]`, `.unwrap()` (sin argumentos), `.expect(`,
  `panic!`/`unreachable!` con `(` o `{`, token `unsafe`.
- "No-test por ruta": excluye `tests/` y archivos con `test` en el nombre.
- "Sin `#[cfg(test)]`": elimina el ítem siguiente a cada atributo exacto. Es una
  aproximación estática, no una compilación condicional.

## 2. Panics y unsafe: matiz importante

- **Raw:** 2,994 `unwrap`/`expect`, 95 `panic`/`unreachable`, 262 `unsafe`.
- **No-test por ruta:** 2,059 `unwrap`/`expect`, 53 `panic`/`unreachable`.
- **Excluyendo también `#[cfg(test)]`:** 294 `unwrap`/`expect`, 8
  `unreachable!` y **0 `panic!` textuales de producción**.
- Los 8 `unreachable!` de producción están en `raf_editor` (6), `raf_core` (1)
  y `raf_render` (1). No se eliminaron.
- `unsafe` se concentra en `raf_agb` (191), `raf_render` (29) y `raf_editor`
  (27). La cifra 43 del informe se parece a `unsafe fn` (41), no al total.

Conclusión: el riesgo de panic no es "52 panics de producción"; es un problema
de **`unwrap`/`expect` en superficie de comandos y estados recuperables**. Hay
que medirlo con la regla anterior y no mezclar tests con producción.

## 3. `raf_hardware`

**Veredicto: parcial.** 6 archivos, 924 líneas, 3 tests.

Real:

- `SerialSession`: worker serial JSON-line, historial acotado (4096 lecturas,
  32 canales, 16 outputs), receipts con ACK, timeout de 2 s, diagnóstico de
  enlace y `set_output`.
- `enumerate` limita a 64 puertos; sensores y actuadores son modelos de datos
  completos.

Placeholder o cobertura parcial:

- `robot.rs` y `ml.rs` se declaran "structural placeholder"; no hay control de
  robot, dataset writer ni inferencia.
- `ActuatorCommand` no se envía por el serial; solo existe `set_output`.
- Los 3 tests cubren receipts e historial, no apertura real de puerto,
  `enumerate`, `identify`, `set_output` exitoso, reconexión ni `Drop`.
- `open()` devuelve `Ok` antes de abrir el puerto; el fallo llega después por
  `LinkState::Failed`.
- `Drop` no hace `join`; el worker puede seguir hasta 50 ms.
- No se validan outputs duplicados ni bounds OTA en `hello`.

Riesgo principal: el camino real de I/O serial no está cubierto por tests del
crate y el fallo de apertura es asíncrono.

## 4. `raf_net`

**Veredicto: transporte TCP real; WebRTC parcial.** 15 archivos, 7,414 líneas,
52 tests.

Real:

- Framing binario con lectura parcial, límites de payload y cola de envío.
- Handshake con token/instancia, `Welcome`, `Bye`, timeouts y métricas.
- Input autoritativo con 89 teclas, sanitización y secuencias.
- Video RGBA8 con metadata; métricas p50/p95/p99.
- WebRTC: SDP, ICE candidates, signaling JSON/TCP y state machine.

Ausente o limitado:

- No hay códec de vídeo/audio, compresión, control de congestión, DTLS, SRTP,
  SCTP, RTP ni interoperabilidad real con navegadores.
- `webrtc` es solo control plane; el payload `RAF1` no es un códec de navegador.
- El servidor signaling no tiene límite global de `read_buffer`; un cliente
  autenticado puede crecer memoria con líneas JSON pequeñas.
- No hay TLS/replay protection; el token va en `Hello` y aparece en `Debug`.
- No hay rate limit por peer; roles pueden enviar input/video simétricamente.
- Una configuración 4K pasa `StreamConfig`, pero `send_video` rechaza >16 MiB;
  el host lo trata como error fatal.
- `validate_answer` no compara `rtpmap` (encoding/clock rate), solo payload type.
- `VideoFrame::from_rgba` copia cada frame; el host podría usar
  `from_rgba_owned`.

## 5. CI y release (P0 implementado)

- `.cargo/config.toml`: se eliminó `build.target`; el build local usa el host.
  Queda `target-dir = "target_gnu"`.
- CI: jobs `formatting`, `clippy` (estricto en `raf_net`), `linux`, `windows`
  (GNU), `macos-core` (`raf_core`/`raf_net`), `headless-smoke`, `wasm`.
- El formato bloqueante cubre solo archivos cambiados, porque el repo todavía
  tiene archivos legacy sin formatear.
- Clippy estricto solo cubre `raf_net`; el resto tiene deuda de warnings.
- Release: Windows/Linux obligatorios; macOS opcional; publica zip/tar.gz en
  GitHub Releases al empujar un tag `v*`.
- La matriz macOS no valida el editor todavía; solo core/transporte.

## 6. Pendiente después de 0.12

Ver la sección TEMP de [ROADMAP.md](ROADMAP.md): `generate_circuit`, división de
`raf_editor`, política de panics, archivos comunitarios, deuda de Clippy y
bring-up del editor en macOS.
