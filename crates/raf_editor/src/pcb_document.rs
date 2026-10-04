//! PCB document persistence.
//!
//! Shares the RON primitives owned by `schematic_document` so both documents
//! fail the same way: an unreadable file is backed up, reported and never
//! silently replaced by an empty document.

use std::path::Path;

use raf_electronics::PcbLayout;

use crate::schematic_document::{
    load_ron_document, refuse_overwrite_of_unreadable_document, write_ron_document, DocumentLoad,
};

pub fn load_pcb_document(path: &Path) -> DocumentLoad<PcbLayout> {
    load_ron_document(path, |raw| {
        ron::from_str::<PcbLayout>(raw).map_err(|error| error.to_string())
    })
}

pub fn save_pcb_document(
    path: &Path,
    layout: &PcbLayout,
) -> Result<(), Box<dyn std::error::Error>> {
    refuse_overwrite_of_unreadable_document::<PcbLayout, _>(path, |raw| {
        ron::from_str::<PcbLayout>(raw).map_err(|error| error.to_string())
    })?;
    write_ron_document(path, layout, |value| {
        ron::ser::to_string_pretty(value, ron::ser::PrettyConfig::default()).map_err(Into::into)
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp_dir(tag: &str) -> PathBuf {
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|elapsed| elapsed.as_nanos())
            .unwrap_or_default();
        let dir = std::env::temp_dir().join(format!("raf-pcb-{tag}-{stamp}"));
        std::fs::create_dir_all(&dir).expect("temp directory");
        dir
    }

    #[test]
    fn unreadable_pcb_document_is_reported_as_corrupt() {
        let dir = temp_dir("corrupt");
        let path = dir.join("board.ron");
        std::fs::write(&path, "(unterminated").expect("write");

        match load_pcb_document(&path) {
            DocumentLoad::Corrupt { backup, .. } => {
                assert!(backup.is_some_and(|backup| backup.exists()));
            }
            _ => panic!("expected Corrupt"),
        }
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn pcb_round_trip_keeps_the_document() {
        let dir = temp_dir("roundtrip");
        let path = dir.join("board.ron");
        let layout = PcbLayout::new("Board");
        save_pcb_document(&path, &layout).expect("save");

        match load_pcb_document(&path) {
            DocumentLoad::Loaded(loaded) => assert_eq!(loaded.name, "Board"),
            _ => panic!("expected Loaded"),
        }
        let _ = std::fs::remove_dir_all(&dir);
    }
}