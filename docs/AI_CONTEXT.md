# AI Context Map

This is the short entry point for humans and coding agents. Read these six
files before changing behavior across the editor, UI, renderer, or engine:

1. [`Agent.md`](../Agent.md): repository rules, language conventions, and
   required validation.
2. [`.ai/SYSTEM_TRUTH.md`](../.ai/SYSTEM_TRUTH.md): what is active today,
   what is transitional, and what must not be presented as complete.
3. [`docs/ARCHITECTURE.md`](ARCHITECTURE.md): ownership boundaries and the
   active data/render paths.
4. [`.ai/STUDIO_GRADE_UI.md`](../.ai/STUDIO_GRADE_UI.md): product-wide visual
   defaults, reference handling, and UI quality criteria.
5. The domain document that matches the change:
   [`APIGRAPHICBASIC.md`](APIGRAPHICBASIC.md), [`RENDERER.md`](RENDERER.md),
   [`RAF_UI.md`](RAF_UI.md), or [`EDITOR_RAFUI.md`](EDITOR_RAFUI.md).

`docs/STABILIZATION_STATUS.md` is a project status record. It is not a visual
authority and is not part of the default design-reading chain. Consult it only
when the task specifically concerns historical stabilization status or its
verification record.

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
- **RafUI is the active UI runtime.** The native Winit host composes retained
  RafUI surfaces and ApiGraphicBasic canvas layers. The retired widget toolkit
  is historical migration evidence only and is not a runtime dependency.
- **Low-end hardware is a product requirement.** Prefer retained state,
  bounded caches, incremental work, profiling, and explicit fallbacks over
  always-on heavy systems.
- **Viewport baseline (2026-07-19).** The active scene path has CPU/GPU line
  batching, bounded persistent mesh reuse, physical-pixel targets, world-scale
  culling, and orthographic 3D View2D. `Sprite2D` is only a legacy alias to
  `Plane`; advanced effects remain separate, explicitly scoped capabilities.

## Change routing

| If the change affects... | Start here | Then verify |
| --- | --- | --- |
| Renderer, viewport, WGPU, CPU fallback | `docs/APIGRAPHICBASIC.md` | `docs/RENDERER.md`, renderer tests |
| RAFUI core or editor shell | `.ai/STUDIO_GRADE_UI.md` and `docs/RAF_UI.md` | `docs/EDITOR_RAFUI.md`, focused UI/editor checks |
| Project status or scope | `docs/STABILIZATION_STATUS.md` | `docs/ROADMAP.md` |
| Cross-domain behavior | `docs/ARCHITECTURE.md` | update every contradicted domain document |

If the six files do not answer a question, inspect the closest code boundary
and document the answer in the relevant domain document. Do not invent a
second source of truth.
