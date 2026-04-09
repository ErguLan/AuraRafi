//! Asset browser panel - view and manage project assets.

use egui::Ui;
use raf_assets::importer::AssetType;
use raf_core::config::Language;
use raf_core::i18n::t;

/// State for the asset browser panel.
pub struct AssetBrowserPanel {
    pub search_query: String,
    pub selected_filter: Option<AssetType>,
    /// Simulated asset entries for the MVP.
    pub entries: Vec<AssetEntry>,
}

/// A display entry in the asset browser.
pub struct AssetEntry {
    pub name: String,
    pub asset_type: AssetType,
    pub size_display: String,
}

impl Default for AssetBrowserPanel {
    fn default() -> Self {
        Self {
            search_query: String::new(),
            selected_filter: None,
            entries: Vec::new(),
        }
    }
}

impl AssetBrowserPanel {
    /// Draw the asset browser panel.
    pub fn show(&mut self, ui: &mut Ui, lang: Language) {
        ui.horizontal(|ui| {
            ui.label(t("app.search", lang));
            ui.text_edit_singleline(&mut self.search_query);

            if ui.button(t("app.all", lang)).clicked() {
                self.selected_filter = None;
            }
            if ui.button(t("app.images", lang)).clicked() {
                self.selected_filter = Some(AssetType::Image);
            }
            if ui.button(t("app.models", lang)).clicked() {
                self.selected_filter = Some(AssetType::Model3D);
            }
            if ui.button(t("app.audio", lang)).clicked() {
                self.selected_filter = Some(AssetType::Audio);
            }
        });

        ui.separator();

        // Asset grid/list.
        egui::ScrollArea::horizontal().show(ui, |ui| {
            ui.horizontal(|ui| {
                let filtered: Vec<&AssetEntry> = self
                    .entries
                    .iter()
                    .filter(|e| {
                        if let Some(filter) = &self.selected_filter {
                            if e.asset_type != *filter {
                                return false;
                            }
                        }
                        if !self.search_query.is_empty() {
                            return e
                                .name
                                .to_lowercase()
                                .contains(&self.search_query.to_lowercase());
                        }
                        true
                    })
                    .collect();

                if filtered.is_empty() {
                    ui.label(t("app.no_assets", lang));
                } else {
                    for entry in filtered {
                        let type_label = match entry.asset_type {
                            AssetType::Image => "[IMG]",
                            AssetType::Model3D => "[3D]",
                            AssetType::Audio => "[SFX]",
                            AssetType::Scene => "[SCN]",
                            AssetType::Unknown => "[???]",
                        };
                        ui.group(|ui| {
                            ui.vertical(|ui| {
                                ui.label(type_label);
                                ui.label(&entry.name);
                                ui.label(&entry.size_display);
                            });
                        });
                    }
                }
            });
        });
    }
}
