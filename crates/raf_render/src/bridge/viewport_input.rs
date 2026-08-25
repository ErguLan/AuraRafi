//! Backend-neutral input view for Scene viewports.
//!
//! The platform host owns raw events, `raf_core::InputRouter` owns capture,
//! and this module exposes only the filtered state needed by camera and gizmo
//! controllers. No hover state can acquire ownership.

use raf_core::{
    CaptureMode, InputKey, InputKeySet, InputModifiers, InputOwner, InputRouter, InputSnapshot,
    PointerButton, PointerButtonSet,
};

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ViewportInputRect {
    pub origin: [f32; 2],
    pub size: [f32; 2],
}

impl ViewportInputRect {
    pub const fn new(x: f32, y: f32, width: f32, height: f32) -> Self {
        Self {
            origin: [x, y],
            size: [width, height],
        }
    }

    pub fn contains(self, point: [f32; 2]) -> bool {
        point[0] >= self.origin[0]
            && point[1] >= self.origin[1]
            && point[0] < self.origin[0] + self.size[0].max(0.0)
            && point[1] < self.origin[1] + self.size[1].max(0.0)
    }

    pub fn to_local(self, point: [f32; 2]) -> [f32; 2] {
        [point[0] - self.origin[0], point[1] - self.origin[1]]
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ViewportInputFrame {
    pub pointer_local: Option<[f32; 2]>,
    pub pointer_delta: [f32; 2],
    pub scroll_delta: [f32; 2],
    pub keys_down: InputKeySet,
    pub keys_pressed: InputKeySet,
    pub keys_released: InputKeySet,
    pub buttons_down: PointerButtonSet,
    pub buttons_pressed: PointerButtonSet,
    pub buttons_released: PointerButtonSet,
    pub modifiers: InputModifiers,
    pub pointer_enabled: bool,
    pub keyboard_enabled: bool,
    pub time_seconds: f64,
    pub delta_seconds: f32,
}

impl Default for ViewportInputFrame {
    fn default() -> Self {
        Self {
            pointer_local: None,
            pointer_delta: [0.0; 2],
            scroll_delta: [0.0; 2],
            keys_down: InputKeySet::default(),
            keys_pressed: InputKeySet::default(),
            keys_released: InputKeySet::default(),
            buttons_down: PointerButtonSet::default(),
            buttons_pressed: PointerButtonSet::default(),
            buttons_released: PointerButtonSet::default(),
            modifiers: InputModifiers::default(),
            pointer_enabled: false,
            keyboard_enabled: false,
            time_seconds: 0.0,
            delta_seconds: 0.0,
        }
    }
}

impl ViewportInputFrame {
    pub fn from_snapshot(
        input: &InputSnapshot,
        router: &InputRouter,
        rect: ViewportInputRect,
        viewport_active: bool,
    ) -> Self {
        if !input.window_focused {
            return Self::default();
        }

        let pointer_inside = input
            .pointer_position
            .is_some_and(|position| rect.contains(position));
        let viewport_has_capture = [
            PointerButton::Primary,
            PointerButton::Secondary,
            PointerButton::Middle,
        ]
        .into_iter()
        .any(|button| {
            matches!(
                router.pointer_owner(button),
                Some(
                    InputOwner::ViewportCamera
                        | InputOwner::ViewportGizmo
                        | InputOwner::ViewportTool
                )
            )
        });
        let pointer_enabled = pointer_inside || viewport_has_capture;
        let keyboard_enabled = viewport_active
            && matches!(
                router.keyboard_owner(),
                None | Some(
                    InputOwner::ViewportCamera
                        | InputOwner::ViewportGizmo
                        | InputOwner::ViewportTool
                )
            );

        let pointer_local = input.pointer_position.and_then(|position| {
            (pointer_enabled && (pointer_inside || viewport_has_capture))
                .then(|| rect.to_local(position))
        });

        let mut buttons_down = PointerButtonSet::default();
        let mut buttons_pressed = PointerButtonSet::default();
        let mut buttons_released = PointerButtonSet::default();
        if pointer_enabled {
            for button in [
                PointerButton::Primary,
                PointerButton::Secondary,
                PointerButton::Middle,
            ] {
                let available = matches!(
                    router.pointer_owner(button),
                    None | Some(
                        InputOwner::ViewportCamera
                            | InputOwner::ViewportGizmo
                            | InputOwner::ViewportTool
                    )
                );
                if !available {
                    continue;
                }
                if input.button_down(button) {
                    buttons_down.insert(button);
                }
                if input.button_pressed(button) {
                    buttons_pressed.insert(button);
                }
                if input.button_released(button) {
                    buttons_released.insert(button);
                }
            }
        }

        Self {
            pointer_local,
            pointer_delta: if pointer_enabled {
                input.pointer_delta
            } else {
                [0.0; 2]
            },
            scroll_delta: if pointer_inside {
                input.scroll_delta
            } else {
                [0.0; 2]
            },
            keys_down: if keyboard_enabled {
                input.keys_down
            } else {
                InputKeySet::default()
            },
            keys_pressed: if keyboard_enabled {
                input.keys_pressed
            } else {
                InputKeySet::default()
            },
            keys_released: if keyboard_enabled {
                input.keys_released
            } else {
                InputKeySet::default()
            },
            buttons_down,
            buttons_pressed,
            buttons_released,
            modifiers: input.modifiers,
            pointer_enabled,
            keyboard_enabled,
            time_seconds: input.time_seconds,
            delta_seconds: input.delta_seconds,
        }
    }

    pub fn key_down(self, key: InputKey) -> bool {
        self.keyboard_enabled && self.keys_down.contains(key)
    }

    pub fn key_pressed(self, key: InputKey) -> bool {
        self.keyboard_enabled && self.keys_pressed.contains(key)
    }

    pub fn button_down(self, button: PointerButton) -> bool {
        self.pointer_enabled && self.buttons_down.contains(button)
    }

    pub fn button_pressed(self, button: PointerButton) -> bool {
        self.pointer_enabled && self.buttons_pressed.contains(button)
    }

    pub fn button_released(self, button: PointerButton) -> bool {
        self.pointer_enabled && self.buttons_released.contains(button)
    }

    pub fn fly_axis(self) -> [f32; 3] {
        if !self.keyboard_enabled {
            return [0.0; 3];
        }
        let forward = bool_axis(self.key_down(InputKey::W), self.key_down(InputKey::S));
        let right = bool_axis(self.key_down(InputKey::D), self.key_down(InputKey::A));
        let up = bool_axis(self.key_down(InputKey::E), self.key_down(InputKey::Q));
        [right, up, forward]
    }

    pub fn has_camera_motion(self) -> bool {
        let axis = self.fly_axis();
        axis != [0.0; 3]
            || self.button_down(PointerButton::Secondary)
            || self.button_down(PointerButton::Middle)
            || self.scroll_delta != [0.0; 2]
    }

    pub fn requires_continuous_redraw(self) -> bool {
        self.has_camera_motion()
            || self.button_down(PointerButton::Primary)
            || self.pointer_delta != [0.0; 2]
    }
}

pub fn try_capture_camera(
    router: &mut InputRouter,
    input: &InputSnapshot,
    rect: ViewportInputRect,
    button: PointerButton,
) -> bool {
    if !input.button_pressed(button) {
        return false;
    }
    let Some(position) = input.pointer_position.filter(|point| rect.contains(*point)) else {
        return false;
    };
    router.try_capture_pointer(
        button,
        InputOwner::ViewportCamera,
        CaptureMode::Exclusive,
        position,
        input.time_seconds,
    )
}

pub fn try_capture_gizmo(
    router: &mut InputRouter,
    input: &InputSnapshot,
    rect: ViewportInputRect,
    gizmo_hit: bool,
) -> bool {
    if !gizmo_hit || !input.button_pressed(PointerButton::Primary) {
        return false;
    }
    let Some(position) = input.pointer_position.filter(|point| rect.contains(*point)) else {
        return false;
    };
    router.try_capture_pointer(
        PointerButton::Primary,
        InputOwner::ViewportGizmo,
        CaptureMode::Exclusive,
        position,
        input.time_seconds,
    )
}

pub fn try_capture_viewport_tool(
    router: &mut InputRouter,
    input: &InputSnapshot,
    rect: ViewportInputRect,
) -> bool {
    if !input.button_pressed(PointerButton::Primary) {
        return false;
    }
    let Some(position) = input.pointer_position.filter(|point| rect.contains(*point)) else {
        return false;
    };
    router.try_capture_pointer(
        PointerButton::Primary,
        InputOwner::ViewportTool,
        CaptureMode::Exclusive,
        position,
        input.time_seconds,
    )
}

fn bool_axis(positive: bool, negative: bool) -> f32 {
    (positive as i8 - negative as i8) as f32
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wasd_axis_keeps_w_forward_and_s_backward() {
        let mut frame = ViewportInputFrame {
            keyboard_enabled: true,
            ..ViewportInputFrame::default()
        };

        frame.keys_down.insert(InputKey::W);
        assert_eq!(frame.fly_axis()[2], 1.0);

        frame.keys_down.clear();
        frame.keys_down.insert(InputKey::S);
        assert_eq!(frame.fly_axis()[2], -1.0);
    }
}
