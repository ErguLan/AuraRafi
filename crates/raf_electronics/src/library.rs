//! Component library - built-in electronic parts.

use crate::component::ElectronicComponent;

/// Library of electronic components available for placement.
pub struct ComponentLibrary {
    pub components: Vec<ComponentTemplate>,
}

/// A component template in the library.
pub struct ComponentTemplate {
    pub name: String,
    pub category: String,
    pub description: String,
    /// Data-driven template component.
    pub template: ElectronicComponent,
}

impl ComponentTemplate {
    /// Create a new unique instance from this template.
    pub fn instantiate(&self) -> ElectronicComponent {
        let mut comp = self.template.clone();
        comp.id = uuid::Uuid::new_v4();
        for pin in &mut comp.pins {
            pin.id = uuid::Uuid::new_v4();
            pin.net = String::new();
        }
        comp
    }
}

impl ComponentLibrary {
    /// Create a library with built-in basic components.
    pub fn default_library() -> Self {
        Self {
            components: vec![
                ComponentTemplate {
                    name: "Resistor".to_string(),
                    category: "Passive".to_string(),
                    description: "Standard resistor".to_string(),
                    template: ElectronicComponent::resistor("10k"),
                },
                ComponentTemplate {
                    name: "Capacitor".to_string(),
                    category: "Passive".to_string(),
                    description: "Standard capacitor".to_string(),
                    template: ElectronicComponent::capacitor("100nF"),
                },
                ComponentTemplate {
                    name: "LED".to_string(),
                    category: "Diode".to_string(),
                    description: "Light-emitting diode".to_string(),
                    template: ElectronicComponent::led(),
                },
                ComponentTemplate {
                    name: "Magnet".to_string(),
                    category: "Magnet".to_string(),
                    description: "Electromagnetic component".to_string(),
                    template: ElectronicComponent::magnet("0.5T"),
                },
                ComponentTemplate {
                    name: "Battery".to_string(),
                    category: "Power".to_string(),
                    description: "DC Voltage Source".to_string(),
                    template: ElectronicComponent::dc_source(9.0),
                },
                ComponentTemplate {
                    name: "Ground".to_string(),
                    category: "Power".to_string(),
                    description: "0V Reference".to_string(),
                    template: ElectronicComponent::ground(),
                },
            ],
        }
    }

    /// Load external components from ElectricalAssets directory.
    pub fn load_external_assets(&mut self) {
        let assets_dir = std::path::Path::new("ElectricalAssets");
        if !assets_dir.exists() {
            let _ = std::fs::create_dir_all(assets_dir);
            // Export current defaults for modders to see
            for tmpl in &self.components {
                if let Ok(ron_str) = ron::ser::to_string_pretty(&tmpl.template, ron::ser::PrettyConfig::default()) {
                    let file_name = format!("{}.ron", tmpl.name.replace(" ", "_"));
                    let _ = std::fs::write(assets_dir.join(file_name), ron_str);
                }
            }
            return;
        }

        // Load all .ron files
        if let Ok(entries) = std::fs::read_dir(assets_dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.extension().and_then(|e| e.to_str()) == Some("ron") {
                    if let Ok(contents) = std::fs::read_to_string(&path) {
                        if let Ok(template) = ron::from_str::<ElectronicComponent>(&contents) {
                            let name = path.file_stem().unwrap().to_str().unwrap().to_string();
                            self.components.push(ComponentTemplate {
                                name,
                                category: template.category.clone(),
                                description: format!("Loaded from {}", path.display()),
                                template,
                            });
                        }
                    }
                }
            }
        }
    }

    /// Filter components by category.
    pub fn by_category(&self, category: &str) -> Vec<&ComponentTemplate> {
        self.components
            .iter()
            .filter(|c| c.category == category)
            .collect()
    }

    /// Search components by name.
    pub fn search(&self, query: &str) -> Vec<&ComponentTemplate> {
        let q = query.to_lowercase();
        self.components
            .iter()
            .filter(|c| c.name.to_lowercase().contains(&q))
            .collect()
    }
}
