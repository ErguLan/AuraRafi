---
project: ProyectRaf
feature: AuraRafi UI Quality Charter
binds_to: .ulpi/design/DESIGN.md
design_system: RafUI retained primitives
---

# The AuraRafi UI Quality Charter

## The heart of AuraRafi

AuraRafi is a precision instrument, not a decorated dashboard. Its interface
must make complex work feel legible, stable, and deliberate. Quality is not a
pile of effects; it is the absence of visual doubt when the user reads,
targets, drags, resizes, or changes state.

The product has five non-negotiable promises:

1. **Crispness:** text, borders, and icons resolve to the physical pixels that
   display them. No accidental blur, pixel crawl, or texture shimmer.
2. **Stability:** hover, focus, selection, and resize change state feedback,
   not the geometry of neighboring controls.
3. **Hierarchy:** the active orange edge tells the user where agency is. It is
   a signal, never wallpaper.
4. **Truth:** every visible status, count, scene label, and preview comes from
   the real document or renderer-owned model.
5. **Restraint:** motion, shadow, radius, and color exist only when they
   improve reading, targeting, or state comprehension.

## Name the failure before fixing it

When an icon or label appears to sparkle while the surface changes, classify
it before changing layout:

- **Pixel shimmer / texture shimmering:** a texture is sampled at changing
  fractional positions or scales across frames.
- **Subpixel jitter:** logical geometry lands on different physical pixels as
  a panel or target changes size.
- **Temporal aliasing / pixel crawl:** a high-contrast edge changes apparent
  shape from frame to frame instead of remaining stable.
- **Texture bleeding:** linear sampling reads neighboring transparent or
  unrelated pixels from an atlas or undersized image.
- **Resampling blur:** a surface is rendered at one density and filtered again
  while being composited at another density.

These are rendering-contract defects. They must not be “fixed” by making the
button larger, adding a glow, increasing contrast everywhere, or replacing a
real icon with a decorative one.

## Crispness contract

Every retained surface follows this pipeline:

```text
logical document
  -> intrinsic measurement and layout
  -> physical target allocation
  -> pixel-snapped UI geometry
  -> one retained GPU/CPU paint
  -> one host composition
```

The logical layout and pointer coordinates remain stable. The renderer converts
all solid, text, and image geometry into the physical target before NDC or CPU
rasterization. The physical target participates in cache identity.

Presentation policy:

- exact physical source/destination dimensions use nearest sampling, including
  rounded fractional-DPI targets;
- linear sampling is reserved for a real size conversion. A fractional host
  origin alone must not turn a crisp retained surface into a blurred image;
- minified UI images use alpha-correct mipmaps and select one complete mip level
  instead of blending adjacent levels;
- text atlas sampling retains a one-pixel guard around slots;
- text may use a bounded source-density floor (1.25x at 100% display scale)
  before resolving into the same physical target. This applies to glyphs only:
  borders, panels, and icons must not inherit a second blur-producing scale;
- semantic text weight must affect atlas coverage. A `Medium` or `Bold`
  request may increase vector coverage, but must not add a soft outline or
  expand the text layout bounds;
- icons are authored as a coherent high-density family and are never stretched
  from a low-resolution source;
- Viewport, Schematic, and PCB keep their renderer-specific sampling policy.

## Quality review, before visual approval

Review each surface at 100%, 125%, 150%, and 200% display scale, in both GPU
and CPU recovery paths. Check the same interaction sequence at each scale:

1. Open the surface without moving the pointer.
2. Hover an icon, select a tab, focus a field, and resize the dock.
3. Watch high-contrast edges and small images for shimmer or point flashes.
4. Leave the control and verify that geometry returns exactly.
5. Compare a static frame before and after the interaction.

Acceptance criteria:

- no icon changes apparent shape when hover state changes;
- no one-pixel border crawls during resize or repaint;
- no text becomes softer merely because another control changed state;
- no tooltip or menu changes its owner surface dimensions;
- no cache key omits physical target size, raster density, or resolved text;
- no visual fix bypasses RafUI ownership by adding an Egui duplicate.

## Ownership rules

`raf_ui` owns semantic nodes, layout, state-neutral style, and input contracts.
`raf_render` owns text measurement, physical geometry, atlas/image sampling,
and GPU/CPU parity. `raf_editor` owns surface recipes and typed application
actions. The compatibility bridge may place a completed texture, but may not
paint retained text, icons, borders, menus, or tooltips itself.

This charter is the visual heart of AuraRafi: precision before decoration,
stability before novelty, and evidence before subjective polish.
