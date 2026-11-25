//! Component library access and canvas artwork resolution.

use super::*;

impl NativeElectronicsEditor {
    pub fn library(&self) -> &ComponentLibrary {
        &self.library
    }

    /// Re-read external component templates from `<project_path>/ElectricalAssets/`
    /// and merge them into the live library. Safe to call after the editor is
    /// already open; touch the UI revision so the navigator refreshes.
    ///
    /// The library is rebuilt from its built-in baseline first, so repeated
    /// refreshes replace project entries instead of stacking duplicates.
    pub fn refresh_library(&mut self, project_path: &std::path::Path) {
        self.library = ComponentLibrary::default_library();
        self.library.load_external_assets_from(project_path);
        // Indexes are positional; reloading can reorder project entries, so a
        // previously armed placement must not silently point at another part.
        self.placement_template = None;
        self.touch_ui();
    }

    pub fn component_asset_key(&self, source_id: Uuid) -> &'static str {
        self.schematic
            .components
            .iter()
            .find(|component| component.id == source_id)
            .map(|component| component_asset_key(component.kind_label()))
            .unwrap_or("electronics://symbols/generic.png")
    }

    /// Schematic canvas artwork for a component body. Schematic symbols are
    /// white ink tinted by the overlay; the palette icons live in the rails.
    pub fn schematic_artwork_key(&self, source_id: Uuid) -> &'static str {
        self.component_asset_key(source_id)
    }

    /// PCB canvas artwork for a placement body, resolved through the footprint
    /// the layout assigned at sync time.
    pub fn pcb_artwork_key(&self, source_id: Uuid) -> &'static str {
        self.pcb
            .components
            .iter()
            .find(|placement| placement.component_id == source_id)
            .map(|placement| pcb_asset_key_for_footprint(&placement.footprint))
            .unwrap_or("electronics://footprints/generic.png")
    }

    /// Imported footprints are rendered from physical geometry, with no
    /// generic illustration covering copper or the component orientation.
    pub fn pcb_has_physical_footprint(&self, source_id: Uuid) -> bool {
        self.pcb.components.iter().any(|placement| {
            placement.component_id == source_id
                && self.pcb.footprint_assets.contains_key(&placement.footprint)
        })
    }
}

pub(super) fn component_asset_key(kind_label: &str) -> &'static str {
    match kind_label {
        "Resistor" => "electronics://symbols/resistor.png",
        "Capacitor" => "electronics://symbols/capacitor.png",
        "LED" => "electronics://symbols/led.png",
        "Magnet" => "electronics://symbols/magnet.png",
        "Battery" => "electronics://symbols/battery.png",
        "Ground" => "electronics://symbols/ground.png",
        _ => "electronics://symbols/generic.png",
    }
}

/// Canvas artwork for a PCB placement body, keyed by its footprint id. The
/// copper pads stay live geometry behind the artwork, so the PNG only carries
/// the silhouette and the silkscreen.
fn pcb_asset_key_for_footprint(footprint: &str) -> &'static str {
    match footprint {
        "0805" => "electronics://footprints/0805.png",
        "MAG-10x5" => "electronics://footprints/MAG-10x5.png",
        "BAT-18650" => "electronics://footprints/BAT-18650.png",
        "TP-GND" => "electronics://footprints/TP-GND.png",
        _ => "electronics://footprints/generic.png",
    }
}
