# AuraRafi — AI Prompt & Coding Instructions

This file defines the strict, non-negotiable rules for code quality, behavior, and modular design. All AI Agents must adhere to these policies.

## 1. Syntax & Language Standards
* **NO EMOJIS IN CODE**: Do not use graphical emoji characters in code files, comment lines, or structural documentation.
* **ENGLISH ONLY FOR SOURCE CODE**: Write all code, structural variables, function names, types, comments, and internal documentation in English.
* **i18n STRINGS MULTILINGUAL RULES**: Never use inline string localizations like `if is_es { "Texto" } else { "Text" }`. All UI strings must be routed through `t("namespace.key", self.lang)` mapping. Every new key added must have entries configured in both:
  * `crates/raf_core/locales/en.json` (English)
  * `crates/raf_core/locales/es.json` (Spanish)

---

## 2. Structural & Architectural Modularity
* **SLIM app.rs PROTOCOL**: Do not write raw drawing calls, panel menus, or specific components logic directly inside `crates/raf_editor/src/app.rs`. 
  * `app.rs` acts as a macro loop container, state registry, and auto-save controller.
  * Every widget panel must be modularly structured in its own file under `crates/raf_editor/src/panels/` exposing standard `show(&mut self, ui: &mut egui::Ui)` methods.
* **COMMAND BUS MUTATIONS**: All modifications to scene assets or schematic shapes must register actions to the `CommandBus` or execute transactional snapshots to sustain the Undo/Redo stack. Avoid silent global state mutations.
* **PERSISTENT CONFIGURATION SETTINGS**: New persistent variables must be declared under `EngineSettings` in `crates/raf_core/src/config.rs` featuring appropriate `#[serde(default)]` serialization overlays.

---

## 3. Manual `/` Console Commands & Tools Consistency
* Every new core action must be linked to its manual Console slash command mapped inside `docs/COMMANDS.md`.
* Console command executes should return proper `CommandOutput` containing:
  * Title block.
  * Informational debug message lines.
  * Structured JSON payloads.
  * A boolean `changed` flag.

---

## 4. Feature Development Flow & Verification
* **COMPLETE IMPLEMENTATION BEFORE VERIFICATION TEST**: When any feature is requested, first program everything completely across all necessary modules and files. Implement all logical branches, structures, and tests.
* **RUN TESTS ONLY AT THE VERY END**: Do not run checks or test executions midway. Only run `cargo test` (or cargo checks under specific request) *at the absolute end* of the complete implementation process to verify system integrity.
* **DO NOT AUTO-FIX OR TOUCH UNRELATED COMPILER ERRORS**: If a compilation error is encountered from unfinished user work or unrelated code sections, **do not attempt to fix it, modify it, or run automatic repairs**. Report comments cleanly and preserve the files exactly as they are.

---

## 5. Vision Triage Protocol
When presented with a visual reference, screenshot, or UI diagnostic page without textual instructions:

* **Triage Layouts**: Inspect matching panel alignments, tabs selections, and menu balances.
* **Diagnose Compilations**: Look for build errors or warnings printed inside terminal panes, console windows, or log outputs.
* **Examine Surfaces**: Verify depth-sorting overlap/interpenetrations, coordinate lines, pad footprints, or cable tracing gaps.
* **Auto-Triage Rule**:
  > **"When I send you an app shot with no context, try your best to figure out what you want me to do with it, diagnose any layout alignment issues, active panel discrepancies, compile errors inside the console or visual bugs in the viewport grid/schematic, and update your appshot triage skill based on what you see."**
