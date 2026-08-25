//! Single source of truth for editor shortcuts.
//!
//! RafUI menu accelerators are presentation metadata. This module keeps the
//! metadata beside the input matching so a visible accelerator cannot drift
//! away from the command boundary again.

use raf_core::config::Language;
use raf_core::i18n::t;
use raf_core::{InputKey, InputModifiers, InputSnapshot};

use crate::application_menu::command;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ShortcutSpec {
    pub command: &'static str,
    pub accelerator: &'static str,
    pub label_key: &'static str,
    pub scope: ShortcutScope,
    pub chord: Option<ShortcutChord>,
    pub requires_selection: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ShortcutScope {
    Global,
    Viewport,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ShortcutChord {
    pub key: InputKey,
    pub modifiers: InputModifiers,
}

impl ShortcutChord {
    pub const fn plain(key: InputKey) -> Self {
        Self {
            key,
            modifiers: InputModifiers {
                shift: false,
                control: false,
                alt: false,
                command: false,
            },
        }
    }

    pub const fn command(key: InputKey) -> Self {
        Self {
            key,
            modifiers: InputModifiers {
                shift: false,
                control: true,
                alt: false,
                command: false,
            },
        }
    }

    pub const fn command_shift(key: InputKey) -> Self {
        Self {
            key,
            modifiers: InputModifiers {
                shift: true,
                control: true,
                alt: false,
                command: false,
            },
        }
    }

    pub fn matches(self, input: &InputSnapshot) -> bool {
        if !input.key_pressed(self.key) {
            return false;
        }
        let command = input.modifiers.command_modifier();
        let expected_command = self.modifiers.command_modifier();
        input.modifiers.shift == self.modifiers.shift
            && input.modifiers.alt == self.modifiers.alt
            && command == expected_command
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct ShortcutContext {
    pub keyboard_captured_by_text: bool,
    pub modal_open: bool,
    pub viewport_active: bool,
    pub has_selection: bool,
}

/// Commands handled by the editor application boundary.
pub const EDITOR_SHORTCUTS: &[ShortcutSpec] = &[
    ShortcutSpec {
        command: command::PROJECT_SAVE,
        accelerator: "Ctrl+S",
        label_key: "app.save_menu",
        scope: ShortcutScope::Global,
        chord: Some(ShortcutChord::command(InputKey::S)),
        requires_selection: false,
    },
    ShortcutSpec {
        command: command::SEARCH_OPEN,
        accelerator: "Ctrl+K",
        label_key: "app.command_search",
        scope: ShortcutScope::Global,
        chord: Some(ShortcutChord::command(InputKey::K)),
        requires_selection: false,
    },
    ShortcutSpec {
        command: command::EDIT_UNDO,
        accelerator: "Ctrl+Z",
        label_key: "app.undo_menu",
        scope: ShortcutScope::Global,
        chord: Some(ShortcutChord::command(InputKey::Z)),
        requires_selection: false,
    },
    ShortcutSpec {
        command: command::EDIT_REDO,
        accelerator: "Ctrl+Y / Ctrl+Shift+Z",
        label_key: "app.redo_menu",
        scope: ShortcutScope::Global,
        chord: Some(ShortcutChord::command(InputKey::Y)),
        requires_selection: false,
    },
    ShortcutSpec {
        command: command::EDIT_REDO,
        accelerator: "Ctrl+Shift+Z",
        label_key: "app.redo_menu",
        scope: ShortcutScope::Global,
        chord: Some(ShortcutChord::command_shift(InputKey::Z)),
        requires_selection: false,
    },
    ShortcutSpec {
        command: command::EDIT_DUPLICATE,
        accelerator: "Ctrl+D",
        label_key: "app.duplicate_menu",
        scope: ShortcutScope::Global,
        chord: Some(ShortcutChord::command(InputKey::D)),
        requires_selection: true,
    },
    ShortcutSpec {
        command: command::EDIT_COPY,
        accelerator: "Ctrl+C",
        label_key: "app.copy_menu",
        scope: ShortcutScope::Global,
        chord: Some(ShortcutChord::command(InputKey::C)),
        requires_selection: true,
    },
    ShortcutSpec {
        command: command::EDIT_PASTE,
        accelerator: "Ctrl+V",
        label_key: "app.paste_menu",
        scope: ShortcutScope::Global,
        chord: Some(ShortcutChord::command(InputKey::V)),
        requires_selection: false,
    },
    ShortcutSpec {
        command: command::EDIT_DELETE,
        accelerator: "Del",
        label_key: "app.delete_menu",
        scope: ShortcutScope::Global,
        chord: Some(ShortcutChord::plain(InputKey::Delete)),
        requires_selection: true,
    },
    ShortcutSpec {
        command: command::EDIT_SELECT_ALL,
        accelerator: "Ctrl+A",
        label_key: "app.select_all_menu",
        scope: ShortcutScope::Global,
        chord: Some(ShortcutChord::command(InputKey::A)),
        requires_selection: false,
    },
];

/// Viewport-owned shortcuts are documented here, but are intentionally not
/// dispatched by the application layer. The viewport remains the owner of
/// transform and camera gestures.
pub const VIEWPORT_SHORTCUTS: &[ShortcutSpec] = &[
    ShortcutSpec {
        command: "viewport.move",
        accelerator: "G",
        label_key: "app.viewport_move",
        scope: ShortcutScope::Viewport,
        chord: Some(ShortcutChord::plain(InputKey::G)),
        requires_selection: true,
    },
    ShortcutSpec {
        command: "viewport.rotate",
        accelerator: "R",
        label_key: "app.viewport_rotate",
        scope: ShortcutScope::Viewport,
        chord: Some(ShortcutChord::plain(InputKey::R)),
        requires_selection: true,
    },
    ShortcutSpec {
        command: "viewport.scale",
        accelerator: "T",
        label_key: "app.viewport_scale",
        scope: ShortcutScope::Viewport,
        chord: Some(ShortcutChord::plain(InputKey::T)),
        requires_selection: true,
    },
    ShortcutSpec {
        command: "viewport.select",
        accelerator: "C",
        label_key: "app.viewport_select",
        scope: ShortcutScope::Viewport,
        chord: Some(ShortcutChord::plain(InputKey::C)),
        requires_selection: false,
    },
    ShortcutSpec {
        command: "viewport.focus",
        accelerator: "F",
        label_key: "app.focus_entity",
        scope: ShortcutScope::Viewport,
        chord: Some(ShortcutChord::plain(InputKey::F)),
        requires_selection: true,
    },
    ShortcutSpec {
        command: "viewport.edit_mode",
        accelerator: "Tab",
        label_key: "app.shortcut_toggle_edit_mode",
        scope: ShortcutScope::Viewport,
        chord: Some(ShortcutChord::plain(InputKey::Tab)),
        requires_selection: true,
    },
    ShortcutSpec {
        command: "viewport.camera_fly",
        accelerator: "RMB + W/A/S/D + Q/E",
        label_key: "app.shortcut_fly_camera",
        scope: ShortcutScope::Viewport,
        chord: None,
        requires_selection: false,
    },
    ShortcutSpec {
        command: "viewport.camera_orbit",
        accelerator: "RMB + drag",
        label_key: "app.shortcut_orbit_camera",
        scope: ShortcutScope::Viewport,
        chord: None,
        requires_selection: false,
    },
];

pub const CAMERA_BOOKMARK_SHORTCUTS: &[ShortcutSpec] = &[
    ShortcutSpec {
        command: "camera.bookmark.save",
        accelerator: "Ctrl+1/2/3",
        label_key: "app.shortcut_camera_bookmarks",
        scope: ShortcutScope::Viewport,
        chord: None,
        requires_selection: false,
    },
    ShortcutSpec {
        command: "camera.bookmark.restore",
        accelerator: "1/2/3",
        label_key: "app.shortcut_camera_bookmarks",
        scope: ShortcutScope::Viewport,
        chord: None,
        requires_selection: false,
    },
];

pub fn accelerator_for(command_id: &str) -> &'static str {
    EDITOR_SHORTCUTS
        .iter()
        .chain(VIEWPORT_SHORTCUTS)
        .find(|shortcut| shortcut.command == command_id)
        .map(|shortcut| shortcut.accelerator)
        .unwrap_or("")
}

/// Return an application command for the current frame's keyboard input.
/// Text fields and retained controls are filtered by the caller's RafUI
/// keyboard-capture boundary before this function is used.
pub fn global_command(input: &InputSnapshot, context: ShortcutContext) -> Option<&'static str> {
    resolve_command(EDITOR_SHORTCUTS, input, context)
}

pub fn viewport_command(input: &InputSnapshot, context: ShortcutContext) -> Option<&'static str> {
    resolve_command(VIEWPORT_SHORTCUTS, input, context)
}

fn resolve_command(
    specs: &'static [ShortcutSpec],
    input: &InputSnapshot,
    context: ShortcutContext,
) -> Option<&'static str> {
    if context.keyboard_captured_by_text || context.modal_open {
        return None;
    }
    specs.iter().find_map(|shortcut| {
        if shortcut.scope == ShortcutScope::Viewport && !context.viewport_active {
            return None;
        }
        if shortcut.requires_selection && !context.has_selection {
            return None;
        }
        shortcut
            .chord
            .filter(|chord| chord.matches(input))
            .map(|_| shortcut.command)
    })
}

pub fn camera_bookmark(input: &InputSnapshot, context: ShortcutContext) -> Option<(usize, bool)> {
    if context.keyboard_captured_by_text || context.modal_open || !context.viewport_active {
        return None;
    }
    let slot = if input.key_pressed(InputKey::Digit1) || input.key_pressed(InputKey::Numpad1) {
        Some(0)
    } else if input.key_pressed(InputKey::Digit2) || input.key_pressed(InputKey::Numpad2) {
        Some(1)
    } else if input.key_pressed(InputKey::Digit3) || input.key_pressed(InputKey::Numpad3) {
        Some(2)
    } else {
        None
    }?;

    Some((slot, input.modifiers.command_modifier()))
}

pub fn help_text(language: Language) -> String {
    let mut specs = Vec::with_capacity(
        EDITOR_SHORTCUTS.len() + VIEWPORT_SHORTCUTS.len() + CAMERA_BOOKMARK_SHORTCUTS.len(),
    );
    specs.extend_from_slice(EDITOR_SHORTCUTS);
    specs.extend_from_slice(VIEWPORT_SHORTCUTS);
    specs.extend_from_slice(CAMERA_BOOKMARK_SHORTCUTS);

    let details = specs
        .iter()
        .map(|shortcut| {
            let label = strip_accelerator(&t(shortcut.label_key, language));
            format!("{}: {}", shortcut.accelerator, label)
        })
        .collect::<Vec<_>>()
        .join(" · ");
    format!("{}: {details}", t("app.keyboard_shortcuts", language))
}

fn strip_accelerator(label: &str) -> String {
    let label = label
        .split_once("  (")
        .map(|(label, _)| label.trim().to_string())
        .unwrap_or_else(|| label.trim().to_string());
    label
        .split_once("  -  ")
        .map(|(_, label)| label.trim().to_string())
        .unwrap_or(label)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn input_with(key: InputKey, shift: bool) -> InputSnapshot {
        let mut input = InputSnapshot::default();
        input.modifiers.control = true;
        input.modifiers.shift = shift;
        input.keys_pressed.insert(key);
        input
    }

    fn context() -> ShortcutContext {
        ShortcutContext {
            viewport_active: true,
            has_selection: true,
            ..ShortcutContext::default()
        }
    }

    #[test]
    fn global_shortcuts_share_the_application_command_ids() {
        assert_eq!(
            global_command(&input_with(InputKey::S, false), context()),
            Some(command::PROJECT_SAVE)
        );
        assert_eq!(
            global_command(&input_with(InputKey::A, false), context()),
            Some(command::EDIT_SELECT_ALL)
        );
        assert_eq!(
            global_command(&input_with(InputKey::C, false), context()),
            Some(command::EDIT_COPY)
        );
        assert_eq!(
            global_command(&input_with(InputKey::V, false), context()),
            Some(command::EDIT_PASTE)
        );
        assert_eq!(
            global_command(&input_with(InputKey::Z, true), context()),
            Some(command::EDIT_REDO)
        );
    }

    #[test]
    fn delete_is_global_without_a_command_modifier() {
        let mut input = InputSnapshot::default();
        input.keys_pressed.insert(InputKey::Delete);
        assert_eq!(
            global_command(&input, context()),
            Some(command::EDIT_DELETE)
        );
    }

    #[test]
    fn scale_uses_t_without_claiming_camera_s() {
        let scale = VIEWPORT_SHORTCUTS
            .iter()
            .find(|shortcut| shortcut.command == "viewport.scale")
            .expect("scale shortcut should be registered");
        assert_eq!(scale.accelerator, "T");
        assert!(VIEWPORT_SHORTCUTS
            .iter()
            .all(|shortcut| shortcut.accelerator != "S"));
    }
}
