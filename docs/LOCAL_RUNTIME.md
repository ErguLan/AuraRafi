# Local Game Runtime

Status: local implementation present; native acceptance and measured
performance remain separate gates.

## Verification checkpoint (2026-10-03)

- `cargo test --offline -p raf_script -p raf_nodes -p raf_runtime --lib`:
  passed, 28 tests (12 scripting, 7 Nodes, 9 runtime). Covers camera activation,
  Parts with cameras, lenses, safe hierarchy changes, late-update after physics,
  follow/switch/missing targets, actual example scripts, graph flow/Delay and
  bounded failure without mutating the authoring scene.
- Focused editor camera tests: 10 passed. Hierarchy semantic icons: 1 passed.
  Nodes catalog: 2 passed, including all 22 new node labels, pins and properties
  in EN/ES. The catalog camera test also appears in the camera-filtered run.
- Core camera observation/diff test: 1 passed. UI icon identity: 2 passed.
  Renderer image/cache/budget tests: 8 passed. Player controls: 1 passed.
  These runs cover 52 distinct tests; repeated runs are not counted twice.
- `cargo check --offline -p raf_player -p raf_editor --tests`: passed.
  The older Electronics compilation blockers below are historical; no
  unrelated Electronics repairs were made in this camera pass.
- Command/locale JSON parsing, `git diff --check` and C++ example syntax check:
  passed. The C++ file is a contract sketch, not a shipping engine SDK.
- Main executable build: cancelled at the user's request. No executable
  delivery is claimed; further validation must stay minimal and focused.
- No native Play/Stop/window-mode visual acceptance, real audio-output check
  or whole-machine resource measurement is claimed.

Artifacts use `target_runtime_validation_20261002/`; this pass puts compiler
temporaries in `.codex-run-logs/runtime-tmp-20261003/`. It does not modify or
launch the executable in the existing `target_gnu/` directory.

## Verification checkpoint (2026-10-02)

- `cargo test --offline -p raf_script -p raf_runtime --lib`: passed, 14 tests
  (9 scripting, 5 runtime). Covers owner/scope isolation, real hook parsing,
  cancellation/operation bounds, invalid handles, reload recovery, exact-root
  confinement, simulation controls, input edges, missing camera and secret-free
  snapshot serialization.
- `cargo check --offline -p raf_player --tests`: passed. Player controls tests
  compile; this check does not execute them or open native windows.
- Command catalog and both locale JSON files parse successfully.
- `git diff --check`: passed.
- Full editor integration check remains blocked by seven concurrent
  Electronics-related errors: an incomplete `ComponentDrag` initializer,
  fingerprint function shadowing, analysis-title borrowing and Electronics
  save-handler borrow conflicts. These unrelated changes were preserved.
  No complete editor build, native Play/Stop smoke test, real audio output or
  combined-resource measurement is claimed at this checkpoint.

Validation artifacts are isolated in
`target_runtime_validation_20261002/`; temporaries use
`.codex-run-logs/runtime-tmp-20261002/`. Existing `target_gnu/` permissions
were not changed.

This feature follows `Agent.md`, `SCRIPTING_SYSTEM.md`, `EDITOR_RAFUI.md`,
`RAF_UI.md`, and `APIGRAPHICBASIC.md`. It does not introduce another renderer,
UI toolkit, scene format, or editor mutation path.

## Authorized delivery

- A renderer-independent runtime core with an isolated scene snapshot.
- Rhai first: compile shared code once, independent owner/state per attachment,
  real validation, bounded execution/logs, lifecycle, and safe missing handles.
- A lightweight local player rather than a second editor initialization.
- Play, Pause, Step and Stop backed by confirmed lifecycle state.
- Reuse the existing startup loading visual, replacing tips with real work
  progress; present it before the game in either launch mode.
- Explicit runtime camera and input ownership. No implicit player, controller,
  camera binding, or modification of the authoring document.
- Launch/resource preferences through existing Settings contracts, bilingual
  text, local-console controls, and no automatic Agent/attached activation.
- Same-window and separate-window execution reuse the same runtime logic.
- Bounded repeated sessions, cancellation, diagnostics and resource cleanup.
- Only focused minimal tests after implementation; native-window validation
  is a separate acceptance gate and is not inferred from unit tests.

## Expansion boundaries

Server networking, WASM, native plugins and advanced
render effects remain later milestones from the approved proposal. Keep the
core independent of windows/editor state so a future headless server can use
it. Never advertise these reserved capabilities as active controls.

Camera and supported graph-to-Rhai execution are implemented in the
2026-10-03 pass; see [Camera Runtime](CAMERA_RUNTIME.md). The checkpoint above
is historical, not validation of this newer implementation.

## Local usage and explicit limits

Game projects expose native Play/Pause/Step/Stop in the top bar. Configure
Settings -> Runtime for separate-window (default) or same-window launch,
1-4 simultaneous instances, window size, FPS, hot reload, diagnostics and
script error policy. Project Settings -> Runtime selects the startup session,
active camera, fixed frequency, entity limit and bounded JSON input actions.
"Use selected object" adds a default game camera through scene history;
clearing the reference does not delete the object. No automatic character,
controller, camera or authored HUD is injected. No camera shows a diagnostic;
scripts still run if present. Stop discards the snapshot, never saves it.

Play launches the already-built executable with `--runtime`; no Cargo, Hub,
editor shell or Agent initialization occurs in the player route. Separate
instances currently share executable code, not a second editor session.
The splash is the exact shared startup document. Its text reports real
preparation stages and completed/total compilation work, not fictional timing
or a fabricated percentage for disk/UI initialization. Escape cancels.

Rhai globals persist independently per attachment. `self_node()` is explicit;
missing/ambiguous/deleted/cross-instance handles fail safely. Public hooks are
validated from the AST. `on_update(dt)`, `on_fixed_update(dt)` and the
post-physics `on_late_update(dt)` run at the configured fixed frequency
(15-120 Hz), at most four catch-up ticks per
display iteration. Step executes exactly one tick while remaining paused.
Error pause aborts remaining hooks, but does not roll back mutations already
performed earlier in that tick. A failed hot reload keeps the previous code;
successful reload preserves scope and does not rerun top-level initialization.
Additions to global initialization require Stop/Play. Script-generated events
and authored UI events use `on_event(name, value)`; they cannot execute editor
commands. Event queues, logs, source size, operations and deadlines are bounded.

Initial physics supports explicit translation, gravity/damping and swept AABB
contacts only: no angular dynamics, mesh/hull narrow phase, impulse solver,
automatic character controller or trigger event delivery. Unsupported collider
types fail preparation when physics is enabled. The initial roster is limited
to 64 dynamic bodies, 1024 colliders and 4096 body/collider pairs.
Windows audio initially supports PCM 16-bit mono/stereo WAV files only, with
four voices and 8 MiB per clip. Other formats/platforms report unsupported
output. Stop/focus-pause release or pause the relevant playback resources.

Screen-space authored UI is supported; camera-space UI requires an explicit
binding to an authored camera; it is visible/interactive only while that
camera is active, including script-driven switches. World-space UI is rejected rather than
rendered incorrectly. Base-color texture/geometry policies and world-region
visibility use the existing renderer. Advanced effect presets, legacy software
depth controls and LOD bias are not newly activated. Internal render output is
capped at 1920x1080 (960x540 for explicit CPU-only), preserves aspect ratio and
uses the existing dynamic-resolution controller with measured CPU frame cost.
Static/paused sessions poll at 4 Hz; active display rate is configurable and
input-triggered presentation is capped. These are budgets, not measured
whole-machine RAM/CPU/GPU guarantees.

## Acceptance gates

1. Two owners using one Rhai asset preserve independent state.
2. Missing/deleted/cross-instance references cannot address a different object.
3. Parse errors, wrong hook signatures, loops, cancellation and log floods have
   bounded diagnostics and recovery.
4. Play uses unsaved authoring state without writing runtime changes back.
5. Loading progress follows completed preparation; no Cargo invocation on Play.
6. Pause stops simulation time, Step performs one fixed tick, Stop cleans up.
7. No character or camera produces explicit empty/diagnostic behavior.
8. Repeated Play/Stop, resize, focus changes and window close do not leak state.
9. Settings actions reach the actual runtime consumer.
10. Combined editor/player resource use and live GPU presentation are measured
    before making low-resource or visual-success claims.
