//! End-to-end smoke test for the headless streaming host.
//!
//! Ignored by default because it needs a working graphics adapter. Run with:
//!   cargo test -p raf_player --test stream_smoke -- --ignored --nocapture
//!
//! It starts the real headless host on a loopback port, completes the token
//! handshake, sends one input frame and waits for a rendered RGBA video frame.

use raf_core::InputSnapshot;
use raf_net::protocol::bye_reason;
use raf_net::{epoch_millis, InputFrame, StreamConfig, StreamMessage, StreamPeer, VideoFrame};
use raf_player::stream_host::{self, StreamHostOptions};
use raf_player::PlayerLaunch;
use raf_runtime::manifest::RuntimeManifest;
use std::net::{SocketAddr, TcpListener};
use std::path::PathBuf;
use std::time::{Duration, Instant};
use uuid::Uuid;

fn free_port() -> SocketAddr {
    let listener = TcpListener::bind(("127.0.0.1", 0)).expect("free port");
    let address = listener.local_addr().expect("local address");
    drop(listener);
    address
}

#[test]
#[ignore = "requires a graphics adapter and the headless fixture"]
fn stream_host_serves_a_real_frame_after_input() {
    let manifest_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../examples/headless/project/runtime.ron");
    assert!(
        manifest_path.is_file(),
        "run the write_headless_fixture example first"
    );
    let manifest = RuntimeManifest::read(&manifest_path).expect("fixture manifest");
    let launch = PlayerLaunch {
        manifest: manifest_path,
        settings: manifest.host_settings.clone(),
        project_name: manifest.project_name.clone(),
        instance: manifest.instance,
        control_address: None,
        control_token: Uuid::new_v4(),
    };

    let address = free_port();
    let token = Uuid::new_v4();
    let instance = manifest.instance;
    let options = StreamHostOptions {
        bind: address,
        token,
        size: [320, 180],
        max_fps: 15,
        max_frames: 30,
        idle_timeout: Duration::from_secs(10),
        fixed_hz: 60,
        report_path: None,
    };

    let host = std::thread::spawn(move || stream_host::run(launch, options));

    let config = StreamConfig::new(token, instance);
    let deadline = Instant::now() + Duration::from_secs(20);
    let mut peer = loop {
        match StreamPeer::connect(address, &config, "smoke-client") {
            Ok(peer) => break peer,
            Err(error) if Instant::now() < deadline => {
                let _ = error;
                std::thread::sleep(Duration::from_millis(25));
            }
            Err(error) => panic!("stream connect: {error}"),
        }
    };

    let welcome_deadline = Instant::now() + Duration::from_secs(20);
    while Instant::now() < welcome_deadline && !peer.is_open() {
        let messages = peer.poll().expect("handshake poll");
        for message in messages {
            if let StreamMessage::Bye { reason } = message {
                panic!("host rejected the handshake: {reason}");
            }
        }
        if !peer.is_open() {
            std::thread::sleep(Duration::from_millis(10));
        }
    }
    assert!(peer.is_open(), "handshake did not complete");

    let mut keys = raf_core::InputKeySet::empty();
    keys.insert(raf_core::InputKey::W);
    let snapshot = InputSnapshot {
        keys_down: keys,
        window_focused: true,
        ..Default::default()
    };
    peer.send(StreamMessage::Input(InputFrame::from_snapshot(
        1,
        epoch_millis(),
        &snapshot,
    )))
    .expect("send input");

    let frame_deadline = Instant::now() + Duration::from_secs(20);
    let mut frame: Option<VideoFrame> = None;
    while Instant::now() < frame_deadline && frame.is_none() {
        for message in peer.poll().expect("stream poll") {
            if let StreamMessage::Video(video) = message {
                frame = Some(video);
            }
        }
        if frame.is_none() {
            std::thread::sleep(Duration::from_millis(10));
        }
    }

    let frame = frame.expect("host did not produce a video frame");
    assert_eq!(frame.width, 320);
    assert_eq!(frame.height, 180);
    assert_eq!(
        frame.pixels.len(),
        320 * 180 * 4,
        "RGBA frame length must match its dimensions"
    );
    assert!(
        frame.pixels.chunks_exact(4).any(|pixel| pixel[3] != 0),
        "frame must not be fully transparent"
    );

    peer.close(bye_reason::NORMAL).expect("close stream");
    let result = host.join().expect("stream host thread");
    result.expect("stream host");
}
