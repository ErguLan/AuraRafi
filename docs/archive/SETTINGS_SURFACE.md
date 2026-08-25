# Settings RafUI

The shared retained-surface rules and exact menu pattern live in the
[RafUI Authoring Guide](RAF_UI_AUTHORING.md).

## Scope

Status: active retained RafUI migration surface. The old widget screen was
decommissioned, but the current viewport-first application boundary mounts
the global Settings route and the Project Settings downbar tab through their
RafUI hosts.

The active Settings screen is a retained RafUI surface. It replaces the
legacy widget presentation in `panels/settings_panel.rs`; that file stays in
the source tree only as a recovery reference while the editor shell migrates.

Settings is intentionally separate from Project Settings. This surface edits
the global `EngineSettings` file. Project-specific flags continue to belong to
the selected project and its documents.

The Project Settings tab now follows the same retained-host pattern through
`project_settings_surface.rs` and `ProjectSettingsSurfaceHost`. It edits the
active project's `ProjectSettings` and keeps the established immediate
`project.ron` save boundary. The global Settings draft and per-project
settings therefore never share mutable state.

## Responsibilities

`settings_surface.rs` is declarative. It builds the navigation, responsive
layout, labels, toggles, ranges, segmented controls, password inputs, and
Save/Cancel actions from an `EngineSettings` draft. It does not write files,
open providers, or change renderer state directly.

`panels/settings_surface_host.rs` owns transient RafUI state and translates
the retained actions into the existing draft:

1. A Settings entry clones the committed `EngineSettings` into
   `AuraRafiApp.settings_draft`.
2. The host composes the document through ApiGraphicBasic. GPU is the normal
   path; the CPU host is a recovery path when no shared GPU render state is
   available.
3. Toggle, range, text, and command actions update only the draft. Numeric
   values are clamped at the same boundaries used by the legacy screen.
4. Save replaces the committed settings with the draft and writes the existing
   RON settings file. Cancel discards the draft. Cancel or Escape with changes
   uses the existing confirmation dialog instead of silently losing changes.

The active UI settings resolve to the draft while this screen is open. Theme,
language, typography, and scale can therefore preview inside Settings before
Save. Closing without Save restores the committed values.

The document never embeds provider secrets. API keys live only in transient
control state and are rendered as password input. They are copied into the
draft only after input actions reach the host.

## Migrated Controls

Every user-editable `EngineSettings` field exposed by the old settings panel
has a retained counterpart:

| Section | Connected settings |
| --- | --- |
| Appearance | simple mode, Dark/Light/System theme, experimental theme amount, font size, automatic/manual UI scale, language |
| Performance | quality tier, execution policy, FPS limit/unlimited, FPS counter, VSync, multithreading |
| Editor | grid visibility, snap, grid size, grid load distance, autosave interval, display units, command console |
| Viewport | render mode, labels, focus lock, surface edges, X-ray, face tonality, mouse/WASD inversion, movement speed, gizmo sensitivities, uniform scale, gizmo growth |
| Scripting | prepared runtime switch, default language, hot reload, timeout, external editor command |
| AI | OpenRouter/OpenAI default provider, passive/active agent mode, model shortcuts, provider enabled state, base URL, model, API key visibility and API key input |
| Platform | desktop/mobile/web/cloud/console target, responsive layout, headless mode |

Only OpenRouter and OpenAI are exposed by the current editor Settings surface,
matching the providers enabled for this development phase. Provider support is
not inferred from a visual card; it remains driven by the existing provider
configuration and command backend.

## Presentation Contract

The screen has one fixed navigation rail, one vertical scroll region, and a
fixed Save/Cancel footer. Each control uses RafUI semantic theme tokens, so
Dark and Light are generated from one palette definition rather than from
hard-coded widget colors. Ranges use the minimal two-layer track: a neutral
track represents the complete interval and a white fill/thumb represents the
current value. Orange remains the selection and primary-action accent.

Logical layout and input stay in points. ApiGraphicBasic allocates the output
target at physical DPI, keeping text and range geometry sharp on high-density
displays without changing control values or hit regions.

## Ownership Boundaries

- `AuraRafiApp`: creates/discards/commits the draft and owns persistence.
- `SettingsSurfaceHost`: owns transient focus, input, scroll, password-mask
  visibility, target texture, and action translation.
- `settings_surface.rs`: owns only the retained document and styling.
- ApiGraphicBasic: owns composition and the GPU/CPU presentation paths.
- The native window host provides the event loop and final texture placement.
  It is not the Settings layout or widget system.

This separation keeps the RafUI window host independent from Settings data,
validation, translations, and persistence.
