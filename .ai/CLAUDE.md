# Deprecated AI entry point

Status: deprecated.

This file is retained only for tooling that still looks for `.ai/CLAUDE.md`.
It is not an independent project specification and must not override current
repository rules.

Always consult `Agent.md` first, then `.ai/SYSTEM_TRUTH.md`,
`.ai/instructions.md`, `docs/RAF_UI.md`, and `docs/EDITOR_RAFUI.md` before
changing the project. For graphics work also consult
`.ai/APIGRAPHICBASIC.md` and `docs/RENDERER.md`.

It was deprecated because of high costs Antrophic's models, So I decided to Love/move OpenCode and Codex OAI (13082026)

All AI agents must strictly follow the core mandates:
1. Never mention competitors in code or docs.
2. All UI strings translated via i18n (`en.json` and `es.json`).
3. One-pass complete implementation — never split by phases; finish everything end-to-end without stopping ("sin miedo al exito").
4. No targets or builds without user permission. Only if explicitly authorized, run minimal tests at the very end, single crate (`cargo test -p <crate> --lib`, no `--workspace`). FORBIDDEN custom `--target-dir target_*`: only `target_gnu/` is valid. Do not create `target*` in the repo.
5. Do not touch or fix unrelated errors (someone may be working in parallel).
6. Holistic impact analysis: think through how every change affects callers, sibling crates, shared state, and edge cases to avoid bugs.
7. Strict modularity & zero hardcoding (anti-spaghetti & anti-god-code): this is a complex desktop engine, not a web landing page; decompose logic and create new modular files proactively.