use raf_player::headless::{self, HeadlessOptions};
use raf_player::stream_host::{self, StreamHostOptions};
use raf_player::{run, PlayerLaunch};
use raf_runtime::manifest::RuntimeManifest;
use std::net::SocketAddr;
use std::path::PathBuf;
use std::time::Duration;
use uuid::Uuid;

fn main() {
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::INFO)
        .with_target(false)
        .init();

    if let Err(error) = launch() {
        tracing::error!(%error, "player stopped with an error");
        std::process::exit(1);
    }
}

fn launch() -> Result<(), String> {
    let arguments = std::env::args_os()
        .skip(1)
        .map(|argument| {
            argument
                .into_string()
                .map_err(|_| "player arguments must be valid Unicode".to_string())
        })
        .collect::<Result<Vec<_>, _>>()?;
    if arguments.iter().any(|argument| argument == "--headless") {
        return launch_headless(&arguments);
    }
    if arguments.iter().any(|argument| argument == "--stream") {
        if arguments.iter().any(|argument| argument == "--headless") {
            return Err(
                "--stream and --headless cannot be combined: --stream serves frames to a client, \
                 --headless captures frames to disk"
                    .into(),
            );
        }
        return launch_stream(&arguments);
    }
    let mut arguments = arguments.into_iter();
    let Some(option) = arguments.next() else {
        return Err(usage());
    };
    match option.as_str() {
        "--runtime" => {
            let payload = arguments.next().ok_or_else(usage)?;
            if payload.len() > 8192 || arguments.next().is_some() {
                return Err(usage());
            }
            let launch = serde_json::from_str::<PlayerLaunch>(&payload)
                .map_err(|error| format!("invalid runtime launch payload: {error}"))?;
            run(launch)
        }
        "--manifest" => {
            let manifest_path = PathBuf::from(arguments.next().ok_or_else(usage)?);
            if arguments.next().is_some() {
                return Err(usage());
            }
            let manifest = RuntimeManifest::read(&manifest_path)?;
            run(PlayerLaunch {
                manifest: manifest_path,
                settings: manifest.host_settings,
                project_name: manifest.project_name,
                instance: manifest.instance,
                control_address: None,
                control_token: Uuid::new_v4(),
            })
        }
        "--help" | "-h" => {
            println!("{}", usage());
            Ok(())
        }
        _ => Err(usage()),
    }
}

fn launch_headless(arguments: &[String]) -> Result<(), String> {
    if arguments.iter().any(|argument| argument == "--runtime") {
        return Err(
            "--runtime and --headless cannot be combined: --runtime opens the windowed host, \
             --headless captures frames without a window"
                .into(),
        );
    }
    let defaults = HeadlessOptions::default();
    let mut manifest_path: Option<PathBuf> = None;
    let mut frames = defaults.frames;
    let mut size = defaults.size;
    let mut output_dir = defaults.output_dir;
    let mut input_script: Option<PathBuf> = None;
    let mut report_path: Option<PathBuf> = None;
    let mut fixed_hz = defaults.fixed_hz;
    let mut index = 0;
    while index < arguments.len() {
        let option = arguments[index].as_str();
        if option == "--headless" {
            index += 1;
            continue;
        }
        if option == "--help" || option == "-h" {
            println!("{}", usage());
            return Ok(());
        }
        let value = arguments
            .get(index + 1)
            .ok_or_else(|| format!("{option} requires a value\n{}", usage()))?;
        match option {
            "--manifest" => {
                if manifest_path.is_some() {
                    return Err(usage());
                }
                manifest_path = Some(PathBuf::from(value.as_str()));
            }
            "--frames" => frames = parse_u32(value, "--frames")?,
            "--size" => size = parse_size(value)?,
            "--out" => output_dir = PathBuf::from(value.as_str()),
            "--input" => input_script = Some(PathBuf::from(value.as_str())),
            "--report" => report_path = Some(PathBuf::from(value.as_str())),
            "--fixed-hz" => fixed_hz = parse_u32(value, "--fixed-hz")?,
            _ => return Err(format!("unknown option {option}\n{}", usage())),
        }
        index += 2;
    }
    let manifest_path =
        manifest_path.ok_or_else(|| format!("--headless requires --manifest\n{}", usage()))?;
    let report_path = report_path.unwrap_or_else(|| output_dir.join("report.json"));
    let manifest = RuntimeManifest::read(&manifest_path)?;
    let launch = PlayerLaunch {
        manifest: manifest_path,
        settings: manifest.host_settings,
        project_name: manifest.project_name,
        instance: manifest.instance,
        control_address: None,
        control_token: Uuid::new_v4(),
    };
    headless::run(
        launch,
        HeadlessOptions {
            size,
            frames,
            output_dir,
            input_script,
            report_path,
            fixed_hz,
        },
    )
}

fn launch_stream(arguments: &[String]) -> Result<(), String> {
    let mut manifest_path: Option<PathBuf> = None;
    let mut bind: Option<SocketAddr> = None;
    let mut token: Option<Uuid> = None;
    let mut size = [1280, 720];
    let mut max_fps = 30_u16;
    let mut max_frames = 0_u64;
    let mut idle_timeout = 30_u64;
    let mut fixed_hz = 60_u32;
    let mut report_path: Option<PathBuf> = None;
    let mut index = 0;
    while index < arguments.len() {
        let option = arguments[index].as_str();
        if option == "--help" || option == "-h" {
            println!("{}", usage());
            return Ok(());
        }
        let value = arguments
            .get(index + 1)
            .ok_or_else(|| format!("{option} requires a value\n{}", usage()))?;
        match option {
            "--stream" => {
                bind = Some(
                    value
                        .parse()
                        .map_err(|_| format!("--stream expects ADDR:PORT, received {value}"))?,
                );
            }
            "--manifest" => {
                if manifest_path.is_some() {
                    return Err(usage());
                }
                manifest_path = Some(PathBuf::from(value.as_str()));
            }
            "--stream-token" => {
                token = Some(
                    Uuid::parse_str(value).map_err(|error| format!("--stream-token: {error}"))?,
                );
            }
            "--size" => size = parse_size(value)?,
            "--stream-fps" => max_fps = parse_u16(value, "--stream-fps")?,
            "--stream-max-frames" => max_frames = parse_u64(value, "--stream-max-frames")?,
            "--idle-timeout" => idle_timeout = parse_u64(value, "--idle-timeout")?,
            "--fixed-hz" => fixed_hz = parse_u32(value, "--fixed-hz")?,
            "--report" => report_path = Some(PathBuf::from(value.as_str())),
            _ => return Err(format!("unknown option {option}\n{}", usage())),
        }
        index += 2;
    }
    let bind = bind.ok_or_else(|| format!("--stream requires ADDR:PORT\n{}", usage()))?;
    let manifest_path =
        manifest_path.ok_or_else(|| format!("--stream requires --manifest\n{}", usage()))?;
    let token = token.unwrap_or_else(|| {
        let token = Uuid::new_v4();
        tracing::info!(%token, "generated stream token");
        token
    });
    let manifest = RuntimeManifest::read(&manifest_path)?;
    let launch = PlayerLaunch {
        manifest: manifest_path,
        settings: manifest.host_settings,
        project_name: manifest.project_name,
        instance: manifest.instance,
        control_address: None,
        control_token: Uuid::new_v4(),
    };
    stream_host::run(
        launch,
        StreamHostOptions {
            bind,
            token,
            size,
            max_fps,
            max_frames,
            idle_timeout: Duration::from_secs(idle_timeout.max(1)),
            fixed_hz,
            report_path,
        },
    )
}

fn parse_u32(value: &str, option: &str) -> Result<u32, String> {
    value
        .parse::<u32>()
        .map_err(|_| format!("{option} expects a non-negative integer, received {value}"))
}

fn parse_u16(value: &str, option: &str) -> Result<u16, String> {
    value
        .parse::<u16>()
        .map_err(|_| format!("{option} expects an integer in 0..=65535, received {value}"))
}

fn parse_u64(value: &str, option: &str) -> Result<u64, String> {
    value
        .parse::<u64>()
        .map_err(|_| format!("{option} expects a non-negative integer, received {value}"))
}

fn parse_size(value: &str) -> Result<[u32; 2], String> {
    let (width, height) = value
        .split_once('x')
        .or_else(|| value.split_once('X'))
        .ok_or_else(|| format!("--size expects WxH, received {value}"))?;
    Ok([parse_u32(width, "--size")?, parse_u32(height, "--size")?])
}

fn usage() -> String {
    "Usage:\n  aurarafi-player --manifest <runtime.ron>\n  aurarafi-player --runtime <launch-json>\n  aurarafi-player --headless --manifest <runtime.ron> [--frames N] [--size WxH] [--out DIR] [--input FILE] [--report FILE] [--fixed-hz N]\n  aurarafi-player --stream ADDR:PORT --manifest <runtime.ron> [--stream-token UUID] [--size WxH] [--stream-fps N] [--stream-max-frames N] [--idle-timeout SECONDS] [--fixed-hz N] [--report FILE]".into()
}
