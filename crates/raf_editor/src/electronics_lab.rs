//! Session-scoped data for Electronics inside the existing Agent workspace.
use std::collections::{BTreeMap, VecDeque};
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver, TryRecvError};
use std::time::{Duration, Instant};

use raf_electronics::{Netlist, Schematic, SimulationResults};
use raf_hardware::serial_session::{self, LinkState, Port, Reading, SerialSession, Snapshot};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

const MAX_RUNS: usize = 16;
const MAX_RUN_VALUES: usize = 512;
const MAX_POINTS: usize = 4096;
const MAX_STORE_BYTES: u64 = 12 * 1024 * 1024;
const MAX_FIRMWARE_BYTES: u64 = 32 * 1024 * 1024;
const REFRESH_INTERVAL: Duration = Duration::from_millis(100);
pub(crate) const MAX_GRAPH_PARTS: usize = 96;
pub(crate) const MAX_GRAPH_WIRES: usize = 192;

type Job<T> = Receiver<Result<T, String>>;

fn run_job<T: Send + 'static>(
    name: &str,
    f: impl FnOnce() -> Result<T, String> + Send + 'static,
) -> Result<Job<T>, String> {
    let (tx, rx) = mpsc::sync_channel(1);
    std::thread::Builder::new()
        .name(name.into())
        .spawn(move || {
            let _ = tx.send(f());
        })
        .map_err(|error| error.to_string())?;
    Ok(rx)
}

fn poll_job<T>(job: &mut Option<Job<T>>) -> Option<Result<T, String>> {
    let result = match job.as_ref()?.try_recv() {
        Ok(result) => result,
        Err(TryRecvError::Empty) => return None,
        Err(TryRecvError::Disconnected) => {
            Err("Background job ended before returning a result.".into())
        }
    };
    *job = None;
    Some(result)
}

#[derive(Clone, Copy, Default, PartialEq, Eq)]
pub(crate) enum LabView {
    #[default]
    Simulation,
    Device,
    Captures,
    Chat,
}

#[derive(Clone, Serialize, Deserialize)]
pub(crate) struct DcValue {
    pub component: Uuid,
    pub designator: String,
    pub pin: Option<usize>,
    pub value: f64,
    pub unit: String,
}

#[derive(Clone, Serialize, Deserialize)]
pub(crate) struct DcRun {
    pub id: Uuid,
    pub unix_ms: i64,
    pub schematic: String,
    pub fingerprint: u64,
    pub converged: bool,
    pub kcl_residual_a: f64,
    pub power_residual_w: f64,
    pub messages: Vec<String>,
    pub values: Vec<DcValue>,
    pub values_truncated: bool,
}

#[derive(Clone, Serialize, Deserialize)]
pub(crate) struct Capture {
    pub id: Uuid,
    pub unix_ms: i64,
    pub port: String,
    pub device: Option<String>,
    pub lost_prefix: bool,
    pub points: Vec<Reading>,
}

#[derive(Clone, Copy, Serialize, Deserialize)]
pub(crate) struct Probe {
    pub component: Uuid,
    pub pin: Option<usize>,
}

#[derive(Clone, Default, Serialize, Deserialize)]
struct SavedLab {
    runs: Vec<DcRun>,
    captures: Vec<Capture>,
    bindings: BTreeMap<String, Probe>,
}

#[derive(Clone)]
pub(crate) struct Graph {
    pub parts: Vec<GraphPart>,
    pub nets: Vec<(usize, String)>,
    pub edges: Vec<(Uuid, usize, usize)>,
    pub wires: Vec<GraphWire>,
    pub limited: bool,
}

#[derive(Clone)]
pub(crate) struct GraphPart {
    pub id: Uuid,
    pub name: String,
    pub is_source: bool,
    pub is_led: bool,
    pub position: [f32; 2],
    pub terminals: Vec<[f32; 2]>,
}

#[derive(Clone)]
pub(crate) struct GraphWire {
    pub start: [f32; 2],
    pub end: [f32; 2],
    pub net: Option<usize>,
}

impl Graph {
    fn build(schematic: &Schematic, netlist: Option<&Netlist>) -> Self {
        use raf_electronics::component::SimModel;
        let mut visible: Vec<_> = schematic
            .components
            .iter()
            .enumerate()
            .take(MAX_GRAPH_PARTS)
            .collect();
        visible.sort_by(|(_, a), (_, b)| {
            let source = |part: &raf_electronics::component::ElectronicComponent| {
                matches!(part.sim_model, SimModel::DcSource { .. })
            };
            source(b)
                .cmp(&source(a))
                .then_with(|| a.position.y.total_cmp(&b.position.y))
                .then_with(|| a.position.x.total_cmp(&b.position.x))
        });
        let mut nets = BTreeMap::new();
        let mut edges = Vec::new();
        if let Some(netlist) = netlist {
            for (index, part) in &visible {
                for pin in 0..part.pins.len().min(12) {
                    if let Some(net) = netlist
                        .net_for_pin(*index, pin)
                        .filter(|net| net.pins.len() > 1)
                    {
                        nets.entry(net.id).or_insert_with(|| net.name.clone());
                        edges.push((part.id, pin, net.id));
                    }
                }
            }
        }
        let limited = visible.len() < schematic.components.len()
            || edges.len() > MAX_GRAPH_WIRES
            || schematic.wires.len() > MAX_GRAPH_WIRES
            || (netlist.is_none() && !schematic.wires.is_empty());
        edges.truncate(MAX_GRAPH_WIRES);
        Self {
            parts: visible
                .into_iter()
                .map(|(_, part)| GraphPart {
                    id: part.id,
                    name: part.designator.clone(),
                    is_source: matches!(part.sim_model, SimModel::DcSource { .. }),
                    is_led: matches!(part.sim_model, SimModel::Led { .. }),
                    position: part.position.to_array(),
                    terminals: part
                        .pins
                        .iter()
                        .take(12)
                        .map(|pin| {
                            raf_electronics::schematic::component_pin_world_position(part, pin)
                                .to_array()
                        })
                        .collect(),
                })
                .collect(),
            nets: nets.into_iter().collect(),
            edges,
            wires: schematic
                .wires
                .iter()
                .take(MAX_GRAPH_WIRES)
                .filter_map(|wire| {
                    let a = wire
                        .start_anchor
                        .and_then(|anchor| schematic.resolve_anchor(anchor))
                        .unwrap_or(wire.start);
                    let b = wire
                        .end_anchor
                        .and_then(|anchor| schematic.resolve_anchor(anchor))
                        .unwrap_or(wire.end);
                    (a.is_finite() && b.is_finite()).then(|| GraphWire {
                        start: a.to_array(),
                        end: b.to_array(),
                        net: netlist
                            .and_then(|nets| nets.net_for_wire(wire.id))
                            .map(|net| net.id),
                    })
                })
                .collect(),
            limited,
        }
    }
}

#[derive(Clone, Serialize)]
pub(crate) struct OtaPreparation {
    pub board: String,
    pub installed_firmware: String,
    pub target: String,
    pub filename: String,
    pub bytes: u64,
    pub sha256: String,
}

#[derive(Default)]
pub(crate) struct ElectronicsLab {
    pub view: LabView,
    pub version: u64,
    pub graph: Option<Graph>,
    graph_fingerprint: Option<u64>,
    pub graph_version: u64,
    pub show_model_details: bool,
    pub selected: Option<Uuid>,
    pub selected_pin: Option<usize>,
    pub selected_channel: Option<String>,
    pub selected_run: Option<Uuid>,
    pub selected_capture: Option<Uuid>,
    pub ports: Vec<Port>,
    pub scanned: bool,
    pub baud: u32,
    pub port: Option<String>,
    pub serial: Option<Snapshot>,
    pub recent: Vec<Reading>,
    pub error: Option<String>,
    pub ota: Option<OtaPreparation>,
    pub ota_export: Option<PathBuf>,
    pub replay_ms: u64,
    replay_started: Option<(Instant, u64)>,
    pub recording: Option<u64>,
    pub output_values: BTreeMap<usize, String>,
    link: Option<SerialSession>,
    refresh: Option<Instant>,
    discover: Option<Job<Vec<Port>>>,
    ota_job: Option<Job<OtaPreparation>>,
    saved: SavedLab,
    store_path: Option<PathBuf>,
    load: Option<Job<SavedLab>>,
    write: Option<Job<()>>,
    write_pending: bool,
    store_readable: bool,
    pub dc_requested: bool,
    dc_tool: Option<String>,
    completed_tools: VecDeque<(String, Result<DcRun, String>)>,
    pub dc_failure: Option<String>,
    pub dc_persist: bool,
    transient_run: Option<DcRun>,
}

impl ElectronicsLab {
    pub fn touch(&mut self) {
        self.version = self.version.wrapping_add(1);
    }

    pub fn load_for(&mut self, project: &Path, relative: &Path) {
        if self.store_path.is_some() {
            return;
        }
        let relative = relative.with_extension("lab.json");
        let path = match raf_core::project_store::confined_path(project, &relative) {
            Ok(path) => path,
            Err(error) => {
                self.error = Some(error);
                self.touch();
                return;
            }
        };
        self.store_path = Some(path.clone());
        match run_job("raf-lab-load", move || {
            let file = match std::fs::File::open(path) {
                Ok(file) => file,
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                    return Ok(SavedLab::default())
                }
                Err(error) => return Err(error.to_string()),
            };
            let mut bytes = Vec::new();
            file.take(MAX_STORE_BYTES + 1)
                .read_to_end(&mut bytes)
                .map_err(|error| error.to_string())?;
            if bytes.len() as u64 > MAX_STORE_BYTES {
                return Err("Saved laboratory data exceeds the size limit.".into());
            }
            let mut saved: SavedLab =
                serde_json::from_slice(&bytes).map_err(|error| error.to_string())?;
            saved.runs.truncate(MAX_RUNS);
            saved.captures.truncate(MAX_RUNS);
            for capture in &mut saved.captures {
                capture.points.truncate(MAX_POINTS);
            }
            Ok(saved)
        }) {
            Ok(job) => self.load = Some(job),
            Err(error) => self.error = Some(error),
        }
        self.touch();
    }

    pub fn sync_graph(&mut self, schematic: &Schematic, fingerprint: u64) {
        if self.graph_fingerprint == Some(fingerprint) {
            return;
        }
        self.graph_fingerprint = Some(fingerprint);
        let bounded = schematic.components.len() <= MAX_GRAPH_PARTS
            && schematic.wires.len() <= MAX_GRAPH_WIRES;
        let netlist = bounded.then(|| schematic.netlist());
        self.graph = Some(Graph::build(schematic, netlist.as_ref()));
        self.graph_version = self.graph_version.wrapping_add(1);
        self.touch();
    }

    pub fn runs(&self) -> &[DcRun] {
        &self.saved.runs
    }
    pub fn captures(&self) -> &[Capture] {
        &self.saved.captures
    }
    pub fn binding(&self, channel: &str) -> Option<Probe> {
        self.saved.bindings.get(channel).copied()
    }
    pub fn run(&self) -> Option<&DcRun> {
        self.selected_run
            .and_then(|id| {
                self.saved
                    .runs
                    .iter()
                    .chain(self.transient_run.iter())
                    .find(|run| run.id == id)
            })
            .or_else(|| self.saved.runs.first())
    }
    pub fn capture(&self) -> Option<&Capture> {
        self.selected_capture
            .and_then(|id| self.saved.captures.iter().find(|capture| capture.id == id))
            .or_else(|| self.saved.captures.first())
    }

    pub fn accept_dc(
        &mut self,
        schematic: &Schematic,
        fingerprint: u64,
        netlist: &Netlist,
        result: &SimulationResults,
    ) {
        self.graph_fingerprint = Some(fingerprint);
        self.graph = Some(Graph::build(schematic, Some(netlist)));
        self.graph_version = self.graph_version.wrapping_add(1);
        let mut run = DcRun {
            id: Uuid::new_v4(),
            unix_ms: chrono::Utc::now().timestamp_millis(),
            schematic: schematic.name.clone(),
            fingerprint,
            converged: result.converged,
            kcl_residual_a: result.max_kcl_residual_amps,
            power_residual_w: result.power_balance_watts,
            messages: result.messages.iter().take(16).cloned().collect(),
            values: Vec::new(),
            values_truncated: false,
        };
        if result.converged {
            for (index, part) in schematic.components.iter().enumerate() {
                if let Some(current) = result
                    .component_currents
                    .get(&index)
                    .filter(|value| value.is_finite())
                {
                    run.values.push(DcValue {
                        component: part.id,
                        designator: part.designator.clone(),
                        pin: None,
                        value: *current,
                        unit: "A".into(),
                    });
                }
                for pin in 0..part.pins.len() {
                    if let Some(voltage) = netlist
                        .net_for_pin(index, pin)
                        .and_then(|net| result.node_voltages.get(&net.id))
                        .filter(|value| value.is_finite())
                    {
                        run.values.push(DcValue {
                            component: part.id,
                            designator: part.designator.clone(),
                            pin: Some(pin),
                            value: *voltage,
                            unit: "V".into(),
                        });
                    }
                    if run.values.len() >= MAX_RUN_VALUES {
                        break;
                    }
                }
                if run.values.len() >= MAX_RUN_VALUES {
                    run.values_truncated = true;
                    break;
                }
            }
        }
        self.selected_run = Some(run.id);
        if self
            .selected
            .is_none_or(|id| !schematic.components.iter().any(|part| part.id == id))
        {
            self.selected = schematic
                .components
                .iter()
                .find(|part| {
                    matches!(
                        part.sim_model,
                        raf_electronics::component::SimModel::Led { .. }
                    )
                })
                .or_else(|| schematic.components.first())
                .map(|part| part.id);
            self.selected_pin = None;
        }
        if self.dc_tool.is_some() {
            self.finish_tool(Ok(run.clone()));
        }
        if self.dc_persist {
            self.saved.runs.insert(0, run);
            self.saved.runs.truncate(MAX_RUNS);
            self.write_pending = true;
            self.transient_run = None;
        } else {
            self.transient_run = Some(run);
        }
        self.dc_requested = false;
        self.dc_failure = None;
        self.touch();
    }

    pub fn request_dc(&mut self, tool: Option<String>, persist: bool) {
        self.dc_requested = true;
        self.dc_failure = None;
        self.dc_tool = tool;
        self.dc_persist = persist;
    }

    pub fn fail_dc(&mut self, message: &str) {
        if self.dc_requested {
            self.dc_requested = false;
            self.dc_failure = Some(message.into());
            self.finish_tool(Err(message.into()));
            self.touch();
        }
    }

    pub fn pending_dc_tool(&self, call: &str) -> bool {
        self.dc_tool.as_deref() == Some(call)
            || self.completed_tools.iter().any(|(id, _)| id == call)
    }

    pub fn take_dc_tool(&mut self, call: &str) -> Option<Result<DcRun, String>> {
        let index = self.completed_tools.iter().position(|(id, _)| id == call)?;
        self.completed_tools.remove(index).map(|(_, result)| result)
    }

    fn finish_tool(&mut self, result: Result<DcRun, String>) {
        if let Some(id) = self.dc_tool.take() {
            if self.completed_tools.len() >= MAX_RUNS {
                self.completed_tools.pop_front();
            }
            self.completed_tools.push_back((id, result));
        }
    }

    pub fn pending(&self) -> bool {
        self.link.is_some()
            || self.discover.is_some()
            || self.ota_job.is_some()
            || self.load.is_some()
            || self.write.is_some()
            || (self.write_pending && self.store_readable)
            || self.replay_started.is_some()
    }

    pub fn poll(&mut self) -> bool {
        let before = self.version;
        if let Some(result) = poll_job(&mut self.load) {
            match result {
                Ok(mut saved) => {
                    self.saved.runs.append(&mut saved.runs);
                    self.saved.runs.truncate(MAX_RUNS);
                    self.saved.captures.append(&mut saved.captures);
                    self.saved.captures.truncate(MAX_RUNS);
                    for (name, probe) in saved.bindings {
                        self.saved.bindings.entry(name).or_insert(probe);
                    }
                    self.store_readable = true;
                }
                Err(error) => self.error = Some(error),
            }
            self.touch();
        }
        if let Some(result) = poll_job(&mut self.write) {
            if let Err(error) = result {
                self.error = Some(error);
                self.touch();
            }
        }
        if self.write_pending && self.store_readable && self.write.is_none() {
            if let Some(path) = self.store_path.clone() {
                self.write_pending = false;
                let saved = self.saved.clone();
                match run_job("raf-lab-save", move || {
                    let bytes = serde_json::to_vec(&saved).map_err(|error| error.to_string())?;
                    raf_core::project_store::write_atomic(&path, &bytes)
                }) {
                    Ok(job) => self.write = Some(job),
                    Err(error) => {
                        self.error = Some(error);
                        self.touch();
                    }
                }
            }
        }
        if let Some(result) = poll_job(&mut self.discover) {
            match result {
                Ok(ports) => {
                    self.ports = ports;
                    self.scanned = true;
                }
                Err(error) => self.error = Some(error),
            }
            self.touch();
        }
        if let Some(result) = poll_job(&mut self.ota_job) {
            match result {
                Ok(ota) => self.ota = Some(ota),
                Err(error) => self.error = Some(error),
            }
            self.touch();
        }
        if self
            .refresh
            .is_some_and(|last| last.elapsed() < REFRESH_INTERVAL)
        {
            return self.version != before;
        }
        self.refresh = Some(Instant::now());
        if let Some(link) = self.link.as_ref() {
            if let Some(snapshot) =
                link.snapshot_after(self.serial.as_ref().map_or(0, |view| view.version))
            {
                let failed = matches!(snapshot.state, LinkState::Failed(_));
                if self
                    .selected_channel
                    .as_ref()
                    .is_none_or(|key| !snapshot.latest.contains_key(key))
                {
                    self.selected_channel = snapshot.latest.keys().next().cloned();
                }
                if self.view == LabView::Device {
                    self.recent = self
                        .selected_channel
                        .as_ref()
                        .and_then(|key| link.recent(key, 256))
                        .unwrap_or_default();
                }
                self.serial = Some(snapshot);
                if failed && self.recording.is_none() {
                    self.link = None;
                }
                self.touch();
            }
        }
        if let Some((started, offset)) = self.replay_started {
            let end = self
                .capture()
                .and_then(|capture| capture.points.last())
                .map_or(0, |point| point.ms);
            self.replay_ms = (offset + started.elapsed().as_millis() as u64).min(end);
            if self.replay_ms >= end {
                self.replay_started = None;
            }
            self.touch();
        }
        self.version != before
    }

    pub fn leave(&mut self) {
        if self.recording.is_some() {
            if let Err(error) = self.stop_recording() {
                self.error = Some(error);
            }
        }
        self.disconnect();
        self.replay_started = None;
        self.touch();
    }

    fn disconnect(&mut self) {
        self.link = None;
        self.serial = None;
        self.port = None;
        self.ota = None;
        self.ota_export = None;
        self.ota_job = None;
        self.recent.clear();
        self.recording = None;
        self.output_values.clear();
    }

    fn stop_recording(&mut self) -> Result<(), String> {
        let start = self.recording.ok_or("No capture is recording.")?;
        let link = self.link.as_ref().ok_or("Serial link closed.")?;
        let (end, mut history) = link.history().ok_or("Serial link busy; retry.")?;
        let count = end.saturating_sub(start) as usize;
        let lost_prefix = count > history.len();
        if count < history.len() {
            history.drain(..history.len() - count);
        }
        self.recording = None;
        if history.is_empty() {
            return Err("The recording contains no samples.".into());
        }
        let first = history[0].ms;
        for point in &mut history {
            point.ms -= first;
        }
        let capture = Capture {
            id: Uuid::new_v4(),
            unix_ms: chrono::Utc::now().timestamp_millis(),
            port: self.port.clone().unwrap_or_default(),
            device: self
                .serial
                .as_ref()
                .and_then(|view| view.device.as_ref())
                .map(|device| device.board.clone()),
            lost_prefix,
            points: history,
        };
        self.selected_capture = Some(capture.id);
        self.saved.captures.insert(0, capture);
        self.saved.captures.truncate(MAX_RUNS);
        self.write_pending = true;
        Ok(())
    }

    pub fn action(&mut self, command: &str) {
        self.error = self.apply(command).err();
        self.touch();
    }

    fn apply(&mut self, command: &str) -> Result<(), String> {
        let (verb, argument) = command.split_once(':').unwrap_or((command, ""));
        match verb {
            "simulation" => self.view = LabView::Simulation,
            "model-details" => self.show_model_details = !self.show_model_details,
            "device" => self.view = LabView::Device,
            "captures" => self.view = LabView::Captures,
            "chat" => self.view = LabView::Chat,
            "scan" if self.discover.is_none() => {
                self.discover = Some(run_job("raf-port-discovery", serial_session::enumerate)?)
            }
            "scan" => {}
            "connect" if self.link.is_none() => {
                let port = self
                    .ports
                    .get(index(argument)?)
                    .ok_or("Port no longer available.")?;
                self.link = Some(SerialSession::open(port.name.clone(), self.baud_rate())?);
                self.port = Some(port.name.clone());
                self.serial = None;
                self.refresh = None;
                self.link.as_ref().unwrap().identify()?;
            }
            "disconnect" => {
                if self.recording.is_some() {
                    self.stop_recording()?;
                }
                self.disconnect();
            }
            "identify" => self
                .link
                .as_ref()
                .ok_or("Serial link closed.")?
                .identify()?,
            "baud" if self.link.is_none() => {
                self.baud = argument
                    .parse::<u32>()
                    .ok()
                    .filter(|baud| (1200..=2_000_000).contains(baud))
                    .ok_or("Invalid baud rate.")?;
            }
            "record" if self.recording.is_none() => {
                let snapshot = self
                    .link
                    .as_ref()
                    .and_then(|link| link.snapshot_after(0))
                    .ok_or("Serial link unavailable.")?;
                if snapshot.state != LinkState::Open {
                    return Err("Serial link unavailable.".into());
                }
                self.recording = Some(snapshot.received);
            }
            "stop" => self.stop_recording()?,
            "channel" => {
                self.selected_channel = self
                    .serial
                    .as_ref()
                    .and_then(|view| view.latest.keys().nth(index(argument).ok()?).cloned())
                    .or_else(|| {
                        self.capture().and_then(|capture| {
                            capture
                                .points
                                .get(index(argument).ok()?)
                                .map(|point| point.channel.clone())
                        })
                    });
                self.recent = self
                    .link
                    .as_ref()
                    .zip(self.selected_channel.as_deref())
                    .and_then(|(link, key)| link.recent(key, 256))
                    .unwrap_or_default();
            }
            "capture-channel" => {
                self.selected_channel = self
                    .capture()
                    .and_then(|capture| capture.points.get(index(argument).ok()?))
                    .map(|point| point.channel.clone());
            }
            "part" => {
                self.selected = Uuid::parse_str(argument).ok();
                self.selected_pin = None;
            }
            "pin" => {
                self.selected_pin = if argument.is_empty() {
                    None
                } else {
                    Some(index(argument)?)
                }
            }
            "run" => self.selected_run = Uuid::parse_str(argument).ok(),
            "capture" => {
                self.selected_capture = Uuid::parse_str(argument).ok();
                self.replay_ms = 0;
                self.replay_started = None;
            }
            "play" => {
                if let Some(end) = self
                    .capture()
                    .and_then(|capture| capture.points.last())
                    .map(|point| point.ms)
                {
                    if self.replay_ms >= end {
                        self.replay_ms = 0;
                    }
                    self.replay_started = Some((Instant::now(), self.replay_ms));
                }
            }
            "pause" => self.replay_started = None,
            "seek" => {
                let end = self
                    .capture()
                    .and_then(|capture| capture.points.last())
                    .map_or(0, |point| point.ms);
                let percent = argument
                    .parse::<u64>()
                    .map_err(|_| "Invalid playback position.")?
                    .min(100);
                self.replay_ms = end.saturating_mul(percent) / 100;
                self.replay_started = None;
            }
            "bind" => {
                let component = self.selected.ok_or("Select a component first.")?;
                let channel = self
                    .selected_channel
                    .as_ref()
                    .ok_or("Select a channel first.")?;
                if self.saved.bindings.len() >= 64 && !self.saved.bindings.contains_key(channel) {
                    return Err("The session has reached its probe limit.".into());
                }
                self.saved.bindings.insert(
                    channel.clone(),
                    Probe {
                        component,
                        pin: self.selected_pin,
                    },
                );
                self.write_pending = true;
            }
            "unbind" => {
                let channel = self
                    .selected_channel
                    .as_ref()
                    .ok_or("Select a channel first.")?;
                self.saved.bindings.remove(channel);
                self.write_pending = true;
            }
            "output-value" => {
                let (output, value) = argument.split_once(':').ok_or("Invalid output value.")?;
                let output = index(output)?;
                if output < 16 && value.len() <= 24 {
                    self.output_values.insert(output, value.into());
                }
            }
            "output" => {
                let output_index = index(argument)?;
                let output = self
                    .serial
                    .as_ref()
                    .and_then(|view| view.device.as_ref())
                    .and_then(|device| device.outputs.get(output_index))
                    .ok_or("Output unavailable.")?;
                let value = self
                    .output_values
                    .get(&output_index)
                    .and_then(|value| value.trim().parse::<f64>().ok())
                    .ok_or("Enter a numeric output value.")?;
                self.link
                    .as_mut()
                    .ok_or("Serial link closed.")?
                    .set_output(&output.id, value)?;
            }
            _ => return Err("Action unavailable in the current laboratory state.".into()),
        }
        Ok(())
    }

    pub fn preparing_ota(&self) -> bool {
        self.ota_job.is_some()
    }
    pub fn scanning(&self) -> bool {
        self.discover.is_some()
    }
    pub fn connected(&self) -> bool {
        self.link.is_some()
    }
    pub fn baud_rate(&self) -> u32 {
        if self.baud == 0 {
            115_200
        } else {
            self.baud
        }
    }

    pub fn prepare_ota(&mut self, path: PathBuf) -> Result<(), String> {
        if self.ota_job.is_some() {
            return Err("Firmware verification is already running.".into());
        }
        let device = self
            .serial
            .as_ref()
            .filter(|view| view.state == LinkState::Open)
            .and_then(|view| view.device.clone())
            .ok_or("Identify a connected device first.")?;
        let capability = device
            .ota
            .ok_or("The device does not advertise OTA preparation.")?;
        if capability.target.is_empty()
            || capability.target.len() > 96
            || capability.max_image_bytes == 0
        {
            return Err("The OTA capability descriptor is incomplete.".into());
        }
        self.ota = None;
        self.ota_export = None;
        self.error = None;
        self.ota_job = Some(run_job("raf-lab-firmware", move || {
            let filename = path
                .file_name()
                .and_then(|value| value.to_str())
                .ok_or("Invalid firmware filename.")?
                .to_owned();
            if !filename.to_ascii_lowercase().ends_with(".bin") {
                return Err("Select a compiled .bin firmware image.".into());
            }
            let file = std::fs::File::open(path).map_err(|error| error.to_string())?;
            let limit = capability.max_image_bytes.min(MAX_FIRMWARE_BYTES);
            let mut reader = file.take(limit + 1);
            let mut hash = ring::digest::Context::new(&ring::digest::SHA256);
            let mut buffer = [0; 16 * 1024];
            let mut bytes = 0;
            loop {
                let read = reader
                    .read(&mut buffer)
                    .map_err(|error| error.to_string())?;
                if read == 0 {
                    break;
                }
                bytes += read as u64;
                hash.update(&buffer[..read]);
            }
            if bytes == 0 || bytes > limit {
                return Err(format!(
                    "Firmware size must be between 1 and {limit} bytes."
                ));
            }
            let sha256 = hash
                .finish()
                .as_ref()
                .iter()
                .map(|byte| format!("{byte:02x}"))
                .collect::<String>();
            Ok(OtaPreparation {
                board: device.board,
                installed_firmware: device.firmware,
                target: capability.target,
                filename,
                bytes,
                sha256,
            })
        })?);
        self.touch();
        Ok(())
    }

    pub fn export_ota(&mut self, folder: PathBuf) {
        if let Some(preparation) = &self.ota {
            let path = folder.join(format!("raf-ota-{}.json", Uuid::new_v4()));
            let result = serde_json::to_vec_pretty(preparation)
                .map_err(|error| error.to_string())
                .and_then(|bytes| raf_core::project_store::write_atomic(&path, &bytes));
            match result {
                Ok(()) => {
                    self.ota_export = Some(path);
                    self.error = None;
                }
                Err(error) => self.error = Some(error),
            }
            self.touch();
        }
    }
}

fn index(value: &str) -> Result<usize, String> {
    value.parse().map_err(|_| "Invalid item index.".into())
}
