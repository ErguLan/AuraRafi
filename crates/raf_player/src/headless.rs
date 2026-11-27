//! Deterministic headless player runner.
//!
//! The runner owns one ApiGraphicBasic headless host, one `PlayerSession`, and
//! one editor compositor. It never creates a Winit event loop, never sleeps
//! between frames, and drives simulation with an exact fixed delta so captured
//! frames are reproducible. Every frame is composed by the AGB host (scene plus
//! retained RafUI) and read back through `NativeUiWindowHost::capture`.

use crate::application::PlayerLaunch;
use crate::session::{PlayerSession, PlayerSource};
use raf_core::{InputKey, InputModifiers, InputSnapshot, PointerButton};
use raf_render::api_graphic_basic::ui_surface::{
    NativeUiInputBridge, NativeUiWindowHost,
};
use raf_render::api_graphic_basic::NativeEditorCompositor;
use raf_runtime::manifest::RuntimeManifest;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::time::Instant;

const REPORT_PROTOCOL: &str = "aurarafi.headless.report/1";
const DEFAULT_FIXED_HZ: u32 = 60;
const MIN_FIXED_HZ: u32 = 15;
const MAX_FIXED_HZ: u32 = 120;
const MAX_FRAMES: u32 = 100_000;
const MAX_SIZE: [u32; 2] = [3840, 2160];
const MAX_INPUT_FILE_BYTES: u64 = 4 * 1024 * 1024;
const MAX_INPUT_ENTRIES: usize = 100_000;
const MAX_KEYS_PER_FRAME: usize = 32;
const MAX_BUTTONS_PER_FRAME: usize = 8;
const MAX_DIAGNOSTICS: usize = 256;
const MAX_DIAGNOSTIC_CHARS: usize = 512;
const MAX_UNKNOWN_KEYS: usize = 16;
const CLEAR_COLOR: [u8; 4] = [8, 11, 15, 255];

/// Bounded options for one deterministic headless capture run.
#[derive(Clone, Debug)]
pub struct HeadlessOptions {
    pub size: [u32; 2],
    pub frames: u32,
    pub output_dir: PathBuf,
    pub input_script: Option<PathBuf>,
    pub report_path: PathBuf,
    pub fixed_hz: u32,
}

impl Default for HeadlessOptions {
    fn default() -> Self {
        let output_dir = PathBuf::from("headless");
        Self {
            size: [1280, 720],
            frames: 1,
            report_path: output_dir.join("report.json"),
            output_dir,
            input_script: None,
            fixed_hz: DEFAULT_FIXED_HZ,
        }
    }
}

/// Runs `frames` deterministic frames and writes PNG captures plus a report.
pub fn run(launch: PlayerLaunch, options: HeadlessOptions) -> Result<(), String> {
    let frames = validate_frames(options.frames)?;
    let size = validate_size(options.size)?;
    let fixed_hz = validate_fixed_hz(options.fixed_hz)?;
    std::fs::create_dir_all(&options.output_dir)
        .map_err(|error| format!("headless output directory: {error}"))?;
    if let Some(parent) = options.report_path.parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent)
                .map_err(|error| format!("headless report directory: {error}"))?;
        }
    }

    let input_script = HeadlessInputScript::load(options.input_script.as_deref())?;
    let mut manifest = RuntimeManifest::read(&launch.manifest)?;
    if manifest.instance != launch.instance {
        return Err("runtime snapshot instance mismatch".to_string());
    }
    // The world's fixed step must match the injected delta, otherwise the
    // accumulator would drift from the requested deterministic cadence.
    manifest.project_settings.runtime.fixed_hz = fixed_hz;
    let settings = launch.settings.clone();
    let config = crate::graphics::window_config(settings.render_execution_policy, size);

    let mut host = NativeUiWindowHost::create_headless(size, config)?;
    let mut compositor = NativeEditorCompositor::new(&host.graphics_context(), CLEAR_COLOR);
    let mut session = PlayerSession::new(
        &host,
        PlayerSource::Snapshot(Box::new(manifest)),
        settings,
        launch.instance,
    );
    session.finish_preparation(&host)?;

    let fixed_dt = 1.0 / f64::from(fixed_hz);
    let mut input = NativeUiInputBridge::default();
    input.set_scale_factor(f64::from(host.scale_factor()));
    let mut frame_input = InputSnapshot::default();
    let mut input_state = FrameInputState::default();
    let mut frame_name = String::with_capacity(32);
    let mut diagnostics = Vec::new();
    for note in input_script.diagnostics() {
        push_diagnostic(&mut diagnostics, note);
    }

    let mut first_frame_hash = 0_u64;
    let mut last_frame_hash = 0_u64;
    let mut tick_ms = 0.0_f64;
    let mut render_ms = 0.0_f64;
    let mut capture_ms = 0.0_f64;
    let mut scene_rebuilds = 0_u64;
    let mut ui_rebuilds = 0_u64;
    let mut upload_bytes = 0_u64;
    let mut draw_calls = 0_u64;

    for frame in 0..frames {
        input_script.fill_snapshot(frame, fixed_dt, &mut input_state, &mut frame_input);
        input.snapshot_mut().clone_from(&frame_input);

        let step_started = Instant::now();
        session.step_with_snapshot(&mut host, &mut compositor, &input, &frame_input, fixed_dt)?;
        let step_ms = step_started.elapsed().as_secs_f64() * 1000.0;
        let metrics = compositor.metrics();
        scene_rebuilds = scene_rebuilds.saturating_add(u64::from(metrics.scene_packet_rebuilds));
        ui_rebuilds = ui_rebuilds.saturating_add(u64::from(metrics.ui_packet_rebuilds));
        upload_bytes = upload_bytes.saturating_add(metrics.upload_bytes);
        draw_calls = draw_calls.saturating_add(u64::from(metrics.draw_calls));
        let frame_render_ms = session.last_render_millis().clamp(0.0, step_ms);
        render_ms += frame_render_ms;
        tick_ms += (step_ms - frame_render_ms).max(0.0);

        let capture_started = Instant::now();
        let rgba = host.capture()?;
        let [width, height] = host.size();
        capture_ms += capture_started.elapsed().as_secs_f64() * 1000.0;

        let hash = fnv1a64(&rgba);
        if frame == 0 {
            first_frame_hash = hash;
        }
        last_frame_hash = hash;

        let path = {
            use std::fmt::Write as _;
            frame_name.clear();
            write!(&mut frame_name, "frame_{frame:04}.png")
                .map_err(|error| format!("headless frame name: {error}"))?;
            options.output_dir.join(&frame_name)
        };
        image::save_buffer(&path, &rgba, width, height, image::ColorType::Rgba8)
            .map_err(|error| format!("headless frame {}: {error}", path.display()))?;

        for message in session.drain_diagnostics() {
            push_diagnostic(&mut diagnostics, message);
        }
    }

    let divisor = f64::from(frames);
    let report = HeadlessReport {
        protocol: REPORT_PROTOCOL,
        backend: session.backend_name(),
        size,
        frames,
        fixed_hz,
        fixed_ticks: session.status().fixed_ticks,
        camera_ready: session.status().camera_ready,
        phase: format!("{:?}", session.status().phase),
        avg_tick_ms: tick_ms / divisor,
        avg_render_ms: render_ms / divisor,
        avg_capture_ms: capture_ms / divisor,
        scene_packet_rebuilds: scene_rebuilds,
        ui_packet_rebuilds: ui_rebuilds,
        upload_bytes,
        avg_draw_calls: draw_calls as f64 / divisor,
        first_frame_hash: hex_hash(first_frame_hash),
        last_frame_hash: hex_hash(last_frame_hash),
        diagnostics,
    };
    let bytes =
        serde_json::to_vec_pretty(&report).map_err(|error| format!("headless report: {error}"))?;
    std::fs::write(&options.report_path, bytes)
        .map_err(|error| format!("headless report {}: {error}", options.report_path.display()))?;
    Ok(())
}

#[derive(Serialize)]
struct HeadlessReport {
    protocol: &'static str,
    backend: &'static str,
    size: [u32; 2],
    frames: u32,
    fixed_hz: u32,
    fixed_ticks: u64,
    camera_ready: bool,
    phase: String,
    avg_tick_ms: f64,
    avg_render_ms: f64,
    avg_capture_ms: f64,
    scene_packet_rebuilds: u64,
    ui_packet_rebuilds: u64,
    upload_bytes: u64,
    avg_draw_calls: f64,
    first_frame_hash: String,
    last_frame_hash: String,
    diagnostics: Vec<String>,
}

/// Reusable per-frame input state: previous keys/buttons for edge detection and
/// scratch sets for the scripted frame. Capacity survives across frames.
#[derive(Default)]
struct FrameInputState {
    keys: Vec<InputKey>,
    buttons: Vec<PointerButton>,
    scratch_keys: Vec<InputKey>,
    scratch_buttons: Vec<PointerButton>,
    modifiers: InputModifiers,
}

/// One fully parsed script frame. Unknown key names are reported once during
/// load, never per frame.
#[derive(Default)]
struct ResolvedScriptFrame {
    keys: Vec<InputKey>,
    buttons: Vec<PointerButton>,
    modifiers: InputModifiers,
    mouse_delta: [f32; 2],
    wheel: [f32; 2],
}

/// Scripted input source. Missing files mean empty input.
struct HeadlessInputScript {
    frames: BTreeMap<u32, ResolvedScriptFrame>,
    unknown_keys: Vec<String>,
}

impl HeadlessInputScript {
    fn load(path: Option<&Path>) -> Result<Self, String> {
        let mut script = Self {
            frames: BTreeMap::new(),
            unknown_keys: Vec::new(),
        };
        let Some(path) = path else {
            return Ok(script);
        };
        let bytes = match std::fs::read(path) {
            Ok(bytes) => bytes,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(script),
            Err(error) => return Err(format!("headless input script: {error}")),
        };
        if bytes.len() as u64 > MAX_INPUT_FILE_BYTES {
            return Err(format!(
                "headless input script exceeds {MAX_INPUT_FILE_BYTES} bytes"
            ));
        }
        let entries: Vec<ScriptFrame> = serde_json::from_slice(&bytes)
            .map_err(|error| format!("headless input script: {error}"))?;
        if entries.len() > MAX_INPUT_ENTRIES {
            return Err(format!(
                "headless input script exceeds {MAX_INPUT_ENTRIES} entries"
            ));
        }
        for entry in entries {
            let target = script.frames.entry(entry.frame).or_default();
            for name in entry.keys.iter().take(MAX_KEYS_PER_FRAME) {
                if target.keys.len() >= MAX_KEYS_PER_FRAME {
                    break;
                }
                match parse_input_modifier(name) {
                    Some(modifiers) => merge_modifiers(&mut target.modifiers, modifiers),
                    None => match parse_input_key(name) {
                        Some(key) if !target.keys.contains(&key) => target.keys.push(key),
                        Some(_) => {}
                        None => note_unknown_key(&mut script.unknown_keys, name),
                    },
                }
            }
            for button in entry.buttons.iter().take(MAX_BUTTONS_PER_FRAME) {
                if let Some(button) = pointer_button(*button) {
                    if target.buttons.len() < MAX_BUTTONS_PER_FRAME
                        && !target.buttons.contains(&button)
                    {
                        target.buttons.push(button);
                    }
                }
            }
            target.mouse_delta[0] =
                (target.mouse_delta[0] + entry.mouse_dx).clamp(-1_000_000.0, 1_000_000.0);
            target.mouse_delta[1] =
                (target.mouse_delta[1] + entry.mouse_dy).clamp(-1_000_000.0, 1_000_000.0);
            target.wheel[0] = (target.wheel[0] + entry.wheel[0]).clamp(-1_000_000.0, 1_000_000.0);
            target.wheel[1] = (target.wheel[1] + entry.wheel[1]).clamp(-1_000_000.0, 1_000_000.0);
        }
        Ok(script)
    }

    fn diagnostics(&self) -> Vec<String> {
        self.unknown_keys
            .iter()
            .map(|key| format!("headless.input.unknown_key: {key}"))
            .collect()
    }

    fn fill_snapshot(
        &self,
        frame: u32,
        fixed_dt: f64,
        state: &mut FrameInputState,
        out: &mut InputSnapshot,
    ) {
        state.scratch_keys.clear();
        state.scratch_buttons.clear();
        let mut modifiers = InputModifiers::default();
        let mut mouse_delta = [0.0_f32; 2];
        let mut wheel = [0.0_f32; 2];
        if let Some(source) = self.frames.get(&frame) {
            state.scratch_keys.extend_from_slice(&source.keys);
            state.scratch_buttons.extend_from_slice(&source.buttons);
            modifiers = source.modifiers;
            mouse_delta = source.mouse_delta;
            wheel = source.wheel;
        }

        out.keys_down.clear();
        out.keys_pressed.clear();
        out.keys_released.clear();
        out.pointer_buttons_down.clear();
        out.pointer_pressed_buttons.clear();
        out.pointer_released_buttons.clear();
        out.pointer_position = None;
        out.pointer_delta = mouse_delta;
        out.relative_pointer_delta = mouse_delta;
        out.scroll_delta = wheel;
        out.text_input.clear();
        out.ime_preedit.clear();
        out.window_focused = true;
        out.time_seconds = f64::from(frame) * fixed_dt;
        out.delta_seconds = fixed_dt as f32;
        out.modifiers = modifiers;
        out.modifiers_pressed = InputModifiers {
            shift: modifiers.shift && !state.modifiers.shift,
            control: modifiers.control && !state.modifiers.control,
            alt: modifiers.alt && !state.modifiers.alt,
            command: modifiers.command && !state.modifiers.command,
        };
        out.modifiers_released = InputModifiers {
            shift: !modifiers.shift && state.modifiers.shift,
            control: !modifiers.control && state.modifiers.control,
            alt: !modifiers.alt && state.modifiers.alt,
            command: !modifiers.command && state.modifiers.command,
        };

        for key in &state.scratch_keys {
            out.keys_down.insert(*key);
            if !state.keys.contains(key) {
                out.keys_pressed.insert(*key);
            }
        }
        for key in &state.keys {
            if !state.scratch_keys.contains(key) {
                out.keys_released.insert(*key);
            }
        }
        for button in &state.scratch_buttons {
            out.pointer_buttons_down.insert(*button);
            if !state.buttons.contains(button) {
                out.pointer_pressed_buttons.insert(*button);
            }
        }
        for button in &state.buttons {
            if !state.scratch_buttons.contains(button) {
                out.pointer_released_buttons.insert(*button);
            }
        }

        state.keys.clear();
        state.keys.extend_from_slice(&state.scratch_keys);
        state.buttons.clear();
        state.buttons.extend_from_slice(&state.scratch_buttons);
        state.modifiers = modifiers;
    }
}

#[derive(Deserialize, Default)]
struct ScriptFrame {
    #[serde(default)]
    frame: u32,
    #[serde(default)]
    keys: Vec<String>,
    #[serde(default)]
    mouse_dx: f32,
    #[serde(default)]
    mouse_dy: f32,
    #[serde(default)]
    buttons: Vec<u32>,
    #[serde(default)]
    wheel: [f32; 2],
}

fn validate_frames(frames: u32) -> Result<u32, String> {
    if frames == 0 || frames > MAX_FRAMES {
        return Err(format!(
            "--frames must be 1..={MAX_FRAMES}, received {frames}"
        ));
    }
    Ok(frames)
}

fn validate_size(size: [u32; 2]) -> Result<[u32; 2], String> {
    if size[0] == 0 || size[1] == 0 || size[0] > MAX_SIZE[0] || size[1] > MAX_SIZE[1] {
        return Err(format!(
            "--size must be 1..={}x{}, received {}x{}",
            MAX_SIZE[0], MAX_SIZE[1], size[0], size[1]
        ));
    }
    Ok(size)
}

fn validate_fixed_hz(fixed_hz: u32) -> Result<u32, String> {
    if !(MIN_FIXED_HZ..=MAX_FIXED_HZ).contains(&fixed_hz) {
        return Err(format!(
            "--fixed-hz must be {MIN_FIXED_HZ}..={MAX_FIXED_HZ}, received {fixed_hz}"
        ));
    }
    Ok(fixed_hz)
}

fn merge_modifiers(target: &mut InputModifiers, source: InputModifiers) {
    target.shift |= source.shift;
    target.control |= source.control;
    target.alt |= source.alt;
    target.command |= source.command;
}

fn pointer_button(index: u32) -> Option<PointerButton> {
    match index {
        0 => Some(PointerButton::Primary),
        1 => Some(PointerButton::Secondary),
        2 => Some(PointerButton::Middle),
        3 => Some(PointerButton::Back),
        4 => Some(PointerButton::Forward),
        _ => None,
    }
}

fn parse_input_modifier(name: &str) -> Option<InputModifiers> {
    let mut modifiers = InputModifiers::default();
    match name.trim().to_ascii_lowercase().as_str() {
        "shift" => modifiers.shift = true,
        "ctrl" | "control" => modifiers.control = true,
        "alt" => modifiers.alt = true,
        "cmd" | "command" | "super" | "meta" | "win" => modifiers.command = true,
        _ => return None,
    }
    Some(modifiers)
}

/// Parses the script key vocabulary. Layout-independent canonical names and
/// the common short spellings are accepted; unknown names are ignored.
fn parse_input_key(name: &str) -> Option<InputKey> {
    let name = name.trim().to_ascii_lowercase();
    if name.len() == 1 {
        let byte = name.as_bytes()[0];
        if byte.is_ascii_alphabetic() {
            return letter_key(byte.to_ascii_uppercase());
        }
        if byte.is_ascii_digit() {
            return digit_key(byte - b'0');
        }
    }
    Some(match name.as_str() {
        "escape" | "esc" => InputKey::Escape,
        "enter" | "return" => InputKey::Enter,
        "tab" => InputKey::Tab,
        "space" | "spacebar" => InputKey::Space,
        "backspace" => InputKey::Backspace,
        "delete" | "del" => InputKey::Delete,
        "insert" | "ins" => InputKey::Insert,
        "home" => InputKey::Home,
        "end" => InputKey::End,
        "pageup" | "page_up" | "pgup" => InputKey::PageUp,
        "pagedown" | "page_down" | "pgdn" => InputKey::PageDown,
        "arrowup" | "up" => InputKey::ArrowUp,
        "arrowdown" | "down" => InputKey::ArrowDown,
        "arrowleft" | "left" => InputKey::ArrowLeft,
        "arrowright" | "right" => InputKey::ArrowRight,
        "f1" => InputKey::F1,
        "f2" => InputKey::F2,
        "f3" => InputKey::F3,
        "f4" => InputKey::F4,
        "f5" => InputKey::F5,
        "f6" => InputKey::F6,
        "f7" => InputKey::F7,
        "f8" => InputKey::F8,
        "f9" => InputKey::F9,
        "f10" => InputKey::F10,
        "f11" => InputKey::F11,
        "f12" => InputKey::F12,
        "minus" | "-" => InputKey::Minus,
        "equal" | "=" => InputKey::Equal,
        "bracketleft" | "[" => InputKey::BracketLeft,
        "bracketright" | "]" => InputKey::BracketRight,
        "backslash" | "\\" => InputKey::Backslash,
        "semicolon" | ";" => InputKey::Semicolon,
        "quote" | "'" => InputKey::Quote,
        "backquote" | "`" => InputKey::Backquote,
        "comma" | "," => InputKey::Comma,
        "period" | "." => InputKey::Period,
        "slash" | "/" => InputKey::Slash,
        "numpad0" => InputKey::Numpad0,
        "numpad1" => InputKey::Numpad1,
        "numpad2" => InputKey::Numpad2,
        "numpad3" => InputKey::Numpad3,
        "numpad4" => InputKey::Numpad4,
        "numpad5" => InputKey::Numpad5,
        "numpad6" => InputKey::Numpad6,
        "numpad7" => InputKey::Numpad7,
        "numpad8" => InputKey::Numpad8,
        "numpad9" => InputKey::Numpad9,
        "numpadadd" => InputKey::NumpadAdd,
        "numpadsubtract" => InputKey::NumpadSubtract,
        "numpadmultiply" => InputKey::NumpadMultiply,
        "numpaddivide" => InputKey::NumpadDivide,
        "numpaddecimal" => InputKey::NumpadDecimal,
        _ => return None,
    })
}

fn letter_key(letter: u8) -> Option<InputKey> {
    Some(match letter {
        b'A' => InputKey::A,
        b'B' => InputKey::B,
        b'C' => InputKey::C,
        b'D' => InputKey::D,
        b'E' => InputKey::E,
        b'F' => InputKey::F,
        b'G' => InputKey::G,
        b'H' => InputKey::H,
        b'I' => InputKey::I,
        b'J' => InputKey::J,
        b'K' => InputKey::K,
        b'L' => InputKey::L,
        b'M' => InputKey::M,
        b'N' => InputKey::N,
        b'O' => InputKey::O,
        b'P' => InputKey::P,
        b'Q' => InputKey::Q,
        b'R' => InputKey::R,
        b'S' => InputKey::S,
        b'T' => InputKey::T,
        b'U' => InputKey::U,
        b'V' => InputKey::V,
        b'W' => InputKey::W,
        b'X' => InputKey::X,
        b'Y' => InputKey::Y,
        b'Z' => InputKey::Z,
        _ => return None,
    })
}

fn digit_key(digit: u8) -> Option<InputKey> {
    Some(match digit {
        0 => InputKey::Digit0,
        1 => InputKey::Digit1,
        2 => InputKey::Digit2,
        3 => InputKey::Digit3,
        4 => InputKey::Digit4,
        5 => InputKey::Digit5,
        6 => InputKey::Digit6,
        7 => InputKey::Digit7,
        8 => InputKey::Digit8,
        9 => InputKey::Digit9,
        _ => return None,
    })
}

fn note_unknown_key(unknown_keys: &mut Vec<String>, name: &str) {
    if unknown_keys.len() >= MAX_UNKNOWN_KEYS
        || unknown_keys
            .iter()
            .any(|known| known.eq_ignore_ascii_case(name))
    {
        return;
    }
    unknown_keys.push(name.to_string());
}

fn push_diagnostic(diagnostics: &mut Vec<String>, message: String) {
    if diagnostics.len() >= MAX_DIAGNOSTICS {
        return;
    }
    if message.chars().count() > MAX_DIAGNOSTIC_CHARS {
        diagnostics.push(message.chars().take(MAX_DIAGNOSTIC_CHARS).collect());
    } else {
        diagnostics.push(message);
    }
}

fn hex_hash(hash: u64) -> String {
    format!("{hash:016x}")
}

/// FNV-1a 64 over RGBA bytes. Stable across platforms and dependency-free.
fn fnv1a64(bytes: &[u8]) -> u64 {
    const OFFSET_BASIS: u64 = 0xcbf2_9ce4_8422_2325;
    const PRIME: u64 = 0x0000_0100_0000_01b3;
    let mut hash = OFFSET_BASIS;
    for byte in bytes {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(PRIME);
    }
    hash
}
