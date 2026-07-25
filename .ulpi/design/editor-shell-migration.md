# Editor Shell and Settings Migration

This spec binds to [DESIGN.md](DESIGN.md). Every screen must read as the same product if placed side by side.

## Scope and Decision

RafUI already has the correct primitive model: `DockLayout`, `DockWorkspaceController`, retained surfaces, z-aware hit testing, a text atlas, and GPU or CPU presentation. Settings has completed the bounded control migration and now exercises the shared toggle, range, menu, text, focus, scroll, theme, validation, save/cancel, and persistence contracts.

The active migration is one editor shell, never separate Game and Electronics shells. Their center surfaces and domain panels differ; their workbench structure, docking policy, focus path, theme tokens, and persisted arrangement do not.

## Target Architecture

```text
NativeUiWindowHost or temporary eframe bridge
  -> ApplicationMenu (File | Edit | View | Project | Help)
  -> EditorShellSurface
       -> TopContextSlot
       -> WorkspaceDock
            -> LeftDock: Hierarchy or CAD tree
            -> CenterSlot: GameViewport | SchematicCanvas | PcbCanvas
            -> RightDock: Properties | Sessions
            -> FixedBottomDock: Console | Assets | Node Editor | Agent | Project Settings
       -> FloatingDockLayer
```

`EditorShellSurface` owns only layout, panel visibility, docking, focus routing, and persisted workspace arrangement. It never owns scene mutation, electronics mutation, AI calls, rendering state, or project persistence. Each visible panel receives a typed model and emits typed intents at the application boundary.

`UiApplicationMenu` is a sibling application contract, not a contextual panel.
It stores stable command IDs, i18n label keys, enabled/checked state, and
accelerators. The temporary eframe command bar renders that same model. When
the Winit shell owns the event loop, `NativeApplicationMenuAdapter` installs it
through the platform menu API and returns only `UiMenuActivation` values to the
same application dispatcher. No platform has its own File/Edit/View command
logic.

## Docking Rules

- `Hierarchy`, `Properties`, and `Sessions` are movable among left, right, and floating locations.
- `Sessions` is a peer to Properties, not a sub-card inside Properties.
- The bottom dock remains fixed to the lower edge as requested. Its tabs can reorder and its height can resize, but it cannot become a side rail or floating panel.
- `TopContextSlot` is contextual. Game places viewport commands there; Electronics places schematic/PCB mode and CAD commands there. It never duplicates global File/Edit/Project commands.
- `ApplicationMenu` is global and platform-facing. It holds File/Edit/View/Project/Help; it is never placed inside a domain dock or copied into `TopContextSlot`.
- Center surfaces are exclusive and full-size. Viewport, schematic, and PCB remain renderer-owned surfaces rather than being redrawn as generic UI.
- Dock descriptors, visibility, preferred sizes, and floating rectangles are serializable workspace data. Drag state, hit regions, and focus remain transient session state.
- Panels have stable IDs, minimum dimensions, allowed dock sides, and an optional static policy. The initial policy: `bottom` static, `center` exclusive, other panels movable.

## Game and Electronics Boundaries

| concern | Game editor | Electronics editor | shared shell responsibility |
|---|---|---|---|
| center surface | 3D `ViewportSurfaceHost`, camera, gizmo, scene selection | `ElectronicsCadSurfaceHost`, schematic/PCB world, CAD selection | size, focus, surface slot only |
| hierarchy | scene graph entities and folders | components, nets, layers, board objects | dock placement and selection handoff |
| properties | transform, material, scene metadata | component, net, trace, footprint metadata | inspector tab and session association |
| contextual tools | transform, camera, render mode | schematic/PCB mode, route, outline, DRC | top contextual slot |
| bottom work | console, assets, nodes, agent | same, with simulation/DRC details when available | fixed tab dock |

This is why a shared shell comes before either full editor migration. Games and Electronics differ primarily in the center surface and domain panels, not in the workbench around them.

## Migration Order

1. Complete the retained Hub polish and keep its eframe adapter only as a temporary presentation bridge.
2. Build `SettingsSurface` using shared RafUI controls and the locked theme tokens. Keep the existing settings backend and save path unchanged.
3. Extract `EditorShellSurface` around the existing `DockLayout`. Ship only structural docks with temporary panel adapters first.
4. Move shared panels into the shell: hierarchy adapter, properties adapter, sessions, and fixed bottom tabs.
5. Connect the Game viewport to the central slot. Its renderer remains `ViewportSurfaceHost`; only shell ownership changes.
6. Move Electronics through the same shell, then migrate schematic and PCB tool overlays incrementally while `ElectronicsCadSurfaceHost` remains the canvas owner.
7. Move the loading screen last. It is useful visual cleanup but proves no editor interaction contract.

## Implemented Shell Foundation

`editor_shell.rs` persists an `EditorShellLayout` beside each project as
`editor_shell.ron`. It intentionally stores only layout data and has no scene,
CAD, renderer, AI, command, or project mutation references.

- `hierarchy`, `properties`, and `sessions` are movable supporting docks. They
  can occupy the left or right track and later become floating panels.
- `bottom` is fixed to the lower edge. It cannot become a side or floating
  panel. Its height is persisted independently of its tab content.
- `center` is fixed and exclusive. It is the future mount point for either the
  scene viewport or the schematic/PCB canvas, never generic UI content.
- `DockPanelPolicy` and `allowed_dock_sides` are serializable RafUI contracts.
  They survive a dock-to-floating-to-dock round trip and reject invalid drops.
- Invalid or older layout files are repaired by restoring missing structural
  docks, normalizing fixed infrastructure, clamping later floating geometry,
  and preserving valid user dimensions.
- The current Egui editor uses temporary dimension adapters: it reads preferred
  widths/heights from the persisted layout and writes completed border-resizing
  back after pointer release. A window resize or compact layout never replaces
  the user's preferred dock dimension. The panel body migration remains
  separate from this structural state, avoiding a second incompatible layout
  system.

`editor_shell_surface.rs` is the retained document for the direct shell host.
It receives project type, center mode, active bottom tab, inspector state, and
the persisted `DockLayout`; it emits only typed shell intents. It does not own
the scene, CAD documents, renderer, or provider calls. It is ready to become
the visual shell while the current panel bodies are migrated one by one.

The fixed bottom tab strip and the Properties/Sessions selector are now live
RafUI adapters. Their legacy panel bodies remain mounted temporarily, but the
tab chrome is composed and hit-tested through ApiGraphicBasic. This validates
one shared interactive path before moving the domain bodies.

`Project Settings` is the first retained body inside that fixed bottom dock.
`ProjectSettingsSurfaceHost` maps its controls back to the existing
`ProjectSettings` fields and preserves the current immediate `project.ron`
save behavior. It stays distinct from global Settings, whose draft and
Save/Cancel transaction belong only to `EngineSettings`.

`ConsoleSurfaceHost` is the second retained bottom-dock body. The console data
model remains independent from its surface: logs, command output and history
stay in `ConsolePanel`, while RafUI handles filters, scroll offset, input,
JSON disclosure, and typed submissions. Its Tab completion handler consumes
Tab only while the console input owns that key; ordinary controls retain the
normal shell focus traversal.

Electronics also mounts its Schematic/PCB context strip through the same
bridge. The surface emits only a requested center mode; cross-probe and PCB
sync remain typed application actions instead of renderer behavior.

### Electronics CAD Adapter State

The first CAD pass is live through the shared shell without replacing the CAD
backend. The left dock is a project navigator plus the existing searchable
component library. The center owns a direct schematic or PCB surface, with
high-resolution toolbar assets, zoom indicator, world minimap, and CAD status
drawn in the canvas layer. The right dock is an inspector adapter around the
current properties forms, preserving all field bindings while establishing the
new spatial hierarchy.

For Electronics only, the fixed bottom dock adds `DRC` and `Simulation` next
to the shared Console, Assets, Project Settings, Node Editor, and Agent tabs.
Those analysis tabs present data from `DrcReport` and `SimulationResults`; they
do not duplicate validation or solver logic. Any CAD document mutation clears
the displayed results until the user runs the respective action again.

This is deliberately a body-by-body migration: the canvas remains an
ApiGraphicBasic-owned GPU surface, while the chrome is RafUI and the legacy
forms act as temporary document adapters. It prevents a parallel UI model from
becoming a parallel schematic/PCB data model.

While a renderer path is being brought to parity, temporary overlays may show
authoritative selections or wires only when they consume the same document and
world transform as the canvas, have a bounded budget, and carry a removal
condition. A fake minimap, duplicated CAD scene, or UI-only wire route is not
an acceptable migration shortcut.

## Flow: Open and Arrange an Editor Workspace

Goal: a creator opens a project, works on its main surface, and arranges supporting panels without losing a valid layout.

```text
[Open project]
  -> [Resolve project kind]
  -> [Restore workspace layout]
  -> [Mount one center surface]
  -> [Show compatible docks]
  -> [Edit, move or hide a dock]
  -> [Clamp and persist resolved layout]
```

States:

| state | behavior |
|---|---|
| loading | skeleton only in non-canvas panels; do not obscure a ready center surface |
| empty | show factual empty state with one available creation action |
| active | selection moves between center surface, hierarchy, and properties through typed intents |
| invalid saved layout | restore default layout, keep project data intact, show recoverable notice |
| narrow window | preserve center surface; collapse or tab secondary docks before shrinking controls below their minimum |
| offline or AI unavailable | editor remains operable; Agent tab shows provider state and retry, never blocks the shell |

Keyboard contract: Tab traverses visible controls; Enter activates focused commands; Space toggles binary controls; Escape closes menus, popovers, and floating transient controls; arrow keys navigate menus and segmented selections. Focus must be visible through the signature active edge.

## Component Specifications

### `EditorShellSurface`

- Inputs: project kind, active center surface, `DockLayout`, panel descriptors, global command state, theme, language.
- Output: typed intents for panel visibility, move, resize, dock, undock, tab selection, and contextual command activation.
- No raw scene/electronics references are stored inside its retained document.
- On widths below 1024, secondary side docks collapse into tab rails. Below 760, the active center surface remains first and side content becomes overlay docks.
- Rendering budget: shell layout rebuilds only when a model/token/layout value changes; hover and focus are session state. Text stays in the bounded atlas.

### `SettingsSurface`

- Sections: Appearance, Editor, Viewport, Performance, Scripting, AI, Platform. These are navigation sections, not nested cards.
- Implemented surface: `settings_surface.rs` builds the retained document and `SettingsSurfaceHost` maps its typed actions onto the existing `EngineSettings` draft. The active application route no longer calls the legacy settings widget panel.
- Form rows use a fixed label/help/control grid at regular widths and stack at compact widths. The navigation rail and Save/Cancel footer are fixed; only the section content scrolls.
- Toggle for binary settings; segmented buttons for finite options; range control with gray 100-percent track, white progress fill, and white draggable thumb for numeric values; password text inputs for provider API keys.
- `Save` is the only primary action. `Cancel` restores the draft. `Reset section` is destructive-secondary and confirms only when unsaved values would be lost.
- Changing a previewable theme updates the local draft immediately. Persist only on Save.
- The surface migrates the prior global fields: appearance/language, render policy and FPS, editor grid/autosave/units, viewport camera/gizmo controls, prepared scripting, OpenRouter/OpenAI provider configuration, Agent mode, and platform controls. Existing validation and persistence remain at the application boundary.

### `DockPanel`

- Inputs: stable ID, title key, policy, min size, preferred size, visibility, content surface.
- States: docked, floating, focused, collapsed, hidden, resized, and unavailable.
- A floating panel has a 28px title bar, bounded lower-right resize handle, z-order raise on focus, and clamp to workspace.
- Dragging near an allowed edge previews the drop target. Dropping in the center keeps it floating. The fixed bottom dock never accepts a drop.
- Overflow text truncates in titles and retains a tooltip/accessibility label; content panels own their own scroll region.

### `ContextMenu`

- Opens with secondary pointer or a hovered project-card overflow button.
- Closes on Escape, outside primary/secondary click, selection of any command, project/filter change, or surface rebuild that removes its target.
- Menu is placed within the surface bounds and never captures unrelated scroll input.

## Performance and Accessibility Acceptance

- GPU composition is the default path. CPU is a recovery path only and uses the same retained draw list.
- Layout/input (`UiSurfaceFrame`) and paint (`UiSurfaceDrawList`) are cached as separate stages. An idle frame cannot rebuild both stages, reupload geometry, or create a second UI command list.
- The compositor retains vertex buffers, deduplicates image uploads, and merges only adjacent compatible paint runs so it preserves the visual stacking contract.
- No browser engine, HTML runtime, JavaScript runtime, or continuous idle repaint is introduced.
- No background blur, unconstrained textures, or animated decoration is permitted in editor chrome.
- Controls have at least 32px desktop target height, 44px touch target when touch mode is enabled, 3:1 visual focus contrast, and 4.5:1 text contrast.
- All visible text uses i18n keys; a translation may grow vertically but cannot overlap or resize neighboring controls unpredictably.
- Every shell and settings view is tested in Industrial Dark and Paper Light, compact and regular widths, keyboard-only input, GPU host, and CPU recovery host.

## Build Handoff

Target: Codex / Rust RafUI engineer.

Design system: RafUI retained primitives. This is intentionally a Rust-native renderer-neutral system, not a web component library. Implement exactly this spec using the existing `UiNode`, `UiLayout`, `UiStyleSheet`, `DockLayout`, `DockWorkspaceController`, and typed intent boundary. Theme the primitives with the locked tokens; do not redesign or re-implement their semantics inside a panel.

Acceptance criteria:

- Settings saves through the existing `EngineSettings` path with no behavior regression.
- Game and Electronics mount through one `EditorShellSurface` without duplicating dock logic.
- Bottom dock is static; left/right/floating panels restore their layout safely.
- The viewport and CAD canvases remain direct renderer surfaces.
- Menus, focus, scroll, text, and images work in GPU and CPU recovery hosts.
- The shell produces no continuous idle present loop and allocates no per-frame unbounded UI resources.

## Design Pre-Flight

- Identity lock: pass. This spec uses only `DESIGN.md` tokens and the existing RafUI primitive vocabulary.
- Anti-slop: pass. No blue/purple gradient, glass treatment, fake project data, nested cards, stock imagery, or default dashboard composition is specified.
- State coverage: pass. Loading, empty, invalid layout, compact width, offline AI, and disabled capability states are defined.
- Accessibility: pass. Keyboard path, Escape behavior, focus rule, contrast contract, touch target policy, and translated-text behavior are specified.
- Layout craft: pass. Rails, center canvas, fixed bottom dock, and floating layer are distinct structural families with a single focal surface.
- Cognitive load: pass. One primary command per settings view and contextual tools are separated from global commands.

Scored self-critique: distinctiveness 3, hierarchy 4, consistency 4, accessibility 3, state coverage 4, copy quality 3, restraint 4, motion motivation 4. Total 29/32. Revised during pre-flight: the earlier idea of a user profile/search header was removed because the engine has no corresponding data or workflow and it would make the shell read as generic dashboard chrome.

## Frontier core binding

The shell now binds to the shared RafUI core for intrinsic sizing, global
overlays, time-based motion, logical/physical density, semantic component
recipes, and frame diagnostics. Tooltip and menu ownership stays with the
triggering surface, while placement and clipping are resolved at the window
layer. This keeps the editor shell readable and prevents transient behavior
from accumulating in `app.rs` or one surface bridge.
