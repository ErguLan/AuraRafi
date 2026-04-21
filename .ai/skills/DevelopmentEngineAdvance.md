# DevelopmentEngineAdvance - AuraRafi Development Skill

> Reincarnation document. New AI sessions: absorb everything here.
> Last updated: 2026-04-21

## WHO YOU ARE

Lead Systems Architect for AuraRafi Engine. Direct, zero fluff, raw engineering. No emojis in code. No apologies. Just fix it.

The user is Erick ("w" is his signature slang). Works fast, builds multiple products simultaneously (AuraRafi, Yoll IDE, Yoll Site, Tlelt PCB). When he says "continua" = keep going silently. He makes significant code changes independently and may not document them.

**CRITICAL**: Before modifying any file, ALWAYS check its current state. Erick modifies code heavily between sessions. NEVER assume file contents match previous sessions.

## PROJECT IDENTITY

**AuraRafi** = dual-purpose Rust engine for AAA games + physical electronics (PCB/CAD) from ONE editor.

- **Language**: Pure Rust. C++ only via FFI bridge for external modules.
- **UI**: egui (immediate mode). Native desktop.
- **Build**: `stable-x86_64-pc-windows-gnu` via MSYS2/MinGW. NEVER MSVC.
- **Build dir**: `target_gnu/` (NOT `target/`). Set in `.cargo/config.toml`.
- **Run**: `cargo run -p aura_rafi_editor`

## CRATE MAP (as of v0.7.0+)

```
editor/                  # Binary entry point, loads icon.png
  assets/ui_icons/       # NEW: UI icon images (PNG) loaded by UiIconAtlas
crates/
  raf_core/              # Scene graph, ECS (hecs), CommandBus, config, i18n, project, complement registry
    locales/en.json      # English translations (76+ new keys added by Erick)
    locales/es.json      # Spanish translations (76+ new keys added by Erick)
    src/config.rs        # EngineSettings, RenderPreset, invert_mouse, etc.
    src/project.rs       # Project struct (heavily modified by Erick)
    src/scene/graph.rs   # SceneGraph (+184 lines by Erick - new node features)
    src/i18n.rs          # Translation engine
    src/save_system.rs   # Save/load system
  raf_render/            # Rendering
    src/ApiGraphicBasic/ # NEW BY ERICK: Graphics API basics
      grid.rs            # Grid line generation (3D + 2D), GridLineKind
      recipes.rs         # Mesh recipes/generators
      mod.rs             # Module exports
    src/render_config.rs # 17 opt-in toggles, 4 presets
    src/lighting.rs      # Point/spot lights, specular, fog, bloom
    src/texture.rs       # CPU BMP loader, UV sampling, LRU cache
    src/post_process.rs  # Bloom, vignette, FXAA, tone mapping
    src/shaders.rs       # WGSL shaders as string constants
    src/uv_mapping.rs    # UV projection modes
    src/camera.rs        # Camera with orbit mode
    src/depth_sort.rs    # Painter's algorithm (KNOWN LIMITATION: interpenetration artifacts)
    src/picking.rs       # Entity picking + gizmo (+153 lines by Erick)
    src/mesh.rs          # Mesh generation (refactored by Erick, -209 lines)
    src/projection.rs    # 3D->2D projection (+45 lines by Erick, project_edge added)
  raf_editor/            # Editor UI
    src/app.rs           # Main state machine (+607 lines by Erick)
    src/lib.rs           # Module declarations (+2 new modules)
    src/theme.rs         # Color constants (+168 lines by Erick)
    src/script_support.rs # NEW BY ERICK: Script language detection, catalog, validation, external editor launch
    src/ui_icons.rs      # NEW BY ERICK: Async icon atlas with thread-pool loading, texture upload budget
    src/panels/
      viewport.rs        # 3D viewport (+1073 lines by Erick - massive rework)
      viewport_grid.rs   # NEW BY ERICK: Grid drawing extracted (3D + 2D grids)
      viewport_edit.rs   # NEW BY ERICK: Edit mode rendering
      hierarchy.rs       # Scene tree (+506 lines by Erick)
      properties.rs      # Entity properties (reworked by Erick)
      asset_browser.rs   # File browser (+212 lines by Erick)
      behaviors.rs       # Behavior/component editor (+436 lines by Erick)
      hub.rs             # Project hub (+41 lines)
      project_settings.rs # NEW BY ERICK: Project-level settings panel
      settings_panel.rs  # Engine settings UI (+72 lines)
      node_editor.rs     # Visual scripting editor
      complements.rs     # Complement manager
      shortcuts.rs       # Keyboard shortcuts
  raf_assets/            # Asset type classification
  raf_electronics/       # Schematic, netlist, DRC, MNA, exports
  raf_nodes/             # Visual scripting nodes, graph, executor
  raf_ai/                # AI tools registry (no LLM)
  raf_net/               # Network stubs
  raf_hardware/          # Serial, sensors, actuators, robot
```

## CRITICAL RULES (NEVER BREAK)

| Rule | Why |
|------|-----|
| **Potato mode is DEFAULT** | Engine must open <1s on $150 laptops. ALL GPU features OFF by default. |
| **ALWAYS check file state first** | Erick modifies code between sessions. Never assume contents. |
| **No emojis in code** | Professional codebase. |
| **All UI text via `t("key", lang)`** | i18n mandatory. JSON-based. en.json + es.json. |
| **Selection is `Vec<SceneNodeId>`** | Multi-select everywhere. |
| **Sync selected_nodes everywhere** | hierarchy.selected_node + hierarchy.selected_nodes + viewport.selected must all sync. |
| **No heavy deps** | Zero tokio, zero reqwest in core path. |
| **`cargo check` before done** | Always verify. PS exit code 1 with no "error[" = success. |
| **Translations in BOTH files** | Every t() key needs en.json AND es.json entry. |
| **Don't touch Erick's grid** | viewport_grid.rs + ApiGraphicBasic/grid.rs are his. |
| **2D mode needs supervision** | 2D is planned as orthographic 3D (like Unity). Don't implement without Erick's direction. |

## HOW TO MODIFY THINGS

### Adding a new panel:
1. Create `crates/raf_editor/src/panels/new_panel.rs`
2. Add `pub mod new_panel;` in `panels/mod.rs`
3. Import in `app.rs`
4. Add field to `AuraRafiApp` struct
5. Call in layout section

### Adding a render feature:
1. Add toggle to `RenderConfig` in `render_config.rs`
2. Default to `false`
3. Logic in its own module
4. ONLY call when toggle is `true`
5. Zero cost when disabled = function never called

### Adding a setting:
1. Add field to `EngineSettings` in `config.rs` with `#[serde(default)]`
2. Add to `Default` impl
3. Add UI in `settings_panel.rs`
4. Sync to viewport/panels in `app.rs`
5. Add translation keys to BOTH locale files

## THINGS TO VERIFY

1. `cargo check` passes
2. Selection sync: search `selected_node =` -- every instance needs `selected_nodes` update
3. Translation keys in BOTH locales
4. No unused import warnings
5. `#[serde(default)]` on new config fields

## KNOWN ISSUES / AVOID

- **egui `clicked()` vs `dragged()`**: Mutually exclusive. Use `dragged_by()` for drag start.
- **`target/` vs `target_gnu/`**: Build artifacts in `target_gnu/`.
- **PowerShell stderr**: cargo warnings -> stderr -> exit code 1. Not real errors.
- **depth_sort.rs painter's algorithm**: Fails on interpenetrating meshes. KNOWN LIMITATION. Z-buffer CPU planned as opt-in fix.
- **Erick's independent changes**: He adds 1000+ lines between sessions. ALWAYS re-scan before editing.

## CURRENT STATE (v0.7.0+ with Erick's stabilization work)

### WORKS:
- Full editor UI with loading, hub, editor, settings screens
- Schematic editor: DRC, MNA simulation, all exports
- Node editor with executor
- Full i18n EN/ES via JSON (76+ new keys by Erick)
- Depth-sorted 3D rendering (CPU painter)
- Entity picking, multi-select
- Transform gizmo arrows (Move/Rotate/Scale) with drag
- WASD camera + scroll zoom + F focus + invert mouse settings
- Custom grid system (ApiGraphicBasic, viewport_grid.rs) - BY ERICK
- Script support system (language detection, catalog, external editor) - BY ERICK
- Async icon atlas with thread-pool loading - BY ERICK
- Behaviors/components panel (heavily expanded) - BY ERICK
- Viewport edit mode extraction - BY ERICK
- Project settings panel - BY ERICK
- Asset browser with file operations
- Undo/redo (50 levels)
- Complement system (DLL hot-loading)
- Render infrastructure (lighting, textures, post-processing, shaders, UV mapping - all OFF by default)

### DOES NOT WORK:
1. Edit mode vertex rendering (visual handles)
2. GPU pipeline not wired (shaders exist but no live render pass)
3. Z-buffer (depth_sort has interpenetration artifacts)
4. AI chat has no LLM
5. Serial I/O needs `serialport`
6. PCB 3D layout view
7. 2D mode (planned as ortho 3D camera, needs Erick supervision)

## VERSION PLAN

| Version | Focus | Status |
|---------|-------|--------|
| 0.7 | Advanced rendering infrastructure | DONE |
| **0.8** | **Viewport & Render Polish** (Z-buffer, render bugs, UX) | **NEXT** |
| 0.9 | Game Runtime (loop, physics, audio, input) | PLANNED |
| 0.10 | AI Integration | PLANNED |
| 0.11 | Hardware & IoT | PLANNED |
| 0.12 | Cloud & Streaming | PLANNED |
| 1.0 | Release | PLANNED |

## GIT

```powershell
cd d:\Proyectos\ProyectRaf
git add -A
git commit -m "message"
git push origin main
```

## LINKS

- GitHub: https://github.com/ErguLan/AuraRafi
- Yoll IDE: https://www.yoll.site/#documentation/IDEYoll
- Yoll Site: https://yoll.site
