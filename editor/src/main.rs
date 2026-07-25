//! AuraRafi Editor - Main entry point.
//!
//! This binary launches the full AuraRafi editor application.

fn main() -> eframe::Result<()> {
    if let Some(command_line) = external_command_line() {
        run_external_command(&command_line);
        return Ok(());
    }

    // Initialize logging.
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::INFO)
        .with_target(false)
        .init();

    tracing::info!("Proyecto Rafi Editor starting...");

    // Load custom icon from embedded PNG.
    let icon = load_icon();

    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("Proyecto Rafi")
            .with_inner_size([660.0, 540.0])
            .with_min_inner_size([660.0, 540.0])
            .with_resizable(false)
            .with_decorations(false)
            .with_transparent(true)
            .with_icon(icon),
        ..Default::default()
    };

    eframe::run_native(
        "Proyecto Rafi",
        options,
        Box::new(|cc| Ok(Box::new(raf_editor::AuraRafiApp::new(cc)))),
    )
}

fn external_command_line() -> Option<String> {
    let mut args = std::env::args().skip(1);
    match args.next().as_deref() {
        Some("--rafui-studio-preview") => {
            let options = args.collect::<Vec<_>>().join(" ");
            if options.is_empty() {
                Some("/rafui.studio.preview".to_string())
            } else {
                Some(format!("/rafui.studio.preview {options}"))
            }
        }
        Some("--rafui-command") => args.next(),
        _ => None,
    }
}

fn run_external_command(command_line: &str) {
    let parsed = match raf_editor::commands::parse_console_input(command_line) {
        Ok(raf_editor::commands::ParsedInput::Command(command)) => command,
        Ok(raf_editor::commands::ParsedInput::Message(_)) => {
            println!("RafUI Studio: expected a slash command.");
            return;
        }
        Err(error) => {
            println!("RafUI Studio: {error}");
            return;
        }
    };
    let output = if matches!(
        parsed.name.as_str(),
        "rafui.studio.preview" | "rafui.preview"
    ) {
        raf_editor::commands::ui_document::standalone_studio_preview(&parsed)
    } else {
        raf_editor::commands::CommandOutput::error(
            "RafUI Studio",
            "External mode currently exposes the read-only rafui.studio.preview command.",
        )
    };
    println!("{}", output.title);
    for line in output.lines {
        println!("{line}");
    }
    println!("json: {}", output.json);
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
