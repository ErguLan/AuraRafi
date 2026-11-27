# AuraRafi — AI Prompt & Architect Registry

All specifications, architectural schemas, file registries, and developer personality guides have been modularized to optimize token consumption and eliminate agent regressions.

---

## 1. Universal AI Directives & Non-Negotiable Mandates

All AI models and code contributors must strictly adhere to these fundamental operating rules across every task:

1. **NEVER MENTION COMPETITORS IN CODE OR DOCS**: Never reference competing software, external engines, or competing companies in source code, variable names, comments, documentation, commit messages, or prompts. All terminology and identity belong exclusively to AuraRafi and RafUI.
2. **ALL UI STRINGS FULLY LOCALIZED VIA i18n**: Never hardcode user-facing strings in UI code or use ad-hoc conditional strings (`if is_es ...`). Every label, tooltip, notification, dialog, and title must be registered through `t("namespace.key", lang)` with synchronized entries in both:
   - `crates/raf_core/locales/en.json` (English)
   - `crates/raf_core/locales/es.json` (Spanish)
3. **ONE-PASS COMPLETE IMPLEMENTATION — NEVER SPLIT BY PHASES**: Never divide work into "Phase 1 / Phase 2", partial milestones, or half-finished drafts. Code the entire solution directly end-to-end across all necessary modules, files, handlers, and branches in a single pass without stopping until finished. Propose and deliver the fullest, highest-value implementation boldly ("sin miedo al exito"), skipping unnecessary manual test detours.
4. **NO TARGETS OR BUILDS WITHOUT USER PERMISSION (MINIMAL TESTS ONLY IF AUTHORIZED)**: **NEVER** create target directories or run builds/tests (`cargo build`, `cargo check`, `cargo test`) without explicit permission from the user. If the user has not authorized a build or test run, do **not** run cargo compilation/test commands at all. Only when explicitly permitted, run minimal, targeted tests (`cargo test -p <crate> --lib`, single crate, no `--workspace`) at the absolute end of the implementation. FORBIDDEN to create or use a custom `--target-dir target_*` (`target_ai`, `target_check`, `target_fix`, etc.): the only valid target-dir in this repo is `target_gnu/` set in `.cargo/config.toml`. Never create `target*` folders inside the code or repo; if an isolated build is ever explicitly authorized, use `CARGO_TARGET_DIR` outside the repo (`/tmp/...`).
5. **DO NOT FIX OR TOUCH UNRELATED ERRORS**: If you encounter compiler warnings, errors, or broken tests in parts of the codebase you did not modify, DO NOT attempt to fix, repair, or refactor them. Other developers may be working in parallel. Strictly contain changes to the requested scope.
6. **HOLISTIC IMPACT ANALYSIS & ZERO-BUG ENGINEERING**: Before and during coding, think through every detail of how your change ripples across callers, sibling crates, shared state, Undo/Redo, layout bounds, edge cases, and ergonomics. Proactively refine those touchpoints and edge guards so the system stays bug-free as a whole.
7. **MODULAR ARCHITECTURE & ZERO HARDCODING (ANTI-SPAGHETTI & ANTI-GOD-CODE)**:
   - AuraRafi is a serious, complex creative desktop engine—never treat it like a simple web landing page where shortcuts and hardcoding are tolerated.
   - Eliminate hardcoded values: Never use magic numbers, rigid inline style blobs, or inflexible data structures that prevent future expansion.
   - Eliminate God-functions and God-files: Never write monster functions or bloated files. Decompose logic into clean, focused, composable units.
   - Proactive modular file creation: The AI is explicitly empowered and expected to create as many dedicated files (helpers, sub-surfaces, hosts, models) as needed to maintain high modularity and clean architectural growth.

---

## 2. Quick Task Routing Matrix

Load only the documents relevant to your active task:

| Task Area | Required Reading & Contracts |
| :--- | :--- |
| **Repo Structure & Coding Standards** | [.ai/SYSTEM_TRUTH.md](.ai/SYSTEM_TRUTH.md), [.ai/instructions.md](.ai/instructions.md), [.ai/roles/cto_lead.md](.ai/roles/cto_lead.md) |
| **Large File Splits / Refactoring** | [docs/MODULARIZATION.md](docs/MODULARIZATION.md) + *Large File & God Module Guardrail* below |
| **Graphics, WGPU, ApiGraphicBasic, Render** | [.ai/APIGRAPHICBASIC.md](.ai/APIGRAPHICBASIC.md), [docs/APIGRAPHICBASIC.md](docs/APIGRAPHICBASIC.md), [docs/AGB_MIGRATION.md](docs/AGB_MIGRATION.md), [.ai/roles/render_math.md](.ai/roles/render_math.md) |
| **RafUI Surfaces, Editor Chrome, Panels** | [docs/RAF_UI.md](docs/RAF_UI.md), [docs/EDITOR_RAFUI.md](docs/EDITOR_RAFUI.md) + *Dual Authority Design* below |
| **UI Visual Design (System Defaults)** | [.ai/STUDIO_GRADE_UI.md](.ai/STUDIO_GRADE_UI.md) |
| **UI Visual Design (New UI / Features / Craft)** | [.ai/CREATIVE_DESIGN_AUTHORSHIP.md](.ai/CREATIVE_DESIGN_AUTHORSHIP.md), [.agents/skills/raf-creative-ui-craftsmanship/SKILL.md](.agents/skills/raf-creative-ui-craftsmanship/SKILL.md) |
| **Electrical Schematics & PCBs CAD** | [.ai/roles/electronics.md](.ai/roles/electronics.md), [docs/EDITOR_RAFUI.md](docs/EDITOR_RAFUI.md) |
| **In-Engine Agent, CLI, MCP & Authoring** | [docs/AGENT_CLI_MCP_EXPANSION.md](docs/AGENT_CLI_MCP_EXPANSION.md), [docs/CLI_MCP_QUICKSTART.md](docs/CLI_MCP_QUICKSTART.md), [.ai/skills/raf-game-authoring/SKILL.md](.ai/skills/raf-game-authoring/SKILL.md) |

---

## 3. Part A — Code Contributors & Engineering AI Rules

### Core State & Mutation Contracts
- **CommandBus & Undo/Redo**: All modifications to scene assets or schematic shapes must go through `CommandBus` or transactional snapshots to preserve Undo/Redo; avoid silent global state mutations.
- **Persistent Settings**: New persistent settings belong in `EngineSettings` (`crates/raf_core/src/config.rs`) with `#[serde(default)]`.
- **Presentation Boundary**: RafUI documents are presentation data; they never become a second scene, CAD, asset, or application-state model.

### Large File & God Module Guardrail

These rules exist because monolithic files block review, merge safety, and the ApiGraphicBasic backend migration:

- A Rust source file above **1,500 lines** is a design smell. Above **2,500 lines** it must be split before adding or moving more features.
- New files must stay below **1,200 lines**. A feature that would cross that limit gets its own module from the start.
- Split by responsibility that changes for a different reason, never by an arbitrary line count or by "types vs functions".
- Keep the original file as the module facade. Add a sibling directory with the same stem (for example `device.rs` + `device/backend_wgpu.rs`) and declare the submodules in the original file. Do not edit shared `mod.rs` or `lib.rs` files just to register a split.
- Extract `#[cfg(test)] mod tests` into `tests.rs` first. It is the lowest-risk cut and usually removes hundreds of lines.
- A split is a move-only change. Do not mix it with behavior, naming, formatting, or feature work in the same pass.
- Verify module wiring and imports statically; only compile the owning crate after a split if the user explicitly authorized builds/tests.
- Never leave two copies of a function, a re-export that hides a duplicate, or a module that only exists to forward one symbol.

Current high-risk files include `ui_surface/mod.rs`, `ui_surface/render.rs`, `raf_ai/agent_runtime.rs`, `native_attached_executor.rs`, and `electronics_inspector_surface.rs`. The split inventory and remaining debt are tracked in [docs/MODULARIZATION.md](docs/MODULARIZATION.md) as a living inventory.

### Graphics Rule Loading (`ApiGraphicBasic` & Native Backends)

Any task touching `ApiGraphicBasic`, WGPU, renderer resources, viewport, CAD surfaces, RafUI presentation, shaders, GPU assets, or native backends must load [.ai/APIGRAPHICBASIC.md](.ai/APIGRAPHICBASIC.md) before proposing or changing architecture. WGPU is a controlled compatibility backend under `ApiGraphicBasic`, never the public owner.

Before product migration, use [docs/AGB_MIGRATION.md](docs/AGB_MIGRATION.md) to complete the contract, coverage matrix, and isolated startup. Reuse existing product hosts and producers directly. Native builds can select a native API for the target output at startup, then recover with the same API. WGPU stays in the current product until platform cutover, then leaves that default build; it remains available as an independent benchmark reference. Do not add a WGPU fallback.

### Visual Design Governance: Dual Authority Framework

Any task touching a retained surface, editor chrome, menus, docks, scroll, inspector, hub, settings, or CAD/viewport overlays must load [docs/RAF_UI.md](docs/RAF_UI.md) and [docs/EDITOR_RAFUI.md](docs/EDITOR_RAFUI.md), and consult two co-equal visual authorities:

1. **Studio Grade UI Foundation** ([.ai/STUDIO_GRADE_UI.md](.ai/STUDIO_GRADE_UI.md)):
   - **Domain**: Product-wide baseline visual language, token contracts, restrained warm-orange active signature, accessibility, and shell consistency across Game and Electronics.
   - **When to prioritize**: Existing standard chrome, global settings, docks, system panels, token alignment, and uniform workbench scaffolding.
2. **Creative Instrument Craftsmanship** ([.ai/CREATIVE_DESIGN_AUTHORSHIP.md](.ai/CREATIVE_DESIGN_AUTHORSHIP.md) & Skill: [.agents/skills/raf-creative-ui-craftsmanship/SKILL.md](.agents/skills/raf-creative-ui-craftsmanship/SKILL.md)):
   - **Domain**: Living desktop-grade instruments, tactile kinetic ergonomics (elastic Bezier cables, magnetic snapping, proximity halos), continuous surfaces (anti-spreadsheet borderless rows), 4-island modular toolbars, and expressive canvas environments.
   - **When to prioritize**: Authoring **new interfaces**, creating **new features**, designing interactive workbenches, visual node scripting, schematic/PCB drafting, deep hierarchy navigators, and dynamic creative manipulation tools where innovation and tactile feedback are essential.

#### Agent Selection Protocol & User Alignment
- **Autonomous Context Evaluation**: When implementing **new interfaces, new features**, interactive workbenches, node graphs, or CAD surfaces, leverage *Creative Instrument Craftsmanship* to explore expressive, fluid, and tactile solutions. When refining existing general editor chrome, dialogs, or enforcing global token consistency, leverage *Studio Grade UI*.
- **User Agency First**: Both paradigms have full authority. If an interface challenge can be solved through either paradigm, or if uncertain which style best serves the user's vision, **ask the user which approach they prefer** rather than making arbitrary assumptions. Historical documents under `.ai/archive/` and `docs/archive/` remain reference-only.

### RafUI Failure Guardrails

- RafUI is not browser CSS. A normal `Row` or `Column` must reserve both its main-axis extent and its largest cross-axis child. A `FitContent` row may not report zero height while its children paint. Fix shared intrinsic layout with non-overlap box regressions instead of panel-specific offsets.
- Long transcripts, logs, and tool output must use bounded text tiles plus viewport virtualization at both message and intra-message tile level. Scroll state may follow every input event, but expensive document, layout, paint, and atlas projection must not rebuild for every pixel.
- Frequently changing fixed diagnostics such as FPS are paint-only updates and must never revise the whole workbench.
- The text atlas must compact requests that are no longer part of the current document. Loading an unchanged Agent history must not force an immediate full-file rewrite, and queued history saves must coalesce by project. See [docs/RAF_UI.md](docs/RAF_UI.md) for the full layout and text performance contract.

### UI Motion Default

For interface work, prioritize purposeful motion wherever the user perceives a state change: opening menus, selecting controls, dragging or reordering panels, creating or removing dock groups, and changing layout density. Use the shared `raf_ui::motion::UiTween` / `UiMotionSpec` primitives; hosts own transient targets and timing while surfaces remain declarative. Motion must communicate continuity and spatial cause, respect reduced-motion preferences, and avoid decorative always-on effects. Keep GPU/CPU frame time, allocations, texture/atlas work, and idle repaint cost bounded (and benchmark/measure when builds or runs are authorized by the user).

---

## 4. Part B — Native In-Engine / CLI / MCP Agent Contract

These rules govern the runtime Agent, CLI, and MCP tooling operating on user projects inside AuraRafi:

- **Agent Completion & Visual Evidence**: Finish what you start. A gameplay request is not complete after geometry alone: it needs a camera, authored lighting where appropriate, attached and validated Rhai gameplay source, a `game_completion_audit` pass, and at least two inspected `viewport_capture` angles. Image input is automatic: attachments are always accepted, known text-only families receive a textual `visible_to_model: false` evidence note instead of pixels, unknown models are attempted, and a provider rejection retries once without images before it becomes a recoverable error. Captures are attached to the next native provider request and returned as MCP image content; contact sheets combine multiple angles into one bounded evidence image. Never claim a visual or gameplay result was verified when only a mutation receipt exists.
- **Native Agent Status Strip**: May project the shared attached task lifecycle (`task.list`, `task.get`, `task.events`, `task.cancel`) while a run is active. Keep the task manager transport-neutral and bounded; do not move task state into the RafUI surface or create a second execution loop there.

> Developed by Yoll. More info: [yoll.site](https://yoll.site).
