//! Cross-boundary checks of real authoring modules without booting the editor shell.
//! Kept beside the AGB surface QA so unrelated shell compilation cannot hide
//! failures in staged placement, thumbnail capture or field-query semantics.
#![allow(dead_code)]

#[path = "../../raf_editor/src/asset_previews.rs"]
mod asset_previews;
#[path = "../../raf_editor/src/authoring_placement.rs"]
mod authoring_placement;
#[path = "../../raf_editor/src/panels/hierarchy_query.rs"]
mod hierarchy_query;

#[path = "../../raf_editor/src/commands/output.rs"]
mod command_output;
#[path = "../../raf_editor/src/commands/parser.rs"]
mod command_parser;
#[path = "../../raf_editor/src/commands/game.rs"]
mod game_commands;

mod commands {
    pub(crate) use super::command_output as output;
    pub(crate) use super::command_parser as parser;
}
