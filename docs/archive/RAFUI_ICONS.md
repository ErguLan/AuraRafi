# RafUI icon family

RafUI surfaces use `raf_ui::UiIconId` as the stable icon contract. Panels do
not know whether the renderer uses WGPU or the ApiGraphicBasic CPU path, and
they do not carry paths to individual image files.

## Sources and runtime

- Editable masters: `crates/raf_render/assets/ui_icons/svg/`
- Runtime PNGs: `crates/raf_render/assets/ui_icons/png/`
- Application-bar PNGs: `editor/assets/ui_icons/top/`
- Application-bar SVG masters: `editor/assets/ui_icons/top/svg/`
- Generator: `tools/ui_assets/generate_rafui_builtin_icons.py`

The family is intentionally white, linear, transparent, and independent from
the orange product accent. State is communicated by the surrounding control,
not by changing the artwork of the icon.

ApiGraphicBasic decodes each generated PNG once into its image store. WGPU and
the CPU fallback consume the same decoded pixels; the icons are not added to
the text atlas and are not loaded once per hierarchy row. The SVGs remain the
source of truth for future density or visual refinements.

Regenerate the family from the repository root with:

```text
python tools/ui_assets/generate_rafui_builtin_icons.py
```

If a new semantic icon is added, update `UiIconId`, the generated icon list,
and the renderer asset resolver in the same change.
