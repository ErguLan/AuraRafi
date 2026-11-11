//! Export the live PCB into a new project-local job, preserving earlier jobs.
use super::*;
impl NativeElectronicsEditor {
    pub fn fabrication_status(&self) -> Option<&str> {
        self.fabrication_status.as_deref()
    }
    pub fn fabrication_failed(&self) -> bool {
        self.fabrication_error.is_some()
    }
    pub fn fabrication_summary(&self) -> &[String] {
        &self.fabrication_summary
    }
    pub fn fabrication_warnings(&self) -> &[String] {
        &self.fabrication_warnings
    }
    pub fn fabrication_job_available(&self) -> bool {
        self.fabrication_job.is_some()
    }
    /// Saves the already validated job through the editor's destination picker.
    /// A fresh filename preserves existing deliveries; this never edits the PCB.
    pub fn deliver_fabrication_zip(
        &mut self,
        folder: &std::path::Path,
    ) -> Result<std::path::PathBuf, String> {
        let result = (|| {
            let job = self
                .fabrication_job
                .as_ref()
                .ok_or_else(|| i18n::t("electronics.fabrication.export_first", self.language))?;
            let destination = folder.join(format!("PCB-{}.zip", Uuid::new_v4()));
            let mut source = std::fs::File::open(job.join("board-fabrication.zip"))
                .map_err(|e| e.to_string())?;
            let mut target = std::fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&destination)
                .map_err(|e| e.to_string())?;
            if let Err(error) =
                std::io::copy(&mut source, &mut target).and_then(|_| target.sync_all())
            {
                drop(target);
                let _ = std::fs::remove_file(&destination);
                return Err(error.to_string());
            }
            Ok(destination)
        })();
        self.fabrication_error = result.as_ref().err().cloned();
        self.fabrication_status = Some(match &result {
            Ok(path) => format!(
                "{} {}",
                i18n::t("electronics.fabrication.saved", self.language),
                path.display()
            ),
            Err(error) => format!(
                "{} {}",
                i18n::t("electronics.fabrication.blocked", self.language),
                error
            ),
        });
        self.touch_ui();
        result
    }
    pub fn export_physical_pcb(&mut self) -> Result<std::path::PathBuf, String> {
        self.fabrication_job = None;
        self.fabrication_summary.clear();
        self.fabrication_warnings.clear();
        let result = (|| {
            if self.sync_is_stale() {
                return Err(i18n::t("electronics.sync.pcb_stale", self.language));
            }
            let electrical = raf_electronics::run_drc(&self.schematic);
            let powered = self.schematic.components.iter().any(|component| {
                matches!(
                    component.sim_model,
                    raf_electronics::component::SimModel::DcSource { .. }
                )
            });
            let blockers: Vec<_> = electrical
                .errors
                .iter()
                .chain(electrical.warnings.iter().filter(|issue| {
                    matches!(
                        issue.rule.as_str(),
                        "led_no_resistor" | "floating_pin" | "dangling_wire_endpoint"
                    )
                }))
                .collect();
            if powered && !blockers.is_empty() {
                return Err(blockers
                    .iter()
                    .map(|issue| format!("ERC {}: {}", issue.rule, issue.message))
                    .collect::<Vec<_>>()
                    .join("\n"));
            }
            let simulation = self.schematic.simulate_dc();
            if powered && !simulation.converged {
                return Err(simulation.messages.join("\n"));
            }
            let root = self
                .footprint_project_root
                .as_ref()
                .ok_or("Open an Electronics project before exporting.")?;
            let output = raf_electronics::pcb::export_manufacturing_files(&self.pcb)?;
            let mut assembly = raf_electronics::pcb::assembly::export_assembly_files(&self.pcb)?;
            assembly.insert(
                "PINMAP.csv".into(),
                raf_electronics::pcb::assembly::export_pin_map(&self.schematic, &self.pcb)?,
            );
            let jobs = root.join("ElectricalAssets").join("Manufacturing");
            std::fs::create_dir_all(&jobs).map_err(|e| e.to_string())?;
            let job = jobs.join(format!("board-{}", Uuid::new_v4()));
            std::fs::create_dir(&job).map_err(|e| e.to_string())?;
            // COMPLETE is the last write. A failed job remains recoverable and
            // clearly incomplete; no previous job can be overwritten.
            for (name, content) in &output.files {
                std::fs::write(job.join(name), content)
                    .map_err(|e| format!("Incomplete export {}: {e}", job.display()))?;
            }
            std::fs::write(
                job.join("board-fabrication.zip"),
                fabrication_zip(&output.files)?,
            )
            .map_err(|e| format!("Incomplete export {}: {e}", job.display()))?;
            for (name, content) in &assembly {
                std::fs::write(job.join(name), content)
                    .map_err(|e| format!("Incomplete assembly export: {e}"))?;
            }
            std::fs::write(job.join("board-assembly.zip"), fabrication_zip(&assembly)?)
                .map_err(|e| e.to_string())?;
            std::fs::write(
                job.join("electrical-check.txt"),
                electrical.to_string_list().join("\n"),
            )
            .map_err(|e| e.to_string())?;
            std::fs::write(
                job.join("simulation-dc.json"),
                serde_json::to_string_pretty(&simulation).map_err(|e| e.to_string())?,
            )
            .map_err(|e| e.to_string())?;
            std::fs::write(job.join("COMPLETE"),"Bare-board output written. Validate in an independent CAM reader before ordering.\n")
                .map_err(|e|e.to_string())?;
            let t = |key| i18n::t(key, self.language);
            self.fabrication_summary = vec![
                t("electronics.fabrication.preflight_passed"),
                format!(
                    "{} {} · {} {} · {} {}",
                    self.pcb.components.len(),
                    t("electronics.fabrication.parts"),
                    self.pcb.traces.len(),
                    t("electronics.fabrication.tracks"),
                    self.pcb.vias.len(),
                    t("electronics.fabrication.vias")
                ),
                format!(
                    "{} {} · {} {}",
                    electrical.errors.len(),
                    t("electronics.analysis.errors"),
                    electrical.warnings.len(),
                    t("electronics.analysis.warnings")
                ),
                format!(
                    "{}: {}",
                    t("electronics.analysis.simulation_title"),
                    if simulation.converged {
                        t("electronics.analysis.passed")
                    } else {
                        t("electronics.analysis.not_run")
                    }
                ),
                t("electronics.fabrication.contents"),
                format!(
                    "{}: {}",
                    t("electronics.fabrication.archive"),
                    job.join("board-fabrication.zip").display()
                ),
                t("electronics.fabrication.cam_pending"),
            ];
            self.fabrication_warnings = output.warnings.clone();
            self.fabrication_job = Some(job.clone());
            Ok(job)
        })();
        if let Some(previous) = self.fabrication_error.take() {
            self.surface_errors.retain(|error| error != &previous);
        }
        self.fabrication_error = result.as_ref().err().cloned();
        self.fabrication_status = Some(match &result {
            Ok(path) => format!(
                "{} {}",
                i18n::t("electronics.fabrication.saved", self.language),
                path.display()
            ),
            Err(error) => format!(
                "{} {}",
                i18n::t("electronics.fabrication.blocked", self.language),
                error
            ),
        });
        self.touch_ui();
        result
    }
}
/// Small uncompressed ZIP32 archive for CAM text files. No timestamps, paths or
/// external state enter the archive. Reject ZIP64 rather than truncate fields.
fn fabrication_zip(files: &std::collections::BTreeMap<String, String>) -> Result<Vec<u8>, String> {
    fn u16le(out: &mut Vec<u8>, n: u16) {
        out.extend(n.to_le_bytes());
    }
    fn u32le(out: &mut Vec<u8>, n: u32) {
        out.extend(n.to_le_bytes());
    }
    fn crc32(bytes: &[u8]) -> u32 {
        let mut crc = !0u32;
        for &byte in bytes {
            crc ^= byte as u32;
            for _ in 0..8 {
                crc = (crc >> 1) ^ (0xedb88320 & 0u32.wrapping_sub(crc & 1));
            }
        }
        !crc
    }
    let count = u16::try_from(files.len()).map_err(|_| "Too many CAM files")?;
    let mut out = Vec::new();
    let mut directory = Vec::new();
    for (name, content) in files {
        if !name.is_ascii()
            || name.is_empty()
            || matches!(name.as_str(), "." | "..")
            || name.chars().any(char::is_control)
            || name.contains(['/', '\\'])
        {
            return Err("Invalid CAM filename".into());
        }
        let name_len = u16::try_from(name.len()).map_err(|_| "CAM filename too long")?;
        let size = u32::try_from(content.len()).map_err(|_| "CAM file exceeds ZIP32")?;
        let offset = u32::try_from(out.len()).map_err(|_| "CAM archive exceeds ZIP32")?;
        let crc = crc32(content.as_bytes());
        u32le(&mut out, 0x04034b50);
        for n in [20, 0, 0, 0, 33] {
            u16le(&mut out, n);
        }
        for n in [crc, size, size] {
            u32le(&mut out, n);
        }
        u16le(&mut out, name_len);
        u16le(&mut out, 0);
        out.extend(name.as_bytes());
        out.extend(content.as_bytes());
        u32le(&mut directory, 0x02014b50);
        for n in [20, 20, 0, 0, 0, 33] {
            u16le(&mut directory, n);
        }
        for n in [crc, size, size] {
            u32le(&mut directory, n);
        }
        for n in [name_len, 0, 0, 0, 0] {
            u16le(&mut directory, n);
        }
        u32le(&mut directory, 0);
        u32le(&mut directory, offset);
        directory.extend(name.as_bytes());
    }
    let offset = u32::try_from(out.len()).map_err(|_| "CAM archive exceeds ZIP32")?;
    let size = u32::try_from(directory.len()).map_err(|_| "CAM directory exceeds ZIP32")?;
    out.extend(directory);
    u32le(&mut out, 0x06054b50);
    for n in [0, 0, count, count] {
        u16le(&mut out, n);
    }
    u32le(&mut out, size);
    u32le(&mut out, offset);
    u16le(&mut out, 0);
    Ok(out)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn physical_export_writes_new_jobs_and_refuses_stale_schematic() {
        let root = std::env::temp_dir().join(format!("raf-manufacturing-job-{}", Uuid::new_v4()));
        let mut editor = NativeElectronicsEditor::empty("physical job");
        editor.footprint_project_root = Some(root.clone());
        editor
            .schematic
            .add_component(raf_electronics::component::ElectronicComponent::resistor(
                "1k",
            ));
        editor.pcb.sync_from_schematic(&editor.schematic);
        let target = editor.schematic.components[0].id;
        let asset = raf_electronics::pcb::parse_kicad_footprint(
            r#"(footprint "RealJob"
          (pad "1" smd rect (at -1 0) (size 1 1) (layers "F.Cu" "F.Mask"))
          (pad "2" smd rect (at 1 0) (size 1 1) (layers "F.Cu" "F.Mask")))"#,
        )
        .unwrap();
        editor.assign_physical_footprint(target, asset).unwrap();
        let history = editor.history.len();
        let first = editor.export_physical_pcb().unwrap();
        let second = editor.export_physical_pcb().unwrap();
        assert!(!editor.fabrication_failed());
        assert!(editor.fabrication_job_available());
        assert!(!editor.fabrication_summary().is_empty());
        let delivered = editor.deliver_fabrication_zip(&root).unwrap();
        assert_eq!(
            std::fs::read(delivered).unwrap(),
            std::fs::read(second.join("board-fabrication.zip")).unwrap()
        );
        let archive = std::fs::read(first.join("board-fabrication.zip")).unwrap();
        assert_eq!(&archive[..4], b"PK\x03\x04");
        if let Some(path) = std::env::var_os("RAF_CAM_ZIP_QA") {
            std::fs::write(path, &archive).unwrap();
        }
        assert_ne!(first, second);
        assert!(first.join("COMPLETE").is_file() && second.join("COMPLETE").is_file());
        assert!(std::fs::read_to_string(first.join("board-F_Cu.gbr"))
            .unwrap()
            .contains("%MOMM*%"));
        assert_eq!(
            history,
            editor.history.len(),
            "Export does not create document edits"
        );
        editor.schematic.components[0].value = "2k".into();
        editor.rebuild_scene();
        assert!(
            editor.fabrication_status().is_none(),
            "An old success must not describe an edited board"
        );
        assert!(editor.export_physical_pcb().is_err());
        assert_eq!(
            std::fs::read_dir(first.parent().unwrap()).unwrap().count(),
            2,
            "Blocked output creates no job"
        );
        assert!(root.starts_with(std::env::temp_dir()));
        std::fs::remove_dir_all(root).unwrap();
    }
}
