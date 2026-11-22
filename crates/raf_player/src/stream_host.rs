//! Headless streaming host.
//!
//! One process owns one headless graphics host, one `PlayerSession` and one
//! authenticated stream server. The host renders only while a client is
//! connected; with no peer it sleeps instead of burning cycles. Input frames
//! are authoritative state, so the server derives press/release edges and a
//! lost packet cannot leave a key stuck. Each composed AGB frame is read back
//! through `NativeUiWindowHost::capture`.

use crate::{PlayerLaunch, PlayerSession, PlayerSource};
use raf_core::InputSnapshot;
use raf_net::input::InputTracker;
use raf_net::protocol::bye_reason;
use raf_net::{epoch_millis, StreamConfig, StreamMessage, StreamPeer, StreamServer, VideoFrame};
use raf_render::api_graphic_basic::ui_surface::{
    NativeUiInputBridge, NativeUiWindowHost,
};
use raf_render::api_graphic_basic::NativeEditorCompositor;
use raf_runtime::manifest::RuntimeManifest;
use serde::Serialize;
use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};
use uuid::Uuid;

const REPORT_PROTOCOL: &str = "aurarafi.stream.report/1";
const CLEAR_COLOR: [u8; 4] = [8, 11, 15, 255];
const MIN_SIZE: [u32; 2] = [64, 64];
const MAX_SIZE: [u32; 2] = [3840, 2160];
const MIN_FPS: u16 = 1;
const MAX_FPS: u16 = 240;
const MIN_FIXED_HZ: u32 = 15;
const MAX_FIXED_HZ: u32 = 120;
const IDLE_SLEEP: Duration = Duration::from_millis(5);
const HANDSHAKE_SLEEP: Duration = Duration::from_millis(2);
const MAX_DIAGNOSTICS: usize = 256;
const MAX_DIAGNOSTIC_CHARS: usize = 512;

/// Bounded options for one streaming session host.
#[derive(Clone, Debug)]
pub struct StreamHostOptions {
    pub bind: SocketAddr,
    pub token: Uuid,
    pub size: [u32; 2],
    pub max_fps: u16,
    /// Zero means until the peer disconnects or the process is stopped.
    pub max_frames: u64,
    pub idle_timeout: Duration,
    pub fixed_hz: u32,
    pub report_path: Option<PathBuf>,
}

impl StreamHostOptions {
    pub fn validate(&self) -> Result<(), String> {
        if self.token.is_nil() {
            return Err("stream token must not be nil".into());
        }
        if self.size[0] < MIN_SIZE[0]
            || self.size[1] < MIN_SIZE[1]
            || self.size[0] > MAX_SIZE[0]
            || self.size[1] > MAX_SIZE[1]
        {
            return Err(format!(
                "stream size {}x{} is outside {}x{}..={}x{}",
                self.size[0], self.size[1], MIN_SIZE[0], MIN_SIZE[1], MAX_SIZE[0], MAX_SIZE[1]
            ));
        }
        if self.max_fps < MIN_FPS || self.max_fps > MAX_FPS {
            return Err(format!(
                "stream fps {} is outside {MIN_FPS}..={MAX_FPS}",
                self.max_fps
            ));
        }
        if self.fixed_hz < MIN_FIXED_HZ || self.fixed_hz > MAX_FIXED_HZ {
            return Err(format!(
                "fixed_hz {} is outside {MIN_FIXED_HZ}..={MAX_FIXED_HZ}",
                self.fixed_hz
            ));
        }
        if self.idle_timeout.is_zero() {
            return Err("stream idle timeout must be greater than zero".into());
        }
        Ok(())
    }
}

#[derive(Serialize)]
struct StreamReport {
    protocol: &'static str,
    backend: &'static str,
    size: [u32; 2],
    max_fps: u16,
    fixed_hz: u32,
    frames_rendered: u64,
    fixed_ticks: u64,
    avg_tick_ms: f64,
    avg_render_ms: f64,
    avg_capture_ms: f64,
    frames_sent: u64,
    frames_dropped: u64,
    bytes_sent: u64,
    input_p50_ms: u32,
    input_p95_ms: u32,
    close_reason: Option<u8>,
    diagnostics: Vec<String>,
}

/// Serve one authenticated stream session from a headless runtime.
pub fn run(launch: PlayerLaunch, options: StreamHostOptions) -> Result<(), String> {
    options.validate()?;

    let mut manifest = RuntimeManifest::read(&launch.manifest)?;
    if manifest.instance != launch.instance {
        return Err("runtime snapshot instance mismatch".into());
    }
    manifest.project_settings.runtime.fixed_hz = options.fixed_hz;

    let settings = launch.settings.clone();
    let config = crate::graphics::window_config(settings.render_execution_policy, options.size);

    let mut host = NativeUiWindowHost::create_headless(options.size, config)?;
    let mut compositor = NativeEditorCompositor::new(&host.graphics_context(), CLEAR_COLOR);
    let mut session = PlayerSession::new(
        &host,
        PlayerSource::Snapshot(Box::new(manifest)),
        settings,
        launch.instance,
    );
    session.finish_preparation(&host)?;

    let stream_config = StreamConfig {
        token: options.token,
        instance: launch.instance,
        max_width: options.size[0] as u16,
        max_height: options.size[1] as u16,
        max_fps: options.max_fps,
        queue_frames: 8,
        idle_timeout: options.idle_timeout,
    };
    let server = StreamServer::bind(options.bind, stream_config)
        .map_err(|error| format!("stream bind {}: {error}", options.bind))?;
    tracing::info!(address = %options.bind, "stream host listening");

    let fixed_dt = 1.0 / f64::from(options.fixed_hz);
    let frame_interval = Duration::from_secs_f64(1.0 / f64::from(options.max_fps));
    let mut bridge = NativeUiInputBridge::default();
    bridge.set_scale_factor(f64::from(host.scale_factor()));
    let mut tracker = InputTracker::new();
    let mut snapshot = InputSnapshot::default();
    let mut peer: Option<StreamPeer> = None;
    let mut next_frame = Instant::now();
    let mut diagnostics = Vec::new();
    let mut frames_rendered = 0u64;
    let mut tick_ms = 0.0_f64;
    let mut render_ms = 0.0_f64;
    let mut capture_ms = 0.0_f64;
    let mut video_seq = 0u64;
    let mut close_reason = None;

    loop {
        if peer.is_none() {
            match server.accept() {
                Ok(Some(connected)) => {
                    tracker.reset();
                    snapshot = InputSnapshot::default();
                    next_frame = Instant::now();
                    tracing::info!(
                        peer = ?connected.peer_addr().ok(),
                        "stream peer accepted"
                    );
                    peer = Some(connected);
                }
                Ok(None) => {
                    std::thread::sleep(IDLE_SLEEP);
                    continue;
                }
                Err(error) => return Err(format!("stream accept: {error}")),
            }
        }

        let Some(active) = peer.as_mut() else {
            continue;
        };
        if !active.is_open() {
            if let Err(error) = active.poll() {
                push_diagnostic(&mut diagnostics, format!("stream handshake: {error}"));
                close_reason = active.close_reason();
                break;
            }
            if active.is_closed() {
                close_reason = active.close_reason();
                break;
            }
            if !active.is_open() {
                std::thread::sleep(HANDSHAKE_SLEEP);
                continue;
            }
        }

        match active.poll() {
            Ok(messages) => {
                for message in messages {
                    match message {
                        StreamMessage::Input(frame) => match tracker.accept(frame) {
                            Ok(Some(next)) => snapshot = next,
                            Ok(None) => {}
                            Err(error) => {
                                push_diagnostic(&mut diagnostics, format!("stream input: {error}"));
                            }
                        },
                        StreamMessage::Bye { .. } => {
                            close_reason = active.remote_bye();
                        }
                        _ => {}
                    }
                }
            }
            Err(error) => {
                push_diagnostic(&mut diagnostics, format!("stream receive: {error}"));
                close_reason = active.close_reason();
                break;
            }
        }
        if active.is_closed() || close_reason.is_some() {
            close_reason = close_reason.or(active.close_reason());
            break;
        }

        let now = Instant::now();
        if now < next_frame {
            std::thread::sleep(next_frame - now);
        }
        next_frame = Instant::now() + frame_interval;

        bridge.snapshot_mut().clone_from(&snapshot);
        let step_started = Instant::now();
        session.step_with_snapshot(&mut host, &mut compositor, &bridge, &snapshot, fixed_dt)?;
        let step_ms = step_started.elapsed().as_secs_f64() * 1000.0;
        let frame_render_ms = session.last_render_millis().clamp(0.0, step_ms);
        render_ms += frame_render_ms;
        tick_ms += (step_ms - frame_render_ms).max(0.0);

        let capture_started = Instant::now();
        let rgba = host.capture()?;
        let [width, height] = host.size();
        capture_ms += capture_started.elapsed().as_secs_f64() * 1000.0;

        let video = VideoFrame::from_rgba(
            video_seq,
            epoch_millis(),
            width as u16,
            height as u16,
            &rgba,
        )
        .map_err(|error| format!("stream video frame: {error}"))?;
        if let Err(error) = active.send(StreamMessage::Video(video)) {
            push_diagnostic(&mut diagnostics, format!("stream send: {error}"));
            close_reason = active.close_reason();
            break;
        }
        video_seq = video_seq.wrapping_add(1);
        frames_rendered = frames_rendered.saturating_add(1);

        for message in session.drain_diagnostics() {
            push_diagnostic(&mut diagnostics, message);
        }
        if options.max_frames > 0 && frames_rendered >= options.max_frames {
            break;
        }
    }

    let metrics = peer.as_ref().map(StreamPeer::metrics).unwrap_or_default();
    if let Some(mut active) = peer.take() {
        let _ = active.close(bye_reason::NORMAL);
        let _ = active.poll();
    }

    let divisor = frames_rendered.max(1) as f64;
    let report = StreamReport {
        protocol: REPORT_PROTOCOL,
        backend: session.backend_name(),
        size: options.size,
        max_fps: options.max_fps,
        fixed_hz: options.fixed_hz,
        frames_rendered,
        fixed_ticks: session.status().fixed_ticks,
        avg_tick_ms: tick_ms / divisor,
        avg_render_ms: render_ms / divisor,
        avg_capture_ms: capture_ms / divisor,
        frames_sent: metrics.frames_sent,
        frames_dropped: metrics.frames_dropped,
        bytes_sent: metrics.bytes_sent,
        input_p50_ms: metrics.input_latency.p50(),
        input_p95_ms: metrics.input_latency.p95(),
        close_reason: close_reason,
        diagnostics,
    };
    if let Some(path) = options.report_path.as_deref() {
        write_report(path, &report)?;
    }
    tracing::info!(
        frames = report.frames_rendered,
        sent = report.frames_sent,
        dropped = report.frames_dropped,
        "stream host stopped"
    );
    Ok(())
}

fn write_report(path: &Path, report: &StreamReport) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent)
                .map_err(|error| format!("stream report directory: {error}"))?;
        }
    }
    let bytes =
        serde_json::to_vec_pretty(report).map_err(|error| format!("stream report: {error}"))?;
    std::fs::write(path, bytes)
        .map_err(|error| format!("stream report {}: {error}", path.display()))
}

fn push_diagnostic(diagnostics: &mut Vec<String>, message: String) {
    if diagnostics.len() >= MAX_DIAGNOSTICS {
        return;
    }
    let mut message = message;
    if message.len() > MAX_DIAGNOSTIC_CHARS {
        message.truncate(MAX_DIAGNOSTIC_CHARS);
    }
    diagnostics.push(message);
}
