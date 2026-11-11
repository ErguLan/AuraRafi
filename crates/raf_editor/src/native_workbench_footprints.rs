//! Workbench-local state and project-scoped actions for the footprint chooser.

use super::*;
use crate::panels::electronics_footprint_surface::{
    footprint_mapping_is_compatible, FootprintChooserCandidate, FootprintChooserModel,
};
use std::path::PathBuf;

#[derive(Debug, Default)]
pub(super) struct FootprintChooserState {
    import_job:
        Option<std::sync::mpsc::Receiver<Result<raf_electronics::pcb::FootprintAsset, String>>>,
    preview_signature: String,
    pub open: bool,
    pending_open: bool,
    refresh_requested: bool,
    selected_after_refresh: Option<String>,
    target_id: Option<uuid::Uuid>,
    target_designator: String,
    target_footprint: String,
    project_id: Option<uuid::Uuid>,
    project_path: Option<PathBuf>,
    selected_asset_id: Option<String>,
    all_candidates: Vec<FootprintChooserCandidate>,
    pub model: FootprintChooserModel,
    focus_after_sync: Option<String>,
}

impl FootprintChooserState {
    pub fn request_open(&mut self) {
        self.open = true;
        self.pending_open = true;
        self.refresh_requested = false;
        self.focus_after_sync = Some("electronics.footprint.query.input".to_string());
    }

    pub fn close(&mut self) {
        self.open = false;
        self.import_job = None;
        self.model.import_pending = false;
        self.model.import_status.clear();
        self.model.import_error = false;
        self.preview_signature.clear();
        self.model.preview_image_key = None;
        self.pending_open = false;
        self.refresh_requested = false;
        self.selected_after_refresh = None;
        self.target_id = None;
        self.target_designator.clear();
        self.target_footprint.clear();
        self.project_id = None;
        self.project_path = None;
        self.selected_asset_id = None;
        self.all_candidates.clear();
        self.model.candidates.clear();
        self.model.selected_index = None;
        self.focus_after_sync = Some("electronics.inspector.footprint.change".to_string());
    }

    pub fn set_query(&mut self, query: String) {
        if self.model.query == query {
            return;
        }
        self.model.query = query;
        self.selected_asset_id = None;
        self.rebuild_visible_candidates();
    }

    pub fn set_import_path(&mut self, path: String) {
        self.model.import_path = path;
    }

    pub fn set_import_status(&mut self, key: &str, detail: &str, error: bool, language: Language) {
        let prefix = raf_core::i18n::t(key, language);
        self.model.import_status = if detail.trim().is_empty() {
            prefix
        } else {
            format!("{prefix} {detail}")
        };
        self.model.import_error = error;
    }

    pub fn request_refresh(&mut self, select_asset: Option<String>) {
        self.selected_after_refresh = select_asset;
        self.refresh_requested = true;
    }

    pub fn is_open(&self) -> bool {
        self.open
    }

    pub fn take_focus_after_sync(&mut self) -> Option<String> {
        self.focus_after_sync.take()
    }

    pub fn sync(
        &mut self,
        project: Option<&Project>,
        electronics: Option<&NativeElectronicsEditor>,
    ) -> bool {
        if !self.open {
            return false;
        }
        let Some(project) =
            project.filter(|project| project.project_type == ProjectType::Electronics)
        else {
            self.close();
            return true;
        };
        let Some(electronics) = electronics else {
            self.close();
            return true;
        };
        let import_result = self
            .import_job
            .as_ref()
            .and_then(|job| match job.try_recv() {
                Ok(result) => Some(result),
                Err(std::sync::mpsc::TryRecvError::Empty) => None,
                Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                    Some(Err("Footprint download stopped".into()))
                }
            });
        if let Some(result) = import_result {
            self.import_job = None;
            self.model.import_pending = false;
            match result {
                Ok(asset) => {
                    self.model.import_status = asset.name.clone();
                    self.model.import_error = false;
                    self.request_refresh(Some(asset.id));
                }
                Err(error) => {
                    self.model.import_status = error;
                    self.model.import_error = true;
                    self.refresh_requested = true;
                }
            }
        }
        if self.pending_open {
            self.pending_open = false;
            self.project_id = Some(project.id);
            self.project_path = Some(project.path.clone());
            let (target, designator, footprint) = electronics
                .footprint_target()
                .map(|(id, name, footprint)| (Some(id), name, footprint))
                .unwrap_or((None, String::new(), String::new()));
            self.target_id = target;
            self.target_designator = designator;
            self.target_footprint = footprint;
            self.model.component_designator = self.target_designator.clone();
            self.model.current_footprint = self.target_footprint.clone();
            self.model.component_pins = electronics.footprint_pin_names();
            self.refresh_candidates(electronics);
            self.select_initial_candidate(None);
            self.rebuild_visible_candidates();
            return true;
        }
        if self.project_id != Some(project.id) || self.project_path.as_ref() != Some(&project.path)
        {
            self.close();
            return true;
        }
        // A side library follows canvas selection while assignment remains bound
        // to the explicit project and UUID shown in the inspector.
        let target = electronics
            .footprint_target()
            .map(|(id, name, footprint)| (Some(id), name, footprint))
            .unwrap_or((None, String::new(), String::new()));
        if target
            != (
                self.target_id,
                self.target_designator.clone(),
                self.target_footprint.clone(),
            )
        {
            let preferred = self.selected_asset_id.clone();
            (
                self.target_id,
                self.target_designator,
                self.target_footprint,
            ) = target;
            self.model.component_designator = self.target_designator.clone();
            self.model.current_footprint = self.target_footprint.clone();
            self.model.component_pins = electronics.footprint_pin_names();
            self.refresh_candidates(electronics);
            self.select_initial_candidate(preferred);
            self.rebuild_visible_candidates();
            return true;
        }
        if self.refresh_requested {
            self.refresh_requested = false;
            self.refresh_candidates(electronics);
            let preferred = self.selected_after_refresh.take();
            self.select_initial_candidate(preferred);
            self.rebuild_visible_candidates();
            return true;
        }
        false
    }

    pub fn select_visible_index(&mut self, index: usize) {
        let Some(candidate) = self.model.candidates.get(index) else {
            return;
        };
        self.selected_asset_id = Some(candidate.id.clone());
        self.model.selected_index = Some(index);
    }

    fn refresh_candidates(&mut self, electronics: &NativeElectronicsEditor) {
        match electronics.footprint_candidates() {
            Ok(candidates) => self.all_candidates = candidates,
            Err(error) => {
                self.all_candidates.clear();
                self.model.import_error = true;
                self.model.import_status = error;
            }
        }
        self.preview_signature.clear();
    }

    pub fn cycle_selected_pin_mapping(&mut self, pin_index: usize) -> bool {
        let Some(asset_id) = self.selected_asset_id.as_deref() else {
            return false;
        };
        let Some(candidate) = self
            .all_candidates
            .iter_mut()
            .find(|candidate| candidate.id == asset_id)
        else {
            return false;
        };
        let Some(pin) = self.model.component_pins.get(pin_index) else {
            return false;
        };
        let mut pad_numbers = Vec::new();
        for pad in &candidate.pads {
            let number = pad.number.trim();
            if !number.is_empty() && !pad_numbers.iter().any(|existing| existing == number) {
                pad_numbers.push(number.to_string());
            }
        }
        if pad_numbers.is_empty() {
            return false;
        }
        let Some(mapping_index) = candidate
            .pin_pad_mapping
            .iter()
            .position(|(mapped_pin, _)| mapped_pin == pin)
        else {
            candidate
                .pin_pad_mapping
                .push((pin.clone(), pad_numbers[0].clone()));
            self.rebuild_visible_candidates();
            return true;
        };
        let mapped_pad = &mut candidate.pin_pad_mapping[mapping_index].1;
        let next = if mapped_pad.is_empty() {
            Some(pad_numbers[0].clone())
        } else {
            pad_numbers
                .iter()
                .position(|number| number == mapped_pad)
                .and_then(|index| pad_numbers.get(index + 1).cloned())
        };
        *mapped_pad = next.unwrap_or_default();
        self.rebuild_visible_candidates();
        true
    }

    pub fn assignment_command(&self, project: Option<&Project>) -> Option<String> {
        let project = project.filter(|project| {
            project.project_type == ProjectType::Electronics
                && self.project_id == Some(project.id)
                && self.project_path.as_ref() == Some(&project.path)
        })?;
        let _ = project;
        let target = self.target_id?;
        let candidate = self
            .model
            .selected_index
            .and_then(|index| self.model.candidates.get(index))?;
        if !footprint_mapping_is_compatible(candidate, &self.model.component_pins) {
            return None;
        }
        let mapping = serde_json::to_vec(&candidate.pin_pad_mapping).ok()?;
        Some(format!(
            "electronics.footprint.assign_for:{target}:{}:{}",
            hex_encode(candidate.id.as_bytes()),
            hex_encode(&mapping)
        ))
    }

    fn select_initial_candidate(&mut self, preferred_asset: Option<String>) {
        let preferred = preferred_asset.as_deref();
        let current = self.target_footprint.as_str();
        let selected = preferred
            .and_then(|id| {
                self.all_candidates
                    .iter()
                    .position(|candidate| candidate.id == id)
            })
            .or_else(|| {
                self.all_candidates
                    .iter()
                    .position(|candidate| candidate.id == current || candidate.name == current)
            })
            .or_else(|| {
                self.all_candidates.iter().position(|candidate| {
                    footprint_mapping_is_compatible(candidate, &self.model.component_pins)
                })
            })
            .or_else(|| (!self.all_candidates.is_empty()).then_some(0));
        self.selected_asset_id = selected.map(|index| self.all_candidates[index].id.clone());
    }

    fn rebuild_visible_candidates(&mut self) {
        let query = self.model.query.trim().to_lowercase();
        self.model.candidates = self
            .all_candidates
            .iter()
            .filter(|candidate| candidate_matches(candidate, &query))
            .cloned()
            .collect();
        self.model.selected_index = self.selected_asset_id.as_ref().and_then(|selected| {
            self.model
                .candidates
                .iter()
                .position(|candidate| &candidate.id == selected)
        });
        if self.model.selected_index.is_none() {
            self.selected_asset_id = None;
        }
    }
}

impl NativeGameWorkbench {
    pub(super) fn sync_inspector_footprint_preview(
        &mut self,
        electronics: Option<&NativeElectronicsEditor>,
    ) {
        let target = electronics.and_then(|editor| editor.footprint_target());
        let signature = format!(
            "{:?}:{:?}:{}:{:?}",
            target,
            self.palette,
            electronics.map_or(0, NativeElectronicsEditor::revision),
            electronics.map(NativeElectronicsEditor::pcb_label_visibility)
        );
        if self.inspector_footprint_signature == signature {
            return;
        }
        self.inspector_footprint_signature = signature;
        self.inspector_footprint_presentation = Some(
            crate::panels::electronics_inspector_surface::ElectronicsInspectorPresentation {
                board_label_visibility: electronics
                    .filter(|e| e.active_surface() == raf_electronics::CadSurfaceKind::Pcb)
                    .map(NativeElectronicsEditor::pcb_label_visibility),
                ..Default::default()
            },
        );
        let Some((id, _, footprint)) = target else {
            return;
        };
        let Some(editor) = electronics else {
            return;
        };
        let component_name = editor
            .schematic()
            .components
            .iter()
            .find(|c| c.id == id)
            .map(|c| c.kind_label().to_string());
        if let Some(presentation) = self.inspector_footprint_presentation.as_mut() {
            presentation.component_symbol_image_key = Some(editor.component_asset_key(id).into());
            presentation.component_name = component_name.clone();
        }
        let Some(asset) = editor.physical_footprint_asset(&footprint) else {
            return;
        };
        let dimensions = asset
            .bounds_mm()
            .map(|b| [b.width_mm() as f32, b.height_mm() as f32]);
        let mapping = editor
            .schematic()
            .components
            .iter()
            .find(|c| c.id == id)
            .map(|c| {
                c.pins
                    .iter()
                    .map(|p| {
                        let number = c.footprint_pin_map.get(&p.name).unwrap_or(&p.name);
                        (
                            p.name.clone(),
                            if asset.pads.iter().any(|pad| pad.number == *number) {
                                number.clone()
                            } else {
                                String::new()
                            },
                        )
                    })
                    .collect()
            })
            .unwrap_or_default();
        let mut presentation =
            crate::panels::electronics_inspector_surface::ElectronicsInspectorPresentation {
                component_symbol_image_key: Some(editor.component_asset_key(id).into()),
                component_name,
                board_label_visibility: (editor.active_surface()
                    == raf_electronics::CadSurfaceKind::Pcb)
                    .then(|| editor.pcb_label_visibility()),
                footprint_name: Some(asset.name.clone()),
                footprint_dimensions_mm: dimensions,
                footprint_pad_count: Some(asset.pads.len()),
                pin_pad_mapping: mapping,
                ..Default::default()
            };
        if asset.is_fabrication_ready() {
            let (size, pixels) =
                crate::electronics_minimap::footprint_preview_rgba(asset, self.palette);
            let key = "electronics://inspector-footprint";
            if self
                .host
                .images_mut()
                .insert_rgba(key, size, pixels)
                .is_ok()
            {
                presentation.footprint_preview_image_key = Some(key.into());
            }
        }
        self.inspector_footprint_presentation = Some(presentation);
    }
    pub fn is_footprint_chooser_open(&self) -> bool {
        self.footprint_chooser.is_open()
    }
    pub fn has_pending_footprint_import(&self) -> bool {
        self.footprint_chooser.import_job.is_some()
    }
    pub(super) fn sync_footprint_preview(&mut self, electronics: Option<&NativeElectronicsEditor>) {
        if !self.footprint_chooser.open {
            return;
        }
        let Some(candidate) = self
            .footprint_chooser
            .model
            .selected_index
            .and_then(|i| self.footprint_chooser.model.candidates.get(i))
        else {
            self.footprint_chooser.model.preview_image_key = None;
            self.footprint_chooser.preview_signature.clear();
            return;
        };
        let signature = format!("{}:{:?}", candidate.id, self.palette);
        if signature == self.footprint_chooser.preview_signature {
            return;
        }
        self.footprint_chooser.preview_signature = signature;
        self.footprint_chooser.model.preview_image_key = None;
        let Some(root) = &self.footprint_chooser.project_path else {
            return;
        };
        let asset = electronics
            .and_then(|editor| editor.physical_footprint_asset(&candidate.id))
            .cloned()
            .or_else(|| {
                raf_electronics::pcb::FootprintLibrary::open(root)
                    .and_then(|library| library.load(&candidate.id))
                    .ok()
                    .flatten()
            });
        if let Some(asset) = asset {
            if asset.pads.iter().any(|pad| {
                matches!(
                    pad.shape,
                    raf_electronics::pcb::FootprintPadShape::Unsupported { .. }
                        | raf_electronics::pcb::FootprintPadShape::RoundRect { corner_ratio: None }
                )
            }) {
                return;
            }
            let (size, pixels) =
                crate::electronics_minimap::footprint_preview_rgba(&asset, self.palette);
            let key = "electronics://footprint-preview";
            if self
                .host
                .images_mut()
                .insert_rgba(key, size, pixels)
                .is_ok()
            {
                self.footprint_chooser.model.preview_image_key = Some(key.into());
            }
        }
        self.toolbar_revision = self.toolbar_revision.wrapping_add(1).max(1);
    }
    pub(super) fn handle_footprint_chooser_command(
        &mut self,
        name: &str,
        project: Option<&Project>,
        intents: &mut Vec<NativeWorkbenchIntent>,
    ) -> bool {
        match name {
            "electronics.footprint.open" => {
                self.electronics_workspace.pcb_menu = None;
                self.electronics_navigator_tab = "library".into();
                if self.electronics_workspace.pcb_active {
                    self.electronics_workspace.pcb_open = true;
                } else {
                    self.electronics_workspace.schematic_collapsed = false;
                }
                self.electronics_workspace.inspector_hidden = false;
                self.footprint_chooser.request_open();
                self.bump_footprint_surface();
                true
            }
            "electronics.footprint.close" => {
                self.footprint_chooser.close();
                self.bump_footprint_surface();
                true
            }
            "electronics.footprint.query" => {
                self.bump_footprint_surface();
                true
            }
            "electronics.footprint.import" => {
                self.import_footprint_file(project);
                true
            }
            "electronics.footprint.assign" => {
                if let Some(command) = self.footprint_chooser.assignment_command(project) {
                    intents.push(NativeWorkbenchIntent::Command(command));
                    self.footprint_chooser.close();
                } else {
                    let language = self.agent_panel.borrow_mut().language;
                    self.console.log(
                        crate::console::LogLevel::Warning,
                        raf_core::i18n::t("electronics.footprint.assignment_invalid", language),
                    );
                }
                self.bump_footprint_surface();
                true
            }
            _ => {
                if let Some(index) = name.strip_prefix("electronics.footprint.select:") {
                    if let Ok(index) = index.parse::<usize>() {
                        self.footprint_chooser.select_visible_index(index);
                    }
                    self.bump_footprint_surface();
                    return true;
                }
                if let Some(index) = name.strip_prefix("electronics.footprint.map:") {
                    if let Ok(index) = index.parse::<usize>() {
                        self.footprint_chooser.cycle_selected_pin_mapping(index);
                    }
                    self.bump_footprint_surface();
                    return true;
                }
                false
            }
        }
    }

    fn import_footprint_file(&mut self, project: Option<&Project>) {
        if self.footprint_chooser.import_job.is_some() {
            return;
        }
        let Some(project) = project.filter(|project| {
            project.project_type == ProjectType::Electronics
                && self.footprint_chooser.project_id == Some(project.id)
                && self.footprint_chooser.project_path.as_ref() == Some(&project.path)
        }) else {
            return;
        };
        let language = self.agent_panel.borrow_mut().language;
        let raw_path = self.footprint_chooser.model.import_path.trim();
        if raw_path.starts_with("https://") {
            if !official_footprint_url(raw_path) {
                self.footprint_chooser.set_import_status(
                    "electronics.footprint.official_url_required",
                    "",
                    true,
                    language,
                );
                self.bump_footprint_surface();
                return;
            }
            let url = raw_path.to_string();
            let root = project.path.clone();
            let (sender, receiver) = std::sync::mpsc::channel();
            self.footprint_chooser.import_job = Some(receiver);
            self.footprint_chooser.model.import_pending = true;
            self.footprint_chooser.set_import_status(
                "electronics.footprint.downloading",
                "",
                false,
                language,
            );
            std::thread::spawn(move || {
                let result = download_official_footprint(&root, &url);
                let _ = sender.send(result);
            });
            self.bump_footprint_surface();
            return;
        }
        if raw_path.is_empty() {
            self.footprint_chooser.set_import_status(
                "electronics.footprint.import_path_required",
                "",
                true,
                language,
            );
            self.bump_footprint_surface();
            return;
        }
        let path = PathBuf::from(raw_path);
        let path = if path.is_absolute() {
            path
        } else {
            project.path.join(path)
        };
        if !path
            .extension()
            .and_then(|extension| extension.to_str())
            .is_some_and(|extension| extension.eq_ignore_ascii_case("kicad_mod"))
        {
            self.footprint_chooser.set_import_status(
                "electronics.footprint.import_bad_extension",
                "",
                true,
                language,
            );
            self.bump_footprint_surface();
            return;
        }
        let result = raf_electronics::pcb::FootprintLibrary::open(&project.path)
            .and_then(|library| library.import_kicad_file(&path, None));
        match result {
            Ok(asset) => {
                self.footprint_chooser.set_import_status(
                    "electronics.footprint.imported",
                    &asset.name,
                    false,
                    language,
                );
                self.footprint_chooser.request_refresh(Some(asset.id));
            }
            Err(error) => {
                self.footprint_chooser.set_import_status(
                    "electronics.footprint.import_failed",
                    &error.to_string(),
                    true,
                    language,
                );
            }
        }
        self.bump_footprint_surface();
    }

    fn bump_footprint_surface(&mut self) {
        self.toolbar_revision = self.toolbar_revision.wrapping_add(1).max(1);
    }
}

fn official_footprint_url(url: &str) -> bool {
    (url.starts_with("https://raw.githubusercontent.com/KiCad/kicad-footprints/")
        || url.starts_with("https://gitlab.com/kicad/libraries/kicad-footprints/-/raw/"))
        && url.ends_with(".kicad_mod")
}

fn download_official_footprint(
    root: &std::path::Path,
    url: &str,
) -> Result<raf_electronics::pcb::FootprintAsset, String> {
    use std::io::Read;
    if !official_footprint_url(url) {
        return Err("Expected a direct official KiCad footprint URL".into());
    }
    let agent = ureq::AgentBuilder::new()
        .timeout_connect(std::time::Duration::from_secs(10))
        .timeout_read(std::time::Duration::from_secs(20))
        .build();
    let response = agent.get(url).call().map_err(|e| e.to_string())?;
    let mut source = String::new();
    response
        .into_reader()
        .take(4 * 1024 * 1024 + 1)
        .read_to_string(&mut source)
        .map_err(|e| e.to_string())?;
    if source.len() > 4 * 1024 * 1024 {
        return Err("Footprint download exceeds 4 MiB".into());
    }
    raf_electronics::pcb::FootprintLibrary::open(root)
        .map_err(|e| e.to_string())?
        .import_kicad_source_with_provenance(
            &source,
            raf_electronics::pcb::FootprintProvenance {
                source_path: Some(url.into()),
                library: Some("KiCad official libraries".into()),
                license: Some("CC-BY-SA-4.0 with KiCad library exception".into()),
                ..Default::default()
            },
        )
        .map_err(|e| e.to_string())
}

fn candidate_matches(candidate: &FootprintChooserCandidate, query: &str) -> bool {
    if query.is_empty() {
        return true;
    }
    candidate.name.to_lowercase().contains(query)
        || candidate.source.to_lowercase().contains(query)
        || candidate.description.to_lowercase().contains(query)
        || candidate
            .tags
            .iter()
            .any(|tag| tag.to_lowercase().contains(query))
        || candidate
            .pads
            .iter()
            .any(|pad| pad.number.to_lowercase().contains(query))
}

fn hex_encode(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut encoded = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        encoded.push(HEX[(byte >> 4) as usize] as char);
        encoded.push(HEX[(byte & 0x0f) as usize] as char);
    }
    encoded
}

#[cfg(test)]
mod tests {
    #[test]
    fn official_download_rejects_lookalike_hosts_and_non_footprint_urls() {
        assert!(super::official_footprint_url("https://raw.githubusercontent.com/KiCad/kicad-footprints/master/Resistor_SMD.pretty/R_0805_2012Metric.kicad_mod"));
        for url in [
            "http://raw.githubusercontent.com/KiCad/kicad-footprints/a.kicad_mod",
            "https://raw.githubusercontent.com.evil.test/KiCad/kicad-footprints/a.kicad_mod",
            "https://gitlab.com/another-project/a.kicad_mod",
            "https://raw.githubusercontent.com/KiCad/kicad-footprints/master/README.md",
        ] {
            assert!(!super::official_footprint_url(url));
        }
    }

    #[test]
    #[ignore = "Requires access to the official KiCad repository"]
    fn official_footprint_download_preserves_source_and_offline_library() {
        let directory =
            std::env::temp_dir().join(format!("raf-official-footprint-{}", uuid::Uuid::new_v4()));
        let url="https://raw.githubusercontent.com/KiCad/kicad-footprints/master/Resistor_SMD.pretty/R_0805_2012Metric.kicad_mod";
        let asset = super::download_official_footprint(&directory, url).unwrap();
        assert_eq!(asset.pads.len(), 2);
        assert!(
            asset.is_fabrication_ready(),
            "{:?}",
            asset.unsupported_features
        );
        assert_eq!(asset.provenance.source_path.as_deref(), Some(url));
        let loaded = raf_electronics::pcb::FootprintLibrary::open(&directory)
            .unwrap()
            .load(&asset.id)
            .unwrap()
            .unwrap();
        assert_eq!(loaded, asset);
        std::fs::remove_dir_all(directory).unwrap();
    }
    use super::*;
    use crate::panels::electronics_footprint_surface::FootprintChooserPad;

    fn candidate() -> FootprintChooserCandidate {
        FootprintChooserCandidate {
            id: "local-footprint".to_string(),
            name: "Test:TwoPad".to_string(),
            source: "Project library".to_string(),
            description: "Two physical pads".to_string(),
            tags: vec!["Test".to_string()],
            bounds_min_mm: [-1.0, -1.0],
            bounds_size_mm: [2.0, 2.0],
            pads: vec![
                FootprintChooserPad {
                    number: "1".to_string(),
                    position_mm: [-0.5, 0.0],
                    size_mm: [0.5, 1.0],
                    shape: "rect".to_string(),
                    rotation_degrees: 0.0,
                },
                FootprintChooserPad {
                    number: "2".to_string(),
                    position_mm: [0.5, 0.0],
                    size_mm: [0.5, 1.0],
                    shape: "rect".to_string(),
                    rotation_degrees: 0.0,
                },
            ],
            pin_pad_mapping: vec![
                ("A".to_string(), String::new()),
                ("K".to_string(), String::new()),
            ],
            compatible: false,
            compatibility_detail: String::new(),
            fabrication_ready: true,
            fabrication_detail: String::new(),
        }
    }

    fn state() -> FootprintChooserState {
        let candidate = candidate();
        let mut state = FootprintChooserState {
            open: true,
            target_id: Some(uuid::Uuid::nil()),
            target_designator: "D1".to_string(),
            target_footprint: String::new(),
            project_id: Some(uuid::Uuid::nil()),
            project_path: Some(PathBuf::from("project")),
            selected_asset_id: Some(candidate.id.clone()),
            all_candidates: vec![candidate.clone()],
            model: FootprintChooserModel {
                component_pins: vec!["A".to_string(), "K".to_string()],
                ..Default::default()
            },
            ..Default::default()
        };
        state.rebuild_visible_candidates();
        state
    }

    #[test]
    fn search_matches_asset_metadata_and_pad_numbers() {
        let mut state = state();
        state.all_candidates.push(FootprintChooserCandidate {
            id: "other".to_string(),
            name: "Other".to_string(),
            source: "External library".to_string(),
            ..candidate()
        });
        state.set_query("project LIBRARY".to_string());
        assert_eq!(state.model.candidates.len(), 1);
        assert_eq!(state.model.candidates[0].id, "local-footprint");
        state.set_query("2".to_string());
        assert_eq!(state.model.candidates.len(), 2);
    }

    #[test]
    fn pin_mapping_cycles_explicitly_and_assignment_is_project_scoped() {
        let mut state = state();
        assert!(!footprint_mapping_is_compatible(
            &state.model.candidates[0],
            &state.model.component_pins
        ));
        assert!(state.cycle_selected_pin_mapping(0));
        assert!(state.cycle_selected_pin_mapping(1));
        assert!(!footprint_mapping_is_compatible(
            &state.model.candidates[0],
            &state.model.component_pins
        ));
        assert!(state.cycle_selected_pin_mapping(1));
        let candidate = &state.model.candidates[0];
        assert_eq!(
            candidate.pin_pad_mapping[0],
            ("A".to_string(), "1".to_string())
        );
        assert_eq!(
            candidate.pin_pad_mapping[1],
            ("K".to_string(), "2".to_string())
        );
        assert!(footprint_mapping_is_compatible(
            candidate,
            &state.model.component_pins
        ));

        let project = Project {
            id: uuid::Uuid::nil(),
            name: "Project".to_string(),
            project_type: ProjectType::Electronics,
            path: PathBuf::from("project"),
            created_at: chrono::Utc::now(),
            modified_at: chrono::Utc::now(),
            engine_version: String::new(),
            settings: Default::default(),
        };
        let command = state
            .assignment_command(Some(&project))
            .expect("valid project-scoped assignment");
        assert!(command.starts_with("electronics.footprint.assign_for:"));
        let changed_project = Project {
            path: PathBuf::from("different-project"),
            ..project
        };
        assert!(state.assignment_command(Some(&changed_project)).is_none());
    }
}
