//! Authoritative input state carried by the streaming protocol.
//!
//! The client sends the whole held-key and pointer state with every frame.
//! The server derives press and release edges by diffing against the previous
//! state, so a lost packet never leaves a key stuck or hides a release: the
//! next received frame is authoritative.

use crate::codec::{PayloadReader, PayloadWriter};
use crate::protocol::MAX_HELD_KEYS;
use raf_core::{InputKey, InputModifiers, InputSnapshot, PointerButton};
use std::fmt;
use std::io;

/// Absolute limit applied to pointer position, delta and wheel values.
pub const MAX_POINTER_ABS: f32 = 1_000_000.0;

/// Primary pointer button mask bit.
pub const BUTTON_PRIMARY: u8 = 1 << 0;
/// Secondary pointer button mask bit.
pub const BUTTON_SECONDARY: u8 = 1 << 1;
/// Middle pointer button mask bit.
pub const BUTTON_MIDDLE: u8 = 1 << 2;
/// Back pointer button mask bit.
pub const BUTTON_BACK: u8 = 1 << 3;
/// Forward pointer button mask bit.
pub const BUTTON_FORWARD: u8 = 1 << 4;

/// All defined pointer button bits.
pub const BUTTON_MASK: u8 =
    BUTTON_PRIMARY | BUTTON_SECONDARY | BUTTON_MIDDLE | BUTTON_BACK | BUTTON_FORWARD;

/// Control modifier mask bit.
pub const MODIFIER_CONTROL: u8 = 1 << 0;
/// Shift modifier mask bit.
pub const MODIFIER_SHIFT: u8 = 1 << 1;
/// Alt modifier mask bit.
pub const MODIFIER_ALT: u8 = 1 << 2;
/// Command modifier mask bit.
pub const MODIFIER_COMMAND: u8 = 1 << 3;

/// All defined modifier bits.
pub const MODIFIER_MASK: u8 = MODIFIER_CONTROL | MODIFIER_SHIFT | MODIFIER_ALT | MODIFIER_COMMAND;

const BUTTONS: [(PointerButton, u8); 5] = [
    (PointerButton::Primary, BUTTON_PRIMARY),
    (PointerButton::Secondary, BUTTON_SECONDARY),
    (PointerButton::Middle, BUTTON_MIDDLE),
    (PointerButton::Back, BUTTON_BACK),
    (PointerButton::Forward, BUTTON_FORWARD),
];

/// Canonical key table indexed by wire code.
///
/// The code is the `raf_core::InputKey` discriminant. Keeping the table here
/// avoids a hundred-arm match while still validating every incoming code.
const INPUT_KEYS: [InputKey; 89] = [
    InputKey::A,
    InputKey::B,
    InputKey::C,
    InputKey::D,
    InputKey::E,
    InputKey::F,
    InputKey::G,
    InputKey::H,
    InputKey::I,
    InputKey::J,
    InputKey::K,
    InputKey::L,
    InputKey::M,
    InputKey::N,
    InputKey::O,
    InputKey::P,
    InputKey::Q,
    InputKey::R,
    InputKey::S,
    InputKey::T,
    InputKey::U,
    InputKey::V,
    InputKey::W,
    InputKey::X,
    InputKey::Y,
    InputKey::Z,
    InputKey::Digit0,
    InputKey::Digit1,
    InputKey::Digit2,
    InputKey::Digit3,
    InputKey::Digit4,
    InputKey::Digit5,
    InputKey::Digit6,
    InputKey::Digit7,
    InputKey::Digit8,
    InputKey::Digit9,
    InputKey::Escape,
    InputKey::Enter,
    InputKey::Tab,
    InputKey::Space,
    InputKey::Backspace,
    InputKey::Delete,
    InputKey::Insert,
    InputKey::Home,
    InputKey::End,
    InputKey::PageUp,
    InputKey::PageDown,
    InputKey::ArrowUp,
    InputKey::ArrowDown,
    InputKey::ArrowLeft,
    InputKey::ArrowRight,
    InputKey::F1,
    InputKey::F2,
    InputKey::F3,
    InputKey::F4,
    InputKey::F5,
    InputKey::F6,
    InputKey::F7,
    InputKey::F8,
    InputKey::F9,
    InputKey::F10,
    InputKey::F11,
    InputKey::F12,
    InputKey::Minus,
    InputKey::Equal,
    InputKey::BracketLeft,
    InputKey::BracketRight,
    InputKey::Backslash,
    InputKey::Semicolon,
    InputKey::Quote,
    InputKey::Backquote,
    InputKey::Comma,
    InputKey::Period,
    InputKey::Slash,
    InputKey::Numpad0,
    InputKey::Numpad1,
    InputKey::Numpad2,
    InputKey::Numpad3,
    InputKey::Numpad4,
    InputKey::Numpad5,
    InputKey::Numpad6,
    InputKey::Numpad7,
    InputKey::Numpad8,
    InputKey::Numpad9,
    InputKey::NumpadAdd,
    InputKey::NumpadSubtract,
    InputKey::NumpadMultiply,
    InputKey::NumpadDivide,
    InputKey::NumpadDecimal,
];

/// Wire code for a canonical key.
pub fn input_key_code(key: InputKey) -> u16 {
    key as u16
}

/// Resolve a wire key code without trusting the peer.
pub fn input_key_from_code(code: u16) -> Option<InputKey> {
    INPUT_KEYS
        .get(code as usize)
        .copied()
        .filter(|key| input_key_code(*key) == code)
}

/// One authoritative input state sample.
#[derive(Debug, Clone, PartialEq)]
pub struct InputFrame {
    /// Strictly increasing per client.
    pub seq: u32,
    /// Client wall-clock milliseconds, used for input latency accounting.
    pub client_millis: u64,
    /// Held keys as canonical [`InputKey`] codes.
    pub held_keys: Vec<u16>,
    /// Logical pointer position.
    pub pointer: [f32; 2],
    /// Pointer movement accumulated since the previous sample.
    pub pointer_delta: [f32; 2],
    /// Wheel movement accumulated since the previous sample.
    pub wheel: [f32; 2],
    /// Bit 0 primary, 1 secondary, 2 middle, 3 back, 4 forward.
    pub buttons: u8,
    /// Bit 0 control, 1 shift, 2 alt, 3 command.
    pub modifiers: u8,
}

impl Default for InputFrame {
    fn default() -> Self {
        Self {
            seq: 0,
            client_millis: 0,
            held_keys: Vec::new(),
            pointer: [0.0; 2],
            pointer_delta: [0.0; 2],
            wheel: [0.0; 2],
            buttons: 0,
            modifiers: 0,
        }
    }
}

impl InputFrame {
    pub fn new(seq: u32, client_millis: u64) -> Self {
        Self {
            seq,
            client_millis,
            ..Self::default()
        }
    }

    /// Capture the authoritative state from an editor/runtime snapshot.
    ///
    /// `held_keys` is emitted in ascending canonical code order. The caller
    /// supplies the monotonic sequence and timestamp. The returned delta uses
    /// `InputSnapshot::pointer_delta`; a client that runs with a captured
    /// cursor can overwrite it with `relative_pointer_delta` before sending.
    pub fn from_snapshot(seq: u32, client_millis: u64, snapshot: &InputSnapshot) -> Self {
        let mut held_keys = Vec::new();
        for &key in &INPUT_KEYS {
            if snapshot.keys_down.contains(key) {
                held_keys.push(input_key_code(key));
            }
        }
        let mut buttons = 0u8;
        for &(button, mask) in &BUTTONS {
            if snapshot.pointer_buttons_down.contains(button) {
                buttons |= mask;
            }
        }
        let modifiers = snapshot.modifiers;
        let modifier_bits = (if modifiers.control {
            MODIFIER_CONTROL
        } else {
            0
        }) | (if modifiers.shift { MODIFIER_SHIFT } else { 0 })
            | (if modifiers.alt { MODIFIER_ALT } else { 0 })
            | (if modifiers.command {
                MODIFIER_COMMAND
            } else {
                0
            });
        Self {
            seq,
            client_millis,
            held_keys,
            pointer: snapshot.pointer_position.unwrap_or([0.0; 2]),
            pointer_delta: snapshot.pointer_delta,
            wheel: snapshot.scroll_delta,
            buttons,
            modifiers: modifier_bits,
        }
    }

    fn has_finite_values(&self) -> bool {
        self.pointer
            .iter()
            .chain(self.pointer_delta.iter())
            .chain(self.wheel.iter())
            .all(|value| value.is_finite())
    }

    fn clamp_values(&mut self) {
        for value in self
            .pointer
            .iter_mut()
            .chain(self.pointer_delta.iter_mut())
            .chain(self.wheel.iter_mut())
        {
            *value = value.clamp(-MAX_POINTER_ABS, MAX_POINTER_ABS);
        }
    }

    fn validate_keys(&self) -> Result<(), InputError> {
        if self.held_keys.len() > MAX_HELD_KEYS {
            return Err(InputError::TooManyKeys);
        }
        for &code in &self.held_keys {
            if input_key_from_code(code).is_none() {
                return Err(InputError::UnknownKey(code));
            }
        }
        Ok(())
    }

    /// Normalize an incoming or outgoing frame.
    ///
    /// Non-finite floats are rejected. Pointer, delta and wheel values are
    /// clamped to [`MAX_POINTER_ABS`]. Unknown button or modifier bits are
    /// masked, held keys are sorted and deduplicated, and the key count is
    /// capped at [`MAX_HELD_KEYS`].
    ///
    /// The frame may already be partially normalized when an error is
    /// returned; callers must not use a rejected frame.
    pub fn sanitize(&mut self) -> Result<(), InputError> {
        if !self.has_finite_values() {
            return Err(InputError::NonFiniteFloats);
        }
        self.clamp_values();
        self.buttons &= BUTTON_MASK;
        self.modifiers &= MODIFIER_MASK;
        self.held_keys.sort_unstable();
        self.held_keys.dedup();
        self.validate_keys()
    }

    /// Validate a frame without mutating it.
    pub fn validate(&self) -> Result<(), InputError> {
        if !self.has_finite_values() {
            return Err(InputError::NonFiniteFloats);
        }
        self.validate_keys()
    }

    pub(crate) fn encode_payload_into(&self, out: &mut PayloadWriter) -> io::Result<()> {
        self.validate().map_err(input_io_invalid_input)?;
        out.u32(self.seq);
        out.u64(self.client_millis);
        out.u16(self.held_keys.len() as u16);
        for &code in &self.held_keys {
            out.u16(code);
        }
        for value in &self.pointer {
            out.f32(*value);
        }
        for value in &self.pointer_delta {
            out.f32(*value);
        }
        for value in &self.wheel {
            out.f32(*value);
        }
        out.u8(self.buttons);
        out.u8(self.modifiers);
        Ok(())
    }

    pub(crate) fn decode_payload(payload: &[u8]) -> io::Result<Self> {
        let mut reader = PayloadReader::new(payload);
        let seq = reader.u32("input.seq")?;
        let client_millis = reader.u64("input.client_millis")?;
        let key_count = reader.u16("input.held_keys")? as usize;
        if key_count > MAX_HELD_KEYS {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!("input.held_keys: {key_count} entries exceed the {MAX_HELD_KEYS} limit"),
            ));
        }
        let mut held_keys = Vec::with_capacity(key_count);
        for _ in 0..key_count {
            held_keys.push(reader.u16("input.held_keys")?);
        }
        let pointer = [
            reader.f32("input.pointer.x")?,
            reader.f32("input.pointer.y")?,
        ];
        let pointer_delta = [
            reader.f32("input.pointer_delta.x")?,
            reader.f32("input.pointer_delta.y")?,
        ];
        let wheel = [reader.f32("input.wheel.x")?, reader.f32("input.wheel.y")?];
        let buttons = reader.u8("input.buttons")?;
        let modifiers = reader.u8("input.modifiers")?;
        reader.finish("input frame")?;

        let mut frame = Self {
            seq,
            client_millis,
            held_keys,
            pointer,
            pointer_delta,
            wheel,
            buttons,
            modifiers,
        };
        frame.sanitize().map_err(input_io_invalid_data)?;
        Ok(frame)
    }

    /// Build the editor/runtime snapshot for this state.
    ///
    /// `previous` supplies the previous authoritative state so press and
    /// release edges survive packet loss. The wire carries one pointer delta;
    /// it is exposed as both movement and relative device motion so
    /// first-person camera scripts receive input under a captured cursor.
    pub fn to_snapshot(&self, previous: &InputSnapshot) -> InputSnapshot {
        let mut snapshot = InputSnapshot::default();
        snapshot.window_focused = true;
        snapshot.pointer_position = Some(self.pointer);
        snapshot.pointer_delta = self.pointer_delta;
        snapshot.relative_pointer_delta = self.pointer_delta;
        snapshot.scroll_delta = self.wheel;
        snapshot.modifiers = modifiers_from_bits(self.modifiers);

        let before = previous.modifiers;
        snapshot.modifiers_pressed = InputModifiers {
            shift: snapshot.modifiers.shift && !before.shift,
            control: snapshot.modifiers.control && !before.control,
            alt: snapshot.modifiers.alt && !before.alt,
            command: snapshot.modifiers.command && !before.command,
        };
        snapshot.modifiers_released = InputModifiers {
            shift: !snapshot.modifiers.shift && before.shift,
            control: !snapshot.modifiers.control && before.control,
            alt: !snapshot.modifiers.alt && before.alt,
            command: !snapshot.modifiers.command && before.command,
        };

        for &key in &INPUT_KEYS {
            let code = input_key_code(key);
            let down = self.held_keys.iter().any(|held| *held == code);
            let was_down = previous.keys_down.contains(key);
            if down {
                snapshot.keys_down.insert(key);
            }
            if down && !was_down {
                snapshot.keys_pressed.insert(key);
            }
            if !down && was_down {
                snapshot.keys_released.insert(key);
            }
        }

        for &(button, mask) in &BUTTONS {
            let down = self.buttons & mask != 0;
            let was_down = previous.pointer_buttons_down.contains(button);
            if down {
                snapshot.pointer_buttons_down.insert(button);
            }
            if down && !was_down {
                snapshot.pointer_pressed_buttons.insert(button);
            }
            if !down && was_down {
                snapshot.pointer_released_buttons.insert(button);
            }
        }

        snapshot
    }
}

fn modifiers_from_bits(bits: u8) -> InputModifiers {
    InputModifiers {
        shift: bits & MODIFIER_SHIFT != 0,
        control: bits & MODIFIER_CONTROL != 0,
        alt: bits & MODIFIER_ALT != 0,
        command: bits & MODIFIER_COMMAND != 0,
    }
}

fn input_io_invalid_input(error: InputError) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidInput, format!("input frame: {error}"))
}

fn input_io_invalid_data(error: InputError) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, format!("input frame: {error}"))
}

/// Validation failure for an input frame.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InputError {
    /// One of the float fields is NaN or infinite.
    NonFiniteFloats,
    /// More than [`MAX_HELD_KEYS`] unique keys were supplied.
    TooManyKeys,
    /// A key code is outside the canonical table.
    UnknownKey(u16),
}

impl fmt::Display for InputError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NonFiniteFloats => {
                formatter.write_str("pointer, delta, or wheel contains a non-finite value")
            }
            Self::TooManyKeys => write!(
                formatter,
                "held key count exceeds the {MAX_HELD_KEYS} key limit"
            ),
            Self::UnknownKey(code) => {
                write!(
                    formatter,
                    "held key code {code} is outside the canonical key table"
                )
            }
        }
    }
}

impl std::error::Error for InputError {}

/// Sequence-aware input receiver.
///
/// Duplicate and stale frames are dropped before they reach the simulation,
/// while the latest snapshot keeps edge derivation coherent.
#[derive(Debug, Clone, Default)]
pub struct InputTracker {
    last_seq: Option<u32>,
    last_frame: Option<InputFrame>,
    last_snapshot: InputSnapshot,
    dropped: u64,
}

impl InputTracker {
    pub fn new() -> Self {
        Self::default()
    }

    /// Accept a frame when its sequence is newer than the last accepted one.
    ///
    /// Returns the derived snapshot, or `None` for a duplicate/stale frame.
    /// Invalid frames are rejected and counted as dropped.
    pub fn accept(&mut self, mut frame: InputFrame) -> Result<Option<InputSnapshot>, InputError> {
        if let Err(error) = frame.sanitize() {
            self.dropped = self.dropped.saturating_add(1);
            return Err(error);
        }
        if let Some(last_seq) = self.last_seq {
            let delta = frame.seq.wrapping_sub(last_seq);
            if delta == 0 || delta >= 0x8000_0000 {
                self.dropped = self.dropped.saturating_add(1);
                return Ok(None);
            }
        }
        let snapshot = frame.to_snapshot(&self.last_snapshot);
        self.last_seq = Some(frame.seq);
        self.last_snapshot = snapshot.clone();
        self.last_frame = Some(frame);
        Ok(Some(snapshot))
    }

    pub fn last_seq(&self) -> Option<u32> {
        self.last_seq
    }

    pub fn last_frame(&self) -> Option<&InputFrame> {
        self.last_frame.as_ref()
    }

    pub fn snapshot(&self) -> &InputSnapshot {
        &self.last_snapshot
    }

    pub fn dropped(&self) -> u64 {
        self.dropped
    }

    pub fn reset(&mut self) {
        *self = Self::default();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn frame_with_keys(seq: u32, keys: &[InputKey]) -> InputFrame {
        let mut frame = InputFrame::new(seq, seq as u64 * 16);
        frame.held_keys = keys.iter().map(|key| input_key_code(*key)).collect();
        frame
    }

    #[test]
    fn key_table_matches_canonical_discriminants() {
        assert_eq!(INPUT_KEYS.len(), InputKey::NumpadDecimal as usize + 1);
        for (index, key) in INPUT_KEYS.iter().enumerate() {
            assert_eq!(*key as usize, index);
            assert_eq!(input_key_from_code(index as u16), Some(*key));
        }
        assert!(input_key_from_code(INPUT_KEYS.len() as u16).is_none());
    }

    #[test]
    fn derives_press_and_release_edges_across_frames() {
        let mut tracker = InputTracker::new();
        let mut first = frame_with_keys(1, &[InputKey::W, InputKey::Space]);
        first.modifiers = MODIFIER_SHIFT;

        let first_snapshot = tracker.accept(first).unwrap().unwrap();
        assert!(first_snapshot.key_down(InputKey::W));
        assert!(first_snapshot.key_down(InputKey::Space));
        assert!(first_snapshot.key_pressed(InputKey::W));
        assert!(first_snapshot.key_pressed(InputKey::Space));
        assert!(!first_snapshot.key_released(InputKey::W));
        assert!(first_snapshot.modifiers.shift);
        assert!(first_snapshot.modifiers_pressed.shift);

        let second_snapshot = tracker
            .accept(frame_with_keys(2, &[InputKey::W]))
            .unwrap()
            .unwrap();
        assert!(second_snapshot.key_down(InputKey::W));
        assert!(!second_snapshot.key_down(InputKey::Space));
        assert!(!second_snapshot.key_pressed(InputKey::W));
        assert!(second_snapshot.key_released(InputKey::Space));
        assert!(!second_snapshot.modifiers.shift);
        assert!(second_snapshot.modifiers_released.shift);
    }

    #[test]
    fn duplicate_and_stale_sequences_are_dropped() {
        let mut tracker = InputTracker::new();
        assert!(tracker
            .accept(frame_with_keys(5, &[InputKey::A]))
            .unwrap()
            .is_some());
        assert!(tracker
            .accept(frame_with_keys(5, &[InputKey::B]))
            .unwrap()
            .is_none());
        assert!(tracker
            .accept(frame_with_keys(4, &[InputKey::C]))
            .unwrap()
            .is_none());
        assert_eq!(tracker.dropped(), 2);
        assert!(tracker
            .accept(frame_with_keys(6, &[InputKey::D]))
            .unwrap()
            .is_some());
    }

    #[test]
    fn non_finite_values_are_rejected() {
        let mut frame = InputFrame::new(1, 0);
        frame.pointer[0] = f32::NAN;
        assert_eq!(frame.sanitize(), Err(InputError::NonFiniteFloats));

        let mut frame = InputFrame::new(1, 0);
        frame.pointer_delta[1] = f32::INFINITY;
        assert_eq!(frame.sanitize(), Err(InputError::NonFiniteFloats));
    }

    #[test]
    fn sanitize_sorts_deduplicates_and_masks() {
        let mut frame = InputFrame::new(1, 0);
        frame.held_keys = vec![
            input_key_code(InputKey::Space),
            input_key_code(InputKey::W),
            input_key_code(InputKey::Space),
        ];
        frame.buttons = 0xFF;
        frame.modifiers = 0xFF;
        frame.pointer = [MAX_POINTER_ABS * 4.0, -MAX_POINTER_ABS * 4.0];

        frame.sanitize().unwrap();
        assert_eq!(
            frame.held_keys,
            vec![input_key_code(InputKey::W), input_key_code(InputKey::Space)]
        );
        assert_eq!(frame.buttons, BUTTON_MASK);
        assert_eq!(frame.modifiers, MODIFIER_MASK);
        assert_eq!(frame.pointer, [MAX_POINTER_ABS, -MAX_POINTER_ABS]);
    }

    #[test]
    fn unknown_key_codes_are_rejected() {
        let mut frame = InputFrame::new(1, 0);
        frame.held_keys = vec![u16::MAX];
        assert_eq!(frame.sanitize(), Err(InputError::UnknownKey(u16::MAX)));
    }

    #[test]
    fn snapshot_capture_round_trips_held_state() {
        let mut source = InputSnapshot::default();
        source.keys_down.insert(InputKey::W);
        source.keys_down.insert(InputKey::Space);
        source.pointer_position = Some([5.0, 6.0]);
        source.pointer_delta = [1.0, -2.0];
        source.scroll_delta = [0.0, 3.0];
        source.modifiers.control = true;
        source
            .pointer_buttons_down
            .insert(raf_core::PointerButton::Primary);

        let frame = InputFrame::from_snapshot(9, 1234, &source);
        assert_eq!(
            frame.held_keys,
            vec![input_key_code(InputKey::W), input_key_code(InputKey::Space)]
        );
        assert_eq!(frame.buttons, BUTTON_PRIMARY);
        assert_eq!(frame.modifiers, MODIFIER_CONTROL);
        assert_eq!(frame.pointer, [5.0, 6.0]);
        assert_eq!(frame.wheel, [0.0, 3.0]);

        let snapshot = frame.to_snapshot(&InputSnapshot::default());
        assert!(snapshot.key_down(InputKey::W));
        assert!(snapshot.key_down(InputKey::Space));
        assert!(snapshot.key_pressed(InputKey::W));
        assert!(snapshot.button_down(raf_core::PointerButton::Primary));
        assert!(snapshot.modifiers.control);
    }
}
