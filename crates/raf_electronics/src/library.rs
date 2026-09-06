//! Component library - built-in electronic parts.

use std::path::{Path, PathBuf};

use crate::component::ElectronicComponent;

/// Conventional directory name that modders drop inside a project root to add
/// custom component templates and DRC rules. Resolved against a caller-provided
/// base path so the engine never depends on the current working directory.
pub const ELECTRICAL_ASSETS_DIR: &str = "ElectricalAssets";

/// Library of electronic components available for placement.
pub struct ComponentLibrary {
    pub components: Vec<ComponentTemplate>,
}

/// A component template in the library.
#[derive(Debug, Clone)]
pub struct ComponentTemplate {
    pub name: String,
    pub category: String,
    pub description: String,
    pub icon_asset: Option<&'static str>,
    pub keywords: Vec<String>,
    pub favorite: bool,
    pub datasheet: Option<String>,
    /// Data-driven template component.
    pub template: ElectronicComponent,
}

impl ComponentTemplate {
    /// Create a new unique instance from this template.
    pub fn instantiate(&self) -> ElectronicComponent {
        let mut comp = self.template.clone();
        comp.id = uuid::Uuid::new_v4();
        if comp.datasheet.is_none() {
            comp.datasheet = self.datasheet.clone();
        }
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
        let mut library = Self {
            components: vec![
                ComponentTemplate {
                    name: "Resistor".to_string(),
                    category: "Passive".to_string(),
                    description: "Standard resistor".to_string(),
                    icon_asset: Some("library/resistor.png"),
                    keywords: vec![
                        "resistor".to_string(),
                        "ohm".to_string(),
                        "passive".to_string(),
                    ],
                    favorite: true,
                    datasheet: None,
                    template: ElectronicComponent::resistor("10k"),
                },
                ComponentTemplate {
                    name: "Capacitor".to_string(),
                    category: "Passive".to_string(),
                    description: "Standard capacitor".to_string(),
                    icon_asset: Some("library/capacitor.png"),
                    keywords: vec![
                        "capacitor".to_string(),
                        "cap".to_string(),
                        "passive".to_string(),
                    ],
                    favorite: false,
                    datasheet: None,
                    template: ElectronicComponent::capacitor("100nF"),
                },
                ComponentTemplate {
                    name: "LED".to_string(),
                    category: "Diode".to_string(),
                    description: "Light-emitting diode".to_string(),
                    icon_asset: Some("library/led.png"),
                    keywords: vec!["led".to_string(), "diode".to_string(), "light".to_string()],
                    favorite: true,
                    datasheet: None,
                    template: ElectronicComponent::led(),
                },
                ComponentTemplate {
                    name: "Magnet".to_string(),
                    category: "Magnet".to_string(),
                    description: "Electromagnetic component".to_string(),
                    icon_asset: Some("library/magnet.png"),
                    keywords: vec!["magnet".to_string(), "electromagnetic".to_string()],
                    favorite: false,
                    datasheet: None,
                    template: ElectronicComponent::magnet("0.5T"),
                },
                ComponentTemplate {
                    name: "Battery".to_string(),
                    category: "Power".to_string(),
                    description: "DC Voltage Source".to_string(),
                    icon_asset: Some("library/battery.png"),
                    keywords: vec![
                        "battery".to_string(),
                        "source".to_string(),
                        "power".to_string(),
                    ],
                    favorite: true,
                    datasheet: None,
                    template: ElectronicComponent::dc_source(9.0),
                },
                ComponentTemplate {
                    name: "Ground".to_string(),
                    category: "Power".to_string(),
                    description: "0V Reference".to_string(),
                    icon_asset: Some("library/ground.png"),
                    keywords: vec!["ground".to_string(), "gnd".to_string(), "0v".to_string()],
                    favorite: true,
                    datasheet: None,
                    template: ElectronicComponent::ground(),
                },
            ],
        };

        library.load_registered_extensions();
        library
    }

    /// Load external components from a project-relative `ElectricalAssets/`
    /// directory. The base path is provided by the caller so the engine never
    /// falls back to the current working directory.
    ///
    /// Missing directories are ignored silently; use `export_default_templates_to`
    /// when you want to scaffold a new project.
    pub fn load_external_assets_from(&mut self, base: &Path) {
        let assets_dir = base.join(ELECTRICAL_ASSETS_DIR);
        if !assets_dir.is_dir() {
            return;
        }

        let Ok(entries) = std::fs::read_dir(&assets_dir) else {
            return;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) != Some("ron") {
                continue;
            }
            let Ok(contents) = std::fs::read_to_string(&path) else {
                continue;
            };
            let Ok(template) = ron::from_str::<ElectronicComponent>(&contents) else {
                continue;
            };
            let name = path
                .file_stem()
                .and_then(|stem| stem.to_str())
                .map(|s| s.to_string())
                .unwrap_or_else(|| template.designator.clone());
            self.components.push(ComponentTemplate {
                name,
                category: template.category.clone(),
                description: format!("Loaded from {}", path.display()),
                icon_asset: None,
                keywords: Vec::new(),
                favorite: false,
                datasheet: template.datasheet.clone(),
                template,
            });
        }
    }

    /// Backwards-compatible wrapper that resolves `ElectricalAssets/` from the
    /// current working directory. New callers should prefer
    /// `load_external_assets_from(&project_root)`.
    pub fn load_external_assets(&mut self) {
        self.load_external_assets_from(Path::new("."));
    }

    /// Write every built-in template as a `.ron` file inside
    /// `<base>/ElectricalAssets/`. Used once to scaffold a project so modders
    /// have a starting point. Missing directories are created.
    pub fn export_default_templates_to(&self, base: &Path) -> std::io::Result<()> {
        let assets_dir: PathBuf = base.join(ELECTRICAL_ASSETS_DIR);
        std::fs::create_dir_all(&assets_dir)?;
        for tmpl in &self.components {
            let ron_str =
                ron::ser::to_string_pretty(&tmpl.template, ron::ser::PrettyConfig::default())
                    .map_err(|error| std::io::Error::new(std::io::ErrorKind::InvalidData, error))?;
            let file_name = format!("{}.ron", tmpl.name.replace(' ', "_"));
            std::fs::write(assets_dir.join(file_name), ron_str)?;
        }
        Ok(())
    }

    /// Merge code-registered extensions into the library.
    pub fn load_registered_extensions(&mut self) {
        crate::extensions::extend_library_with_registered_extensions(self);
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
            .filter(|c| {
                c.name.to_lowercase().contains(&q)
                    || c.category.to_lowercase().contains(&q)
                    || c.keywords
                        .iter()
                        .any(|keyword| keyword.to_lowercase().contains(&q))
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::env;

    fn unique_dir(label: &str) -> PathBuf {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        let dir = env::temp_dir().join(format!(
            "rafi_library_test_{label}_{}_{}",
            std::process::id(),
            nanos
        ));
        std::fs::create_dir_all(&dir).expect("temp dir");
        dir
    }

    #[test]
    fn default_library_keeps_existing_component_set_only() {
        let library = ComponentLibrary::default_library();
        let names = library
            .components
            .iter()
            .map(|template| template.name.as_str())
            .collect::<Vec<_>>();

        assert_eq!(
            names,
            vec![
                "Resistor",
                "Capacitor",
                "LED",
                "Magnet",
                "Battery",
                "Ground"
            ]
        );
    }

    #[test]
    fn load_external_assets_from_uses_caller_provided_path() {
        let base = unique_dir("external");
        let assets_dir = base.join(ELECTRICAL_ASSETS_DIR);
        std::fs::create_dir_all(&assets_dir).expect("assets dir");

        let mut custom = ElectronicComponent::resistor("47k");
        custom.designator = "R?".to_string();
        custom.category = "Passive".to_string();
        let ron_str = ron::ser::to_string_pretty(&custom, ron::ser::PrettyConfig::default())
            .expect("serialize template");
        std::fs::write(assets_dir.join("Custom_Resistor.ron"), ron_str).expect("write template");

        let mut library = ComponentLibrary::default_library();
        let before = library.components.len();
        library.load_external_assets_from(&base);
        assert!(library.components.len() > before);
        assert!(library
            .components
            .iter()
            .any(|template| template.name == "Custom_Resistor"));

        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn load_external_assets_from_silently_skips_missing_dir() {
        let base = unique_dir("missing");
        std::fs::remove_dir_all(&base).ok();
        let mut library = ComponentLibrary::default_library();
        let before = library.components.len();
        library.load_external_assets_from(&base);
        assert_eq!(library.components.len(), before);
    }
}
