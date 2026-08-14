//! Manual command console infrastructure.
//!
//! The catalog is data-driven JSON so the same definitions can later be
//! exposed to agents, complements, or external callers. Rust keeps the
//! handlers because the engine mutations still need typed access to editor
//! documents.

pub mod assets;
pub mod catalog;
pub mod electronics;
pub mod game;
pub mod gateway;
pub mod output;
pub mod parser;
pub mod script;
pub mod sessions;
pub mod workspace;

pub use catalog::{CommandCatalog, CommandDefinition};
pub use gateway::{CommandGateway, HeadlessCommandMode, ParsedCommandExecutor};
pub use output::{CommandLevel, CommandOutput};
pub use parser::{parse_console_input, ParsedCommand, ParsedInput};
