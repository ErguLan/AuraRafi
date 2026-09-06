# AuraRafi — AI Prompt & Architect Registry

All specifications, architectural schemas, file registries, and developer personality guides have been modularized to optimize token consumption and eliminate agent regressions.

## Synaptic Directory Map
* **Definitive Architectural Directory**: [.ai/SYSTEM_TRUTH.md](.ai/SYSTEM_TRUTH.md) — All crates, files, and logic paths detailed.
* **Strict Quality & Localisation Policies**: [.ai/instructions.md](.ai/instructions.md) — Languages, modular layouts, commands consistency, and no-emoji mandates.
* **ApiGraphicBasic Controlled Hybrid Rule**: [.ai/APIGRAPHICBASIC.md](.ai/APIGRAPHICBASIC.md) — WGPU containment, capability-by-capability migration, native backend gates, and potato-first graphics ownership.
* **ApiGraphicBasic Contributor Contract**: [docs/APIGRAPHICBASIC.md](docs/APIGRAPHICBASIC.md) — Surface ownership, resource ingress, GPU/CPU recovery, and asset boundaries.
* **RafUI Technical Contract**: [docs/RAF_UI.md](docs/RAF_UI.md) — Retained documents, layout, rendering boundary, and core validation.
* **Editor RafUI Contract**: [docs/EDITOR_RAFUI.md](docs/EDITOR_RAFUI.md) — Game/Electronics shell, panels, Inspector, Hierarchy, interaction, and UX rules.

## Domain Persona Templates
* **Agent/CLI/MCP Expansion Contract**: [docs/AGENT_CLI_MCP_EXPANSION.md](docs/AGENT_CLI_MCP_EXPANSION.md) - Game-first stabilization, attached editor bridge, external project tools, budgets, diffs, and the no-Runtime boundary.
* **CLI/MCP Human Quickstart**: [docs/CLI_MCP_QUICKSTART.md](docs/CLI_MCP_QUICKSTART.md) - Attached beta commands, scripting examples, undo tokens, and MCP stdio setup.
* **AI Game Authoring Skill**: [.ai/skills/raf-game-authoring/SKILL.md](.ai/skills/raf-game-authoring/SKILL.md) - Compact workflow for project-scoped, reversible scene and script authoring.
Always load the specialized context files below based on your active development task to ensure focused, high-performance logic mapping:
* **System Design & Core Assembly**: [.ai/roles/cto_lead.md](.ai/roles/cto_lead.md)
* **Graphics Programming & Matrix Math**: [.ai/roles/render_math.md](.ai/roles/render_math.md)
* **Electrical Schematics & PCBs CAD**: [.ai/roles/electronics.md](.ai/roles/electronics.md)
* **Retained RafUI & Transitional GUI Layouts**: [docs/EDITOR_RAFUI.md](docs/EDITOR_RAFUI.md)

## Graphics Rule Loading

Any task touching `ApiGraphicBasic`, WGPU, renderer resources, viewport, CAD
surfaces, RafUI presentation, shaders, GPU assets, or native backends must load
`.ai/APIGRAPHICBASIC.md` before proposing or changing architecture. WGPU is a
controlled compatibility backend under ApiGraphicBasic, never the public owner.

Any task touching a retained surface, editor chrome, menus, docks, scroll,
inspector, hub, settings, or CAD/viewport overlays must load
`docs/RAF_UI.md`, `docs/EDITOR_RAFUI.md`, and `.ai/STUDIO_GRADE_UI.md` first.
Feature-specific briefs under `.ulpi/design/` are useful context when they are
current, but they are not a second product-wide visual authority. RafUI
documents are presentation data; they never become a second scene, CAD, asset,
or application-state model.

## Visual design governance

`.ai/STUDIO_GRADE_UI.md` defines product-wide visual defaults and the quality
bar. A current user brief or supplied screenshot may intentionally override a
visual default. It may not override architecture, ownership, accessibility,
localization, or measured performance contracts. Historical documents under
`.ai/archive/`, `docs/archive/`, and `.ulpi/design/archive/` are evidence only.

## RafUI Failure Guardrails

RafUI is not browser CSS. A normal `Row` or `Column` must reserve both its
main-axis extent and its largest cross-axis child. A `FitContent` row may not
report zero height while its children paint. Fix shared intrinsic layout with
non-overlap box regressions instead of panel-specific offsets.

Long transcripts, logs, and tool output must use bounded text tiles plus
viewport virtualization at both message and intra-message tile level. Scroll
state may follow every input event, but expensive document, layout, paint, and
atlas projection must not rebuild for every pixel. Frequently changing fixed
diagnostics such as FPS are paint-only updates and must never revise the whole
workbench. The text atlas must compact requests that are no longer part of the
current document. Loading an unchanged Agent history must not force an immediate
full-file rewrite, and queued history saves must coalesce by project. See
`docs/RAF_UI.md` for the detailed layout and text performance contract.

The native Agent status strip may project the shared attached task lifecycle
(`task.list`, `task.get`, `task.events`, `task.cancel`) while a run is active.
Keep the task manager transport-neutral and bounded; do not move task state into
the RafUI surface or create a second execution loop there.

## UI Motion Default

For interface work, prioritize purposeful motion wherever the user perceives
a state change: opening menus, selecting controls, dragging or reordering
panels, creating or removing dock groups, and changing layout density. Use the
shared `raf_ui::motion::UiTween` / `UiMotionSpec` primitives; hosts own transient
targets and timing while surfaces remain declarative. Motion must communicate
continuity and spatial cause, respect reduced-motion preferences, and avoid
decorative always-on effects. Every non-trivial transition is also a
performance item: measure GPU/CPU frame time, allocations, texture/atlas work,
and idle repaint behavior before calling it finished.

> Developed by Yoll. More info: [yoll.site](https://yoll.site).

