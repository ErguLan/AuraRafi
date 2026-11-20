# Cloud Streaming

This document defines the v0.11 cloud and streaming target for AuraRafi. It is
a technical contract, not a claim that every piece is already active.

## Implementation status

The repository currently contains:

- a standalone runtime host, `aurarafi-player`, which accepts `--manifest` and
  `--runtime` launches;
- a daemon-free local runtime snapshot format in `raf_runtime::manifest`;
- a bounded loopback control channel in `raf_runtime::wire` for the windowed
  runtime;
- a windowless ApiGraphicBasic presentation path (`create_headless`) and a
  reusable tightly packed RGBA8 capture, bounded to 3840x2160;
- the binary streaming envelope and session bounds in `raf_net`;
- a prepared backend-neutral frame graph in `raf_render` for v0.11.5;
- one graphics owner, ApiGraphicBasic, with a private WGPU executor and a CPU
  recovery path;
- the container deployment assets in this change.

The headless CLI flag, player-to-session transport wiring, streaming client and
input transport remain v0.11 implementation work. They are specified here so the
implementation, CI smoke test and deployment image share one contract. The CI
job `headless-smoke` is the first gate: it must accept `--headless --help`
before a real frame run is meaningful.

## Goal

Run an AuraRafi project on a Linux host without a visible window, render with
the same scene path used by the desktop runtime, emit raw frames to a client,
and route client input back into the running world.

The host remains authoritative:

```text
client (display + input)                 host (project + simulation + render)
        |                                              |
        |  input packets                          world state
        v                                              v
  transport  -->  session supervisor  -->  headless host
                                                   |
                                          RuntimeWorld / scripts
                                                   |
                                          SceneRenderer / RafUI
                                                   |
                                           ApiGraphicBasic
                                                   |
                                          frame sink --> transport
```

The client is a thin terminal. It does not load the project, own scene state,
run scripts, or decide simulation results.

## Non-goals

- No video codec is promised. v0.11 emits raw RGBA8 frames. A codec can be
  added later behind the same frame contract.
- No WebRTC media plane is included. The control plane (SDP offer/answer, ICE
  candidates, token-authenticated signaling and a negotiation state machine)
  lives in `raf_net::webrtc`; DTLS, SRTP, SCTP, RTP and a browser codec belong
  to v0.12. See [WebRTC](WEBRTC.md).
- No audio transport. Linux audio output is not implemented in the player.
- No adaptive bitrate, congestion control, or packet retransmission.
- No multi-tenant scheduler or GPU sharing in this repository.
- No editor UI streaming. This contract covers the standalone player session.
- No hosted provider integration, account system, or cloud SDK.

## Architecture

### Headless host

The headless host replaces the Winit window loop on the server. It must:

- create no window, surface, or swapchain;
- allocate one offscreen color target at the negotiated extent;
- drive fixed simulation ticks at the project's configured fixed frequency;
- request a frame only when simulation, input, animation, or an explicit
  capture needs one;
- stop the loop on SIGTERM, token revocation, idle timeout, or frame budget;
- keep the same `RuntimeWorld`, `PlayerSession`, script, physics, input, and
  scene contracts as the windowed player.

The headless host is not a second runtime. It is a host variant around the same
session logic.

### ApiGraphicBasic

All rendering continues through ApiGraphicBasic:

- one selected backend per session;
- WGPU is the private executor today;
- CPU recovery remains available when no adapter is present;
- memory and frame-upload budgets come from `GraphicsMemoryBudget`;
- `SceneFrameMetrics` remains the diagnostics source for draw calls, uploads,
  residency, frame time, and budget overruns.

The frame sink consumes the completed target only after the backend reports it
ready. It must not create a second command list or a second renderer path.

### Frame sink

The frame sink owns the handoff from graphics work to transport:

1. Render the session frame into the owned offscreen target.
2. Copy the target into bounded staging buffers and return tightly packed RGBA8.
3. Package one raw RGBA8 video message with a sequence number and metadata tail.
4. Hand the message to the bounded transport queue.
5. Reuse the staging buffer only after the readback completes.

Rules:

- no unbounded frame queue; the send queue is bounded by `MAX_QUEUE_FRAMES` and
  `MAX_QUEUE_BYTES`;
- when the transport is not writable, drop video work before input and count the
  drop; a replaced frame still consumes its sequence number;
- readback must not block the simulation thread on a render-thread fence. The
  current headless capture uses one staging buffer and waits for the map; a
  multi-slot asynchronous ring is a required follow-up, not a current claim;
- a resize invalidates the target and staging resources, then starts a new
  sequence epoch;
- CPU recovery uses the same video payload layout as the GPU path.

### Input path

```text
transport -> input decoder -> bounded per-session queue -> InputRouter
          -> RuntimeWorld input -> scripts / camera / gameplay
```

The input path must:

- accept only known input event types and bounded value ranges;
- attach client sequence and client timestamp, then clamp timestamps to a
  bounded window around host time;
- preserve pointer capture, focus, blur, key up, and relative mouse semantics;
- drop the oldest queued input under overload rather than growing without
  bound, because stale input is worse than missing input;
- never inject events into the host operating system;
- never execute editor commands or attached-agent mutations.

## Protocol contract

One binary session carries control, input and video messages. The envelope and
constants are shared; a separate control port is not required.

### Binary envelope

The streaming session is binary. The canonical constants live in
`crates/raf_net/src/protocol.rs`; message payloads are owned by the `raf_net`
message modules. Every message uses one envelope:

```text
[payload_len: u32 little-endian][kind: u8][payload: payload_len bytes]
```

- `payload_len` excludes the five-byte envelope.
- `FRAME_HEADER_LEN` is 5.
- `MAX_FRAME_PAYLOAD` is 16 MiB.
- A reader buffers at most `MAX_FRAME_PAYLOAD + FRAME_HEADER_LEN + READ_CHUNK`
  bytes, where `READ_CHUNK` is 64 KiB.
- `MAX_MESSAGES_PER_POLL` is 64.
- The send queue defaults to 32 messages, accepts at most 64, and is capped at
  32 MiB. `FLUSH_BUDGET` is 4 MiB per flush pass.
- Unknown kinds, oversized payloads, and truncated fields close the session
  with a protocol error.

### Handshake

The first exchange establishes protocol version, token, session instance, and
stream format:

- Client sends `Hello` with `PROTOCOL_VERSION` (1), the 128-bit session token,
  the session instance, a client name of at most 64 UTF-8 bytes, and the
  requested width, height and frame rate.
- Server validates the token and instance, then replies with `Welcome`.
- `Welcome` carries the accepted width, height and frame-rate cap, plus the
  bounded queue limits.
- `DEFAULT_WIDTH` is 1280, `DEFAULT_HEIGHT` is 720, and `DEFAULT_MAX_FPS` is 60.
  `MAX_FPS` is 240.
- `HANDSHAKE_TIMEOUT` is 2 seconds.

Rules:

- the token is compared without being logged or echoed;
- a mismatch closes the session with `TOKEN_MISMATCH` or `INSTANCE_MISMATCH`;
- a version mismatch closes with `PROTOCOL_MISMATCH`;
- the token is never written to a frame, artifact, or diagnostics line;
- `Welcome` is the only message that establishes a stream;
- close reasons use the `bye_reason` values in `protocol.rs`, including
  `NORMAL`, `PROTOCOL_ERROR`, `IDLE_TIMEOUT`, and `SERVER_SHUTDOWN`.

### Video frames

A video message uses the `Video` kind. Its payload places raw pixels first and a
fixed 21-byte metadata tail last:

```text
[RGBA8 pixels: width * height * 4][seq: u64 LE][timestamp: u64 LE]
[width: u16 LE][height: u16 LE][encoding: u8]
```

- `VIDEO_TAIL_LEN` is 21.
- Pixels are tightly packed RGBA8. Row padding is removed before framing; there
  is no stride field.
- The metadata tail is last so a receiver can adopt the pixel region without an
  extra copy.
- `seq` increments once per captured frame. Gaps indicate dropped frames.
- `timestamp` is the producer timestamp used for latency accounting.
- Only raw RGBA8 is defined. The encoding byte exists so a future codec can be
  added without changing the envelope.
- `MAX_VIDEO_DIMENSION` is 3840. The headless presentation host additionally
  caps capture at 3840x2160.
- `MAX_VIDEO_BYTES` is 32 MiB for the raw video buffer, but the 16 MiB envelope
  remains the hard limit for one transport message. A 1920x1080 or 2560x1440
  RGBA frame fits; 3840x2160 does not fit the current envelope without a codec
  or an explicit cap increase.

There is no retransmission. The client discards stale frames and reports the
sequence gap.

Raw RGBA is intentionally simple and diagnosable. At 1920x1080 and 30 frames per
second it is about 249 MB/s before transport overhead. That is a local-network
or loopback budget, not a wide-area budget. A codec is a separate later
decision and is not promised here.

### Input messages

Input is a bounded binary message on the same session:

- held keys are capped by `MAX_HELD_KEYS` (256);
- pointer movement is relative, with bounded deltas and explicit focus/blur;
- `client_millis` carries the producer timestamp used for latency accounting;
- the session accepts input only after `Welcome`;
- events outside the accepted range or timestamp window are dropped and counted;
- input delivery is best-effort, ordered by client sequence where one is
  present, and never blocks frame emission;
- input never injects operating-system events and never executes editor or
  agent commands.

### Backpressure

The transport never buffers without bound:

- video frames are queued within `DEFAULT_QUEUE_FRAMES` (32) by default;
- the hard limits are `MAX_QUEUE_FRAMES` (64) and `MAX_QUEUE_BYTES` (32 MiB);
- when a video frame cannot be queued, drop video work before input and count
  the drop; do not grow the queue;
- a captured frame that is replaced still consumes its sequence number, so the
  client sees the gap;
- keep the simulation clock advancing unless the session is paused;
- if the transport is disconnected, stop the session and release the graphics
  target rather than rendering forever.

## Session lifecycle

```text
new -> loading -> ready -> streaming -> draining -> closed
             \-> failed
```

1. `new`: container starts, manifest is resolved, token is loaded.
2. `loading`: project is validated, scripts compile, scene and graphics target
   are prepared.
3. `ready`: the first frame is available; the handshake can complete.
4. `streaming`: input and frames flow; traffic keeps the session inside its idle
   window.
5. `draining`: stop input, flush the last frame, release device resources.
6. `closed`: process exits with 0, or 143 after SIGTERM.

Timeouts and limits:

| Limit | Default | Behavior |
| --- | --- | --- |
| Handshake | 2 s (`HANDSHAKE_TIMEOUT`) | close with `HANDSHAKE_TIMEOUT` or a mismatch reason |
| First frame | 30 s | fail the session with diagnostics |
| Idle | 60 s image default (`RAFI_IDLE_TIMEOUT_SECONDS`); 30 s transport default | close or drain, release the target |
| Session maximum | 3600 s | drain and close |
| Frames | 0 = unlimited (`RAFI_MAX_FRAMES`) | test and capture runs can bound it |
| Clients | 1 per session | additional connections are rejected |
| Frame payload | 16 MiB | reject before allocation |
| Send queue | 32 default, 64 max, 32 MiB max | drop video before input, count drops |
| Input held keys | 256 | reject the message |
| Stream extent | 1280x720 default; 3840 max axis; 3840x2160 capture cap | reject or resize to a bounded target |

The values above are contract defaults. The headless implementation must expose
the deployment-facing ones through the listed environment variables or an
explicit manifest setting; it must not silently invent a different limit. The
transport constants live in `crates/raf_net/src/protocol.rs` and are the
authority for message-level bounds.

## Security boundary

- The container runs as uid 10001 (`rafi`), never as root.
- `/session/project` is mounted read-only. `/session/out` is the only writable
  destination. The project mount must be readable by uid 10001; the image cannot
  change host ownership.
- The image contains no token, key, certificate, or provider credential.
- The token is supplied at runtime, is scoped to one session, and is rotated
  when the session is recreated.
- The listener binds to loopback by default. A non-loopback bind is an explicit
  operator action and must be paired with TLS termination and authentication at
  the operator's edge.
- TLS is terminated by the operator. The player does not implement a public
  certificate authority or a TLS terminator in v0.11.
- The current windowed player refuses a non-loopback control address in
  `crates/raf_player/src/application.rs`. The cloud host needs an explicit,
  auditable opt-in before that rule can be relaxed.
- Do not mount the Docker socket, the host home directory, or writable project
  sources into the container.
- Frame content includes the user's project. Treat captured frames as
  confidential as the project itself.

## Running the container

Build:

```bash
docker build -t aurarafi-player:dev .
```

Prepare an exported runtime manifest:

```text
/srv/aurarafi/project/runtime.ron
```

Run with a read-only project and a writable output directory:

```bash
docker run --rm -it \
  --name aurarafi-session \
  --mount type=bind,src=/srv/aurarafi/project,dst=/session/project,readonly \
  --mount type=bind,src=/srv/aurarafi/out,dst=/session/out \
  -e RAFI_TOKEN="$(cat /run/secrets/aurarafi-token)" \
  aurarafi-player:dev
```

The default command is `--headless`. The entrypoint discovers
`/session/project/runtime.ron`; an explicit path can be supplied instead.
`/usr/local/bin/aurarafi-player` is the deploy wrapper; the compiled executable
it starts is `/usr/local/bin/aurarafi-player-bin`.

```bash
docker run --rm \
  -v /srv/aurarafi/project:/session/project:ro \
  -v /srv/aurarafi/out:/session/out \
  -e RAFI_TOKEN="$(cat /run/secrets/aurarafi-token)" \
  aurarafi-player:dev \
  --headless --manifest /session/project/runtime.ron
```

### Streaming mode

`--stream ADDR:PORT` starts the same headless host and serves authenticated
frames to one client instead of writing PNG captures. The host does not render
while no client is connected, so an idle session costs almost nothing.

```bash
aurarafi-player --stream 0.0.0.0:9400 \
  --manifest /session/project/runtime.ron \
  --stream-token "$RAFI_TOKEN" \
  --size 1280x720 --stream-fps 30 --idle-timeout 60 \
  --report /session/out/stream.json
```

`--stream` and `--headless` are mutually exclusive: one serves a client, the
other writes deterministic captures. The token is mandatory; if it is omitted
the player generates one and logs it once. Bind to loopback unless a TLS or
tunnel boundary terminates the public connection. The report records
`frames_rendered`, `frames_sent`, `frames_dropped`, bytes sent, input latency
p50/p95 and the close reason.

GPU hosts may expose the render device explicitly:

```bash
docker run --rm --device /dev/dri \
  -v /srv/aurarafi/project:/session/project:ro \
  -v /srv/aurarafi/out:/session/out \
  aurarafi-player:dev
```

Without `/dev/dri`, Mesa's software Vulkan driver or the CPU recovery path is
used. Do not assume GPU acceleration from a successful container start.

### Volumes

| Path | Mode | Contents |
| --- | --- | --- |
| `/session/project` | read-only | project, assets, exported `runtime.ron` |
| `/session/out` | read-write | diagnostics, captures, session metadata |
| `/session` | workdir | no project data |

### Environment

| Variable | Default | Meaning |
| --- | --- | --- |
| `RAFI_HEADLESS` | `1` in the image | headless mode flag |
| `RAFI_MAX_FRAMES` | `0` | 0 = unlimited; positive = stop after N frames |
| `RAFI_IDLE_TIMEOUT_SECONDS` | `60` | idle time before the session drains |
| `RAFI_MANIFEST` | unset | explicit manifest path for the entrypoint |
| `RAFI_PROJECT_DIR` | `/session/project` | discovery root |
| `RAFI_OUT_DIR` | `/session/out` | writable output root |
| `RAFI_TOKEN` | unset | session token; supply from a secret mount |
| `RAFI_BIND` | loopback | listener address; non-loopback requires explicit opt-in |

Only `RAFI_HEADLESS`, `RAFI_MAX_FRAMES`, and `RAFI_IDLE_TIMEOUT_SECONDS` are
baked into the image. `RAFI_MANIFEST`, `RAFI_PROJECT_DIR`, `RAFI_OUT_DIR`,
`RAFI_TOKEN`, and `RAFI_BIND` are runtime inputs and must not be committed.

## Acceptance checklist

- [ ] The image builds for `linux/amd64` from a clean checkout.
- [ ] The container runs as uid 10001 and cannot write to `/session/project`.
- [ ] `--help` and `--headless --help` complete without a display or GPU.
- [ ] A missing manifest fails with a clear error before the player starts.
- [ ] A valid 1080p fixture presents its first frame within 5 seconds on the
      reference host.
- [ ] Frame sequence numbers are monotonic; a forced transport stall produces a
      bounded drop count, not a growing queue.
- [ ] The client can move the camera through the input path and the host
      acknowledges the applied sequence.
- [ ] SIGTERM drains the session and exits within 5 seconds.
- [ ] The project mount is byte-identical before and after a session.
- [ ] The 10-minute soak does not grow resident memory without bound.
- [ ] CPU recovery runs when no Vulkan adapter is available.
- [ ] A bad token and a replayed token are both rejected.
- [ ] Non-loopback binding is refused unless the explicit opt-in is set.

## Known gaps

- `--headless` is not implemented in the player CLI yet. CI's headless job is
  the acceptance gate for that work.
- The `raf_net` envelope, constants and codec primitives are present, but the
  session/transport wiring, player integration and streaming client are still
  in progress.
- Headless capture currently waits for the readback map on the calling thread;
  the asynchronous multi-slot ring is not implemented.
- There is no codec. Raw RGBA8 is the only defined video format.
- There is no congestion control, retransmission, or adaptive quality.
- Linux audio output is not implemented. `crates/raf_player/src/audio.rs`
  reports unsupported output outside Windows.
- Clipboard, IME, gamepad, and multi-client input are not defined.
- The WebRTC control plane exists, but no browser media carrier is implemented:
  no DTLS, SRTP, SCTP, RTP packetization, or VP8/H264 codec.
- No multi-session supervisor or GPU partitioning exists.
- The committed fixture lives at `examples/headless/project/runtime.ron`; the CI
  smoke runs three frames and validates the report.
