# RafUI

Estado: activo. Este documento es la autoridad tecnica de RafUI. Para el
contrato funcional del editor, workbenches, paneles e interacciones consulta
[`EDITOR_RAFUI.md`](EDITOR_RAFUI.md). Para el contrato grafico consulta
[`RENDERER.md`](RENDERER.md).

Los defaults visuales y la politica para referencias, materiales y excepciones
de diseño viven en [`.ai/STUDIO_GRADE_UI.md`](../.ai/STUDIO_GRADE_UI.md). Este
documento conserva el contrato tecnico y no debe convertirse en otra guia de
identidad visual.

RafUI es la UI retenida oficial de AuraRafi. Las superficies describen
presentacion; los hosts conservan estado temporal y los modulos de dominio
ejecutan comandos, persistencia y undo/redo.

El editor ya no tiene una ruta de runtime del toolkit retirado. Las superficies nuevas y
las migradas deben seguir usando RafUI con el host Winit nativo.
The former RafUI Studio authoring helper was removed during the editor
interface decommission. Its design material is preserved under
`.ulpi/design/archive/` for historical reference and must not be treated as an
active runtime or authoring dependency.

RafUI is AuraRafi's retained, Rust-native user-interface system. It is not an
HTML runtime and it does not embed a browser. Documents are serializable Rust
data, the renderer is ApiGraphicBasic, and platform hosts only provide window,
input, assets, and a target texture.

## Reading routes

- Engine contributors: this document explains current contracts and the source
  map below. Use [RafUI authoring](RAF_UI_AUTHORING.md) for the practical Rust
  surface/host workflow, including the layout replacement pitfall.
- Editor integration: [Editor RafUI](EDITOR_RAFUI.md) owns workbench composition,
  interaction and domain boundaries.
- Game creators: [Game interfaces](GAME_UI.md) owns the visual authoring and
  game-only scripting tutorial. Gameplay scripts cannot address editor UI.
- Graphics contributors: [ApiGraphicBasic](APIGRAPHICBASIC.md) owns compilation,
  text/image presentation, resource admission and GPU/CPU execution.

## Current layers and source map

RafUI is an engine-owned retained UI system, not a wrapper around the graphics
backend. Its vocabulary is data: nodes, controls, layout constraints, style
rules, text/image requests and semantic actions. Its public model has no GPU
resource dependency. Similar layout vocabulary does not imply an HTML runtime
or a browser layout implementation.

```text
Domain state -> compact view model -> UiSurface / UiDocument tree
                                    |
                              retained host/session
                                    |
                        UiSurfaceFrame -> UiSurfaceDrawList
                                    |
                              ApiGraphicBasic
                                    |
                       private GPU executor / CPU recovery

Input -> UiAction -> application/command boundary -> domain state
```

Winit supplies native windows and input. ApiGraphicBasic currently owns the
private WGPU executor. Text measurement and glyph rasterization use the owned
text-atlas module and its font registry; `ab_glyph` is the current outline
rasterizer. These layers do not choose a panel's visual hierarchy or layout.

| Responsibility | Current source |
| --- | --- |
| Serializable nodes, controls and persistent documents | [`node.rs`](../crates/raf_ui/src/node.rs), [`controls.rs`](../crates/raf_ui/src/controls.rs), [`document.rs`](../crates/raf_ui/src/document.rs) |
| Layout vocabulary and component recipes | [`layout.rs`](../crates/raf_ui/src/layout.rs), [`components.rs`](../crates/raf_ui/src/components.rs), [`action_bar.rs`](../crates/raf_ui/src/action_bar.rs) |
| Safe node composition, complete recipes and text drafts | [`composition.rs`](../crates/raf_ui/src/composition.rs), [`components/kit.rs`](../crates/raf_ui/src/components/kit.rs), [`editing.rs`](../crates/raf_ui/src/editing.rs) |
| On-demand layout/style/resource inspection | [`inspection.rs`](../crates/raf_render/src/ApiGraphicBasic/ui_surface/inspection.rs) |
| Themes, styles, geometry effects and typography requests | [`style.rs`](../crates/raf_ui/src/style.rs), [`visual.rs`](../crates/raf_ui/src/visual.rs), [`text.rs`](../crates/raf_ui/src/text.rs) |
| Transient controls, focus, actions and overlays | [`state.rs`](../crates/raf_ui/src/state.rs), [`interaction.rs`](../crates/raf_ui/src/interaction.rs), [`overlays.rs`](../crates/raf_ui/src/overlays.rs) |
| Surface compilation, layout and paint reuse | [`ui_surface/mod.rs`](../crates/raf_render/src/ApiGraphicBasic/ui_surface/mod.rs), [`compilation.rs`](../crates/raf_render/src/ApiGraphicBasic/ui_surface/compilation.rs) |
| Text, images and their resource lifecycle | [`text_atlas.rs`](../crates/raf_render/src/ApiGraphicBasic/ui_surface/text_atlas.rs), [`images.rs`](../crates/raf_render/src/ApiGraphicBasic/ui_surface/images.rs) |
| Editor surface composition and action routing | [`native_workbench_surface.rs`](../crates/raf_editor/src/native_workbench_surface.rs), [`native_workbench_input.rs`](../crates/raf_editor/src/native_workbench_input.rs) |
| Game UI editing, local instances and presentation | [`interface_surface.rs`](../crates/raf_editor/src/panels/interface_surface.rs), [`game_ui.rs`](../crates/raf_ui/src/game_ui.rs), [`player/game_ui.rs`](../crates/raf_player/src/game_ui.rs) |

## Authoring status

The Game editor now includes the session-based Interfaces designer and Assets
insertion. Its authoring, runtime presentation, scripting, and resource contracts
are documented in [`GAME_UI.md`](GAME_UI.md). This uses the retained document
directly and does not restore the retired inspection/recipe helper.

RafUI's retained document, layout, style, text, input, and renderer-neutral
contracts remain available. The old inspection/recipe/snapshot helper is not
currently part of the public crate while the editor surface is being rebuilt.

## Native accessibility

The Windows editor includes an owned UI Automation provider, enabled by default
through `native_accessibility_enabled` in Appearance / Accessibility settings.
It is an accessibility bridge for local tools, not a security sandbox: disabling
it does not prevent screen capture or ordinary operating-system input.

The provider answers `WM_GETOBJECT` and projects cached RafUI geometry and the
current visible document into immutable semantic snapshots. Active modals take
precedence over underlying surfaces. Password and API-key fields do not publish
their contents, including when the visual reveal control is enabled.

Platform callbacks read snapshots and queue bounded, expiring semantic requests.
The normal retained host processes those requests into existing `UiAction`
values on the application thread. Stale or cancelled requests cannot execute
later. No provider callback traverses a scene, rebuilds layout or invokes domain
commands directly. The host only updates snapshots after presentation when a
local accessibility client has recently requested information; settled documents
do not introduce a polling or rendering timer.

The initial patterns cover invocation, editable nonsecret values, ranges,
toggles, selection items, expansion of selects, focus, hit testing and tree
navigation. Text is exposed as bounded visible text elements. Full TextPattern,
offscreen virtual-item realization, semantic canvas objects and platform backends
other than Windows remain outside this initial implementation.

Native client interaction and performance acceptance must be reported separately
from focused semantic/DPI tests.

## Goals

- GPU-first presentation through WGPU and ApiGraphicBasic.
- A compact CPU compositor only as a recovery path for unsupported systems.
- UI documents that are independent from the editor's scene hierarchy.
- Fast, predictable layout that scales down before it consumes excessive CPU
  or memory on modest hardware.
- The same surface model on Windows, Linux, and macOS.

## Theme Model

`UiTheme::raf_ui()` is the editor theme. It provides a
near-black dark surface, a clean light surface, and a warm orange accent.
New user-authored game documents use `UiTheme::game_ui()` with neutral focus
and accent tokens. White element defaults and authoring chrome are scoped to
game interfaces; saved document themes and colors are not migrated implicitly.
Theme tokens are semantic rather than panel-specific:

- `background`, `surface`, `surface_alt`, `surface_raised`, and `canvas`
- `text`, `text_muted`, `border`, `focus`, and `selection`
- `accent`, `accent_hot`, `positive`, `warning`, `danger`, and `skeleton`

Each `UiDocument` owns a serializable `UiTheme`. A user can alter tokens and
metrics in one place, then use `root_style`, `panel_style`, `input_style`, and
`accent_style` to make a whole interface follow it. `UiEnvironment` supplies
viewport size, scale, dark/light preference, reduced-motion preference,
high-contrast preference, and reduce-transparency policy without coupling
documents to Winit or the native host.

`UiSurfaceMaterial` separates surface intent from backend paint policy.
`Opaque` is the default; `TranslucentChrome`, `TranslucentRaised`,
`ModalSurface`, and `BackdropScrim` are reserved for bounded editor chrome.
Attach them with `UiNode::with_material`. ApiGraphicBasic resolves their fill
and border alpha before producing `UiSurfaceDrawList`, so text and functional
icons stay crisp and the GPU and CPU compositors consume identical colors.
High contrast, the explicit Reduce transparency preference, and the Potato
quality budget resolve translucent surfaces to an opaque treatment. This beta
does not allocate blur textures, copy the framebuffer, or add a render pass.

## Layout

`UiLayout` uses a small CSS-like vocabulary without a CSS parser:

```rust
UiLayout {
    flow: UiFlow::Row,
    gap: 8.0,
    justify_content: UiJustify::SpaceBetween,
    align_items: UiAlign::Center,
    compact: UiCompactMode::Auto,
    responsive: vec![UiResponsiveRule {
        max_width: 720.0,
        flow: Some(UiFlow::Column),
        basis: None,
        padding: None,
        gap: Some(6.0),
        compact: Some(UiCompactMode::Stack),
        grid_columns: None,
    }],
    ..UiLayout::fill(UiFlow::Row)
}
```

Supported flows are `None`, `Row`, `Column`, `RowWrap`, and `Grid`. Layout
also supports grow tracks, min/max constraints, start/center/end alignment,
space distribution, automatic grid columns, responsive rules, and compact
policies. `Auto` prefers wrapping where a child can still meet its minimum
width; otherwise it stacks controls in reading order.

`ScrollView` containers apply clipping in both GPU and CPU compositors. A
child outside the clip rectangle cannot receive hit tests or draw over another
panel.

### Contrato de flujo e incidentes de solapamiento

RafUI usa vocabulario parecido a CSS, pero no tiene DOM ni el algoritmo de
layout de un navegador. Cada contenedor debe declarar su flujo y su contrato de
tamano. Los hijos normales participan en `Row` o `Column`; `rect` absoluto se
reserva para overlays o geometria que deliberadamente sale del flujo.

El tamano intrinseco se resuelve en ambos ejes:

- `Row` suma el ancho de sus hijos y reserva como alto el mayor alto requerido;
- `Column` suma el alto de sus hijos y reserva como ancho el mayor ancho requerido;
- el `gap` solo se suma en el eje principal;
- padding, minimos, maximos y mediciones del atlas forman parte del tamano final.

Este contrato evita el incidente donde un `Row` con alto `FitContent` reservaba
cero pixeles, sus hijos se pintaban fuera de esa caja y el siguiente hermano
comenzaba en la misma coordenada vertical. Las superficies no deben ocultar un
fallo equivalente con offsets manuales. Para toolbars, tabs y filas densas se
permite una altura fija o minima cuando sea una restriccion real de diseno; para
contenido localizado o variable se usa `FitContent`.

Toda correccion de layout compartido debe incluir una prueba de cajas resueltas
que compruebe que el siguiente hermano comienza despues del borde final del
anterior. La prueba automatica cubre geometria; la aceptacion visual en la
ventana GPU, DPI y tema reales sigue siendo una puerta separada.

## Controls

RafUI controls are retained node data, not hard-coded editor widgets:

- `UiNode::image`: resource-keyed RGBA/PNG image with contain, cover, or
  stretch fitting. Surface hosts own `UiSurfaceImageStore` and can replace an
  asset without editing a document.
- `UiNode::text_input`: focusable text value bound to a transient key. It
  emits `UiAction::SetText`, supports placeholders, password masking,
  multiline input, and an optional submit command.
- `UiNode::toggle`: binary retained control carrying its current presentation
  value and a stable key. Pointer, Space, and Enter emit `UiAction::SetToggle`;
  the host remains the source of truth.
- `UiNode::range`: numeric retained control with min/max/step metadata. It
  supports pointer drag and arrow/Home/End keys, emitting `UiAction::SetRange`
  without coupling value persistence to the document.
- `UiNode::scroll_view`: horizontal, vertical, or dual-axis scrolling in
  session state, with clipping and platform wheel input.
- `UiNode::grid`: responsive automatic or explicit column layout for repeated
  items.
- `UiNode::skeleton`: lightweight loading placeholders. They do not need a
  timer or texture upload and respect reduced-motion hosts.

Document structure remains persistent. Focus, active values, pointer state,
and scroll offsets belong to `UiSurfaceSession` / `UiControlState`, so opening
two windows does not write transient UI state into project files.

Toggle and range controls are rendered by the RafUI presentation core, not by
individual surfaces. A range that needs exact values composes its slider with
an editable numeric text input. Hosts seed transient text only once per field,
parse and clamp committed values, and preserve partial edits while the user is
typing. This prevents the common failure where a field cannot be cleared or a
small value appears frozen.

The editor downbar starts as one ordered tab group, can reorder tabs horizontally
and split through edge dragging up to three groups. Dragging shows a transient
semantic-accent target preview; only release mutates the layout. Empty source
groups are removed and the layout is persisted per project. Loading repairs
duplicate tab IDs globally, restores missing tabs and the active tab, and saves
the repaired layout. Layout files from an older dock contract are reset once to
the clean default so stale split interaction state cannot survive an upgrade.
The retained source node remains present during a drag, and RafUI captures its
drag actions at pointer-down so a rebuilt tab strip still receives `DragEnd`.
Project Settings is a downbar tab; global Settings remains
in the application bar. Project and Assets consume real view models and show
truthful empty states instead of sample rows.

## Commands

The active native document command adapter supports these operations:

```text
/ui.document.read path=assets/ui/menu.ui.ron
/ui.document.create path=assets/ui/menu.ui.ron name=Menu
```

`ui.document.apply` accepts a structured batch of edits and requires the exact
document revision returned by read; `ui.document.undo` uses the same revision
guard. See [Game interfaces](GAME_UI.md#agent-cli-and-mcp-authoring) for the
payload and the shared command-gateway contract.

The catalog still contains older `ui.node.*`, `ui.document.describe`,
`ui.document.set_space` and `ui.document.bind_camera` descriptions. They are not
the supported operations of `commands/ui.rs`; catalog presence alone is not
evidence of a connected native action. Use document edits and `UiMount` for
the current authoring route.

## Surface Hosts

- `DirectUiSurfaceHost`: direct GPU compositor with WGPU target ownership
  outside the host.
- `CpuUiSurfaceHost`: matching CPU recovery compositor.
- `NativeUiWindowHost`: Winit integration and retained input bridge.

The native Winit shell is now the active editor host. RafUI is the document
system for editor surfaces; it does not inject a default game HUD into user
projects.

### Compilation And Submission

RafUI has two retained products with separate invalidation rules:

```text
UiSurface + UiSurfaceSession
  -> UiSurfaceFrame (layout, hit regions, text requests)
  -> UiSurfaceDrawList (the only CPU/GPU paint payload)
  -> direct GPU target or CPU recovery pixels
```

`UiSurfaceCompilationCache` reuses layout, hit testing, resolved text, and the
paint list while the document, control state, focus, physical scale, and text
atlas revision stay unchanged. `UiSurfaceGpuRenderer` then retains GPU vertex
buffers, uploads only changed geometry, deduplicates image uploads, and merges
adjacent compatible paint runs without reordering UI layers. Metrics expose
cache hits, upload bytes, paint runs, and draw calls. Do not recreate a
`UiSurfaceFrame`, draw list, vertex buffer, or texture merely because the
window requested another idle present.

Paint refresh preserves the retained physical text width: DPI is applied once,
not multiplied again on hover, focus, or animation. Caret and selection belong
to paint keys, not geometry keys. A provisional layout built for input must
still complete its intrinsic measurement before presentation, even if a prior
paint-only update was pending. Icon-only content reserves its icon dimensions
and authored padding on both axes. Fixed diagnostic labels patch their own
retained text request without refreshing unrelated node styles.

Interaction transitions retain per-node target styles: only changed visual
state re-resolves that node's rules, while tween frames interpolate the cached
target. Hierarchy and Assets update scroll coordinates on every event but
materialize another document only when their virtual row window changes.
Hierarchy view persistence uses one coalesced worker slot and a 300 ms quiet
period; serialization and durable file synchronization never run in the UI
frame. Session changes and explicit layout persistence drain that slot before
reading or leaving the previous session.

### Native cost guard

`raf_ui::performance` owns bounded, backend-neutral observations and cooperative
optional-work allowances. ApiGraphicBasic supplies timings for input, layout,
text, CPU paint preparation and submission; hosts report document construction.
The current advisory slow-stage default is 4 ms. This is configurable through
`UiWorkLimits`, not an FPS cap, preemption deadline or performance guarantee.
History retains at most 128 region identities and 256 compact samples per host;
advisory tracing warnings are rate-limited. A native snapshot inspector opens
with `Ctrl+Shift+F12`; refresh is explicit, so it creates no idle repaint loop.
Region identifiers are engine IDs, not localized labels. Submission is CPU work,
never an inferred GPU duration. Native canvas execution remains owned separately.

Independent absolute root regions reuse layout and CPU paint preparation using
their actual subtree, scoped controls, visual state, density, intrinsic sizes,
styles and text-atlas slots. Flow-dependent parents retain the ordinary layout
path. Regions merge in original painter order into the same draw list; they do
not allocate another GPU renderer or presentation target. Layout and paint
caches each bound region count, node count and retained text estimates. These
are cache admission bounds, not a claim to cap the entire process heap.

The workbench additionally reuses the authored Hierarchy document when its view,
selection, scene revision, theme, geometry and settings agree. Transient editing
and drag states bypass that cache. Other builders can still run on a workbench
revision; region layout/paint reuse must not be described as eliminating all
editor document construction.

Project-image admission uses count, byte and elapsed-time allowances. At least
one bounded resource may make progress; an oversized first item is reported by
the observation contract rather than starved forever. Deferred pixels are kept
in one slot, cancelled with the project, and request another necessary frame.
This policy never drops clicks or changes authored quality. The direct-blocking
source guard and focused `ui_cost_guard` tests complement native GPU, DPI and
low-end acceptance; they do not replace those checks.

### Texto largo, scroll y atlas

Un historial virtualizado no debe convertir cada mensaje largo visible en una
sola solicitud de texto ilimitada. Transcripciones, logs y resultados de tools
se dividen en bloques de texto acotados y solo se construye el rango visible
con overscan. El scroll puede actualizarse cada evento, pero la proyeccion del
documento pesado se cuantiza para no reconstruir layout, paint y atlas por cada
pixel recorrido.

La virtualizacion se aplica en dos niveles: entre mensajes y dentro de un solo
mensaje sobredimensionado. Que una tarjeta cruce el viewport no autoriza a
rasterizar todos sus bloques; los bloques fuera de vista se representan con
spacers de altura estable. Esto evita que un unico resultado de tool llene el
atlas, haga desaparecer labels posteriores o provoque un repack grande al
cruzarlo con scroll rapido.

Diagnosticos que cambian con frecuencia, como FPS, usan una pista de tamano fijo
y actualizacion paint-only. Nunca deben cambiar la revision estructural de una
superficie pesada. Un contador dinamico que reconstruye el workbench puede
convertir un overflow puntual del atlas en rasterizacion y uploads recurrentes.

Restaurar un historial valido es una operacion de lectura y no fuerza una
reescritura completa al arrancar. Las escrituras siguen fuera del hilo de UI y
se compactan al snapshot mas reciente por proyecto para que cambios rapidos de
sesion no creen una cola de serializacion o sincronizacion de OneDrive.

El atlas conserva las solicitudes del documento vigente y compacta entradas
obsoletas; navegar hacia abajo no debe hacer crecer indefinidamente su conjunto
de glifos o texturas. Para diagnosticar tirones se inspeccionan por separado
reconstrucciones de documento, cache hits, solicitudes/ocupacion del atlas,
uploads, tiempo de presentacion y numero de nodos visibles. Un contador de FPS
estable no descarta trabajo excesivo dentro de un frame.

### Application Menus

`UiNodeKind::Menu` is only for an in-surface context or overflow menu.
`UiApplicationMenu` is the separate declarative model for the application
menu bar: File, Edit, View, Project, and Help. Its entries carry stable command
IDs, i18n label keys, enabled state, checked state, and optional accelerators.
The application dispatches those IDs at one command boundary.

`NativeUiWindowHost` accepts a `NativeApplicationMenuAdapter` while the native
Winit shell owns the event loop. A platform adapter installs the menu and returns only
`UiMenuActivation` command IDs. It must never execute scene, CAD, persistence,
or editor logic itself.

### Active Hub Migration

`AppScreen::ProjectHub` opens the retained RafUI Hub through
`NativeStudioSurface`. It builds its document from project metadata, renders
through ApiGraphicBasic, and presents it in the native Winit compositor. The
GPU host is the normal path; `CpuUiSurfaceHost` remains the recovery path.

The Hub's retained actions are translated at the editor boundary into typed
intent values for search, filter, theme selection, create, settings, open,
duplicate, and forget. A project can also open the retained context menu with
the secondary pointer button; the menu is an elevated RafUI node. Project
persistence and loading therefore stay outside the UI document.
The native Hub surface is the normal route after Loading; old panel recipes are
historical references only.

The desktop Hub has three independent retained regions: the fixed project rail,
a scrollable workspace, and a fixed-width scrollable side rail. The workspace
contains the greeting, current project, filters, and responsive project grid;
the side rail contains only real quick creation actions and recent project
activity. Project cards expose the same retained context menu through their
three-dot control that appears when a project card is hovered, as well as the
secondary pointer button. Context menus dismiss on Escape, command completion,
or a primary/secondary click outside their bounds.
The side rail keeps its own layout track at compact desktop widths and stacks
only when the available content width can no longer fit both regions. This
avoids the common nested-scroll failure where a sidebar is clipped or controls
collapse into an implicit zero-height track.

The Hub resolves its visual vocabulary through the active Studio Grade UI guide
and semantic theme tokens. `PaperLight` and the dark palette preserve the same
information hierarchy when the user changes mode. The Hub uses high-resolution
game/electronics preview assets through `UiSurfaceImageStore`. They are
generated by local development utilities; image generation is never a runtime
dependency.

On high-density displays, the Hub keeps document layout and pointer input in
logical points while allocating its WGPU target texture in physical pixels.
Text atlas requests rasterize at that physical scale, then the compositor maps
logical geometry to the denser target. The CPU recovery compositor receives
the equivalent scaled draw list. This prevents browser-like texture upscaling
from making text, borders, and icons look soft at 120 DPI and above.

The native input bridge translates wheel input into RafUI's content-offset
convention. A wheel-down gesture increases the content offset and reveals
lower content; a wheel-up gesture returns toward the top.

The editor keeps its retained loading surface visible while Hub, workbench,
and Electronics retained documents are constructed. It is a real startup
stage, not a minimum cosmetic duration. Compilation that happens before the
editor process exists remains outside this in-process surface.

The first Hub pass used a temporary bitmap alphabet to prove the atlas path.
RafUI now rasterizes a bundled vector font into the same bounded alpha atlas,
so text stays proportional and localized without adding a second widget system
widgets. The Hub does not continuously request repaint while idle. It asks for
one follow-up frame only after an interaction changes retained state, avoiding
an unnecessary present loop while the application is static.

### Active Settings Migration

`AppScreen::Settings` mounts the retained settings surface from the existing
`EngineSettings` draft and renders it through the native ApiGraphicBasic host.

The settings document is a movable floating modal over the active Hub or
workbench. It uses a dimmed backdrop, a compact drag header, global search, a
grouped navigation rail, one scrollable property workspace, and an explicit
Cancel/Apply/Save footer. Its sections are Appearance, Performance, Editor,
Viewport, Electronics, Scripting, AI, and Platform. Game and Electronics
preferences are separated so CAD-only controls do not leak into the game
workflow. The modal stays within the window and re-clamps after resize.

Settings controls update only an EngineSettings draft until Apply or Save.
Theme, typography, accessibility, reduce-transparency, and the experimental
warm tint preview flow through the same UiEnvironment used by the workbench. Save commits
the validated draft through the existing RON persistence path; Cancel and
Escape preserve unsaved values instead of discarding them implicitly. Dropdowns
use the shared RafUI Select contract with keyboard navigation, typeahead,
selection state, and click-away close behavior.

The migration keeps existing fields connected: render policy and FPS limit,
grid/autosave/units, viewport camera and gizmo controls, Electronics grid/snap
presentation, prepared scripting preferences, supported AI provider settings,
Agent mode, and target-platform flags. API keys are represented by transient
password input state and are redacted from disk when credential persistence is
disabled. The global console switch remains a capability gate; each project
keeps its own command permission. Nodes-specific execution settings are not
exposed here; the active Nodes surface owns graph authoring state, while runtime
settings remain outside the current Play contract.

Settings ownership, draft/save lifecycle, validation boundaries, DPI behavior,
and the split between app, retained document, host, and ApiGraphicBasic
compositor are defined in [`EDITOR_RAFUI.md`](EDITOR_RAFUI.md).

### Active Project Settings Migration

The `Project Settings` bottom-tab body now uses
`ProjectSettingsSurfaceHost`. Its retained document covers overview, dock
visibility, project runtime flags, linear save, prepared scripting, project
graphics policy, and world streaming. The host applies typed actions to the
active `ProjectSettings`; the native application boundary retains the existing
`project.ron` persistence and the linked global/project console-command gate.

Unlike global Settings, this surface has no Cancel action or second draft:
accepted project settings save immediately to `project.ron`, and the project
name remains beside the `Project Settings` title. It uses the same GPU-first ApiGraphicBasic
composition, CPU recovery host, semantic palette, i18n, retained input, and
bounded text state as the global Settings route.
Its Layout section also exposes `Reset Panels`, which restores the one-group
default and persists the downbar repair for the active project.

### Active Console Migration

The `Console` bottom-tab body now uses `ConsoleSurfaceHost`. `ConsolePanel`
continues to own log entries, structured command output, command history, and
submission records. The retained document owns only presentation, filters,
auto-scroll, entry JSON disclosure, input focus, and typed command intents.

Clear, level filters, auto-scroll, Send/Enter submission, Tab completion, and
Up/Down history remain available. RafUI recognizes an input-level Tab handler
before applying global focus traversal, so command completion does not steal
focus from the console field. Command parsing and execution remain at the
application boundary.

### Shared Editor Shell Foundation

`editor_shell.ron` is a project-local, serializable workspace layout. It owns
only dock arrangement and never scene, CAD, renderer, AI, or command state.
`hierarchy`, `properties`, and `sessions` are movable supporting docks limited
to the left and right tracks. `bottom` remains fixed to the lower edge, while
`center` remains the exclusive renderer slot for Viewport, Schematic, or PCB.

The RafUI `DockPanelPolicy` and `allowed_dock_sides` contracts enforce that
policy even after a dock is floated and restored. Invalid saved layouts repair
their structural panels without discarding valid preferred sizes. During the
adapter phase the active editor reads those persisted dimensions for the old
panel hosts and writes only a completed border resize back to
`editor_shell.ron`; a compact window never overwrites the user's preferred
dock size. Panel body migration remains independent from this layout state.

`editor_shell_surface.rs` now describes the direct retained shell with typed
center-mode, inspector, bottom-tab, visibility, and settings intents. It uses
the same document for Game and Electronics, while the center canvas stays
renderer-owned and is never redrawn as a generic UI widget.

The current live editor downbar adapters are the fixed bottom tab strip,
Console, Assets, Agent, and Project Settings. Game adds the Nodes tab;
Electronics adds DRC and Simulation. They are rendered by RafUI through
ApiGraphicBasic and the native compositor. The Nodes tab is an authoring
surface for the session graph with its own retained body, Inspector, typed
connections, and validation; it does not execute graph logic from Play.
Legacy Project layouts remain readable, while Properties and Sessions stay
outside the default downbar until their real bodies and state contracts are
ready. This keeps the active shell truthful while removing the old hand-painted
tab controls from the editor chrome.

Electronics also uses the retained contextual strip for Schematic and PCB.
Its intent boundary retains cross-probe selection and PCB synchronization in
the application layer, so selecting a visual mode never moves document logic
into the UI renderer.

### Active Electronics CAD Migration

Electronics now consumes the shared shell as a CAD workbench rather than as a
generic scene editor. The native Electronics canvas feeds `CadScene` through
the same ApiGraphicBasic `RenderRuntime` and compositor as Game. Its document
mutation and rich CAD controls are owned by `NativeElectronicsEditor`; RafUI
surfaces emit semantic commands and do not mutate the document directly.

The Electronics left dock presents the project navigator and live component
library, including search, placement selection, clipped catalog scrolling, and
a bounded scroll thumb. The right dock presents the contextual inspector while
preserving the existing editable component, wire, trace, footprint, and board
fields. Schematic and PCB canvases expose high-density toolbar resources,
zoom state, visible-world minimaps, and grid/snap status without allocating a
second CAD scene for UI decoration.

The shared bottom dock exposes real `DRC` and `Simulation` tabs for electronics
projects. Their result bodies consume the existing `raf_electronics` report and
DC solver structures. Checks run as cancellable background tasks and expose
`running`, `completed`, `cancelled`, and `failed` states; editing the CAD
document invalidates cached reports, so stale analysis cannot be shown as
current. The `tools/generate_electronics_ui_icons.ps1` development utility
creates the local high-resolution PNG toolbar and catalog assets; it is never
called by the editor at runtime.

## Boundaries

RafUI itself owns presentation data and interaction contracts. Game UI authoring
and local runtime instances are implemented in the editor/player layers described
in [Game interfaces](GAME_UI.md); the game script API is isolated from editor UI.
An empty game document still receives no HUD or gameplay template automatically.
Scene shading, PBR, world shadows and post-processing remain renderer/domain
responsibilities. UI geometry effects do not implement those scene features.

## RafUI Frontier Core

The retained core now has explicit primitives for interface problems that were
previously solved inside individual editor bridges:

1. `raf_ui::overlays` places menus, popovers, tooltips, drag previews, and
   modals in window coordinates. An overlay keeps semantic ownership of its
   source node but is not clipped by the source surface. `UiPlacement` flips
   and shifts the result into the available viewport.
2. `UiSizeMode::{Auto, Fixed, Fill, FitContent, MinContent, MaxContent}` lets
   controls state how each axis is sized. ApiGraphicBasic applies measured
   atlas bounds to intrinsic text nodes after localization is resolved.
3. `UiInteractionState` records pointer enter/leave transitions, monotonic
   hover time, pointer position, and hover-intent progress. Focus and hover
   remain session state and never enter the serialized document.
4. `raf_ui::motion` provides time-based `UiTween` values with reduced-motion
   support and shared easing. GPU and CPU hosts consume the same value.
5. `raf_ui::components` contains semantic recipes for icon buttons, panel
   headers, tree rows, and compact tooltips. Recipes return normal `UiNode`
   data and do not create a second widget system.
6. `UiEnvironment` centralizes logical/physical size conversion and bounded
   raster density. Layout remains in logical points while text, geometry, and
   paint are produced at the same physical density; a 1x target is never
   filled with a supersampled atlas and then filtered down a second time.
7. `UiSurfaceDiagnostics` exposes layout-box, hit-region, clipping, zero-size,
   text-request, and z-order counts for editor inspection and CI tests.

The active editor tooltip is a separate transparent RafUI surface composed in
the global tooltip layer. The native compositor only places the already-
rendered RafUI surface; it does not draw the tooltip rectangle or its text.

Retained UI canvases use nearest sampling whenever the physical source size
matches the physical host rectangle, preserving one-pixel borders, icon edges,
and small text. This also applies to fractional DPI when the rounded physical
dimensions still match; linear sampling is reserved for a real size conversion,
not for a fractional panel origin. Renderer-owned viewport and CAD canvases keep
their existing linear presentation policy.

Embedded UI images also receive premultiplied-alpha mipmaps at the GPU upload
boundary. Their sampler chooses the nearest complete mip level instead of
blending two levels, keeping 64px source icons stable and defined when displayed
at 12–18px while preventing transparent edge colors from becoming bright point
noise. Semantic technical icons use a separate high-density built-in source and
nearest magnification; CPU recovery follows the same rule instead of bilinear
blurring a small icon.

### Core data flow

```text
UiDocument + UiSurfaceSession
  -> style and responsive resolution
  -> intrinsic layout and text measurement
  -> interaction state and overlay placement
  -> shared UiSurfaceDrawList
  -> ApiGraphicBasic GPU compositor | CPU recovery compositor
```

The tooltip policy lives in
`crates/raf_render/src/ApiGraphicBasic/ui_surface/mod.rs`; its placement contract lives
in `crates/raf_ui/src/overlays.rs`; its visual node recipe lives in
`crates/raf_ui/src/components.rs`. The tooltip is delayed by hover intent,
measured after localization, and anchored below the pointer when the window
has room. This separation prevents tooltip policy from accumulating in a
product panel or compatibility bridge.

### Semantic icons and per-control color

Authors use `UiIconId` or the `icon_button_with_icon`/`tree_row_with_icon`
recipes instead of embedding a PNG path in every surface. ApiGraphicBasic owns
the 64px source raster, upload, mip policy, and backend sampling. Bitmap art is
still available for multicolor assets, but technical UI icons remain light and
stable across 100%, 125%, 150%, and 200% DPI.

Every node can keep its own `UiStyle`, and an ordered `UiStyleSheet` can target
an ID, class, or node kind in a specific state. `UiStylePatch` can override
fill, border, text, opacity, radius, and border width for one control without
forking the renderer. Use semantic `UiTheme` tokens as defaults and local
overrides only when a component genuinely needs a different color.

## Document and presentation craftsmanship contract

The current document version is 4. `UiDocument` persists its root, theme,
`UiStyleSheet`, presentation mounts and value bindings;
`UiSurface::from_document` preserves the tree, theme and stylesheet. Mounts and
bindings are consumed by the game instance layer, not by that surface constructor.
Version 1, 2 and 3 documents migrate through additive defaults and validation. Future versions
are rejected without mutating the original document. Focus, scroll, edit
drafts, decoded images and backend handles are never persisted in that tree.

`UiDocumentUpdate` provides typed text, visibility, enablement and image
updates. A host still decides when an update is accepted and when layout is
invalidated; these updates do not grant authority to mutate scene state.
Hidden nodes do not participate in layout, hit testing or focus order.

### Authoring and styles

`UiNode` sizing/spacing builders and `map_layout` modify the existing layout;
`with_layout` remains a complete replacement. `UiComponents` provides themed
buttons, inputs, Select triggers, property fields and adaptive bars as ordinary
nodes, with one explicit shared state stylesheet. Author rules may override it.
The neutral interface-authoring chrome uses these recipes; other consumers may
still use the compatible older recipes.

`UiTextStyle::for_role`, `label`, `toolbar` and `with_size` centralize semantic
metrics while keeping role/weight and proportional leading. They do not add
full text shaping. `UiImageView` exposes fit/crop/alt/placeholder options through
the existing image contract. Optional `UiLayout::aspect_ratio` reserves height
from resolved width before image loading; fixed or fill height remains explicit.

`UiTextDraftBank` belongs to a host, never to the saved document. Scoped value
reconciliation preserves dirty drafts and IME while clean fields follow model
updates. Cancel restores the latest model value; domain validation still decides
whether Enter commits. The interface inspector uses this contract.

Intrinsic sizing follows compact layout and distributed row widths. Flexible
tracks cannot compress below their minima, and grow weights do not replace an
intrinsic size on the opposite axis. An impossible minimum set remains overflow.
`inspect_surface` provides bounded, on-demand boxes/style/resource snapshots and
authoring issues from the compiled frame; it has no idle polling or new native
inspector panel. `component_gallery` exercises shared controls and resource states
through the existing hosts. See [RafUI authoring](RAF_UI_AUTHORING.md) for usage.

- `UiNode::row`, `column`, `with_children` and scoped child identities simplify
  ordinary Rust construction; no parser or second widget system is required.
- `UiStyleLayers` compiles base, component, variant, state and explicit
  overrides to the existing ordered rules. Legacy rule order remains valid.
- `UiStyleSelector::Control` addresses effective controls without requiring
  `UiNodeKind` to duplicate their payload type.
- `UiThemeMetrics::for_control` resolves component metric overrides.
  `UiTokens::chrome` derives component colors unless the theme overrides them.
- `UiVisual` requests per-corner radii, per-side border widths, a linear
  gradient, bounded analytic shadow falloff, a focus ring and optional child
  clipping. Text/icon color inheritance is explicit through `inherit_text`;
  geometry, borders and isolated group opacity are not inherited.
- Icon and label placement share a measured content group with configurable
  alignment, gap and start/end icon placement. Intrinsic width includes both.
- `UiAlign::Baseline` and `inline_text_row` align ordinary styled text nodes.
  Inline runs are a single row, not a rich paragraph/wrapping engine.
- Numeric edit sessions retain incomplete drafts and emit begin, commit,
  reject and cancel intents. Domain validation and undo remain host-owned.
- `plan_action_bar` uses localized, caller-measured labels and component
  metrics. Semantic groups retain separators and every overflow action stays
  reachable through the shared Select popup contract. Pinned actions expose
  `fits = false` when even the minimum row cannot fit; they are not discarded.

Hosts distinguish geometry changes from paint updates. Hover, focus, selection,
caret movement, enablement and fixed-size text updates refresh retained paint
without rebuilding layout. Intrinsic text, visibility and resource replacement
invalidate geometry conservatively. Short state transitions stop when settled
and obey reduced motion; disabling tooltips does not disable other transitions.
This is bounded paint-list invalidation, not a dirty-rectangle renderer.

`UiNumericEditBank` infers numeric text/range pairs from the authored controls;
it does not duplicate their bounds in a host-specific table. Units are explicit
form metadata. Engine Settings and the native Project Settings dock share that
transient contract. Incomplete drafts survive projection and tab changes;
Enter confirms a field, Escape restores its latest authoritative value, and a
slider replaces that field's draft. Engine Settings Save/Apply validates all
enabled pending fields before committing any of them. Project Settings keeps
its existing per-field intent/persistence boundary; an unfinished draft is not
project data. Specialized scene/CAD property editors retain their existing
domain contracts rather than being blindly converted into generic fields.

This host-side API does not expose internal RafUI mutation to gameplay scripting.
The separate game interface API and visual authoring are implemented; their
presentations, role checks and remaining acceptance limits are documented in
[Game interfaces](GAME_UI.md).

### Images and resources

`UiImageOptions` describes a normalized source region, focal point, source-pixel
nine-slice insets, per-use sampling, alternative text and placeholder policy.
Both compositors use `image_layout` for destination/UV patches. Image content
respects padding and rounded clipping; its border is painted in the foreground.
Declared child clip masks have a validated depth limit of eight, with one extra
mask available for the image's own rounded edge. Overlay layers reset ancestor
clipping. Pointer hit testing honors the same ancestor masks.

Project images accept `assets://ui/logo.png` or
`project://assets/ui/logo.png`. Resolution is project-confined after
canonicalization, including symlink checks. The existing bounded decoder runs
on a worker with bounded request/completion queues. Hosts register resources,
pump a bounded number of completions and cancel the loader on replacement.
An optional completion wakeup requests a redraw; no polling repaint loop is
needed. Pending, missing and failed images have explicit runtime states and
visible placeholders unless the document deliberately requests `Hidden`.

The decoded CPU store has a byte budget and recent-use eviction. CPU pixels
remain straight-alpha; interpolation is alpha-weighted. GPU image uploads
premultiply in linear light, generate matching mip levels and use a matching
image blend pipeline. Text and solid paint retain their separate contracts.

`icon_catalog.rs` is the shared semantic registry for icon IDs, stable keys,
PNG inclusion and the development generator. Missing registered PNGs fail
compilation; decoding and nonempty alpha are covered separately. Per-icon
procedural copies are retired. The GPU packs built-in icons into bounded pages
with padded regions and preserves painter order; an atlas does not promise a
single draw call across different clips or interleaved paint kinds.

`UiIconId::descriptor()` exposes the existing SVG master, source raster size,
design canvas, declared logical display size and intentional lookup aliases.
The logical size is an authoring recommendation, not evidence of per-icon
optical tuning. Stable keys and serde variant names remain compatible. Alias
image URIs normalize to the canonical key before paint/cache lookup, so two
spellings do not require duplicate pixel storage or GPU atlas entries. The
development generator consumes this same manifest without runtime filesystem
access.

`DirectUiSurfaceHost::register_external_image` accepts an AGB texture view for
GPU-produced previews without CPU readback. The host owns presentation and must
unregister stale keys. Views must be filterable 2D targets with premultiplied
sampled RGB; CPU recovery requires a separately supplied raster source. Normal
textures, icon pages and external views share residency accounting. The existing
GPU cache may exceed its soft target for an active frame rather than thrash;
diagnostics report that condition. This is not an unconditional GPU memory cap.

Project image registration is explicit. Adding a new source key after loading a
document requires the host to register its updated resource set; the loader is
not a filesystem watcher. The shared text layout has bounded caching, word wrap,
fallback glyphs and baseline metrics, but does not implement full bidirectional
shaping or a rich paragraph model.

### Quality and acceptance

`UiSurfaceDrawList` remains the only paint payload. Its effect metadata is
shared by GPU and CPU for hollow borders, corner geometry, gradients and clips.
This does not enable MSAA, framebuffer blur or isolated compositing layers.
`craftsmanship_gallery` is a real retained fixture consumable by the existing
native host or CPU density matrix, not an alternate visual implementation.

Source tests and offscreen captures do not establish native GPU appearance,
input behavior or performance. Acceptance still requires real dark/light,
fractional-DPI, resize, keyboard, reduced-motion and low-end checks. No measured
performance improvement should be inferred merely from the new cache or atlas.

> Developed by Yoll. More info: [yoll.site](https://yoll.site).
