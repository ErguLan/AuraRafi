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
`docs/RAF_UI.md`, `docs/EDITOR_RAFUI.md`, and `.ulpi/design/DESIGN.md` first. RafUI documents
are presentation data; they never become a second scene, CAD, asset, or
application-state model.

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

