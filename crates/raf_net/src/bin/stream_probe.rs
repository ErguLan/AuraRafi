//! Manual loopback probe for the streaming transport.
//!
//! Binds a local server, connects one client, performs the authenticated
//! handshake, sends four input frames and two tiny video frames, then prints
//! the decoded results and latency. Every wait is bounded; the probe never
//! runs forever.

use raf_core::InputKey;
use raf_net::{
    bye_reason, epoch_millis, input_key_code, InputFrame, InputTracker, StreamConfig,
    StreamMessage, StreamPeer, StreamServer, VideoFrame, BUTTON_PRIMARY, MODIFIER_SHIFT,
};
use std::error::Error;
use std::time::{Duration, Instant};
use uuid::Uuid;

const HANDSHAKE_WINDOW: Duration = Duration::from_secs(3);
const PUMP_WINDOW: Duration = Duration::from_secs(3);
const LOOP_PAUSE: Duration = Duration::from_millis(1);

fn main() {
    if let Err(error) = run() {
        eprintln!("raf-stream-probe failed: {error}");
        std::process::exit(1);
    }
}

fn run() -> Result<(), Box<dyn Error>> {
    let config = StreamConfig {
        token: Uuid::new_v4(),
        instance: Uuid::new_v4(),
        max_width: 320,
        max_height: 180,
        max_fps: 60,
        queue_frames: 16,
        ..StreamConfig::default()
    };
    let server = StreamServer::bind(("127.0.0.1", 0), config.clone())?;
    let address = server.local_addr()?;
    println!("probe: listening on {address}");

    let mut client = StreamPeer::connect(address, &config, "raf-stream-probe")?;
    let mut server_peer: Option<StreamPeer> = None;
    let mut client_name = String::new();
    let mut session = Uuid::nil();
    let handshake_deadline = Instant::now() + HANDSHAKE_WINDOW;

    loop {
        if server_peer.is_none() {
            server_peer = server.accept()?;
        }
        if let Some(peer) = server_peer.as_mut() {
            for message in peer.poll()? {
                if let StreamMessage::Hello {
                    client_name: name, ..
                } = message
                {
                    client_name = name;
                }
            }
        }
        for message in client.poll()? {
            if let StreamMessage::Welcome {
                session: id,
                width,
                height,
                max_fps,
                ..
            } = message
            {
                session = id;
                println!("probe: handshake session={id} size={width}x{height} max_fps={max_fps}");
            }
        }
        let server_open = server_peer
            .as_ref()
            .map(|peer| peer.is_open())
            .unwrap_or(false);
        if client.is_open() && server_open {
            break;
        }
        if Instant::now() >= handshake_deadline {
            return Err("handshake did not complete within 3 seconds".into());
        }
        std::thread::sleep(LOOP_PAUSE);
    }

    let mut server_peer = match server_peer {
        Some(peer) => peer,
        None => return Err("server never accepted the probe client".into()),
    };
    println!("probe: handshake ready client_name={client_name:?} session={session}");

    let key_sets: [&[InputKey]; 4] = [
        &[InputKey::W, InputKey::Space],
        &[InputKey::W],
        &[InputKey::W, InputKey::Space],
        &[InputKey::Space],
    ];
    for (index, keys) in key_sets.iter().enumerate() {
        let seq = index as u32 + 1;
        let mut frame = InputFrame::new(seq, epoch_millis());
        frame.held_keys = keys.iter().map(|key| input_key_code(*key)).collect();
        frame.pointer = [seq as f32 * 10.0, 20.0];
        frame.pointer_delta = [1.5, -0.5];
        frame.buttons = if seq % 2 == 0 { BUTTON_PRIMARY } else { 0 };
        frame.modifiers = if keys.contains(&InputKey::Space) {
            MODIFIER_SHIFT
        } else {
            0
        };
        client.send(StreamMessage::Input(frame))?;
        std::thread::sleep(LOOP_PAUSE);
    }

    for index in 0..2u64 {
        let width = 8u16;
        let height = 8u16;
        let mut rgba = Vec::with_capacity(8 * 8 * 4);
        for y in 0..8u32 {
            for x in 0..8u32 {
                rgba.extend_from_slice(&[
                    (x * 32) as u8,
                    (y * 32) as u8,
                    (index as u32 * 120) as u8,
                    255,
                ]);
            }
        }
        let frame = VideoFrame::from_rgba(100 + index, epoch_millis(), width, height, &rgba)?;
        server_peer.send(StreamMessage::Video(frame))?;
    }
    client.ping()?;

    let mut tracker = InputTracker::new();
    let mut received_inputs = 0usize;
    let mut received_videos = 0usize;
    let pump_deadline = Instant::now() + PUMP_WINDOW;
    while received_inputs < 4 || received_videos < 2 {
        for message in server_peer.poll()? {
            if let StreamMessage::Input(frame) = message {
                received_inputs += 1;
                let seq = frame.seq;
                let key_count = frame.held_keys.len();
                match tracker.accept(frame) {
                    Ok(Some(snapshot)) => println!(
                        "probe input seq={seq} keys={key_count} pressed_w={} pressed_space={} released_space={}",
                        snapshot.key_pressed(InputKey::W),
                        snapshot.key_pressed(InputKey::Space),
                        snapshot.key_released(InputKey::Space)
                    ),
                    Ok(None) => println!("probe input seq={seq} dropped by tracker"),
                    Err(error) => println!("probe input seq={seq} invalid: {error}"),
                }
            }
        }
        for message in client.poll()? {
            if let StreamMessage::Video(frame) = message {
                received_videos += 1;
                let checksum: u64 = frame.as_rgba().iter().map(|byte| *byte as u64).sum();
                println!(
                    "probe video seq={} size={}x{} bytes={} checksum={checksum}",
                    frame.seq,
                    frame.width,
                    frame.height,
                    frame.as_rgba().len()
                );
            }
        }
        if Instant::now() >= pump_deadline {
            return Err(format!(
                "probe timed out: inputs={received_inputs} videos={received_videos}"
            )
            .into());
        }
        std::thread::sleep(LOOP_PAUSE);
    }

    let server_metrics = server_peer.metrics();
    let client_metrics = client.metrics();
    println!(
        "probe server: frames_sent={} frames_dropped={} bytes_sent={} frames_received={} bytes_received={}",
        server_metrics.frames_sent,
        server_metrics.frames_dropped,
        server_metrics.bytes_sent,
        server_metrics.frames_received,
        server_metrics.bytes_received
    );
    println!(
        "probe input latency: samples={} p50={}ms p95={}ms p99={}ms max={}ms",
        server_metrics.input_latency.samples(),
        server_metrics.input_latency.p50(),
        server_metrics.input_latency.p95(),
        server_metrics.input_latency.p99(),
        server_metrics.input_latency.max()
    );
    println!(
        "probe video send latency: samples={} p50={}ms p95={}ms p99={}ms max={}ms",
        server_metrics.video_send_latency.samples(),
        server_metrics.video_send_latency.p50(),
        server_metrics.video_send_latency.p95(),
        server_metrics.video_send_latency.p99(),
        server_metrics.video_send_latency.max()
    );
    println!(
        "probe client: frames_received={} bytes_received={} rtt={}ms",
        client_metrics.frames_received,
        client_metrics.bytes_received,
        client_metrics.rtt_millis_estimate
    );

    let _ = client.close(bye_reason::NORMAL);
    let _ = client.poll();
    let _ = server_peer.poll();
    let _ = server_peer.close(bye_reason::NORMAL);
    let _ = server_peer.poll();
    println!("probe complete");
    Ok(())
}
