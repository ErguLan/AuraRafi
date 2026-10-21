# Lighting and bounded optimized technologies

## Optimized technology modes (2026-10-19)

**Hierarchy > Lighting > Technologies** exposes exactly two choices:
**Optimized current** and **Optimized improved**. They select implementation,
not art style or hardware ownership. Neutral/Scene remains independent. Missing
technology fields deserialize to Current; projects are never silently upgraded.

Improved adds hemispheric ambient response, optional softened cel transitions
(`sun.toon_softness`, 0..1), and linear-light exposure (`exposure`, 0.1..4) with
a bounded highlight shoulder. These two controls are inactive in Current.
Spare uniform channels carry the values without increasing uniform sizes. The
existing sky pass filters procedural stars using pixel footprint and smooths
the solar disk. No atmosphere, cubemap, GI, bloom or volumetric pass is added.

The scene-only five-tap spatial filter is our small corner-sensitive filter,
**not FXAA or MSAA**. It preserves flat fields, axis-aligned edges and alpha;
it does not use motion vectors/history, recover missing geometry/texture detail
or filter RafUI text. GPU execution needs one additional RGBA8 scene target and
one fullscreen pass. CPU recovery uses one reusable RGBA8 scratch image. Each
owner admits at most min(64 MiB, configured graphics budget / 16); denial skips
the filter and reports it, rather than allocating beyond that allowance.
Current frees the filter resources and submits no filter pass. The first
activation builds the private GPU pipeline synchronously; reactivation can pay
that setup cost again. No new timer, simulation job or forced redraw is added.

Improved shadows still use **one** directional map with the existing resolution,
caster/work and memory limits. Camera-frustum receiver fitting, texel snapping
and projection-aware slope bias improve its utilization. Hard filtering remains
one comparison; Soft filtering is bounded to nine comparisons (Current keeps
four). Existing distance fade and selective geometry-cache ownership remain.
Fitting is not a cascade/PCSS implementation and can still shimmer on rotation
or rapid Sun movement. A larger solar disk does not create physical penumbrae.
CPU recovery remains capped at 256-square shadow depth and approximates ambient
normals per triangle, versus GPU per-fragment normals.

`SceneFrameMetrics.optimized_filter_*` reports pass count, retained bytes and
admission denial. Overall scene GPU timestamps include the filter. Native
presentation may still use dynamic internal-resolution scaling: this filter
does not override the user's budget or make up detail lost by upscaling.

The existing property catalog exposes the new paths to authoring commands,
Agent/CLI/MCP and world/local-view scripting without a separate control API:

```rhai
set_environment("technology", "optimized_improved");
set_environment("exposure", 1.0);
set_environment("sun.toon_softness", 0.35);
set_view_environment("shadows.soft_filter", true);
```

Optional real-project A/B QA: set `RAF_LIGHTING_PROJECTS` to semicolon-separated
project directories, `RAF_LIGHTING_QA_DIR` to an artifact directory, and run the
`optimized_project_ab_read_only_captures` renderer test. `RAF_REQUIRE_GPU_QA=1`
makes adapter unavailability a failure instead of a reported skip. QA strictly
decodes the active saved scene, uses its authored camera (or an explicitly
reported QA default), captures both backends at 720x450, toggles only technology
in memory, and asserts the scene file is unchanged. It does not start Play or
execute scripts. Warm debug offscreen CPU/GPU timing samples are diagnostics,
not native-window, release or low-end performance acceptance.

Historical delivery notes below describe Current unless stated otherwise.

### Optimized modes verification (2026-10-19)

- Core library: 93 tests passed. Renderer library: 307 tests passed, repeated
  after the CPU filter fast path. GPU was required for optimized acceptance;
  actual GPU/CPU readback, filter admission/release/resize, migration, shaders,
  shadow cache/fitting and CPU triangle coverage passed.
- Read-only Amazing project / Project Linis captures and per-capture JSON were
  generated under `artifacts/lighting-optimized-20261019`. Both scenes retained
  their saved content. All eight project PNG hashes stayed identical after
  bypassing mathematically zero-strength CPU filter work.
- Saved configurations differ: Amazing uses Scene lighting, Adaptive resolution
  and Adaptive texture quality; Linis uses Neutral, FullResolution and Detailed
  textures. A/B captures hold camera, size and authored settings constant within
  each project and change technology only. They do not prove the original
  perceived blur was caused by Lighting or reproduce script-driven Play.
- At 720x450, the optional image target is 1296000 bytes. Hardware timestamp
  samples were approximately 0.32 to 0.63 ms for the Amazing fixture. Concurrent
  work heavily varied CPU wall times; these small debug samples are not a
  controlled benchmark, native presentation timing or potato-hardware promise.
- Full Editor verification was blocked by concurrent unrelated compiler errors
  in authoring, Electronics and Runtime code. No native-window, DPI, interactive
  selector or rebuilt-editor acceptance is claimed. The added panel tests still
  need to run once the full Editor build is clear; those other changes were not
  repaired as part of Lighting.

## Instrument presentation update (2026-10-16)

The creative-instrument guide drives the Lighting navigation: borderless rows,
semantic Sun/Sky/Shadows/Clock icons, compact secondary descriptions and a
selection edge. Inspector reset actions live beside each property; choices
wrap into compact groups and all existing commit/validation/history commands
remain authoritative. Runtime transport uses the same generated icon family,
separates Stop from playback and derives selected/disabled states from the
confirmed runtime phase. Hover/focus treatment stays in retained style rules.

SVG masters and 64px transparent PNGs are generated by
`tools/ui_assets/generate_rafui_builtin_icons.py --only play pause step-forward stop sun sky shadows clock`.
Generation is a development operation, not an engine/runtime dependency.
The 2026-10-16 pass does not rerun the older acceptance results below: native
UI, narrow-layout/DPI and GPU/CPU visual parity still require manual checking.

## Active scope

World lighting lives in `raf_core::environment::EnvironmentSettings`, persisted
inside `SceneGraph` / `scene.ron`. Existing scenes without the field deserialize
to **Neutral**: the previous background and Lambert lighting are preserved.
Lighting is not an entity or asset, and never requires a character or selection.

Open **Hierarchy > Lighting**. The left dock navigates Overview, Sun, Sky,
Shadows, Time and Performance; properties use the existing right Inspector.
Select **Scene** in Overview to enable authored lighting. Enter exact numeric
values and press Enter/Apply; colors accept `#RRGGBB`. Each property has Reset.
Invalid values do not apply. The transient **Preview neutral (editor only)**
does not save, create history, or affect Play. It stays enabled for the current
editor view until switched off, including project/session changes. Authoring
changes do have Undo/Redo.

Connected to both GPU and CPU recovery:

- One configurable directional Sun: manual azimuth/elevation, color and intensity.
- Colored ambient light, independent of sky-background intensity.
- Solid or direction-dependent gradient sky, lower hemisphere and solar disk.
- Clock-driven Sun during local Play, respecting Pause/Step/Stop.
- Shared world settings and explicitly scoped local presentation overrides.

Delivery 2 adds opt-in Sun shadows under **Lighting > Shadows**, with independent
**Cast shadows / Receive Sun shadows** controls under each object's Appearance.
Existing scenes retain Neutral and shadows disabled. See the directional map
contract below. Imported skyboxes, contact shadows, cascades, GI, volumetrics and
physically calibrated photometry remain **not implemented**. Advanced graphics
presets do not activate these missing capabilities or implicitly enable shadows.

## Units and ownership

The Inspector displays degrees and hours. The Host API and command protocol use
**radians and seconds**. Colors are three sRGB bytes (0..255); shading uses linear
RGB. Intensity is a multiplier, not lux. The clock's day duration is real seconds
per 24-hour cycle. Clock-controlled angles are read-only until Manual is selected.
The simple clock path is not astronomical geolocation or seasonal simulation.

World data travels with each isolated Play snapshot. Runtime writes never save
back into the editor scene. No networking or replication is shipped by this work.
Local/Client/Server scope guards prepare the boundary without pretending a server
exists: Client cannot write shared world lighting; Server cannot access a local
view. A view cannot own a separate simulation clock.

## Rhai and visual Nodes

```rhai
fn on_start() {
    set_environment("mode", "scene");
    set_environment("sun.control", "manual");
    set_environment("sun.elevation", 0.7);
    set_environment("sun.color", [255, 225, 190]);
    set_environment("sun.intensity", 1.1);
    set_environment("shadows.enabled", true);
    set_environment("shadows.resolution", 512);
    set_environment("shadows.distance", 32.0);
    set_view_environment("shadows.soft_filter", true);
    let caster = entity("Cube");
    set_property(caster, "casts_shadows", true);
    set_property(caster, "receives_shadows", true);
    set_view_environment("ambient_intensity", 0.45);
}
fn on_update(dt) {
    let intensity = get_environment("sun.intensity");
    // Explicit local effect: does not modify the world's intensity.
    set_view_environment("sun.intensity", intensity * 0.8);
}
fn on_destroy() { clear_view_environment(); }
```

`get_view_environment(property)` reads the resolved local value.
`clear_view_environment(property)` removes one override; the zero-argument
version removes all. Overrides inherit every unspecified world property.

Nodes: Get Environment, Set Environment Number/Bool/Choice/Color, and Clear View
Environment. Properties select the environment path and `world` or `view` scope.
Color input is a three-component vector of integer byte values, not normalized RGB.
Graphs compile to Rhai and call exactly the same Host API. No implicit owner is
introduced. WASM execution is separate work; these new operations are not claimed
as executable WASM imports.

## Canonical authoring commands

```text
/lighting.inspect
/lighting.set patch='{"mode":"scene","sun.intensity":1.1}'
/lighting.set {"property":"sun.color","value":[255,225,190]}
/lighting.reset property=sun.intensity
/lighting.set patch='{"shadows.enabled":true,"shadows.resolution":512,"shadows.distance":32}'
/game.update target=Cube casts_shadows=true receives_shadows=false
```

`lighting.inspect` advertises supported properties, bounds and types. Batches
validate atomically, reject unknown fields and do not change geometry revisions.
Native UI and Console use the same handler. Agent/attached CLI/MCP route through
the existing gateway: confirmation for writes, dry-run, revisions, idempotency,
scene-operation budgets, document diffs and scoped Undo. Agent lighting tools are
exposed for lighting intent rather than enlarging every unrelated tool pack.
These commands do not start Play, create files or operate a remote server.

## Performance and acceptance

There is no new dependency or intermediate sky image.
The GPU draws a single fullscreen background in the existing scene pass and uses
small cached uniforms; identical sky uniforms are not uploaded again. Neutral
mode does not create or submit the sky pipeline. CPU recovery uses the same sky
and light model, with the existing Gouraud scanline approximation versus GPU
per-fragment normals. The environment's revision is separate from geometry, so
clock ticks do not rebuild scene transforms or geometry visibility caches.

The clock only advances while simulation ticks; a static scene does not acquire
a new timer/job. Neutral/manual/stopped clocks do not request simulation work.
No FPS or potato-hardware guarantee is implied: native visual QA, cold/warm GPU
startup and representative low-end frame-time measurements remain acceptance
gates independent of compilation/unit tests.

## Delivery 2: directional map contract

- `shadows.enabled` defaults false. `resolution` accepts exactly 256, 512 or
  1024; `distance` is 2–256 world units, `bias` 0–1 world units, `strength` 0–1,
  and `soft_filter` selects one nearest comparison or exactly four comparisons.
  The receiver adds a texel-size/surface-slope term to the authored bias to
  suppress self-shadow speckles at low resolution. Four-tap PCF is centered,
  not offset to one side. Large distance/low resolution can still detach small
  contact shadows; no single bias removes both aliasing and detachment perfectly.
- A camera-relative light-space orthographic volume is texel-snapped in all
  axes. Sun direction is independent of camera direction. The last 20% of the
  radius fades out; positions outside coverage are fully lit. This is not a
  cascaded or large-world shadow solution. Lower Sun elevations/large extents
  still trade detail for coverage; snapping does not eliminate every alias.
- Casters are visible opaque geometry intersecting the light volume, including
  objects outside the camera frustum. World streaming currently culls drawing,
  not asset residency, so it does not discard light-volume casters. Transparent
  objects can receive shadows but do not cast alpha-tested/colored shadows.
- Cached medium-detail primitives provide stable caster geometry independent
  of visual LOD. Vertex-edit overrides invalidate the map and remain bounded.
  Depth keys include projection/resolution and caster mesh identities/transforms.
  Visibility, removal, opacity and cast flags change the caster set; changing
  material tint, receiver flags, light color/intensity, bias, strength or filter
  does not rebuild unchanged depth. Turning the Sun off/below the horizon,
  Neutral, zero strength or disabled shadows releases the active effect.
- GPU uses one instanced depth-only pass when the key changes, then samples
  retained `Depth32Float` while lighting direct Sun only. Ambient is untouched.
  Shadow geometry buffers are a separate, bounded private cache of current
  casters, not a second scene/render path. Memory for this owner is capped at
  min(16 MiB, GPU budget / 16), including map, mesh buffers and instances.
- Admission caps: 4096 casters, 100000 triangles and 8 MiB unique mesh data.
  Exceeding a cap disables the entire effect, never an arbitrary partial map.
  CPU recovery lazily builds/reuses a maximum 256-square map, capped at 4 Mi
  triangle-bounding-box pixel tests per rebuild. Overflow returns an unshadowed
  frame and reports `shadow_budget_exceeded`; it never substitutes a partial map.
- World and local-view shadow settings use the existing Rhai/Nodes environment
  API and scope guards. Entity flags use `get_property/set_property` with a Bool;
  they modify the runtime snapshot just like entity visibility, not a network
  replica. Authoring UI uses canonical `lighting.*` / `game.update` handlers.
- `SceneFrameMetrics` reports active state, effective resolution, casters,
  shadow draws, rebuilds, cache hits, resident bytes, preparation CPU time and
  budget overflow. GPU timestamps include the shadow pass in the overall scene
  interval, not a separately isolated shadow GPU time. The existing performance
  strip shows `SH:<resolution>R/C <MiB>M` (rebuild/cache) or `SH:budget!`.
  No extra timer, blocking readback or idle redraw source is added.

The first activation creates GPU receiver/depth pipelines synchronously. In
the debug validation fixture, observed first preparations ranged from about
18 ms with a warm driver cache to 154 ms on a cold activation. Disabling the
effect releases its GPU state; enabling it again may pay setup cost. This is
not an FPS measurement, release benchmark or low-end acceptance.

## Optional stylized sky and sunlight

`sky.stars_enabled` adds static, directional procedural stars to the existing
sky pass (not an imported cubemap). Density is 0..1 and intensity 0..4. Stars
use a deterministic integer hash on sphere-to-cube cells, fade at the horizon,
and need no geometry, textures, simulation clock or extra fullscreen pass.
They are off by default. Enabled cost scales with sky pixels; this is not free
on CPU. CPU and GPU use equivalent formulas. No temporal antialiasing or
subpixel stabilization is claimed for small distant stars.

`sun.toon_enabled` quantizes the direct Lambert term into 2..6 steps, selected
by integer `sun.toon_steps` (default 3). Ambient remains continuous, and shadow
bias uses the original normal/light angle. This is lightweight stylized
lighting, not a full anime material pipeline, outline pass, bloom or GI.
Existing mesh/sky uniforms reuse spare channels without larger allocations.
Both settings use the same validated Lighting property catalog, persistence,
world/local-view Rhai API, Inspector and `lighting.set` command boundary.
Legacy scenes default to disabled and preserve their previous appearance.

Attached CLI/MCP also exposes editor-safe `script.list`, `script.validate`
and `script.compile_nodes` through the existing script handler. These queries
never start Play or execute user gameplay hooks. `behavior.attach` remains
the canonical attachment write; script files are confined to project assets.

## Delivery 2 verification (2026-10-13)

- `cargo check --workspace --target-dir target_gnu_codex_hierarchy`: passed
  with existing unused-code warnings.
- Focused `shadows` library tests across Core, Editor, Nodes, Render, Runtime
  and Script: 10 passed. Coverage includes migration and property bounds,
  atomic entity flags, WGSL validation, GPU/CPU pixel readback, receiver bypass,
  cache reuse and invalidation, offscreen casters, fade, resolution and work
  budgets. A gray receiver/orange convex caster fixture checks real cast shadow
  pixels separately from unwanted self-shadow speckles.
- Full library suites for those six crates: **844 passed, 3 failed**. The three
  failures remain the pre-existing Editor/Nodes translation
  `nodes.pin.entity-reference.entity` and Runtime camera fixture described in
  the historical delivery 1 record. They were left outside this work.
- Real offscreen GPU and CPU captures inspected. At 128 square with one cube
  and one floor, 612 GPU / 659 CPU receiver pixels darkened; retained shadow
  resources were 1049664 GPU bytes / 262144 CPU bytes. A repeated unchanged
  frame recorded zero shadow draws/rebuilds and one cache hit. These are
  fixture-specific resource/work observations, not whole-process memory or FPS.
- Lighting Inspector CPU captures inspected in dark/light at 320 x 800,
  including Shadows and Performance. Scoped Rust formatting, JSON catalogs
  and `git diff --check` passed. No new dependency was added.
- Native-window Play/Stop interaction, DPI/keyboard-only acceptance, release
  startup, representative low-end P95/CPU/GPU/RAM/idle testing remain pending.
  No networking, alpha-tested casters, cascades, contact shadows or GI is claimed.

## Delivery 1 historical verification (2026-10-10)

- `cargo check --workspace --target-dir target_gnu_codex_hierarchy`: passed,
  with existing unused-code warnings.
- Focused `lighting` library tests across Core, Editor, Nodes, renderer, runtime
  and scripting: 12 passed. This includes actual GPU scene submission/readback,
  unchanged-uniform reuse, Neutral bypass, CPU output, migration, scope guards,
  Nodes execution, clock Pause/Step, command confirmation/dry-run and Undo/Redo.
- CPU UI captures inspected in dark/light at 320 x 800; controls and scrolling
  checked. This does not establish native-window interaction or GPU UI parity.
- Broader library suites: 825 passed, 3 failed outside Lighting. Editor and Nodes
  both encounter the existing missing `nodes.pin.entity-reference.entity`
  translation. Runtime's `scripted_camera_switch_follow_and_stop_leave_authoring_untouched`
  fixture expects camera activation without configuring the parameters required
  by the current `examples/runtime/camera_controller.rhai`. Those tests and
  unrelated implementations were left unchanged.
- Native Play acceptance, cold/warm startup and low-end CPU/GPU/memory/idle
  measurements remain pending. No networking or WASM Lighting execution was tested.
