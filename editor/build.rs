//! Build script for the AuraRafi editor executable.
//!
//! Embeds the Rafi application icon into the Windows resource table so the
//! taskbar, Explorer, and the window before creation show the real
//! application icon instead of the default placeholder icon. This is what
//! makes the icon appear for plain `cargo run` and release builds alike.

fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=icon.ico");

    let is_windows = std::env::var("CARGO_CFG_WINDOWS").is_ok();
    if !is_windows {
        return;
    }

    let manifest_dir = std::env::var("CARGO_MANIFEST_DIR").expect("cargo manifest directory");
    let icon_path = std::path::Path::new(&manifest_dir).join("icon.ico");
    if !icon_path.exists() {
        println!("cargo:warning=editor/icon.ico is missing; skipping icon embedding");
        return;
    }

    let mut resource = winresource::WindowsResource::new();
    resource.set_icon(
        icon_path
            .to_str()
            .expect("editor icon path is valid UTF-8"),
    );
    if let Err(error) = resource.compile() {
        // Embedding the icon is cosmetic; a missing resource compiler must
        // never fail the build, so degrade to a warning instead.
        println!("cargo:warning=could not embed the Rafi icon: {error}");
    }
}
