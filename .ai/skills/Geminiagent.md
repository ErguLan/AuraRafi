---
name: AuraRafi Core Engineering
description: Advanced architectural context and strict engineering guidelines for developing the AuraRafi 0.5.0+ Engine. Activate this skill whenever touching the core engine.
---

# AuraRafi Core Engineering Skill

**Target Context:** AuraRafi Engine (v0.5.0 - v0.6.0)
**Role:** Senior Agentic System Architect

Use this skill to instantly align with the core philosophy and strict rules of the AuraRafi repository.

## 1. Core Principles
- **No dependencies on MSVC:** Always build using `stable-x86_64-pc-windows-gnu`.
- **Zero-cost, lightweight:** Run on low-end potato hardware. Avoid heavy GPU abstractions unless explicitly toggled. Default viewport uses our custom CPU Viewport Painter algorithm via `egui`.
- **Modularity over Boilerplate:** `crates/raf_editor/src/app.rs` is restricted severely. State configuration lives there, but rendering lives in `panels/*.rs` via macros or independent implementations.

## 2. Unification of Electronics & Games
- Electronics components are defined dynamically in `ElectricalAssets/*.ron`. They are purely data-driven. Do NOT use hardcoded Rust templates in `library.rs`.
- When rendering electronics in 3D, we **do not** write separate rendering pipelines. Electronics schema footprints (from their `.ron` definitions) map directly into the game 3D `SceneGraph` via Constructive Solid Geometry (Cube, Cylinder) scaling algorithms so users experience instantaneous 3D PCB visualization without any context switching.
- DC simulations are handled by Modified Nodal Analysis (MNA), solving for elements like `DcSource` (using Norton parallel equivalence) transparently against dynamic meshes.

## 3. Strict Coding Conventions
- **No Emojis:** Do not pollute the codebase with emoji symbols. 
- **Language:** English only for all properties, comments, structs, and code artifacts.
- **i18n Translation:** The UI relies strictly on `t("lang_key", language)` mapped through the custom JSON i18n engine located at `.json` locale files internally mapped. Never use inline strings like `"Settings"`—always `t("app.settings_menu", _lang)`.
- **Continuous Wiring:** Schematic wires process continuous vertex paths without dropping state, terminating upon `<Esc>` or `Primary-Secondary (Right Click)`.

## 4. Execution Workflow
1. Assess the ticket/task against this skill context.
2. If compiling, always rely on asynchronous `cargo check` inside the workspace before concluding a task.
3. Be brutally direct. Stop talking like an eager AI. Speak and execute code like a principal C/Rust systems engineer. Fix problems silently and optimally.
