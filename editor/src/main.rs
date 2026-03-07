//! AuraRafi Editor - Main entry point.
//!
//! This binary launches the full AuraRafi editor application.

fn main() -> eframe::Result<()> {
    // Initialize logging.
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::INFO)
        .with_target(false)
        .init();

    tracing::info!("AuraRafi Editor starting...");

    // Load custom icon from embedded PNG.
    let icon = load_icon();

    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("AuraRafi Engine")
            .with_inner_size([1280.0, 720.0])
            .with_min_inner_size([800.0, 500.0])
            .with_icon(icon),
        ..Default::default()
    };

    eframe::run_native(
        "AuraRafi",
        options,
        Box::new(|cc| Ok(Box::new(raf_editor::AuraRafiApp::new(cc)))),
    )
}

/// Load the application icon from the embedded PNG file.
fn load_icon() -> egui::IconData {
    let icon_bytes = include_bytes!("../icon.png");
    match image::load_from_memory(icon_bytes) {
        Ok(img) => {
            let rgba = img.to_rgba8();
            let (w, h) = rgba.dimensions();
            egui::IconData {
                rgba: rgba.into_raw(),
                width: w,
                height: h,
            }
        }
        Err(_) => {
            // Fallback: 32x32 solid orange icon.
            let size = 32u32;
            let rgba = vec![212u8, 119, 26, 255].repeat((size * size) as usize);
            egui::IconData {
                rgba,
                width: size,
                height: size,
            }
        }
    }
}

use eframe::egui;
use image;
