//! Command-line surface for the `raf` binary.
//!
//! The facade keeps the shared CLI constants, the shared result type and the
//! module registrations. Parsing, attached-editor commands, headless
//! execution, output formatting and the MCP server live in focused siblings.

mod args;
mod attach;
pub(crate) mod commands;
mod mcp;
mod output;
#[cfg(test)]
mod tests;

pub(crate) const PROGRAM: &str = "raf";
pub(crate) const PROTOCOL_VERSION: &str = "2025-06-18";

pub(crate) type CliResult<T> = Result<T, String>;

pub(crate) use mcp::read_plan_resource;
pub(crate) use mcp::mcp_command_is_mutation;
