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
    if let Err(error) = raf_editor::native_application::run_native() {
        tracing::error!(%error, "native editor stopped with an error");
        std::process::exit(1);
    }
}
