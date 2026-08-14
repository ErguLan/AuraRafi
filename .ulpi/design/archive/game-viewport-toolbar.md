# Game viewport toolbar

## Design read

The viewport toolbar is a compact instrument strip: tool choice is immediate, mode choice is legible, and secondary presentation controls stay quiet until used.

## Locked direction

Use the RafUI technical/utilitarian language from `DESIGN.md` with three visual groups:

1. Transform tools: icon-only Select, Move, Rotate, and Scale controls.
2. View mode: a two-segment `2D / 3D` control with the active mode clearly marked.
3. View helpers: Grid, Focus, and Object mode, followed by a flexible spacer and Reset View at the far edge.

Tool icons use one transparent 256px family: cool white structure with Raf orange detail. The toolbar never uses a text label beside every tool because that creates collisions at compact widths; tooltips remain the accessible label.

## Interaction contract

- Existing commands and viewport state remain unchanged.
- Active transform tools use a raised dark surface with an orange border.
- Active 2D/3D mode remains the single filled orange control.
- Hover raises a neutral control without adding glow or animation.
- Reset remains isolated on the trailing edge.
- The bar stays one row and does not become a scroll region.

## Acceptance

- No text or icon overlap at compact, regular, or high-DPI presentation sizes.
- All eight referenced PNGs are 256x256 with transparent corners.
- Every icon-only control has a tooltip key and stable command ID.
- Existing Game viewport actions still dispatch unchanged.

## RafUI overlay behavior

Tooltip content is authored by the toolbar control but rendered in the global
RafUI tooltip layer. It uses `UiPlacement::BottomStart` with an 8px gap from
the pointer anchor, flips above the anchor near the lower edge, and shifts
inside the window near horizontal edges. The tooltip is content-sized from the
resolved text atlas and never changes the toolbar's row height or button size.

Entry and exit use the locked 120ms `UiMotionSpec::tooltip()` transition. The
temporary eframe bridge may place the completed transparent texture, but it
does not draw the tooltip rectangle or text.
