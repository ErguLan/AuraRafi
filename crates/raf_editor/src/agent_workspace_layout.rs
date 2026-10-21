//! Transient workspace geometry and bounded, pointer-owned splitters.
use raf_core::{CaptureMode, InputOwner, InputRegionId, InputRouter, InputSnapshot, PointerButton};
use raf_ui::UiRect;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum WorkspaceDivider {
    Projects,
    Sessions,
    Review,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct WorkspaceLayout {
    pub projects: f32,
    pub sessions: f32,
    pub review: f32,
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn agent_workspace_splitters_keep_chat_and_preview_separate_at_small_sizes() {
        for width in [900.0, 1100.0, 1440.0, 1800.0] {
            for review in [320.0, 480.0, 10000.0] {
                let geometry = WorkspaceLayout {
                    review,
                    ..Default::default()
                }
                .resolve([width, 900.0], true, true);
                let right = geometry.review.unwrap();
                assert!(geometry.chat.width - geometry.sessions >= 340.0);
                assert!(geometry.chat.x + geometry.chat.width <= right.x + 0.01);
                assert!(right.x + right.width <= width);
                let preview = geometry.preview_rect().unwrap();
                assert!(preview.x >= right.x && preview.x + preview.width <= width);
            }
        }
    }
    #[test]
    fn agent_workspace_splitter_capture_is_scoped_and_escape_restores_size() {
        let initial = WorkspaceLayout::default();
        let mut layout = initial;
        let geometry = layout.resolve([1440.0, 900.0], true, true);
        let mut resize = WorkspaceResize::default();
        let mut router = InputRouter::default();
        let mut input = InputSnapshot::default();
        input.pointer_position = Some([geometry.review.unwrap().x, 300.0]);
        input.pointer_pressed_buttons.insert(PointerButton::Primary);
        input.pointer_buttons_down.insert(PointerButton::Primary);
        assert!(resize.input(&input, &mut router, geometry, &mut layout));
        assert!(router.has_exclusive_pointer_capture());
        input.pointer_pressed_buttons.clear();
        input.pointer_position.as_mut().unwrap()[0] -= 100.0;
        resize.input(&input, &mut router, geometry, &mut layout);
        assert_eq!(layout.review, initial.review + 100.0);
        input.keys_pressed.insert(raf_core::InputKey::Escape);
        resize.input(&input, &mut router, geometry, &mut layout);
        assert_eq!(layout, initial);
        assert!(!resize.active());
        assert!(!router.has_pointer_capture());
    }
}
impl Default for WorkspaceLayout {
    fn default() -> Self {
        Self {
            projects: 216.0,
            sessions: 216.0,
            review: 320.0,
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct WorkspaceGeometry {
    pub projects: f32,
    pub sessions: f32,
    pub chat: UiRect,
    pub review: Option<UiRect>,
}
impl WorkspaceLayout {
    pub fn resolve(
        self,
        size: [f32; 2],
        review_open: bool,
        sessions_open: bool,
    ) -> WorkspaceGeometry {
        let width = size[0].max(1.0);
        let projects = self
            .projects
            .clamp(140.0, 320.0)
            .min((width * 0.24).max(140.0));
        let review_width = if review_open {
            self.review
                .clamp(320.0, (width - projects - 340.0).max(320.0))
                .min((width - projects - 200.0).max(0.0))
        } else {
            0.0
        };
        let chat_width = (width - projects - review_width).max(1.0);
        let sessions = if sessions_open && chat_width >= 560.0 {
            self.sessions.clamp(160.0, (chat_width - 340.0).min(340.0))
        } else {
            0.0
        };
        WorkspaceGeometry {
            projects,
            sessions,
            chat: UiRect::new(projects, 52.0, chat_width, (size[1] - 52.0).max(1.0)),
            review: (review_width > 0.0).then(|| {
                UiRect::new(
                    width - review_width,
                    52.0,
                    review_width,
                    (size[1] - 52.0).max(1.0),
                )
            }),
        }
    }
    pub fn adjust(&mut self, divider: WorkspaceDivider, delta: f32, geometry: WorkspaceGeometry) {
        match divider {
            WorkspaceDivider::Projects => {
                self.projects = (geometry.projects + delta).clamp(140.0, 320.0)
            }
            WorkspaceDivider::Sessions => {
                self.sessions = (geometry.sessions + delta).clamp(160.0, 340.0)
            }
            WorkspaceDivider::Review => {
                self.review = (geometry.review.map_or(self.review, |r| r.width) - delta).max(320.0)
            }
        }
    }
}
impl WorkspaceGeometry {
    pub fn preview_rect(self) -> Option<UiRect> {
        self.review.map(|r| {
            UiRect::new(
                r.x + 8.0,
                r.y + 86.0,
                (r.width - 16.0).max(1.0),
                (r.height - 132.0).max(1.0),
            )
        })
    }
    pub fn dividers(self) -> Vec<(WorkspaceDivider, UiRect)> {
        let mut values = vec![(
            WorkspaceDivider::Projects,
            UiRect::new(self.projects - 4.0, 52.0, 8.0, self.chat.height),
        )];
        if self.sessions > 0.0 {
            values.push((
                WorkspaceDivider::Sessions,
                UiRect::new(
                    self.chat.x + self.sessions - 4.0,
                    52.0,
                    8.0,
                    self.chat.height,
                ),
            ));
        }
        if let Some(review) = self.review {
            values.push((
                WorkspaceDivider::Review,
                UiRect::new(review.x - 4.0, 52.0, 8.0, review.height),
            ));
        }
        values
    }
}

#[derive(Default)]
pub(crate) struct WorkspaceResize {
    drag: Option<(WorkspaceDivider, f32, WorkspaceLayout)>,
    pub hovered: Option<WorkspaceDivider>,
}
impl WorkspaceResize {
    fn owner() -> InputOwner {
        InputOwner::Custom(InputRegionId::from_static("agent.workspace.splitter"))
    }
    pub fn cancel(&mut self, router: &mut InputRouter) {
        self.drag = None;
        self.hovered = None;
        router.cancel_owner(Self::owner());
    }
    pub fn active(&self) -> bool {
        self.drag.is_some()
    }
    /// Runs before retained input so a resize can never also edit the chat.
    pub fn input(
        &mut self,
        input: &InputSnapshot,
        router: &mut InputRouter,
        geometry: WorkspaceGeometry,
        layout: &mut WorkspaceLayout,
    ) -> bool {
        if !input.window_focused {
            self.cancel(router);
            return false;
        }
        self.hovered = input.pointer_position.and_then(|p| {
            geometry
                .dividers()
                .into_iter()
                .find(|(_, rect)| rect.contains(p))
                .map(|(kind, _)| kind)
        });
        if input.button_pressed(PointerButton::Primary) {
            if let (Some(kind), Some(point)) = (self.hovered, input.pointer_position) {
                if router.try_capture_pointer(
                    PointerButton::Primary,
                    Self::owner(),
                    CaptureMode::Exclusive,
                    point,
                    input.time_seconds,
                ) {
                    self.drag = Some((kind, point[0], *layout));
                }
            }
        }
        let Some((kind, origin, initial)) = self.drag else {
            return false;
        };
        if input.key_pressed(raf_core::InputKey::Escape) {
            *layout = initial;
            self.cancel(router);
            return true;
        }
        if let Some(point) = input.pointer_position {
            *layout = initial;
            let initial_geometry = initial.resolve(
                [
                    geometry.chat.x
                        + geometry.chat.width
                        + geometry.review.map_or(0.0, |r| r.width),
                    geometry.chat.height + 52.0,
                ],
                geometry.review.is_some(),
                geometry.sessions > 0.0,
            );
            layout.adjust(kind, point[0] - origin, initial_geometry);
        }
        if !input.button_down(PointerButton::Primary)
            || input.button_released(PointerButton::Primary)
        {
            self.cancel(router);
        }
        true
    }
}
