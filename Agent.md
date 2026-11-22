# AuraRafi — AI Prompt & Architect Registry

All specifications, architectural schemas, file registries, and developer personality guides have been modularized to optimize token consumption and eliminate agent regressions.

## Universal AI Directives & Non-Negotiable Mandates

All AI models and contributors must strictly adhere to these fundamental operating rules across every task:

1. **NEVER MENTION COMPETITORS IN CODE OR DOCS**: Never reference competing software, external engines, or competing companies in source code, variable names, comments, documentation, commit messages, or prompts. The system vocabulary belongs exclusively to AuraRafi and RafUI.
2. **ALL UI STRINGS FULLY LOCALIZED VIA i18n**: Never hardcode user-facing strings in UI code or use ad-hoc conditional strings (`if is_es ...`). Every label, tooltip, notification, dialog, and title must be registered through `t("namespace.key", lang)` with synchronized entries in both:
   * `crates/raf_core/locales/en.json` (English)
   * `crates/raf_core/locales/es.json` (Spanish)
3. **ONE-PASS COMPLETE IMPLEMENTATION (DIRECT & BOLD)**: Never waste time with incremental hesitation or piecemeal half-measures. Code the entire solution directly across all necessary modules, files, handlers, and branches in a single focused pass without stopping midway for trivial approvals.
4. **MINIMAL TESTS ONLY AT THE VERY END (NO CUSTOM TARGET-DIR)**: Never interrupt development flow by running builds or test suites midway. Only run minimal, targeted tests (`cargo test -p <crate> --lib`, single crate, no `--workspace`) at the absolute end of the implementation to verify system integrity. FORBIDDEN to create/use a custom `--target-dir target_*` (`target_ai`, `target_check`, `target_fix`, etc.): the only valid target-dir in this repo is `target_gnu/` set in `.cargo/config.toml`. Do not create `target*` folders inside the code or the repo; if an isolated build is needed, use `CARGO_TARGET_DIR` outside the repo (`/tmp/...`).
5. **DO NOT FIX OR TOUCH UNRELATED ERRORS**: If you encounter compiler warnings, errors, or broken tests in parts of the codebase you did not modify, DO NOT attempt to fix, repair, or refactor them. Other developers may be working in parallel. Strictly contain changes to the requested scope.
6. **MODULAR ARCHITECTURE & ZERO HARDCODING (ANTI-SPAGHETTI & ANTI-GOD-CODE)**:
   * AuraRafi is a serious, complex creative desktop engine—never treat it like a simple web landing page where shortcuts and hardcoding are tolerated.
   * Eliminate hardcoded values: Never use magic numbers, rigid inline style blobs, or inflexible data structures that prevent future expansion.
   * Eliminate God-functions and God-files: Never write monster functions or bloated files. Decompose logic into clean, focused, composable units.
   * Proactive modular file creation: The AI is explicitly empowered and expected to create as many dedicated files (helpers, sub-surfaces, hosts, models) as needed to maintain high modularity and clean architectural growth.

## Large File & God Module Guardrail

These rules exist because monolithic files block review, merge safety, and the
ApiGraphicBasic backend migration.

- A Rust source file above **1,500 lines** is a design smell. Above **2,500
  lines** it must be split before adding or moving more features.
- New files must stay below **1,200 lines**. A feature that would cross that
  limit gets its own module from the start.
- Split by responsibility that changes for a different reason, never by an
  arbitrary line count or by "types vs functions".
- Keep the original file as the module facade. Add a sibling directory with the
  same stem (for example `device.rs` + `device/backend_wgpu.rs`) and declare the
  submodules in the original file. Do not edit shared `mod.rs` or `lib.rs` files
  just to register a split.
- Extract `#[cfg(test)] mod tests` into `tests.rs` first. It is the lowest-risk
  cut and usually removes hundreds of lines.
- A split is a move-only change. Do not mix it with behavior, naming, formatting
  or feature work in the same pass.
- Compile the owning crate after each split and fix only that split's errors.
- Never leave two copies of a function, a re-export that hides a duplicate, or a
  module that only exists to forward one symbol.

Current high-risk files include `ui_surface/mod.rs`,
`ui_surface/render.rs`, `raf_ai/agent_runtime.rs`,
`native_attached_executor.rs` and `electronics_inspector_surface.rs`. The
split inventory and the remaining debt are tracked in
[docs/MODULARIZATION.md](docs/MODULARIZATION.md). Treat the list as a living
inventory, not a one-time cleanup.

## Synaptic Directory Map
* **Definitive Architectural Directory**: [.ai/SYSTEM_TRUTH.md](.ai/SYSTEM_TRUTH.md) — All crates, files, and logic paths detailed.
* **Strict Quality & Localisation Policies**: [.ai/instructions.md](.ai/instructions.md) — Languages, modular layouts, commands consistency, and no-emoji mandates.
* **ApiGraphicBasic Native Ownership Rule** — Product contract, isolated startup, platform qualification, and potato-first graphics [rules](.ai/APIGRAPHICBASIC.md).
* **ApiGraphicBasic Contributor Contract**: [docs/APIGRAPHICBASIC.md](docs/APIGRAPHICBASIC.md) — Surface ownership, resource ingress, GPU/CPU recovery, and asset boundaries.
* **ApiGraphicBasic Product Migration**: [docs/AGB_MIGRATION.md](docs/AGB_MIGRATION.md) — Phase 5 entry points, first headless slice, runnable examples, and acceptance criteria.
* **RafUI Technical Contract**: [docs/RAF_UI.md](docs/RAF_UI.md) — Retained documents, layout, rendering boundary, and core validation.
* **Editor RafUI Contract**: [docs/EDITOR_RAFUI.md](docs/EDITOR_RAFUI.md) — Game/Electronics shell, panels, Inspector, Hierarchy, interaction, and UX rules.

## Agent Completion & Visual Evidence
The native Agent, CLI, and MCP are expected to finish what they start. A gameplay
request is not complete after geometry alone: it needs a camera, authored lighting
where appropriate, attached and validated Rhai gameplay source, a
`game_completion_audit` pass, and at least two inspected `viewport_capture` angles.
Image input is automatic: attachments are always accepted, known text-only
families receive a textual `visible_to_model: false` evidence note instead of
pixels, unknown models are attempted, and a provider rejection retries once
without images before it becomes a recoverable error. Captures are attached to
the next native provider request and returned as MCP image content; contact
sheets combine multiple angles into one bounded evidence image.
Never claim a visual or gameplay result was verified when only a mutation receipt
exists.

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
`.ai/APIGRAPHICBASIC.md` before proposing or changing architecture. Before
product migration, use the [AGB guide](docs/AGB_MIGRATION.md) to complete the
contract, coverage matrix, and isolated startup. Reuse existing product hosts
and producers directly. Native builds can select a native API for the target
output at startup, then recover with the same API. WGPU stays in the current
product until platform cutover,
then leaves that default build. It remains available as an independent benchmark
reference. Do not add a WGPU fallback.

Any task touching a retained surface, editor chrome, menus, docks, scroll,
inspector, hub, settings, or CAD/viewport overlays should consult the dual visual
authorities:
* `.ai/STUDIO_GRADE_UI.md` (System defaults, token contracts, and shell coherence)
* `.ai/CREATIVE_DESIGN_AUTHORSHIP.md` and `.agents/skills/raf-creative-ui-craftsmanship/SKILL.md` (Tactile instrument craftsmanship, kinetic ergonomics, and creative freedom)
* Along with technical contracts in `docs/RAF_UI.md` and `docs/EDITOR_RAFUI.md`.
RafUI documents are presentation data; they never become a second scene, CAD, asset,
or application-state model.

## Visual Design Governance: Dual Authority Framework

Interface craftsmanship and visual architecture in AuraRafi are governed by two co-equal authoritative pillars. Neither is relegated or subordinate:

1. **Studio Grade UI Foundation** ([.ai/STUDIO_GRADE_UI.md](.ai/STUDIO_GRADE_UI.md)):
   * **Domain**: Product-wide baseline visual language, token contracts, restrained warm-orange active signature, accessibility, and shell consistency across Game and Electronics.
   * **When to prioritize**: Standard chrome, global settings, docks, system panels, token alignment, and uniform workbench scaffolding.

2. **Creative Instrument Craftsmanship** ([.ai/CREATIVE_DESIGN_AUTHORSHIP.md](.ai/CREATIVE_DESIGN_AUTHORSHIP.md) & Skill: [.agents/skills/raf-creative-ui-craftsmanship/SKILL.md](.agents/skills/raf-creative-ui-craftsmanship/SKILL.md)):
   * **Domain**: Living desktop-grade instruments, tactile kinetic ergonomics (elastic Bezier cables, magnetic snapping, proximity halos), continuous surfaces (anti-spreadsheet borderless rows), 4-island modular toolbars, and expressive canvas environments.
   * **When to prioritize**: Authoring **new interfaces**, creating **new features**, designing interactive workbenches, visual node scripting, schematic/PCB drafting, deep hierarchy navigators, and dynamic creative manipulation tools where innovation and tactile feedback are essential.

### Agent Selection Protocol & User Alignment
* **Autonomous Context Evaluation**: The AI agent must evaluate the nature of the task. When implementing **new interfaces, new features**, interactive workbenches, node graphs, or CAD surfaces, leverage *Creative Instrument Craftsmanship* to explore expressive, fluid, and tactile solutions. When refining existing general editor chrome, dialogs, or enforcing global token consistency, leverage *Studio Grade UI*.
* **User Agency First**: Both paradigms have full authority. If an interface challenge can be solved through either paradigm, or if the agent is uncertain which style best serves the user's vision, **the AI must ask the user which approach they prefer** rather than making arbitrary assumptions. Historical documents under `.ai/archive/` and `docs/archive/` remain reference-only.

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

