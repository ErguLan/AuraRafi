//! Argument parsing, CLI controls, request construction and usage text.

use crate::cli::CliResult;
use crate::cli::PROGRAM;
use raf_core::transaction::ExecutionBudget;
use raf_core::{CommandSource, EngineCommandRequest};
use serde_json::Value;
use std::collections::BTreeMap;
use std::io::{self, Read};
use std::path::{Path, PathBuf};

/// Keep ad-hoc parameter documents bounded. The largest project document the
/// CLI currently edits is a 4 MiB UI file, so 8 MiB leaves room for a JSON
/// envelope without making an accidental multi-gigabyte read possible.
const MAX_PARAMS_BYTES: u64 = 8 * 1024 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum OutputFormat {
    Human,
    Json,
    Ndjson,
}

impl OutputFormat {
    pub(super) fn from_options(options: &Options) -> Self {
        if options.has("ndjson") {
            Self::Ndjson
        } else if options.has("json") || options.value("format") == Some("json") {
            Self::Json
        } else {
            Self::Human
        }
    }
}

#[derive(Debug, Default)]
pub(super) struct Options {
    pub(super) flags: BTreeMap<String, Option<String>>,
    pub(super) positionals: Vec<String>,
}

impl Options {
    pub(super) fn has(&self, name: &str) -> bool {
        self.flags.contains_key(name)
    }

    pub(super) fn value(&self, name: &str) -> Option<&str> {
        self.flags.get(name).and_then(Option::as_deref)
    }

    pub(super) fn path(&self, name: &str) -> Option<PathBuf> {
        self.value(name).map(PathBuf::from)
    }
}

pub(super) fn parse_options(arguments: &[String]) -> Options {
    let mut options = Options::default();
    let mut index = 0;
    while index < arguments.len() {
        let argument = &arguments[index];
        if let Some(raw) = argument.strip_prefix("--") {
            if let Some((name, value)) = raw.split_once('=') {
                options
                    .flags
                    .insert(name.to_ascii_lowercase(), Some(value.to_string()));
            } else if arguments
                .get(index + 1)
                .is_some_and(|next| !next.starts_with('-') || next == "-")
            {
                options
                    .flags
                    .insert(raw.to_ascii_lowercase(), arguments.get(index + 1).cloned());
                index += 1;
            } else {
                options.flags.insert(raw.to_ascii_lowercase(), None);
            }
        } else if let Some(raw) = argument.strip_prefix('-') {
            if raw == "j" {
                options.flags.insert("json".to_string(), None);
            } else {
                options.positionals.push(argument.clone());
            }
        } else {
            options.positionals.push(argument.clone());
        }
        index += 1;
    }
    options
}

pub(super) fn observation_command_request(
    options: &Options,
    name: &str,
    fallback_params: Value,
) -> CliResult<EngineCommandRequest> {
    let params = if options.value("params").is_some() {
        parse_params(options)?
    } else {
        fallback_params
    };
    let mut request = EngineCommandRequest::new(name, params, CommandSource::Cli);
    apply_cli_controls(&mut request, options, false)?;
    Ok(request)
}

pub(super) fn build_command_request(options: &Options) -> CliResult<EngineCommandRequest> {
    let name = options
        .positionals
        .get(
            if options.positionals.first().map(String::as_str) == Some("attach") {
                2
            } else {
                1
            },
        )
        .ok_or_else(|| "raf command requires a command name.".to_string())?;
    let mut request = EngineCommandRequest::new(name, parse_params(options)?, CommandSource::Cli);
    apply_cli_controls(&mut request, options, false)?;
    Ok(request)
}

pub(super) fn named_command_request(
    options: &Options,
    name: &str,
    force_dry_run: bool,
) -> CliResult<EngineCommandRequest> {
    let mut request = EngineCommandRequest::new(name, parse_params(options)?, CommandSource::Cli);
    apply_cli_controls(&mut request, options, force_dry_run)?;
    Ok(request)
}

pub(super) fn parse_params(options: &Options) -> CliResult<Value> {
    if options.value("params").is_some() && options.value("params-file").is_some() {
        return Err("Use only one of --params or --params-file.".to_string());
    }
    let raw = if let Some(path) = options.value("params-file") {
        read_params_source(path)?
    } else if let Some(value) = options.value("params") {
        if let Some(path) = value.strip_prefix('@') {
            read_params_source(path)?
        } else if value == "-" {
            read_params_source("-")?
        } else {
            value.to_string()
        }
    } else {
        "{}".to_string()
    };
    let params: Value = serde_json::from_str(&raw)
        .map_err(|error| format!("Invalid params JSON: {error}"))?;
    if !params.is_object() {
        return Err("params must be a JSON object.".to_string());
    }
    Ok(params)
}

/// Read a JSON parameter document from a project-adjacent file or stdin.
/// `--params-file -` and `--params -` both read stdin so generated payloads
/// never return to the Windows command-line length limit.
fn read_params_source(path: &str) -> CliResult<String> {
    if path == "-" {
        let stdin = io::stdin();
        let mut source = String::new();
        stdin
            .lock()
            .take(MAX_PARAMS_BYTES + 1)
            .read_to_string(&mut source)
            .map_err(|error| format!("Read params from stdin: {error}"))?;
        if source.len() as u64 > MAX_PARAMS_BYTES {
            return Err(format!(
                "params from stdin exceed the {} byte safety limit.",
                MAX_PARAMS_BYTES
            ));
        }
        return Ok(source);
    }
    let file = Path::new(path);
    let metadata = std::fs::metadata(file)
        .map_err(|error| format!("params file {}: {error}", file.display()))?;
    if !metadata.is_file() {
        return Err(format!("params path is not a file: {}", file.display()));
    }
    if metadata.len() > MAX_PARAMS_BYTES {
        return Err(format!(
            "params file {} exceeds the {} byte safety limit.",
            file.display(),
            MAX_PARAMS_BYTES
        ));
    }
    let mut source = String::new();
    std::fs::File::open(file)
        .and_then(|file| file.take(MAX_PARAMS_BYTES + 1).read_to_string(&mut source))
        .map_err(|error| format!("Read params file {}: {error}", file.display()))?;
    if source.len() as u64 > MAX_PARAMS_BYTES {
        return Err(format!(
            "params file {} exceeds the {} byte safety limit.",
            file.display(),
            MAX_PARAMS_BYTES
        ));
    }
    Ok(source)
}

fn apply_cli_controls(
    request: &mut EngineCommandRequest,
    options: &Options,
    force_dry_run: bool,
) -> CliResult<()> {
    request.confirm = options.has("confirm");
    request.dry_run = force_dry_run || options.has("dry-run");
    request.expected_revision = options
        .value("expected-revision")
        .map(|value| {
            value
                .parse::<u64>()
                .map_err(|_| "expected-revision must be an integer.")
        })
        .transpose()?;
    request.idempotency_key = options.value("idempotency-key").map(str::to_string);
    request.budget = budget_from_options(options)?;
    Ok(())
}

pub(super) fn budget_from_options(options: &Options) -> CliResult<Option<ExecutionBudget>> {
    let mut budget = options
        .value("budget")
        .map(|raw| {
            serde_json::from_str::<ExecutionBudget>(raw)
                .map_err(|error| format!("Invalid --budget JSON: {error}"))
        })
        .transpose()?
        .unwrap_or(ExecutionBudget {
            max_tool_calls: None,
            max_milliseconds: None,
            max_scene_operations: None,
            max_scene_entities: None,
            max_result_bytes: None,
            profile: None,
        });

    budget.max_tool_calls = parse_budget_u32(options, "max-tool-calls", budget.max_tool_calls)?;
    budget.max_milliseconds =
        parse_budget_u64(options, "max-milliseconds", budget.max_milliseconds)?;
    budget.max_scene_operations =
        parse_budget_u32(options, "max-scene-operations", budget.max_scene_operations)?;
    budget.max_scene_entities =
        parse_budget_u32(options, "max-scene-entities", budget.max_scene_entities)?;
    budget.max_result_bytes =
        parse_budget_u32(options, "max-result-bytes", budget.max_result_bytes)?;
    if let Some(profile) = options.value("budget-profile") {
        budget.profile = Some(profile.to_string());
    }

    let has_budget = budget.max_tool_calls.is_some()
        || budget.max_milliseconds.is_some()
        || budget.max_scene_operations.is_some()
        || budget.max_scene_entities.is_some()
        || budget.max_result_bytes.is_some()
        || budget.profile.is_some();
    Ok(has_budget.then_some(budget))
}

fn parse_budget_u32(options: &Options, name: &str, current: Option<u32>) -> CliResult<Option<u32>> {
    options
        .value(name)
        .map(|raw| {
            raw.parse::<u32>()
                .map_err(|_| format!("--{name} must be an unsigned integer."))
        })
        .transpose()
        .map(|value| value.or(current))
}

fn parse_budget_u64(options: &Options, name: &str, current: Option<u64>) -> CliResult<Option<u64>> {
    options
        .value(name)
        .map(|raw| {
            raw.parse::<u64>()
                .map_err(|_| format!("--{name} must be an unsigned integer."))
        })
        .transpose()
        .map(|value| value.or(current))
}

pub(super) fn print_help() {
    println!(
        "{PROGRAM} - lightweight AuraRafi command and MCP surface\n\n\
Usage:\n  raf doctor [--json]\n  raf editors [--json]                 list known projects and live editors\n  raf status [--project PATH] [--json]\n  raf attach [ACTION] [--project PATH]  PATH is optional when one editor is live\n  raf open PATH                   launch the editor and wait for its live endpoint\n  raf project create --name NAME [--parent PATH] [--type game|electronics] --confirm\n  raf project open PATH\n  raf project close [--confirm]\n  raf project info [PATH]\n  raf project path [PATH]               print the explicit or attached project root\n  raf capabilities [search QUERY]\n  raf session list [--project PATH]\n  raf session use ID|NAME [--confirm]\n  raf undo | raf redo\n  raf workspace describe [PATH]\n  raf command NAME [--params JSON|@FILE|-] [--params-file FILE|-] [--dry-run] [--confirm]\n  raf mcp serve [--attach PATH [--launch]]   no path: starts unattached; use raf_editors/raf_attach\n  raf serve [--project PATH]            JSONL command endpoint\n\nGlobal output: --json, --ndjson, --format human|json"
    );
    println!("Project persistence: raf project save|checkpoint [--wait] --confirm");
    println!("Budgets: --budget JSON or --max-tool-calls N --max-milliseconds N --max-scene-operations N --max-scene-entities N --max-result-bytes N --budget-profile NAME");
    println!("Additional inspection: raf context [--project PATH]");
    println!("Performance: raf perf [--window SECONDS] [--json]");
    println!("Benchmarks: raf benchmark start [--seconds N] [--warmup N] [--label TEXT] | status [--id ID] | stop [--id ID] | cancel | list | compare A B, all with [--json]");
    println!("Scene automation: raf scene outline|digest|query|inspect|selection|verify|navigation|map|create|update|build|reconcile|repair|program|commit|cancel|brief|batch, raf selection, raf viewport capture [--preset contact_sheet|top_down_map], raf task list|get|events|cancel, raf assets search|inspect, raf scripts search, raf health");
}
