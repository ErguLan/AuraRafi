# AI Context Map

This is the short entry point for humans and coding agents. Read these five
files before changing behavior across the editor, UI, renderer, or engine:

1. [`Agent.md`](../Agent.md): repository rules, language conventions, and
   required validation.
2. [`.ai/SYSTEM_TRUTH.md`](../.ai/SYSTEM_TRUTH.md): what is active today,
   what is transitional, and what must not be presented as complete.
3. [`docs/ARCHITECTURE.md`](ARCHITECTURE.md): ownership boundaries and the
   active data/render paths.
4. [`docs/STABILIZATION_STATUS.md`](STABILIZATION_STATUS.md): the current
   stabilization target, risks, and verification state.
5. The domain document that matches the change:
   [`APIGRAPHICBASIC.md`](APIGRAPHICBASIC.md),
   [`RAF_UI.md`](RAF_UI.md), or [`RAF_UI_AUTHORING.md`](RAF_UI_AUTHORING.md).

## Working model

- **AuraRafi Core** is public infrastructure: engine, editor, renderer,
  RAFUI, tools, and their documentation.
- **Games made with the engine are independent works.** Their source, assets,
  lore, branding, and commercial terms belong to their authors unless those
  authors explicitly publish them under a different license.
- **The current mandate through 0.12.0 is stabilization.** Fix editor bugs,
  strengthen RAFUI, and reduce WGPU coupling behind ApiGraphicBasic before
  expanding scope for a game.
- **ApiGraphicBasic owns the graphics contract.** WGPU is the current adapter,
  not the final public engine identity. Keep new renderer code backend-neutral
  and preserve GPU hardware plus CPU fallback.
- **RAFUI is the target UI runtime.** Egui is a temporary shell/bridge while
  retained RAFUI surfaces replace it safely.
- **Low-end hardware is a product requirement.** Prefer retained state,
  bounded caches, incremental work, profiling, and explicit fallbacks over
  always-on heavy systems.

## Change routing

| If the change affects... | Start here | Then verify |
| --- | --- | --- |
| Renderer, viewport, WGPU, CPU fallback | `docs/APIGRAPHICBASIC.md` | `docs/RENDERER.md`, renderer tests |
| RAFUI or editor shell | `docs/RAF_UI.md` | `docs/RAF_UI_AUTHORING.md`, focused UI/editor checks |
| Project status or scope | `docs/STABILIZATION_STATUS.md` | `docs/ROADMAP.md` |
| Cross-domain behavior | `docs/ARCHITECTURE.md` | update every contradicted domain document |

If the five files do not answer a question, inspect the closest code boundary
and document the answer in the relevant domain document. Do not invent a
second source of truth.
