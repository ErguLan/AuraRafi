# Editor Bottom Dock: RafUI Migration Brief

## Design Read

The bottom dock should read like an instrument console: a stable navigation rail
with one warm active edge, quiet graphite surfaces, and enough separation that a
long-running editor session never feels noisy.

## Locked direction

Technical / utilitarian, bound to `.ulpi/design/DESIGN.md`. The dock uses the
existing RafUI palette, Ubuntu text roles, 4px control radius, 8px spacing, and
the active-edge signature. No gradients, decorative badges, or invented engine
states.

## Scope

- Redesign the retained bottom tab strip for Console, Assets, Project Settings,
  and Node Editor.
- Keep AI Agent behavior and visual treatment unchanged in this pass.
- Keep existing command routing, asset scanning, project persistence, console
  history, and node graph mutations.
- Add one coherent Send icon and reuse the existing editor icon family for the
  dock.

## Surface rules

### Bottom tab strip

- Fixed 32px navigation row inside the bottom dock.
- Icon first, label second for migrated tabs.
- Selected tab uses `tokens.accent` as a 2px active edge and a restrained
  raised surface. It does not become a large orange slab.
- Inactive tabs use `surface_alt` with `border`; hover uses `surface_raised`.
- AI Agent remains on its existing text-only path until its dedicated migration.

### Console

- Toolbar is a single row at regular width and wraps without overlap at narrow
  width.
- Send is the only filled primary action. Clear and filters stay subordinate.
- The command field owns the remaining width and keeps a 30px control height.
- Auto-scroll is a compact binary control with a visible knob and label.

### Project Settings

- Every informational and form row owns an explicit vertical track. This avoids
  zero-height rows collapsing labels into one another.
- Labels and values remain a two-column relationship at regular width and stack
  only below the RafUI narrow breakpoint.
- Toggles, ranges, and inputs keep their existing actions and disabled policy.

### Assets and Node Editor

- Preserve current Egui mechanics while the retained shell migration proceeds.
- Apply the locked palette, calmer grid, explicit panel spacing, and higher
  contrast focus states.
- Asset empty, scanning, and status states remain visible and localized.

## State and accessibility contract

- Every tab is keyboard focusable and exposes its stable command ID.
- Selected, hovered, focused, disabled, scanning, empty, and error states use
  semantic RafUI tokens.
- Icon-only graphics are decorative when paired with a visible label; Send keeps
  the visible localized label and adds the icon as a cue.
- No user-facing string is introduced without an i18n key.

## Acceptance criteria

- `AI Agent` source and behavior are untouched.
- Console Send emits the existing `console.submit` command.
- The project settings overview and form rows no longer overlap at desktop or
  narrow widths.
- Bottom tabs render with stable icons, active edge, and no text collision.
- Existing unit tests, `cargo fmt --check`, and `cargo check -p aura_rafi_editor`
  pass.
- The running engine can switch Console, Assets, Project, and Node Editor
  without a panic or lost state.

## Pre-Flight

- [x] Uses only locked palette, Ubuntu roles, 4px controls, and active-edge signature.
- [x] Avoids gradients, generic glass, nested cards, fake metrics, and decorative copy.
- [x] Covers active, hover, focus, disabled, scanning, empty, and error states.
- [x] Preserves keyboard command IDs and localized visible copy.
- [x] Keeps exactly one primary action in Console: Send.
- [x] Does not modify AI Agent.

Design self-critique: distinctiveness 3/4, hierarchy 4/4, consistency 4/4,
accessibility 3/4, state coverage 3/4, copy quality 4/4, restraint 4/4,
motion motivation 4/4. Total 29/32. The remaining 3 points are deferred to the
dedicated Assets and Node Editor RafUI body migrations.
