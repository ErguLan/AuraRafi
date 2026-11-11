//! Project-confined UI persistence shared by native authoring and command tools.
use raf_ui::UiDocument;
use std::path::{Path, PathBuf};
pub const MAX_DOCUMENT_BYTES: u64 = 4 * 1024 * 1024;

pub fn document_path(root: &Path, relative: &Path) -> Result<PathBuf, String> {
    raf_core::project_store::confined_path(root, relative)
}
pub fn load(path: &Path) -> Result<UiDocument, String> {
    use std::io::Read;
    let mut bytes = Vec::new();
    std::fs::File::open(path)
        .map_err(|e| e.to_string())?
        .take(MAX_DOCUMENT_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| e.to_string())?;
    if bytes.len() as u64 > MAX_DOCUMENT_BYTES {
        return Err("ui.document_budget".into());
    }
    let mut document: UiDocument = ron::de::from_bytes(&bytes).map_err(|e| e.to_string())?;
    document.migrate().map_err(|e| e.join("; "))?;
    Ok(document)
}
pub fn save(path: &Path, document: &UiDocument) -> Result<(), String> {
    document.validate().map_err(|e| e.join("; "))?;
    let bytes = ron::ser::to_string(document).map_err(|e| e.to_string())?;
    if bytes.len() as u64 > MAX_DOCUMENT_BYTES {
        return Err("ui.document_budget".into());
    }
    raf_core::project_store::write_atomic(path, bytes.as_bytes())
}
pub fn revision(document: &UiDocument) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut hash = std::collections::hash_map::DefaultHasher::new();
    if let Ok(text) = ron::ser::to_string(document) {
        text.hash(&mut hash);
    }
    hash.finish()
}
