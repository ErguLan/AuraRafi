use std::path::Path;

use raf_electronics::PcbLayout;

pub fn load_pcb_document(path: &Path) -> Option<PcbLayout> {
    let text = std::fs::read_to_string(path).ok()?;
    ron::from_str::<PcbLayout>(&text).ok()
}

pub fn save_pcb_document(
    path: &Path,
    layout: &PcbLayout,
) -> Result<(), Box<dyn std::error::Error>> {
    let serialized = ron::ser::to_string_pretty(layout, ron::ser::PrettyConfig::default())?;
    std::fs::write(path, serialized)?;
    Ok(())
}