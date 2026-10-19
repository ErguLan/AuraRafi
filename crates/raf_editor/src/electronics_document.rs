//! Recoverable save of the schematic/PCB pair without changing either schema.
use raf_electronics::storage::write_atomic;
use raf_electronics::{PcbLayout, Schematic};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Serialize, Deserialize)]
struct PendingSave {
    schematic_path: PathBuf,
    pcb_path: PathBuf,
    schematic_before: Option<String>,
    pcb_before: Option<String>,
    schematic_after: String,
    pcb_after: String,
}

fn journal_path(schematic: &Path) -> PathBuf {
    schematic.with_file_name(format!(
        ".{}.pending-save.ron",
        schematic.file_name().unwrap_or_default().to_string_lossy()
    ))
}

fn read_optional(path: &Path) -> Result<Option<String>, String> {
    match std::fs::read_to_string(path) {
        Ok(raw) => Ok(Some(raw)),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(format!("{}: {error}", path.display())),
    }
}

fn validate_pair(schematic: Option<&str>, pcb: Option<&str>) -> Result<(), String> {
    if let Some(raw) = schematic {
        ron::from_str::<Schematic>(raw).map_err(|e| format!("Schematic left untouched: {e}"))?;
    }
    if let Some(raw) = pcb {
        ron::from_str::<PcbLayout>(raw).map_err(|e| format!("PCB left untouched: {e}"))?;
    }
    Ok(())
}

/// Complete an interrupted save only when neither document was edited by
/// another writer since the journal was prepared. Conflicting files survive.
pub(super) fn recover(schematic_path: &Path, pcb_path: &Path) -> Result<(), String> {
    let journal = journal_path(schematic_path);
    let Some(raw) = read_optional(&journal)? else {
        return Ok(());
    };
    let pending: PendingSave =
        ron::from_str(&raw).map_err(|e| format!("Save journal unreadable: {e}"))?;
    if pending.schematic_path != schematic_path
        || pending.pcb_path != pcb_path
        || schematic_path == pcb_path
    {
        return Err(
            "Save journal does not match these document paths; files were left untouched.".into(),
        );
    }
    validate_pair(Some(&pending.schematic_after), Some(&pending.pcb_after))?;
    let schematic = read_optional(schematic_path)?;
    let pcb = read_optional(pcb_path)?;
    if (schematic != pending.schematic_before
        && schematic.as_deref() != Some(&pending.schematic_after))
        || (pcb != pending.pcb_before && pcb.as_deref() != Some(&pending.pcb_after))
    {
        return Err(format!("An interrupted save conflicts with external changes. Documents and recovery journal {} were preserved.",journal.display()));
    }
    write_atomic(schematic_path, &pending.schematic_after)
        .map_err(|e| format!("schematic save: {e}"))?;
    write_atomic(pcb_path, &pending.pcb_after).map_err(|e| format!("PCB save: {e}"))?;
    std::fs::remove_file(&journal).map_err(|e| format!("save journal cleanup: {e}"))
}

pub(super) fn save(
    schematic_path: &Path,
    pcb_path: &Path,
    schematic: &Schematic,
    pcb: &PcbLayout,
) -> Result<(), String> {
    if schematic_path == pcb_path {
        return Err("Schematic and PCB must have separate document paths.".into());
    }
    recover(schematic_path, pcb_path)?;
    let schematic_before = read_optional(schematic_path)?;
    let pcb_before = read_optional(pcb_path)?;
    validate_pair(schematic_before.as_deref(), pcb_before.as_deref())?;
    let pending = PendingSave {
        schematic_path: schematic_path.to_path_buf(),
        pcb_path: pcb_path.to_path_buf(),
        schematic_before,
        pcb_before,
        schematic_after: ron::ser::to_string_pretty(schematic, ron::ser::PrettyConfig::default())
            .map_err(|e| e.to_string())?,
        pcb_after: ron::ser::to_string_pretty(pcb, ron::ser::PrettyConfig::default())
            .map_err(|e| e.to_string())?,
    };
    // Persist the entire intended pair before replacing either document.
    let journal = ron::ser::to_string(&pending).map_err(|e| e.to_string())?;
    write_atomic(&journal_path(schematic_path), &journal)
        .map_err(|e| format!("save journal: {e}"))?;
    recover(schematic_path, pcb_path)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn root() -> PathBuf {
        let p = std::env::temp_dir().join(format!("raf-pair-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&p).unwrap();
        p
    }
    #[test]
    fn electronics_pair_replaces_both_existing_documents() {
        let root = root();
        let s = root.join("s.ron");
        let p = root.join("p.ron");
        save(&s, &p, &Schematic::new("before"), &PcbLayout::new("before")).unwrap();
        save(&s, &p, &Schematic::new("after"), &PcbLayout::new("after")).unwrap();
        assert_eq!(
            ron::from_str::<Schematic>(&std::fs::read_to_string(&s).unwrap())
                .unwrap()
                .name,
            "after"
        );
        assert_eq!(
            ron::from_str::<PcbLayout>(&std::fs::read_to_string(&p).unwrap())
                .unwrap()
                .name,
            "after"
        );
        assert!(!journal_path(&s).exists());
        std::fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn electronics_pair_preflight_keeps_schematic_if_pcb_is_unreadable() {
        let root = root();
        let s = root.join("s.ron");
        let p = root.join("p.ron");
        save(&s, &p, &Schematic::new("before"), &PcbLayout::new("before")).unwrap();
        let old = std::fs::read_to_string(&s).unwrap();
        std::fs::write(&p, "broken").unwrap();
        assert!(save(&s, &p, &Schematic::new("after"), &PcbLayout::new("after")).is_err());
        assert_eq!(std::fs::read_to_string(&s).unwrap(), old);
        assert_eq!(std::fs::read_to_string(&p).unwrap(), "broken");
        std::fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn electronics_pair_recovers_interrupted_save_and_preserves_external_conflicts() {
        let root = root();
        let s = root.join("s.ron");
        let p = root.join("p.ron");
        save(&s, &p, &Schematic::new("before"), &PcbLayout::new("before")).unwrap();
        let pending = PendingSave {
            schematic_path: s.clone(),
            pcb_path: p.clone(),
            schematic_before: read_optional(&s).unwrap(),
            pcb_before: read_optional(&p).unwrap(),
            schematic_after: ron::ser::to_string(&Schematic::new("after")).unwrap(),
            pcb_after: ron::ser::to_string(&PcbLayout::new("after")).unwrap(),
        };
        write_atomic(&journal_path(&s), &ron::ser::to_string(&pending).unwrap()).unwrap();
        write_atomic(&s, &pending.schematic_after).unwrap();
        std::fs::write(
            &p,
            ron::ser::to_string(&PcbLayout::new("external")).unwrap(),
        )
        .unwrap();
        assert!(recover(&s, &p).is_err());
        assert!(journal_path(&s).exists());
        assert!(std::fs::read_to_string(&p).unwrap().contains("external"));
        std::fs::write(&p, pending.pcb_before.unwrap()).unwrap();
        recover(&s, &p).unwrap();
        recover(&s, &p).unwrap();
        assert!(std::fs::read_to_string(&p).unwrap().contains("after"));
        std::fs::remove_dir_all(root).unwrap();
    }
}
