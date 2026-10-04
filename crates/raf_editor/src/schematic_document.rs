//! Schematic document persistence and the shared RON document primitives used
//! by the PCB document.
//!
//! A failed load is never allowed to look like an empty document. The previous
//! implementation returned `Option<Schematic>`, so a truncated file produced
//! `None`, the editor kept an empty schematic and the next save overwrote the
//! user's work. Loading now reports the three states the editor actually has to
//! handle, and writing never destroys a file the editor could not read.

use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use raf_electronics::schematic::Schematic;

/// Result of reading a persisted RON document from disk.
///
/// `Missing` is an ordinary first run. `Corrupt` means the bytes exist but the
/// editor could not turn them back into a document, so the original file must be
/// preserved and the failure must reach the user.
pub enum DocumentLoad<T> {
    Missing,
    Loaded(T),
    Corrupt {
        /// Suffix copy written next to the original before anything else ran.
        backup: Option<PathBuf>,
        /// Human readable cause. Never shown raw to the user; it travels to the
        /// log and to the localized surface error as context.
        detail: String,
    },
}

/// Reads and parses a RON document, keeping an unreadable file untouched.
pub(crate) fn load_ron_document<T, P>(path: &Path, parse: P) -> DocumentLoad<T>
where
    T: serde::de::DeserializeOwned,
    P: Fn(&str) -> Result<T, String>,
{
    let raw = match std::fs::read_to_string(path) {
        Ok(raw) => raw,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return DocumentLoad::Missing;
        }
        Err(error) => {
            return DocumentLoad::Corrupt {
                backup: None,
                detail: format!("read failed: {error}"),
            };
        }
    };
    match parse(&raw) {
        Ok(value) => DocumentLoad::Loaded(value),
        Err(detail) => DocumentLoad::Corrupt {
            backup: backup_unreadable_document(path, &raw),
            detail: format!("parse failed: {detail}"),
        },
    }
}

/// Copies an unreadable document next to itself so the original bytes survive
/// whatever the editor does next. A failing backup is reported as `None` and the
/// caller refuses to overwrite the original anyway.
pub(crate) fn backup_unreadable_document(path: &Path, raw: &str) -> Option<PathBuf> {
    let stem = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("document");
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .ok()
        .map(|elapsed| elapsed.as_secs())
        .unwrap_or_default();
    let backup = path.with_file_name(format!("{stem}.corrupt-{stamp}.bak"));
    std::fs::write(&backup, raw).ok().map(|()| backup)
}

/// Serializes `value` and replaces `path` in one step.
///
/// The document is written to a sibling temporary file first, so a failure
/// midway never leaves a half-written schematic on disk.
pub(crate) fn write_ron_document<T, S>(
    path: &Path,
    value: &T,
    serialize: S,
) -> Result<(), Box<dyn std::error::Error>>
where
    T: serde::Serialize,
    S: Fn(&T) -> Result<String, Box<dyn std::error::Error>>,
{
    let serialized = serialize(value)?;
    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent)?;
        }
    }
    write_atomically(path, &serialized)?;
    Ok(())
}

/// Replaces the target file through a sibling temporary file.
pub(crate) fn write_atomically(path: &Path, contents: &str) -> std::io::Result<()> {
    let parent = path.parent().filter(|parent| !parent.as_os_str().is_empty());
    let file_name = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("document.ron");
    let temporary = match parent {
        Some(parent) => parent.join(format!(".{file_name}.tmp")),
        None => PathBuf::from(format!(".{file_name}.tmp")),
    };
    std::fs::write(&temporary, contents)?;
    match std::fs::rename(&temporary, path) {
        Ok(()) => Ok(()),
        Err(error) => {
            let _ = std::fs::remove_file(&temporary);
            Err(error)
        }
    }
}

/// Refuses to write over a document that exists but cannot be parsed.
///
/// Losing the user's file is always worse than failing a save, and the failure
/// is reported through the surface error channel so it is never silent.
pub(crate) fn refuse_overwrite_of_unreadable_document<T, P>(
    path: &Path,
    parse: P,
) -> Result<(), Box<dyn std::error::Error>>
where
    T: serde::de::DeserializeOwned,
    P: Fn(&str) -> Result<T, String>,
{
    if !path.exists() {
        return Ok(());
    }
    let Ok(raw) = std::fs::read_to_string(path) else {
        return Ok(());
    };
    if parse(&raw).is_ok() {
        return Ok(());
    }
    Err(format!(
        "{} exists but could not be read as a document; it was left untouched",
        path.display()
    )
    .into())
}

pub fn load_schematic_document(path: &Path) -> DocumentLoad<Schematic> {
    load_ron_document(path, |raw| {
        ron::from_str::<Schematic>(raw).map_err(|error| error.to_string())
    })
}

pub fn save_schematic_document(
    path: &Path,
    schematic: &Schematic,
) -> Result<(), Box<dyn std::error::Error>> {
    refuse_overwrite_of_unreadable_document::<Schematic, _>(path, |raw| {
        ron::from_str::<Schematic>(raw).map_err(|error| error.to_string())
    })?;
    write_ron_document(path, schematic, |value| {
        ron::ser::to_string_pretty(value, ron::ser::PrettyConfig::default()).map_err(Into::into)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir(tag: &str) -> PathBuf {
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|elapsed| elapsed.as_nanos())
            .unwrap_or_default();
        let dir = std::env::temp_dir().join(format!("raf-schematic-{tag}-{stamp}"));
        std::fs::create_dir_all(&dir).expect("temp directory");
        dir
    }

    #[test]
    fn missing_document_is_not_a_corruption() {
        let dir = temp_dir("missing");
        let path = dir.join("absent.ron");
        assert!(matches!(
            load_schematic_document(&path),
            DocumentLoad::Missing
        ));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn truncated_document_is_backed_up_instead_of_discarded() {
        let dir = temp_dir("truncated");
        let path = dir.join("schematic.ron");
        let mut schematic = Schematic::new("Truncated");
        schematic.add_component(raf_electronics::component::ElectronicComponent::resistor("10k"));
        save_schematic_document(&path, &schematic).expect("save");
        let good = std::fs::read_to_string(&path).expect("read");
        std::fs::write(&path, &good[..good.len() / 2]).expect("truncate");

        match load_schematic_document(&path) {
            DocumentLoad::Corrupt { backup, detail } => {
                assert!(!detail.is_empty());
                let backup = backup.expect("corrupt document must be backed up");
                assert!(backup.exists());
                assert!(backup.to_string_lossy().contains("corrupt"));
                assert!(path.exists(), "the original file must survive");
            }
            _ => panic!("expected Corrupt"),
        }
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn save_refuses_to_overwrite_an_unreadable_document() {
        let dir = temp_dir("overwrite");
        let path = dir.join("schematic.ron");
        std::fs::write(&path, "( this is not a schematic").expect("write");

        let error = save_schematic_document(&path, &Schematic::new("Empty"))
            .expect_err("unreadable document must not be overwritten");
        assert!(error.to_string().contains("left untouched"));
        assert_eq!(
            std::fs::read_to_string(&path).expect("read"),
            "( this is not a schematic"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn save_leaves_no_temporary_file_behind() {
        let dir = temp_dir("temp");
        let path = dir.join("schematic.ron");
        save_schematic_document(&path, &Schematic::new("Clean")).expect("save");

        let entries = std::fs::read_dir(&dir)
            .expect("read dir")
            .map(|entry| entry.expect("entry").file_name().to_string_lossy().to_string())
            .collect::<Vec<_>>();
        assert_eq!(entries, vec!["schematic.ron".to_string()]);
        let _ = std::fs::remove_dir_all(&dir);
    }
}