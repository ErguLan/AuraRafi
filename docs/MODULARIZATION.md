# Modularization Pass (2026-11-19)

This pass reduced the worst god files without changing behavior. Every split is
move-only: public paths, command strings, labels, layout constants and tests stay
the same.

## Rule

- Above 1,500 lines: design smell; split before adding features.
- Above 2,500 lines: mandatory split.
- New files stay below 1,200 lines.
- Keep the original file as the facade and declare a same-stem directory for
  submodules. Do not edit shared `mod.rs`/`lib.rs` just to register a split.
- Extract `#[cfg(test)] mod tests` to `tests.rs` first.
- Compile the owning crate after each split.

The full rule lives in `Agent.md` and `.ai/instructions.md`.

## Files split in this pass

| Area | Original | Facade | Main modules |
| --- | ---: | ---: | --- |
| Render | `ApiGraphicBasic/device.rs` 3,919 | 823 | resources, frame, backend_wgpu, backend_cpu, timestamps, tests |
| Render | `ui_surface/gpu_renderer.rs` 3,240 | 206 | pipelines, buffers, text, images, draw, diagnostics, tests |
| Render | `canvas_presenter.rs` 471 | 101 | blit, cpu, metrics |
| Render | `editor_compositor.rs` 183 | 13 | layers, compose, metrics |
| Render | `ApiGraphicBasic/cad_surface.rs` 2,738 | 542 | paint, schematic, overlays, pcb, hit_test, tests |
| Render | `ui_surface/native_window.rs` ~700 | 43 | window, headless, surface, capture |
| Editor | `panels/agent_surface.rs` 5,484 | 294 | layout, history, messages, evidence, composer, models, settings, tools, tests |
| Editor | `agent_executor.rs` 5,157 | 511 | tool_pack, world_build, evidence, schemas, intent, capture, image, execution, scripting, tests |
| Editor | `commands/game.rs` 4,616 + `game/world.rs` 3,208 | 356 + 59 | entity, transform, material, camera, prefab, build, query, world/{plan,compile,kits,brief,commit,cancel,script,tests} |
| Editor | `studio_surface.rs` 4,610 | 274 | hub, project_cards, recent, layout, actions, overlays, styles, tests |
| Editor | `native_application.rs` 4,373 | 263 | event_loop, input, presentation, window, runtime, shortcuts, document, settings, project_settings, layout, studio, tests |
| Editor | `settings_surface.rs` 3,476 | 273 | sections, general, graphics, runtime, hardware, ai, controls, style, tests |
| Editor | `native_workbench_input.rs` 3,419 | 49 | routing, text_fields, nodes, hierarchy, assets, chrome, layout, project, electronics, tests |
| Editor | `panels/assets_surface.rs` 3,173 | 412 | catalog, preview, create, search, menus, modals, tests |
| Editor | `electronics_controller.rs` 2,949 + `_interaction.rs` 2,442 | 881 + 266 | selection, placement, wiring, pcb, history, commands, inspector, library, pointer, drag, canvas, overlay, gestures, tests |
| Editor | `panels/nodes_surface.rs` 2,934 | 339 | canvas, palette, inspector, links, selection, toolbar, tests |
| Editor | `panels/inspector_surface.rs` 2,831 | 719 | fields, components, transform, runtime, behavior, sections, tests |
| Editor | `native_workbench.rs` 2,761 | 864 | layout, metrics, status, sync, toolbar, tests |
| Editor | `agent_workspace_surface.rs` 2,619 | 190 | tools, evidence, layout, sessions, chat, settings, tests |
| Editor | `panels/electronics_toolbar_surface.rs` 2,381 | 533 | actions, menus, pcb, schematic, styles, tests |
| Editor | `panels/hierarchy_surface.rs` 2,404 | 591 | badges, context, dnd, filters, rows, tests |
| Core | `agent_context.rs` 3,571 | 77 | scene, spatial, audit, assets, scripts, project, diagnostics, compact, support, tests |
| Core | `scene/graph.rs` 1,264 | 125 | nodes, hierarchy, queries, transforms, serde_support, tests |
| Core | `config.rs` 1,045 | 409 | editor, graphics, runtime, ai, defaults, tests |
| CLI | `raf_cli/src/main.rs` 4,373 | 20 | cli/{args,attach,commands,mcp,output,tests} |
| Electronics | `pcb/footprint_asset.rs` 2,825 | 34 | types, geometry, validation, parser, library, tests |
| UI | `raf_ui/src/interaction.rs` 2,462 | 54 | pointer, drag, keyboard, focus, scroll, routing, tests |

## Remaining debt (above 1,500 lines)

Tracked, not yet split. Split only with the same move-only method.

| File | Lines |
| --- | ---: |
| `raf_render/src/ApiGraphicBasic/ui_surface/mod.rs` | 2,337 |
| `raf_render/src/ApiGraphicBasic/ui_surface/render.rs` | 2,304 |
| `raf_ai/src/agent_runtime.rs` | 2,291 |
| `raf_editor/src/native_attached_executor.rs` | 2,192 |
| `raf_editor/src/panels/electronics_inspector_surface.rs` | 2,001 |
| `raf_render/src/scene_renderer.rs` | 1,917 |
| `raf_editor/src/panels/electronics_navigator_surface.rs` | 1,880 |
| `raf_editor/src/panels/electronics_footprint_surface.rs` | 1,868 |
| `raf_editor/src/panels/viewport_controller.rs` | 1,827 |
| `raf_editor/src/panels/ai_chat.rs` | 1,763 |
| `raf_editor/src/panels/electronics_surface.rs` | 1,725 |
| `raf_editor/src/agent_service.rs` | 1,649 |
| `raf_editor/src/native_editor_commands.rs` | 1,633 |
| `raf_electronics/src/cad_scene.rs` | 1,607 |

## Verification

- `cargo check` passes for `raf_core`, `raf_electronics`, `raf_assets`, `raf_ui`,
  `raf_net`, `raf_render`, `raf_player`, `raf_cli`, `raf_editor` and the editor
  binary.
- `raf_web` still passes the `wasm32-unknown-unknown` check.
- Tests: core 113, electronics 106, assets 12, net 52, runtime 38, render 375,
  CLI 14, player 1 + streaming smoke. Editor: 663 pass with the same five
  pre-existing failures (agent tool-pack limit, hierarchy AND filter, project
  settings section count). `raf_ui` has two pre-existing icon-registry count
  failures (72 vs 68 and 22 vs 18) unrelated to the interaction split.
