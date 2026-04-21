use std::path::Path;

use raf_electronics::schematic::Schematic;

pub fn load_schematic_document(path: &Path) -> Option<Schematic> {
    let text = std::fs::read_to_string(path).ok()?;
    ron::from_str::<Schematic>(&text).ok()
}

pub fn save_schematic_document(
    path: &Path,
    schematic: &Schematic,
) -> Result<(), Box<dyn std::error::Error>> {
    let serialized = ron::ser::to_string_pretty(schematic, ron::ser::PrettyConfig::default())?;
    std::fs::write(path, serialized)?;
    Ok(())
}