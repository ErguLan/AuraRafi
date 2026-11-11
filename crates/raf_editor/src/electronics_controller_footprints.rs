//! Physical footprint assignment, scoped to the mounted project document.
use super::*;
use raf_electronics::pcb::{
    FootprintAsset, FootprintLibrary, FootprintPadShape, FootprintPadTechnology,
};

impl NativeElectronicsEditor {
    pub fn physical_footprint_asset(&self, id: &str) -> Option<&FootprintAsset> {
        self.pcb.footprint_assets.get(id)
    }
    pub fn footprint_pin_names(&self) -> Vec<String> {
        self.footprint_target()
            .and_then(|(target, _, _)| self.schematic.components.iter().find(|c| c.id == target))
            .map(|c| c.pins.iter().map(|p| p.name.clone()).collect())
            .unwrap_or_default()
    }
    pub fn footprint_candidates(
        &self,
    ) -> Result<Vec<crate::panels::electronics_footprint_surface::FootprintChooserCandidate>, String>
    {
        use crate::panels::electronics_footprint_surface::{
            FootprintChooserCandidate, FootprintChooserPad,
        };
        let target = self.footprint_target().map(|(id, _, _)| id);
        let Some(root) = &self.footprint_project_root else {
            return Err("No mounted Electronics project".into());
        };
        let library = FootprintLibrary::open(root).map_err(|e| e.to_string())?;
        let mut assets = library
            .load_all()
            .map_err(|e| e.to_string())?
            .into_iter()
            .map(|asset| (asset.id.clone(), asset))
            .collect::<std::collections::BTreeMap<_, _>>();
        // A copied PCB remains a usable library source while offline.
        assets.extend(self.pcb.footprint_assets.clone());
        for asset in raf_electronics::pcb::builtin_footprints::assets() {
            assets.entry(asset.id.clone()).or_insert(asset);
        }
        Ok(assets
            .into_values()
            .into_iter()
            .map(|asset| {
                let mapping = target.map(|target| self.footprint_mapping(target, &asset));
                let ready = asset.is_fabrication_ready();
                let bounds = asset.bounds_mm();
                FootprintChooserCandidate {
                    id: asset.id.clone(),
                    name: asset.name.clone(),
                    source: asset
                        .provenance
                        .library
                        .clone()
                        .unwrap_or_else(|| asset.provenance.source_format.clone()),
                    description: asset.description.clone().unwrap_or_default(),
                    tags: asset.tags.clone(),
                    bounds_min_mm: bounds
                        .map(|b| [b.min.x as f32, b.min.y as f32])
                        .unwrap_or([0.0; 2]),
                    bounds_size_mm: bounds
                        .map(|b| [b.width_mm() as f32, b.height_mm() as f32])
                        .unwrap_or([0.0; 2]),
                    pads: asset
                        .pads
                        .iter()
                        .map(|p| FootprintChooserPad {
                            number: p.number.clone(),
                            position_mm: [p.position_mm.x as f32, p.position_mm.y as f32],
                            size_mm: [p.size_mm.width as f32, p.size_mm.height as f32],
                            shape: match p.shape {
                                FootprintPadShape::Rectangle => "rect",
                                FootprintPadShape::Circle => "circle",
                                FootprintPadShape::Oval => "oval",
                                FootprintPadShape::RoundRect { .. } => "roundrect",
                                FootprintPadShape::Unsupported { .. } => "unsupported",
                            }
                            .into(),
                            rotation_degrees: p.rotation_degrees as f32,
                        })
                        .collect(),
                    compatible: mapping.as_ref().is_some_and(|m| m.is_ok()) && ready,
                    compatibility_detail: mapping.and_then(|m| m.err()).unwrap_or_default(),
                    pin_pad_mapping: self
                        .schematic
                        .components
                        .iter()
                        .find(|c| Some(c.id) == target)
                        .map(|c| {
                            c.pins
                                .iter()
                                .map(|pin| {
                                    let number =
                                        c.footprint_pin_map.get(&pin.name).unwrap_or(&pin.name);
                                    (
                                        pin.name.clone(),
                                        if asset.pads.iter().any(|p| p.number == *number) {
                                            number.clone()
                                        } else {
                                            String::new()
                                        },
                                    )
                                })
                                .collect()
                        })
                        .unwrap_or_default(),
                    fabrication_ready: ready,
                    fabrication_detail: if ready {
                        String::new()
                    } else {
                        i18n::t("electronics.footprint.unsupported", self.language)
                    },
                }
            })
            .collect())
    }
    pub fn footprint_target(&self) -> Option<(Uuid, String, String)> {
        let selection = self.selection?;
        let component = self
            .schematic
            .components
            .iter()
            .find(|c| c.id == selection.source_id)?;
        Some((
            component.id,
            component.designator.clone(),
            component.footprint.clone(),
        ))
    }

    fn footprint_mapping(
        &self,
        target: Uuid,
        asset: &FootprintAsset,
    ) -> Result<Vec<(String, String)>, String> {
        let component = self
            .schematic
            .components
            .iter()
            .find(|c| c.id == target)
            .ok_or_else(|| i18n::t("electronics.footprint.target_missing", self.language))?;
        let mapping = component
            .pins
            .iter()
            .map(|pin| {
                (
                    pin.name.clone(),
                    component
                        .footprint_pin_map
                        .get(&pin.name)
                        .cloned()
                        .unwrap_or_else(|| pin.name.clone()),
                )
            })
            .collect::<Vec<_>>();
        Self::validate_footprint_mapping(component, asset, &mapping, self.language)?;
        Ok(mapping)
    }

    fn validate_footprint_mapping(
        component: &raf_electronics::component::ElectronicComponent,
        asset: &FootprintAsset,
        mapping: &[(String, String)],
        language: Language,
    ) -> Result<(), String> {
        if mapping.len() != component.pins.len()
            || component
                .pins
                .iter()
                .any(|pin| mapping.iter().filter(|(name, _)| name == &pin.name).count() != 1)
            || mapping
                .iter()
                .any(|(name, _)| !component.pins.iter().any(|pin| &pin.name == name))
        {
            return Err(i18n::t("electronics.footprint.unmapped_pin", language));
        }
        let distinct = mapping
            .iter()
            .map(|(_, pad)| pad)
            .collect::<std::collections::BTreeSet<_>>();
        if distinct.len() != mapping.len()
            || mapping.iter().any(|(_, number)| {
                number.is_empty()
                    || !asset.pads.iter().any(|p| {
                        p.number == *number
                            && !matches!(p.technology, FootprintPadTechnology::NonPlatedThroughHole)
                    })
            })
        {
            return Err(i18n::t("electronics.footprint.unmapped_pin", language));
        }
        for pad in &asset.pads {
            if matches!(pad.technology, FootprintPadTechnology::NonPlatedThroughHole) {
                continue;
            }
            if !mapping.iter().any(|(_, number)| number == &pad.number) {
                return Err(format!(
                    "{}: {}",
                    i18n::t("electronics.footprint.unmapped_pad", language),
                    pad.number
                ));
            }
        }
        Ok(())
    }

    pub fn apply_footprint_assignment_command(&mut self, payload: &str) -> bool {
        if let Err(error) = self.execute_footprint_assignment(payload) {
            self.push_surface_error(error);
        }
        true
    }

    pub fn execute_footprint_assignment(&mut self, payload: &str) -> Result<bool, String> {
        let before = (
            schematic_fingerprint(&self.schematic),
            pcb_fingerprint(&self.pcb),
        );
        (|| -> Result<(), String> {
            let mut fields = payload.splitn(3, ':');
            let target = fields.next().ok_or("Invalid footprint assignment")?;
            let encoded = fields.next().ok_or("Invalid footprint assignment")?;
            let target = Uuid::parse_str(target).map_err(|e| e.to_string())?;
            if encoded.len() % 2 != 0 || encoded.len() > 8192 {
                return Err("Invalid footprint reference".into());
            }
            let bytes: Result<Vec<_>, _> = encoded
                .as_bytes()
                .chunks_exact(2)
                .map(|p| {
                    std::str::from_utf8(p)
                        .map_err(|e| e.to_string())
                        .and_then(|s| u8::from_str_radix(s, 16).map_err(|e| e.to_string()))
                })
                .collect();
            let id = String::from_utf8(bytes?).map_err(|e| e.to_string())?;
            let asset = if let Some(asset) = self.pcb.footprint_assets.get(&id) {
                asset.clone()
            } else if let Some(asset) = raf_electronics::pcb::builtin_footprints::assets()
                .into_iter()
                .find(|asset| asset.id == id)
            {
                asset
            } else {
                let root = self
                    .footprint_project_root
                    .as_ref()
                    .ok_or("No mounted project")?;
                FootprintLibrary::open(root)
                    .map_err(|e| e.to_string())?
                    .load(&id)
                    .map_err(|e| e.to_string())?
                    .ok_or("Footprint asset not found")?
            };
            if let Some(encoded_mapping) = fields.next() {
                if encoded_mapping.len() % 2 != 0 || encoded_mapping.len() > 32768 {
                    return Err("Invalid pin mapping".into());
                }
                let raw: Result<Vec<_>, _> = encoded_mapping
                    .as_bytes()
                    .chunks_exact(2)
                    .map(|p| {
                        std::str::from_utf8(p)
                            .map_err(|e| e.to_string())
                            .and_then(|s| u8::from_str_radix(s, 16).map_err(|e| e.to_string()))
                    })
                    .collect();
                let mapping: Vec<(String, String)> =
                    serde_json::from_slice(&raw?).map_err(|e| e.to_string())?;
                self.assign_physical_footprint_with_mapping(target, asset, Some(mapping))
            } else {
                self.assign_physical_footprint(target, asset)
            }
        })()?;
        Ok(before
            != (
                schematic_fingerprint(&self.schematic),
                pcb_fingerprint(&self.pcb),
            ))
    }

    pub fn assign_physical_footprint(
        &mut self,
        target: Uuid,
        asset: FootprintAsset,
    ) -> Result<(), String> {
        self.assign_physical_footprint_with_mapping(target, asset, None)
    }

    pub fn assign_physical_footprint_with_mapping(
        &mut self,
        target: Uuid,
        asset: FootprintAsset,
        mapping: Option<Vec<(String, String)>>,
    ) -> Result<(), String> {
        let component = self
            .schematic
            .components
            .iter()
            .find(|c| c.id == target)
            .ok_or_else(|| i18n::t("electronics.footprint.target_missing", self.language))?;
        if component.locked
            || self
                .pcb
                .components
                .iter()
                .any(|c| c.component_id == target && c.locked)
        {
            return Err(i18n::t(
                "electronics.footprint.target_locked",
                self.language,
            ));
        }
        if !asset.is_fabrication_ready() {
            return Err(i18n::t("electronics.footprint.unsupported", self.language));
        }
        let mapping = mapping.unwrap_or_else(|| {
            component
                .pins
                .iter()
                .map(|pin| (pin.name.clone(), pin.name.clone()))
                .collect()
        });
        Self::validate_footprint_mapping(component, &asset, &mapping, self.language)?;
        let mapping: std::collections::BTreeMap<_, _> = mapping.into_iter().collect();
        if component.footprint == asset.id
            && component.footprint_pin_map == mapping
            && self.pcb.footprint_assets.get(&asset.id) == Some(&asset)
        {
            return Ok(());
        }
        if self
            .pcb
            .footprint_assets
            .get(&asset.id)
            .is_some_and(|existing| existing != &asset)
            && self
                .pcb
                .components
                .iter()
                .any(|c| c.component_id != target && c.footprint == asset.id)
        {
            return Err("This footprint is used by other components. Import the changed geometry under a new name.".into());
        }
        let before = self.snapshot();
        let affected_traces = self.pcb.traces_touching_component(target);
        // Existing routes incident on displaced pads need an explicit reroute.
        let mut retained = Vec::new();
        for trace in self.pcb.traces.drain(..) {
            let affected = affected_traces.contains(&trace.id);
            if affected {
                self.pcb.removed_traces.push(trace);
            } else {
                retained.push(trace);
            }
        }
        self.pcb.traces = retained;
        let id = asset.id.clone();
        self.pcb.footprint_assets.insert(id.clone(), asset);
        let component = self
            .schematic
            .components
            .iter_mut()
            .find(|c| c.id == target)
            .expect("validated target");
        component.footprint = id;
        component.footprint_pin_map = mapping;
        self.pcb.sync_from_schematic(&self.schematic);
        if let Some(placement) = self
            .pcb
            .components
            .iter_mut()
            .find(|c| c.component_id == target)
        {
            placement.image_asset = None;
        }
        self.history.record(before, &self.snapshot());
        self.dirty = true;
        if self.pcb_grid_step > 1.0 {
            self.pcb_grid_step = 0.5;
        }
        self.grid_step = if self.active_surface == CadSurfaceKind::Pcb {
            self.pcb_grid_step
        } else {
            self.grid_step
        };
        self.rebuild_scene();
        if self.active_surface == CadSurfaceKind::Pcb
            && self.selection.is_some_and(|s| s.source_id == target)
        {
            self.recenter_on_selection();
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use raf_electronics::component::ElectronicComponent;
    use raf_electronics::pcb::{parse_kicad_footprint, PcbTrace};

    fn physical_asset() -> FootprintAsset {
        parse_kicad_footprint(
            r#"(footprint "R_Test_Real"
          (version 20240108) (layer "F.Cu")
          (pad "2" smd rect (at 1.2 0) (size 1 1.4) (layers "F.Cu" "F.Mask" "F.Paste"))
          (pad "1" smd rect (at -1.2 0) (size 1 1.4) (layers "F.Cu" "F.Mask" "F.Paste"))
        )"#,
        )
        .expect("physical fixture")
    }

    fn editor() -> NativeElectronicsEditor {
        let mut editor = NativeElectronicsEditor::empty("Footprints");
        editor
            .schematic
            .add_component(ElectronicComponent::resistor("10k"));
        editor.pcb.sync_from_schematic(&editor.schematic);
        editor.rebuild_scene();
        editor
    }

    #[test]
    fn empty_project_offers_bundled_physical_patterns_without_network_or_fake_holder() {
        let root = std::env::temp_dir().join(format!("raf-builtin-test-{}", Uuid::new_v4()));
        let mut editor = editor();
        editor.footprint_project_root = Some(root.clone());
        let browsing = editor.footprint_candidates().unwrap();
        assert!(
            !browsing.is_empty(),
            "The physical library is browsable without a selected part"
        );
        assert!(
            browsing.iter().all(|c| !c.compatible),
            "Browsing does not imply an assignment target"
        );
        assert!(editor.selection().is_none());
        editor.select_component(0);
        let candidates = editor.footprint_candidates().unwrap();
        assert!(candidates
            .iter()
            .any(|c| c.name == "Raf_R_0805_Generic" && c.fabrication_ready));
        assert!(candidates.iter().all(|c| c.name != "BAT-18650"));
        let asset = raf_electronics::pcb::builtin_footprints::assets().remove(0);
        let target = editor.schematic.components[0].id;
        editor.assign_physical_footprint(target, asset).unwrap();
        assert!(editor.pcb_has_physical_footprint(target));
        if root.exists() {
            std::fs::remove_dir_all(root).unwrap();
        }
    }

    #[test]
    fn physical_assignment_is_portable_and_undo_restores_routes_and_reference() {
        let mut editor = editor();
        let target = editor.schematic.components[0].id;
        let position = editor.pcb.components[0].position;
        let old_footprint = editor.schematic.components[0].footprint.clone();
        let old_pad = editor.pcb.pad_world_position(0, 0).unwrap();
        let trace = PcbTrace {
            id: Uuid::new_v4(),
            net: editor.pcb.components[0].pad_nets[0].clone(),
            layer: raf_electronics::pcb::PcbLayer::TopCopper,
            width: 0.25,
            points: vec![old_pad, old_pad + Vec2::new(8.0, 0.0)],
        };
        editor.pcb.traces.push(trace.clone());
        let asset = physical_asset();
        editor
            .assign_physical_footprint(target, asset.clone())
            .unwrap();
        assert_eq!(editor.history.len(), 1);
        assert!(editor.pcb_has_physical_footprint(target));
        assert_eq!(editor.pcb.components[0].position, position);
        assert_eq!(editor.pcb.removed_traces, vec![trace.clone()]);
        assert!(editor.pcb.traces.is_empty());
        assert_eq!(editor.schematic.components[0].value, "10k");
        let serialized = ron::to_string(&editor.pcb).unwrap();
        let portable: PcbLayout = ron::from_str(&serialized).unwrap();
        assert_eq!(portable.footprint_assets.get(&asset.id), Some(&asset));
        assert_eq!(
            portable.pad_world_position(0, 0),
            Some(position + Vec2::new(1.2, 0.0))
        );
        assert!(editor.undo());
        assert_eq!(editor.schematic.components[0].footprint, old_footprint);
        assert_eq!(editor.pcb.traces, vec![trace]);
        assert!(editor.pcb.footprint_assets.is_empty());
        assert!(editor.redo());
        assert!(editor.pcb_has_physical_footprint(target));
    }

    #[test]
    fn invalid_mapping_and_locked_target_leave_documents_and_history_unchanged() {
        let mut editor = editor();
        let target = editor.schematic.components[0].id;
        let before = pcb_fingerprint(&editor.pcb);
        let mut asset = physical_asset();
        asset.pads[0].number = "3".into();
        assert!(editor.assign_physical_footprint(target, asset).is_err());
        editor.schematic.components[0].locked = true;
        assert!(editor
            .assign_physical_footprint(target, physical_asset())
            .is_err());
        assert_eq!(before, pcb_fingerprint(&editor.pcb));
        assert!(editor.history.is_empty());
    }

    #[test]
    fn assignment_uses_explicit_target_when_selection_changes() {
        let mut editor = editor();
        let first = editor.schematic.components[0].id;
        editor
            .schematic
            .add_component(ElectronicComponent::resistor("1k"));
        editor.pcb.sync_from_schematic(&editor.schematic);
        editor.select_component(1);
        let second_footprint = editor.schematic.components[1].footprint.clone();
        let asset = physical_asset();
        editor
            .assign_physical_footprint(first, asset.clone())
            .unwrap();
        assert_eq!(editor.schematic.components[0].footprint, asset.id);
        assert_eq!(editor.schematic.components[1].footprint, second_footprint);
        editor.assign_physical_footprint(first, asset).unwrap();
        assert_eq!(editor.history.len(), 1, "same assignment is a no-op");
    }
}
