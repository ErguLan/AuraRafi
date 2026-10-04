//! AuraRafi Editor native entry point.
//!
//! The executable owns only logging and the Winit event loop. RafUI owns
//! retained editor surfaces and ApiGraphicBasic owns scene/UI composition.

fn main() {
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::INFO)
        .with_target(false)
        .init();

    tracing::info!("Proyecto Rafi Editor starting with native RafUI host");
    if std::env::args().nth(1).as_deref() == Some("--runtime") {
        let result = std::env::args()
            .nth(2)
            .ok_or_else(|| "runtime launch payload is missing".to_string())
            .and_then(|json| {
                if json.len() > 8192 {
                    return Err("runtime launch payload is too large".into());
                }
                serde_json::from_str::<raf_player::PlayerLaunch>(&json).map_err(|e| e.to_string())
            })
            .and_then(raf_player::run);
        if let Err(error) = result {
            tracing::error!(%error, "local runtime stopped with an error");
            std::process::exit(1);
        }
        return;
    }
    if let Err(error) = raf_editor::native_application::run_native() {
        tracing::error!(%error, "native editor stopped with an error");
        std::process::exit(1);
    }
}
