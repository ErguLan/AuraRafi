//! Backend-neutral editor and viewport input contracts.
//!
//! Platform adapters populate [`InputSnapshot`]. RafUI, the Game viewport,
//! Electronics canvases, shortcuts, and automation-facing editor commands
//! consume that snapshot without importing a windowing or widget toolkit.
//! Pointer ownership is acquired on a real button press and remains stable
//! until release or cancellation; passive hover never captures input.

use serde::{Deserialize, Serialize};

/// Canonical non-text keys understood by editor commands and viewport tools.
///
/// Text entry is carried separately by [`InputSnapshot::text_input`] and IME
/// composition, so this enum stays compact and layout-independent.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[repr(u8)]
pub enum InputKey {
    A,
    B,
    C,
    D,
    E,
    F,
    G,
    H,
    I,
    J,
    K,
    L,
    M,
    N,
    O,
    P,
    Q,
    R,
    S,
    T,
    U,
    V,
    W,
    X,
    Y,
    Z,
    Digit0,
    Digit1,
    Digit2,
    Digit3,
    Digit4,
    Digit5,
    Digit6,
    Digit7,
    Digit8,
    Digit9,
    Escape,
    Enter,
    Tab,
    Space,
    Backspace,
    Delete,
    Insert,
    Home,
    End,
    PageUp,
    PageDown,
    ArrowUp,
    ArrowDown,
    ArrowLeft,
    ArrowRight,
    F1,
    F2,
    F3,
    F4,
    F5,
    F6,
    F7,
    F8,
    F9,
    F10,
    F11,
    F12,
    Minus,
    Equal,
    BracketLeft,
    BracketRight,
    Backslash,
    Semicolon,
    Quote,
    Backquote,
    Comma,
    Period,
    Slash,
    Numpad0,
    Numpad1,
    Numpad2,
    Numpad3,
    Numpad4,
    Numpad5,
    Numpad6,
    Numpad7,
    Numpad8,
    Numpad9,
    NumpadAdd,
    NumpadSubtract,
    NumpadMultiply,
    NumpadDivide,
    NumpadDecimal,
}

impl InputKey {
    const COUNT: usize = InputKey::NumpadDecimal as usize + 1;

    const fn index(self) -> usize {
        self as usize
    }
}

/// Fixed-size key set used in the frame hot path.
///
/// It avoids allocating or hashing on every key query while leaving enough
/// room for the complete canonical key list.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct InputKeySet {
    words: [u64; 2],
}

impl Default for InputKeySet {
    fn default() -> Self {
        debug_assert!(InputKey::COUNT <= 128);
        Self { words: [0; 2] }
    }
}

impl InputKeySet {
    pub const fn empty() -> Self {
        Self { words: [0; 2] }
    }

    pub fn insert(&mut self, key: InputKey) {
        let index = key.index();
        self.words[index / 64] |= 1_u64 << (index % 64);
    }

    pub fn remove(&mut self, key: InputKey) {
        let index = key.index();
        self.words[index / 64] &= !(1_u64 << (index % 64));
    }

    pub fn contains(&self, key: InputKey) -> bool {
        let index = key.index();
        self.words[index / 64] & (1_u64 << (index % 64)) != 0
    }

    pub fn clear(&mut self) {
        self.words = [0; 2];
    }

    pub fn is_empty(&self) -> bool {
        self.words == [0; 2]
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct InputModifiers {
    pub shift: bool,
    pub control: bool,
    pub alt: bool,
    pub command: bool,
}

impl InputModifiers {
    pub fn command_modifier(self) -> bool {
        self.control || self.command
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[repr(u8)]
pub enum PointerButton {
    Primary,
    Secondary,
    Middle,
    Back,
    Forward,
}

impl PointerButton {
    const COUNT: usize = PointerButton::Forward as usize + 1;

    const fn index(self) -> usize {
        self as usize
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct PointerButtonSet(u8);

impl PointerButtonSet {
    pub const fn empty() -> Self {
        Self(0)
    }

    pub fn insert(&mut self, button: PointerButton) {
        self.0 |= 1_u8 << button.index();
    }

    pub fn remove(&mut self, button: PointerButton) {
        self.0 &= !(1_u8 << button.index());
    }

    pub fn contains(self, button: PointerButton) -> bool {
        self.0 & (1_u8 << button.index()) != 0
    }

    pub fn clear(&mut self) {
        self.0 = 0;
    }

    pub fn is_empty(self) -> bool {
        self.0 == 0
    }
}

/// Immutable input state consumed during one editor frame.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct InputSnapshot {
    pub keys_down: InputKeySet,
    pub keys_pressed: InputKeySet,
    pub keys_released: InputKeySet,
    pub pointer_buttons_down: PointerButtonSet,
    pub pointer_pressed_buttons: PointerButtonSet,
    pub pointer_released_buttons: PointerButtonSet,
    /// Logical window coordinates.
    pub pointer_position: Option<[f32; 2]>,
    /// Logical delta accumulated since the previous consumed frame.
    pub pointer_delta: [f32; 2],
    pub scroll_delta: [f32; 2],
    pub modifiers: InputModifiers,
    /// Committed text. Commands and movement must never infer keys from it.
    pub text_input: String,
    /// Current IME pre-edit string, if any.
    pub ime_preedit: String,
    pub window_focused: bool,
    pub time_seconds: f64,
    pub delta_seconds: f32,
}

impl Default for InputSnapshot {
    fn default() -> Self {
        Self {
            keys_down: InputKeySet::default(),
            keys_pressed: InputKeySet::default(),
            keys_released: InputKeySet::default(),
            pointer_buttons_down: PointerButtonSet::default(),
            pointer_pressed_buttons: PointerButtonSet::default(),
            pointer_released_buttons: PointerButtonSet::default(),
            pointer_position: None,
            pointer_delta: [0.0; 2],
            scroll_delta: [0.0; 2],
            modifiers: InputModifiers::default(),
            text_input: String::new(),
            ime_preedit: String::new(),
            window_focused: true,
            time_seconds: 0.0,
            delta_seconds: 0.0,
        }
    }
}

impl InputSnapshot {
    pub fn key_down(&self, key: InputKey) -> bool {
        self.keys_down.contains(key)
    }

    pub fn key_pressed(&self, key: InputKey) -> bool {
        self.keys_pressed.contains(key)
    }

    pub fn key_released(&self, key: InputKey) -> bool {
        self.keys_released.contains(key)
    }

    pub fn button_down(&self, button: PointerButton) -> bool {
        self.pointer_buttons_down.contains(button)
    }

    pub fn button_pressed(&self, button: PointerButton) -> bool {
        self.pointer_pressed_buttons.contains(button)
    }

    pub fn button_released(&self, button: PointerButton) -> bool {
        self.pointer_released_buttons.contains(button)
    }

    pub fn clear_transient(&mut self) {
        self.keys_pressed.clear();
        self.keys_released.clear();
        self.pointer_pressed_buttons.clear();
        self.pointer_released_buttons.clear();
        self.pointer_delta = [0.0; 2];
        self.scroll_delta = [0.0; 2];
        self.text_input.clear();
    }

    pub fn cancel_all(&mut self) {
        self.clear_transient();
        self.keys_down.clear();
        self.pointer_buttons_down.clear();
        self.modifiers = InputModifiers::default();
        self.ime_preedit.clear();
    }
}

/// Stable semantic region identifier without retaining or hashing a string in
/// the frame hot path.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct InputRegionId(pub u64);

impl InputRegionId {
    /// FNV-1a hash suitable for stable compile-time IDs from semantic names.
    pub const fn from_static(name: &str) -> Self {
        let bytes = name.as_bytes();
        let mut hash = 0xcbf29ce484222325_u64;
        let mut index = 0;
        while index < bytes.len() {
            hash ^= bytes[index] as u64;
            hash = hash.wrapping_mul(0x100000001b3);
            index += 1;
        }
        Self(hash)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum InputOwner {
    RetainedUi(InputRegionId),
    ViewportCamera,
    ViewportGizmo,
    ViewportTool,
    ElectronicsCanvas,
    Modal(InputRegionId),
    DragDrop(InputRegionId),
    Custom(InputRegionId),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum CaptureMode {
    /// Other buttons may be captured by another owner.
    PerButton,
    /// No other owner may capture any pointer button until release.
    Exclusive,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct PointerCapture {
    pub owner: InputOwner,
    pub mode: CaptureMode,
    pub origin: [f32; 2],
    pub started_at_seconds: f64,
    pub generation: u64,
}

/// Cross-surface pointer and keyboard arbiter.
///
/// A capture can only be acquired explicitly by a press handler. Querying
/// hover state never mutates this router.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct InputRouter {
    pointer: [Option<PointerCapture>; PointerButton::COUNT],
    keyboard: Option<InputOwner>,
    next_generation: u64,
}

impl Default for InputRouter {
    fn default() -> Self {
        Self {
            pointer: [None; PointerButton::COUNT],
            keyboard: None,
            next_generation: 1,
        }
    }
}

impl InputRouter {
    pub fn pointer_owner(&self, button: PointerButton) -> Option<InputOwner> {
        self.pointer[button.index()].map(|capture| capture.owner)
    }

    pub fn pointer_capture(&self, button: PointerButton) -> Option<PointerCapture> {
        self.pointer[button.index()]
    }

    pub fn keyboard_owner(&self) -> Option<InputOwner> {
        self.keyboard
    }

    pub fn has_pointer_capture(&self) -> bool {
        self.pointer.iter().any(Option::is_some)
    }

    pub fn has_exclusive_pointer_capture(&self) -> bool {
        self.pointer
            .iter()
            .flatten()
            .any(|capture| capture.mode == CaptureMode::Exclusive)
    }

    pub fn exclusive_pointer_owner(&self) -> Option<InputOwner> {
        self.pointer
            .iter()
            .flatten()
            .find(|capture| capture.mode == CaptureMode::Exclusive)
            .map(|capture| capture.owner)
    }

    pub fn is_pointer_owned_by(&self, button: PointerButton, owner: InputOwner) -> bool {
        self.pointer_owner(button) == Some(owner)
    }

    pub fn try_capture_pointer(
        &mut self,
        button: PointerButton,
        owner: InputOwner,
        mode: CaptureMode,
        origin: [f32; 2],
        time_seconds: f64,
    ) -> bool {
        if self.is_pointer_owned_by(button, owner) {
            return true;
        }
        if self.pointer[button.index()].is_some() {
            return false;
        }

        let another_owner_is_exclusive = self
            .pointer
            .iter()
            .flatten()
            .any(|capture| capture.owner != owner && capture.mode == CaptureMode::Exclusive);
        if another_owner_is_exclusive {
            return false;
        }
        if mode == CaptureMode::Exclusive
            && self
                .pointer
                .iter()
                .flatten()
                .any(|capture| capture.owner != owner)
        {
            return false;
        }

        let generation = self.next_generation;
        self.next_generation = self.next_generation.wrapping_add(1).max(1);
        self.pointer[button.index()] = Some(PointerCapture {
            owner,
            mode,
            origin,
            started_at_seconds: time_seconds.max(0.0),
            generation,
        });
        true
    }

    pub fn release_pointer(&mut self, button: PointerButton, owner: InputOwner) -> bool {
        if !self.is_pointer_owned_by(button, owner) {
            return false;
        }
        self.pointer[button.index()] = None;
        true
    }

    /// Releases a button after the owning surface has received its release
    /// event. Platform adapters use this for focus-loss recovery as well.
    pub fn release_pointer_unchecked(&mut self, button: PointerButton) {
        self.pointer[button.index()] = None;
    }

    pub fn try_capture_keyboard(&mut self, owner: InputOwner) -> bool {
        match self.keyboard {
            Some(current) => current == owner,
            None => {
                self.keyboard = Some(owner);
                true
            }
        }
    }

    pub fn release_keyboard(&mut self, owner: InputOwner) -> bool {
        if self.keyboard != Some(owner) {
            return false;
        }
        self.keyboard = None;
        true
    }

    pub fn cancel_owner(&mut self, owner: InputOwner) {
        for capture in &mut self.pointer {
            if capture.is_some_and(|capture| capture.owner == owner) {
                *capture = None;
            }
        }
        if self.keyboard == Some(owner) {
            self.keyboard = None;
        }
    }

    pub fn cancel_all(&mut self) {
        self.pointer.fill(None);
        self.keyboard = None;
    }

    /// Reconciles captures before a new frame is routed to any consumer.
    ///
    /// `finish_frame` normally releases these captures after the UI and
    /// viewport have seen the snapshot. A native surface can nevertheless be
    /// replaced between two consumers (for example when a dock tab changes)
    /// or after a focus transition. Clearing captures that no longer have a
    /// physical button down prevents an obsolete owner from starving every
    /// following surface.
    pub fn reconcile_input(&mut self, input: &InputSnapshot) {
        if !input.window_focused {
            self.cancel_all();
            return;
        }
        for button in [
            PointerButton::Primary,
            PointerButton::Secondary,
            PointerButton::Middle,
            PointerButton::Back,
            PointerButton::Forward,
        ] {
            if input.button_pressed(button) || !input.button_down(button) {
                self.release_pointer_unchecked(button);
            }
        }
    }

    /// Enforces release/focus invariants after all consumers process a frame.
    pub fn finish_frame(&mut self, input: &InputSnapshot) {
        if !input.window_focused {
            self.cancel_all();
            return;
        }
        for button in [
            PointerButton::Primary,
            PointerButton::Secondary,
            PointerButton::Middle,
            PointerButton::Back,
            PointerButton::Forward,
        ] {
            if input.button_released(button) || !input.button_down(button) {
                self.release_pointer_unchecked(button);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reconcile_input_releases_capture_when_a_new_press_replaces_a_stale_owner() {
        let owner = InputOwner::ViewportGizmo;
        let mut router = InputRouter::default();
        assert!(router.try_capture_pointer(
            PointerButton::Primary,
            owner,
            CaptureMode::Exclusive,
            [10.0, 10.0],
            1.0,
        ));

        let mut input = InputSnapshot::default();
        input.pointer_buttons_down.insert(PointerButton::Primary);
        input.pointer_pressed_buttons.insert(PointerButton::Primary);
        router.reconcile_input(&input);

        assert_eq!(router.pointer_owner(PointerButton::Primary), None);
    }

    #[test]
    fn reconcile_input_cancels_every_capture_when_the_window_is_unfocused() {
        let owner = InputOwner::ViewportCamera;
        let mut router = InputRouter::default();
        assert!(router.try_capture_pointer(
            PointerButton::Middle,
            owner,
            CaptureMode::PerButton,
            [0.0, 0.0],
            1.0,
        ));
        assert!(router.try_capture_keyboard(owner));

        let input = InputSnapshot {
            window_focused: false,
            ..InputSnapshot::default()
        };
        router.reconcile_input(&input);

        assert!(!router.has_pointer_capture());
        assert_eq!(router.keyboard_owner(), None);
    }
}
