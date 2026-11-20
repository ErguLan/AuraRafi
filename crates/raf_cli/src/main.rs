//! `raf`: the lightweight, headless command and MCP surface for AuraRafi.
//!
//! This binary intentionally depends on `raf_core` only. It can inspect and
//! create projects without opening the renderer or editor window, or attach
//! to an already-open editor through the project-scoped local IPC descriptor.

mod attached;
mod cli;
mod discovery;

use crate::cli::PROGRAM;
use std::env;
use std::process;

fn main() {
    if let Err(error) = cli::commands::run_cli(env::args().skip(1).collect()) {
        eprintln!("{PROGRAM}: {error}");
        process::exit(2);
    }
}
