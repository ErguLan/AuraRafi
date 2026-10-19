//! Recoverable file publication and exclusive project authoring leases.

use std::fs::{self, File, OpenOptions};
use std::io::Write;
use std::path::{Component, Path, PathBuf};

/// A process-wide authoring lease. The OS releases the lock after a crash.
#[derive(Debug)]
pub struct ProjectWriteLease {
    _file: File,
    pub root: PathBuf,
}

impl ProjectWriteLease {
    pub fn acquire(root: &Path) -> Result<Self, String> {
        let root = root.canonicalize().map_err(|e| e.to_string())?;
        let directory = confined_path(&root, Path::new(".aura_rafi"))?;
        fs::create_dir_all(&directory).map_err(|e| e.to_string())?;
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(confined_path(
                &root,
                Path::new(".aura_rafi/authoring.lock"),
            )?)
            .map_err(|e| e.to_string())?;
        file.try_lock()
            .map_err(|_| "agent_workspace.project_busy".to_string())?;
        Ok(Self { _file: file, root })
    }
}

/// Resolve project-owned storage without following an escaping symlink/junction.
pub fn confined_path(root: &Path, relative: &Path) -> Result<PathBuf, String> {
    if relative.as_os_str().is_empty()
        || relative
            .components()
            .any(|part| !matches!(part, Component::Normal(_)))
    {
        return Err("agent_workspace.invalid_document_path".into());
    }
    let root = root.canonicalize().map_err(|error| error.to_string())?;
    let path = root.join(relative);
    for ancestor in path.ancestors().take_while(|ancestor| *ancestor != root) {
        if ancestor.exists()
            && !ancestor
                .canonicalize()
                .map_err(|error| error.to_string())?
                .starts_with(&root)
        {
            return Err("agent_workspace.invalid_document_path".into());
        }
    }
    if let Some(name) = path.file_name() {
        let backup = path.with_file_name(format!(".{}.previous", name.to_string_lossy()));
        if backup.exists()
            && !backup
                .canonicalize()
                .map_err(|error| error.to_string())?
                .starts_with(&root)
        {
            return Err("agent_workspace.invalid_document_path".into());
        }
    }
    Ok(path)
}

/// Publish a complete file with a recoverable previous generation on Windows.
pub fn write_atomic(path: &Path, bytes: &[u8]) -> Result<(), String> {
    let parent = path.parent().ok_or("File has no parent directory")?;
    fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    let name = path
        .file_name()
        .ok_or("File has no name")?
        .to_string_lossy();
    let temporary = parent.join(format!(".{name}.{}.tmp", uuid::Uuid::new_v4()));
    let backup = parent.join(format!(".{name}.previous"));
    let result = (|| {
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temporary)
            .map_err(|e| e.to_string())?;
        file.write_all(bytes)
            .and_then(|_| file.sync_all())
            .map_err(|e| e.to_string())?;
        drop(file);
        if path.exists() {
            if backup.exists() {
                fs::remove_file(&backup).map_err(|e| e.to_string())?;
            }
            fs::rename(path, &backup).map_err(|e| e.to_string())?;
        }
        if let Err(error) = fs::rename(&temporary, path) {
            if backup.exists() {
                let _ = fs::rename(&backup, path);
            }
            return Err(error.to_string());
        }
        Ok(())
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    result
}

pub fn read_recoverable(path: &Path) -> Result<Option<Vec<u8>>, String> {
    match fs::read(path) {
        Ok(bytes) => Ok(Some(bytes)),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            let name = path
                .file_name()
                .ok_or("File has no name")?
                .to_string_lossy();
            let backup = path.with_file_name(format!(".{name}.previous"));
            match fs::read(backup) {
                Ok(bytes) => Ok(Some(bytes)),
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
                Err(error) => Err(error.to_string()),
            }
        }
        Err(error) => Err(error.to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn agent_workspace_lease_is_exclusive_and_previous_generation_is_recoverable() {
        let root = std::env::temp_dir().join(format!("raf-agent-store-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&root).unwrap();
        let lease = ProjectWriteLease::acquire(&root).unwrap();
        assert!(ProjectWriteLease::acquire(&root).is_err());
        assert!(confined_path(&root, Path::new("../outside.ron")).is_err());
        let target = confined_path(&root, Path::new("world.ron")).unwrap();
        write_atomic(&target, b"first").unwrap();
        write_atomic(&target, b"second").unwrap();
        fs::remove_file(&target).unwrap();
        assert_eq!(
            read_recoverable(&target).unwrap().as_deref(),
            Some(b"first".as_slice())
        );
        drop(lease);
        drop(ProjectWriteLease::acquire(&root).unwrap());
        fs::remove_dir_all(root).unwrap();
    }
}
