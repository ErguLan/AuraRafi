//! Pointer arbitration, hover resolution and pointer state helpers.

use super::*;

/// Every pointer button the router can arbitrate.
const POINTER_BUTTONS: [PointerButton; 5] = [
    PointerButton::Primary,
    PointerButton::Secondary,
    PointerButton::Middle,
    PointerButton::Back,
    PointerButton::Forward,
];

pub(super) fn rect_contains(rect: raf_ui::UiRect, point: [f32; 2]) -> bool {
    point[0] >= rect.x
        && point[0] <= rect.x + rect.width
        && point[1] >= rect.y
        && point[1] <= rect.y + rect.height
}

pub(super) fn pan_button_for_press(
    tool: ElectronicsTool,
    space_pan: bool,
    primary_pressed: bool,
    middle_pressed: bool,
) -> Option<PointerButton> {
    if (tool == ElectronicsTool::Pan || space_pan) && primary_pressed {
        Some(PointerButton::Primary)
    } else if middle_pressed {
        Some(PointerButton::Middle)
    } else {
        None
    }
}

/// Maps a CAD object kind onto the editor-owned selection kind.
pub(super) fn selection_kind(kind: CadObjectKind) -> ElectronicsSelectionKind {
    match kind {
        CadObjectKind::Component => ElectronicsSelectionKind::Component,
        CadObjectKind::Pin | CadObjectKind::Pad => ElectronicsSelectionKind::Pin,
        CadObjectKind::Wire => ElectronicsSelectionKind::Wire,
        CadObjectKind::Trace => ElectronicsSelectionKind::Trace,
        _ => ElectronicsSelectionKind::Other,
    }
}

/// True when this canvas itself holds a pointer button.
///
/// Replaces the previous global `!router.has_pointer_capture()` test: a capture
/// held by an unrelated surface (a panel slider, a dock drag, a modal) no longer
/// freezes the viewport, while the canvas still refuses to zoom the world out
/// from under its own in-flight gesture.
pub(super) fn canvas_pointer_busy(router: &InputRouter, owner: InputOwner) -> bool {
    POINTER_BUTTONS
        .into_iter()
        .any(|button| router.pointer_owner(button) == Some(owner))
}

/// True when a different surface owns a pointer button. A passive hover must
/// never fight the surface that is actually being dragged.
pub(super) fn foreign_pointer_capture(router: &InputRouter, owner: InputOwner) -> bool {
    POINTER_BUTTONS.into_iter().any(|button| {
        router
            .pointer_owner(button)
            .is_some_and(|held| held != owner)
    })
}

/// True when the hover pick has to run again for this pointer sample.
pub(super) fn hover_pick_required(previous_world: Option<Vec2>, world: Vec2, zoom: f32) -> bool {
    match previous_world {
        // Re-entering the canvas has no previous sample to compare against, and
        // the frame before it had no hover target to keep.
        None => true,
        Some(previous) => {
            previous.distance(world) * zoom.max(MIN_ZOOM) >= HOVER_REPICK_MIN_DISTANCE_SCREEN
        }
    }
}

/// True once the pointer moved far enough from the press origin to be a drag.
pub(super) fn pointer_left_press_origin(
    router: &InputRouter,
    owner: InputOwner,
    canvas: EditorRect,
    local: Vec2,
) -> bool {
    router
        .pointer_capture(PointerButton::Primary)
        .filter(|capture| capture.owner == owner)
        .and_then(|capture| canvas.local_point(capture.origin))
        .map(|origin| Vec2::from(origin).distance(local) >= PRESS_DRAG_THRESHOLD)
        .unwrap_or(false)
}

impl NativeElectronicsEditor {
    /// World position of the press that started the current primary gesture.
    ///
    /// The pointer capture keeps its origin for the whole gesture, so this is the
    /// stable reference that freezes a vertex grab. It is resolved against the
    /// live camera, which cannot move while the canvas holds the primary button:
    /// the wheel is blocked for this owner and no pan can be armed at the same
    /// time.
    pub(super) fn press_world(
        &self,
        router: &InputRouter,
        owner: InputOwner,
        canvas: EditorRect,
        size: Vec2,
    ) -> Option<Vec2> {
        router
            .pointer_capture(PointerButton::Primary)
            .filter(|capture| capture.owner == owner)
            .and_then(|capture| canvas.local_point(capture.origin))
            .map(|origin| self.camera.world_from_screen(Vec2::from(origin), size))
    }

    /// Refreshes the pointer hover target for one frame.
    ///
    /// Returns `true` when the hover target actually changed, which is the only
    /// reason a redraw is requested: an unchanged hover must never invalidate the
    /// retained chrome.
    pub(super) fn update_pointer_hover(
        &mut self,
        router: &InputRouter,
        owner: InputOwner,
        world: Vec2,
        previous_world: Option<Vec2>,
        over_panel: bool,
        editing: bool,
    ) -> bool {
        // While a gesture is in flight the hover is cleared instead of repainted:
        // the preview and the drag own the cursor, and a highlight that followed
        // the pointer would fight both.
        let target = if editing || over_panel || foreign_pointer_capture(router, owner) {
            None
        } else if hover_pick_required(previous_world, world, self.camera.zoom) {
            let tolerance = PICK_TOLERANCE_SCREEN / self.camera.zoom.max(MIN_ZOOM);
            self.interaction
                .update_hover_editable(&self.scene, world, tolerance)
                .map(|hit| ElectronicsSelection {
                    source_id: hit.selection.source_id.unwrap_or_else(Uuid::nil),
                    kind: selection_kind(hit.selection.kind),
                })
        } else {
            return false;
        };
        let changed = self.hovered != target;
        self.set_hovered(target);
        changed
    }

    /// Commits a drag whose pointer capture disappeared without a release event.
    ///
    /// The document already moved and the user can see it on screen, so dropping
    /// the gesture would leave a persistent change that is in neither the undo
    /// stack nor the dirty flag. The move is committed through the normal
    /// transactional path instead, and the drag is flagged as orphaned so the
    /// record says the gesture lost its grip. `cancel_gesture` must never run for
    /// this case: it would roll the document back and silently undo a movement
    /// the user already performed.
    pub(super) fn commit_lost_drag(&mut self, router: &mut InputRouter, owner: InputOwner) -> bool {
        let Some(mut drag) = self.component_drag.take() else {
            return false;
        };
        drag.orphaned = true;
        let recorded = self.history.record(drag.before, &self.snapshot());
        if recorded {
            self.dirty = true;
        }
        router.release_pointer(PointerButton::Primary, owner);
        self.pointer_world = None;
        self.rebuild_scene();
        self.touch_ui();
        recorded
    }

    pub(super) fn select_secondary_target(&mut self, world: Vec2) {
        // A context menu belongs to the item under the pointer. Select it
        // first so Delete/Duplicate/Properties operate on the same object
        // the user invoked the menu for.
        let tolerance = PICK_TOLERANCE_SCREEN / self.camera.zoom.max(MIN_ZOOM);
        // Editable picking: a painted net label, airwire or DRC marker sitting on
        // top of the real geometry must not become the menu target.
        if let Some(hit) = self
            .interaction
            .select_editable_at(&self.scene, world, tolerance)
        {
            let kind = selection_kind(hit.selection.kind);
            let source_id = hit.selection.source_id.unwrap_or_else(Uuid::nil);
            self.selection = Some(ElectronicsSelection { source_id, kind });
            self.set_hovered(Some(ElectronicsSelection { source_id, kind }));
        } else {
            self.selection = None;
            self.interaction.clear_selection();
        }
    }
}
