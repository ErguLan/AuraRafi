//! Bounded USB serial acquisition for the Electronics workbench.
use std::collections::{BTreeMap, VecDeque};
use std::io::{ErrorKind, Write};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{mpsc, Arc, Mutex, TryLockError};
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};

const MAX_LINE: usize = 4096;
const HISTORY: usize = 4096;
const MAX_CHANNELS: usize = 32;
const MAX_OUTPUTS: usize = 16;
const IO_TIMEOUT: Duration = Duration::from_millis(50);
const ACK_TIMEOUT: Duration = Duration::from_secs(2);

#[derive(Debug, Clone)]
pub struct Port {
    pub name: String,
    pub description: String,
}

pub fn enumerate() -> Result<Vec<Port>, String> {
    serialport::available_ports()
        .map_err(|error| error.to_string())
        .map(|ports| {
            ports
                .into_iter()
                .take(64)
                .map(|port| {
                    let description = match port.port_type {
                        serialport::SerialPortType::UsbPort(info) => {
                            let product = info.product.unwrap_or_default();
                            let vendor = info.manufacturer.unwrap_or_default();
                            format!("{vendor} {product} · {:04x}:{:04x}", info.vid, info.pid)
                        }
                        _ => String::new(),
                    };
                    Port {
                        name: port.port_name,
                        description,
                    }
                })
                .collect()
        })
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Reading {
    pub ms: u64,
    pub channel: String,
    pub value: f64,
    pub unit: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Output {
    pub id: String,
    pub min: f64,
    pub max: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OtaCapability {
    pub target: String,
    pub max_image_bytes: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Device {
    pub board: String,
    pub firmware: String,
    #[serde(default)]
    pub outputs: Vec<Output>,
    #[serde(default)]
    pub ota: Option<OtaCapability>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LinkState {
    Opening,
    Open,
    Failed(String),
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub enum Receipt {
    Pending(u64),
    Accepted(u64),
    Rejected(u64),
    TimedOut(u64),
}

#[derive(Debug, Clone)]
pub struct Snapshot {
    pub version: u64,
    pub state: LinkState,
    pub device: Option<Device>,
    pub latest: BTreeMap<String, Reading>,
    pub received: u64,
    pub invalid: u64,
    pub evicted: u64,
    pub receipt: Option<Receipt>,
    pub diagnostics: VecDeque<String>,
}

struct State {
    snapshot: Snapshot,
    history: VecDeque<Reading>,
    output_pending: Option<(u64, Instant)>,
}

enum Command {
    Identify,
    Set {
        id: String,
        value: f64,
        receipt: u64,
    },
}

#[derive(Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum Frame {
    Hello {
        protocol: u8,
        #[serde(flatten)]
        device: Device,
    },
    Sample {
        channel: String,
        value: f64,
        unit: String,
    },
    Ack {
        id: u64,
        ok: bool,
    },
}

pub struct SerialSession {
    shared: Arc<Mutex<State>>,
    commands: mpsc::SyncSender<Command>,
    next_receipt: u64,
    stopped: Arc<AtomicBool>,
}

impl SerialSession {
    pub fn open(port: String, baud: u32) -> Result<Self, String> {
        if port.is_empty() || baud == 0 {
            return Err("A port and positive baud rate are required.".into());
        }
        let shared = Arc::new(Mutex::new(State {
            snapshot: Snapshot {
                version: 1,
                state: LinkState::Opening,
                device: None,
                latest: BTreeMap::new(),
                received: 0,
                invalid: 0,
                evicted: 0,
                receipt: None,
                diagnostics: VecDeque::new(),
            },
            history: VecDeque::new(),
            output_pending: None,
        }));
        let (commands, input) = mpsc::sync_channel(4);
        let state = Arc::clone(&shared);
        let stopped = Arc::new(AtomicBool::new(false));
        let stop = Arc::clone(&stopped);
        std::thread::Builder::new()
            .name("raf-serial".into())
            .spawn(move || {
                let result = run(&port, baud, &state, input, &stop);
                if let Err(error) = result {
                    let mut state = state.lock().unwrap_or_else(|error| error.into_inner());
                    state.snapshot.state = LinkState::Failed(error);
                    state.snapshot.version += 1;
                }
            })
            .map_err(|error| error.to_string())?;
        Ok(Self {
            shared,
            commands,
            next_receipt: 1,
            stopped,
        })
    }

    pub fn snapshot_after(&self, version: u64) -> Option<Snapshot> {
        let state = match self.shared.try_lock() {
            Ok(state) => state,
            Err(TryLockError::Poisoned(error)) => error.into_inner(),
            Err(TryLockError::WouldBlock) => return None,
        };
        (state.snapshot.version != version).then(|| state.snapshot.clone())
    }

    pub fn history(&self) -> Option<(u64, Vec<Reading>)> {
        let state = self.shared.try_lock().ok()?;
        Some((
            state.snapshot.received,
            state.history.iter().cloned().collect(),
        ))
    }

    pub fn recent(&self, channel: &str, limit: usize) -> Option<Vec<Reading>> {
        let state = self.shared.try_lock().ok()?;
        let mut points: Vec<_> = state
            .history
            .iter()
            .rev()
            .filter(|point| point.channel == channel)
            .take(limit.min(HISTORY))
            .cloned()
            .collect();
        points.reverse();
        Some(points)
    }

    pub fn identify(&self) -> Result<(), String> {
        self.commands
            .try_send(Command::Identify)
            .map_err(|error| error.to_string())
    }

    pub fn set_output(&mut self, id: &str, value: f64) -> Result<u64, String> {
        let mut state = self
            .shared
            .try_lock()
            .map_err(|_| "Serial link busy; retry.")?;
        let output = state
            .snapshot
            .device
            .as_ref()
            .and_then(|device| device.outputs.iter().find(|output| output.id == id))
            .ok_or("The connected firmware has not declared this output.")?;
        if state.snapshot.state != LinkState::Open
            || !value.is_finite()
            || value < output.min
            || value > output.max
        {
            return Err("Output value is out of range or the port is disconnected.".into());
        }
        if state.output_pending.is_some() {
            return Err("Wait for the previous output receipt.".into());
        }
        let receipt = self.next_receipt;
        self.commands
            .try_send(Command::Set {
                id: id.into(),
                value,
                receipt,
            })
            .map_err(|error| error.to_string())?;
        self.next_receipt += 1;
        state.output_pending = Some((receipt, Instant::now()));
        state.snapshot.receipt = Some(Receipt::Pending(receipt));
        state.snapshot.version += 1;
        Ok(receipt)
    }
}

impl Drop for SerialSession {
    fn drop(&mut self) {
        self.stopped.store(true, Ordering::Release);
    }
}

fn run(
    port: &str,
    baud: u32,
    shared: &Mutex<State>,
    commands: mpsc::Receiver<Command>,
    stopped: &AtomicBool,
) -> Result<(), String> {
    let mut port = serialport::new(port, baud)
        .timeout(IO_TIMEOUT)
        .open()
        .map_err(|error| error.to_string())?;
    {
        let mut state = shared.lock().unwrap_or_else(|error| error.into_inner());
        state.snapshot.state = LinkState::Open;
        state.snapshot.version += 1;
    }
    let started = Instant::now();
    let mut bytes = [0; 1024];
    let mut frame = Vec::with_capacity(MAX_LINE);
    let mut oversized = false;
    let mut identify_retry = None;
    while !stopped.load(Ordering::Acquire) {
        match commands.try_recv() {
            Ok(Command::Identify) => {
                send_identify(&mut *port)?;
                identify_retry = Some(Instant::now() + Duration::from_millis(1200));
            }
            Ok(Command::Set { id, value, receipt }) => {
                if stopped.load(Ordering::Acquire) {
                    return Ok(());
                }
                let mut payload = serde_json::to_vec(
                    &serde_json::json!({"type":"set","output":id,"value":value,"id":receipt}),
                )
                .map_err(|error| error.to_string())?;
                payload.push(b'\n');
                port.write_all(&payload)
                    .map_err(|error| error.to_string())?;
            }
            Err(mpsc::TryRecvError::Disconnected) => return Ok(()),
            Err(mpsc::TryRecvError::Empty) => {}
        }
        if identify_retry.is_some_and(|deadline| deadline <= Instant::now()) {
            identify_retry = None;
            if shared
                .lock()
                .unwrap_or_else(|error| error.into_inner())
                .snapshot
                .device
                .is_none()
            {
                send_identify(&mut *port)?;
            }
        }
        let read_started = Instant::now();
        let count = match port.read(&mut bytes) {
            Ok(count) => count,
            Err(error)
                if matches!(
                    error.kind(),
                    ErrorKind::TimedOut | ErrorKind::Interrupted | ErrorKind::WouldBlock
                ) =>
            {
                0
            }
            Err(error) => return Err(error.to_string()),
        };
        if count == 0 {
            expire_receipt(&mut shared.lock().unwrap_or_else(|error| error.into_inner()));
            if read_started.elapsed() < Duration::from_millis(2) {
                std::thread::sleep(Duration::from_millis(8));
            }
            continue;
        }
        let mut state = shared.lock().unwrap_or_else(|error| error.into_inner());
        expire_receipt(&mut state);
        for &byte in &bytes[..count] {
            if byte == b'\n' {
                if oversized {
                    state.snapshot.invalid += 1;
                    state.snapshot.version += 1;
                } else if !frame.is_empty() {
                    accept(&mut state, &frame, started.elapsed().as_millis() as u64);
                }
                oversized = false;
                frame.clear();
            } else if frame.len() == MAX_LINE {
                oversized = true;
            } else if !oversized {
                frame.push(byte);
            }
        }
    }
    Ok(())
}

fn send_identify(port: &mut dyn serialport::SerialPort) -> Result<(), String> {
    port.write_all(b"{\"type\":\"identify\",\"protocol\":1}\n")
        .map_err(|error| error.to_string())
}

fn expire_receipt(state: &mut State) {
    if let Some((id, sent)) = state.output_pending {
        if sent.elapsed() >= ACK_TIMEOUT {
            state.output_pending = None;
            state.snapshot.receipt = Some(Receipt::TimedOut(id));
            state.snapshot.version += 1;
        }
    }
}

fn accept(state: &mut State, bytes: &[u8], ms: u64) {
    let view = &mut state.snapshot;
    view.version += 1;
    match serde_json::from_slice::<Frame>(bytes) {
        Ok(Frame::Hello {
            protocol: 1,
            device,
        }) if !device.board.is_empty()
            && device.board.len() <= 96
            && device.firmware.len() <= 96
            && device.outputs.len() <= MAX_OUTPUTS
            && device.outputs.iter().all(|output| {
                !output.id.is_empty()
                    && output.id.len() <= 64
                    && output.min.is_finite()
                    && output.max.is_finite()
                    && output.min <= output.max
            }) =>
        {
            view.device = Some(device);
        }
        Ok(Frame::Sample {
            channel,
            value,
            unit,
        }) if value.is_finite()
            && !channel.is_empty()
            && channel.len() <= 64
            && unit.len() <= 16
            && view
                .latest
                .get(&channel)
                .is_none_or(|last| last.unit == unit)
            && (view.latest.contains_key(&channel) || view.latest.len() < MAX_CHANNELS) =>
        {
            let point = Reading {
                ms,
                channel: channel.clone(),
                value,
                unit,
            };
            view.latest.insert(channel, point.clone());
            if state.history.len() == HISTORY {
                state.history.pop_front();
                view.evicted += 1;
            }
            state.history.push_back(point);
            view.received += 1;
        }
        Ok(Frame::Ack { id, ok })
            if state
                .output_pending
                .is_some_and(|(pending, _)| pending == id) =>
        {
            state.output_pending = None;
            view.receipt = Some(if ok {
                Receipt::Accepted(id)
            } else {
                Receipt::Rejected(id)
            });
        }
        _ => {
            view.invalid += 1;
            if view.diagnostics.len() == 12 {
                view.diagnostics.pop_front();
            }
            let text = String::from_utf8_lossy(bytes)
                .chars()
                .take(160)
                .map(|character| {
                    if character.is_control() {
                        ' '
                    } else {
                        character
                    }
                })
                .collect();
            view.diagnostics.push_back(text);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn serial_protocol_keeps_bounded_history_and_correlates_receipts() {
        let mut state = State {
            snapshot: Snapshot {
                version: 0,
                state: LinkState::Open,
                device: None,
                latest: BTreeMap::new(),
                received: 0,
                invalid: 0,
                evicted: 0,
                receipt: Some(Receipt::Pending(7)),
                diagnostics: VecDeque::new(),
            },
            history: VecDeque::new(),
            output_pending: Some((7, Instant::now())),
        };
        accept(
            &mut state,
            br#"{"type":"hello","protocol":1,"board":"Arduino","firmware":"lab"}"#,
            0,
        );
        for index in 0..HISTORY + 2 {
            accept(
                &mut state,
                br#"{"type":"sample","channel":"a","value":3.2,"unit":"V"}"#,
                index as u64,
            );
        }
        accept(
            &mut state,
            br#"{"type":"sample","channel":"a","value":9,"unit":"A"}"#,
            5000,
        );
        accept(&mut state, br#"{"type":"ack","id":6,"ok":true}"#, 5001);
        assert_eq!(state.snapshot.receipt, Some(Receipt::Pending(7)));
        accept(&mut state, br#"{"type":"ack","id":7,"ok":true}"#, 5002);
        assert_eq!(state.snapshot.receipt, Some(Receipt::Accepted(7)));
        assert_eq!(state.snapshot.device.unwrap().board, "Arduino");
        assert_eq!(state.history.len(), HISTORY);
        assert_eq!(state.snapshot.evicted, 2);
        assert_eq!(state.snapshot.invalid, 2);
        assert_eq!(state.snapshot.latest["a"].value, 3.2);
    }
}
