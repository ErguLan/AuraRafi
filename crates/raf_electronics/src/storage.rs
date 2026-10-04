//! Filesystem helpers for Electronics document persistence.
//!
//! Schematic and PCB documents are user-authored data, so writes must never
//! leave a half-written file behind. Everything in this module goes through
//! [`write_atomic`].

use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

/// Writes `contents` to `path` atomically: a sibling temporary file is written,
/// flushed and then renamed over the destination so a crash mid-write can never
/// leave a truncated document behind.
///
/// The parent directory is created when missing. On Windows `fs::rename` refuses
/// to replace an existing file, so the destination is removed only after the
/// direct rename failed, and the rename is retried once.
pub fn write_atomic(path: &Path, contents: &str) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() {
            fs::create_dir_all(parent)?;
        }
    }

    let temporary = temporary_path(path);
    let write_result = (|| -> std::io::Result<()> {
        let mut file = fs::File::create(&temporary)?;
        file.write_all(contents.as_bytes())?;
        file.flush()?;
        file.sync_all()?;
        Ok(())
    })();

    if let Err(error) = write_result {
        let _ = fs::remove_file(&temporary);
        return Err(error);
    }

    match fs::rename(&temporary, path) {
        Ok(()) => Ok(()),
        Err(rename_error) => {
            // Windows cannot rename over an existing destination. Only now that
            // the direct rename is known to fail is the previous file replaced.
            let retry = fs::remove_file(path).and_then(|()| fs::rename(&temporary, path));
            match retry {
                Ok(()) => Ok(()),
                Err(_) => {
                    let _ = fs::remove_file(&temporary);
                    Err(rename_error)
                }
            }
        }
    }
}

/// Sibling path used while writing, kept next to the destination so the final
/// rename never crosses a filesystem boundary.
fn temporary_path(path: &Path) -> PathBuf {
    let mut name = path
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| "document".to_string());
    name.push_str(".tmp");
    path.with_file_name(name)
}

#[cfg(test)]
mod tests {
    use super::*;
    use uuid::Uuid;

    fn scratch_dir() -> PathBuf {
        std::env::temp_dir().join(format!("raf_electronics_storage_{}", Uuid::new_v4()))
    }

    fn leftover_temp_files(root: &Path) -> Vec<PathBuf> {
        fs::read_dir(root)
            .expect("scratch directory readable")
            .filter_map(|entry| entry.ok())
            .map(|entry| entry.path())
            .filter(|path| path.extension().map(|ext| ext == "tmp").unwrap_or(false))
            .collect()
    }

    #[test]
    fn write_atomic_creates_parents_replaces_content_and_leaves_no_temp() {
        let root = scratch_dir();
        let document = root.join("nested").join("board.ron");

        write_atomic(&document, "first").expect("first write");
        assert_eq!(fs::read_to_string(&document).expect("first read"), "first");

        // Second write exercises the "destination already exists" path.
        write_atomic(&document, "second").expect("second write");
        assert_eq!(
            fs::read_to_string(&document).expect("second read"),
            "second"
        );

        assert!(
            leftover_temp_files(document.parent().expect("document parent")).is_empty(),
            "the temporary file must not survive a successful write"
        );

        fs::remove_dir_all(&root).expect("scratch cleanup");
    }
}
