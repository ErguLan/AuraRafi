//! Canonical editor command registry.
//!
//! Every entry point emits the same command ID. Presentation surfaces may
//! render labels and accelerators, while the application boundary decides
//! whether the command is available and executes the domain mutation.

use std::collections::VecDeque;

use raf_core::project::ProjectType;
use raf_core::{InputOwner, InputSnapshot};

use crate::application_menu::command;
use crate::editor_shortcuts::{self, ShortcutContext};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EditorCommandScope {
    Application,
    Game,
    Electronics,
    Viewport,
    RetainedUi,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EditorCommandSource {
    Menu,
    Toolbar,
    ContextMenu,
    Shortcut,
    RafUi,
    Cli,
    Mcp,
    Internal,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EditorCommandSpec {
    pub id: &'static str,
    pub scope: EditorCommandScope,
    pub accelerator: &'static str,
    pub requires_project: bool,
    pub requires_selection: bool,
    pub blocked_by_text_input: bool,
}

pub const EDITOR_COMMANDS: &[EditorCommandSpec] = &[
    spec(
        command::PROJECT_NEW,
        EditorCommandScope::Application,
        "",
        false,
        false,
        false,
    ),
    spec(
        command::PROJECT_OPEN,
        EditorCommandScope::Application,
        "",
        false,
        false,
        false,
    ),
    spec(
        command::PROJECT_SAVE,
        EditorCommandScope::Application,
        "Ctrl+S",
        true,
        false,
        true,
    ),
    spec(
        command::EDITOR_SETTINGS,
        EditorCommandScope::Application,
        "",
        false,
        false,
        false,
    ),
    spec(
        command::PROJECT_SETTINGS,
        EditorCommandScope::Application,
        "",
        true,
        false,
        false,
    ),
    spec(
        command::EDIT_UNDO,
        EditorCommandScope::Application,
        "Ctrl+Z",
        true,
        false,
        true,
    ),
    spec(
        command::EDIT_REDO,
        EditorCommandScope::Application,
        "Ctrl+Y / Ctrl+Shift+Z",
        true,
        false,
        true,
    ),
    spec(
        command::EDIT_DUPLICATE,
        EditorCommandScope::Game,
        "Ctrl+D",
        true,
        true,
        true,
    ),
    spec(
        command::EDIT_COPY,
        EditorCommandScope::Game,
        "Ctrl+C",
        true,
        true,
        true,
    ),
    spec(
        command::EDIT_PASTE,
        EditorCommandScope::Game,
        "Ctrl+V",
        true,
        false,
        true,
    ),
    spec(
        command::EDIT_DELETE,
        EditorCommandScope::Application,
        "Del",
        true,
        true,
        true,
    ),
    spec(
        command::EDIT_SELECT_ALL,
        EditorCommandScope::Application,
        "Ctrl+A",
        true,
        false,
        true,
    ),
    spec(
        command::SEARCH_OPEN,
        EditorCommandScope::Application,
        "Ctrl+K",
        false,
        false,
        true,
    ),
    spec(
        "viewport.move",
        EditorCommandScope::Viewport,
        "G",
        true,
        true,
        true,
    ),
    spec(
        "viewport.rotate",
        EditorCommandScope::Viewport,
        "R",
        true,
        true,
        true,
    ),
    spec(
        "viewport.scale",
        EditorCommandScope::Viewport,
        "T",
        true,
        true,
        true,
    ),
    spec(
        "viewport.select",
        EditorCommandScope::Viewport,
        "C",
        true,
        false,
        true,
    ),
    spec(
        "viewport.focus",
        EditorCommandScope::Viewport,
        "F",
        true,
        true,
        true,
    ),
    spec(
        "viewport.view_2d",
        EditorCommandScope::Viewport,
        "Num2",
        true,
        false,
        true,
    ),
    spec(
        "viewport.view_3d",
        EditorCommandScope::Viewport,
        "Num3",
        true,
        false,
        true,
    ),
    spec(
        "viewport.reset_view",
        EditorCommandScope::Viewport,
        "",
        true,
        false,
        false,
    ),
    spec(
        "window.close",
        EditorCommandScope::Application,
        "",
        false,
        false,
        false,
    ),
];

const fn spec(
    id: &'static str,
    scope: EditorCommandScope,
    accelerator: &'static str,
    requires_project: bool,
    requires_selection: bool,
    blocked_by_text_input: bool,
) -> EditorCommandSpec {
    EditorCommandSpec {
        id,
        scope,
        accelerator,
        requires_project,
        requires_selection,
        blocked_by_text_input,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EditorCommandAvailability {
    pub project_open: bool,
    pub project_type: Option<ProjectType>,
    pub has_selection: bool,
    pub text_input_focused: bool,
    pub modal_open: bool,
    pub viewport_active: bool,
}

impl Default for EditorCommandAvailability {
    fn default() -> Self {
        Self {
            project_open: false,
            project_type: None,
            has_selection: false,
            text_input_focused: false,
            modal_open: false,
            viewport_active: false,
        }
    }
}

#[derive(Debug, Clone)]
pub struct EditorCommandEnvelope {
    pub id: String,
    pub source: EditorCommandSource,
    pub sequence: u64,
}

#[derive(Debug, Default)]
pub struct EditorCommandRegistry {
    queue: VecDeque<EditorCommandEnvelope>,
    next_sequence: u64,
}

impl EditorCommandRegistry {
    pub fn spec(id: &str) -> Option<&'static EditorCommandSpec> {
        EDITOR_COMMANDS.iter().find(|spec| spec.id == id)
    }

    pub fn all_specs() -> &'static [EditorCommandSpec] {
        EDITOR_COMMANDS
    }

    pub fn is_available(id: &str, state: EditorCommandAvailability) -> bool {
        let Some(spec) = Self::spec(id) else {
            // Domain commands exposed through CLI/MCP are validated by their
            // own catalog; this registry only governs editor entry points.
            return false;
        };
        if spec.requires_project && !state.project_open {
            return false;
        }
        if spec.requires_selection && !state.has_selection {
            return false;
        }
        if spec.blocked_by_text_input && state.text_input_focused {
            return false;
        }
        if spec.scope == EditorCommandScope::Viewport && !state.viewport_active {
            return false;
        }
        if spec.scope == EditorCommandScope::Game && state.project_type != Some(ProjectType::Game) {
            return false;
        }
        if spec.scope == EditorCommandScope::Electronics
            && state.project_type != Some(ProjectType::Electronics)
        {
            return false;
        }
        !state.modal_open || matches!(spec.scope, EditorCommandScope::Application)
    }

    pub fn enqueue(
        &mut self,
        id: impl Into<String>,
        source: EditorCommandSource,
        state: EditorCommandAvailability,
    ) -> bool {
        let id = id.into();
        if !Self::is_available(&id, state) {
            return false;
        }
        self.next_sequence = self.next_sequence.wrapping_add(1).max(1);
        self.queue.push_back(EditorCommandEnvelope {
            id,
            source,
            sequence: self.next_sequence,
        });
        true
    }

    pub fn enqueue_unchecked(&mut self, id: impl Into<String>, source: EditorCommandSource) -> u64 {
        self.next_sequence = self.next_sequence.wrapping_add(1).max(1);
        let sequence = self.next_sequence;
        self.queue.push_back(EditorCommandEnvelope {
            id: id.into(),
            source,
            sequence,
        });
        sequence
    }

    pub fn enqueue_shortcut(
        &mut self,
        input: &InputSnapshot,
        state: EditorCommandAvailability,
    ) -> bool {
        let context = ShortcutContext {
            keyboard_captured_by_text: state.text_input_focused,
            modal_open: state.modal_open,
            viewport_active: state.viewport_active,
            has_selection: state.has_selection,
        };
        let id = editor_shortcuts::global_command(input, context)
            .or_else(|| editor_shortcuts::viewport_command(input, context));
        let Some(id) = id else {
            return false;
        };
        self.enqueue(id, EditorCommandSource::Shortcut, state)
    }

    pub fn drain(&mut self) -> impl Iterator<Item = EditorCommandEnvelope> + '_ {
        self.queue.drain(..)
    }

    pub fn pending(&self) -> usize {
        self.queue.len()
    }

    pub fn input_owner_blocks_viewport(owner: Option<InputOwner>) -> bool {
        matches!(owner, Some(InputOwner::Modal(_) | InputOwner::DragDrop(_)))
    }
}
