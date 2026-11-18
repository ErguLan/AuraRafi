use raf_player::{run, PlayerLaunch};
use raf_runtime::manifest::RuntimeManifest;
use std::path::PathBuf;
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
    let mut arguments = std::env::args_os().skip(1);
    let Some(option) = arguments.next() else {
        return Err(usage());
    };
    match option.to_string_lossy().as_ref() {
        "--runtime" => {
            let payload = arguments
                .next()
                .ok_or_else(usage)?
                .into_string()
                .map_err(|_| "runtime launch payload is not valid Unicode".to_string())?;
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

fn usage() -> String {
    "Usage:\n  aurarafi-player --manifest <runtime.ron>\n  aurarafi-player --runtime <launch-json>".into()
}
