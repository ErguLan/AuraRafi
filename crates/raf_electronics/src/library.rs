//! Component library - built-in electronic parts and project templates.

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use crate::component::ElectronicComponent;

/// Conventional directory name that modders drop inside a project root to add
/// custom component templates and DRC rules. Resolved against a caller-provided
/// base path so the engine never depends on the current working directory.
pub const ELECTRICAL_ASSETS_DIR: &str = "ElectricalAssets";

/// Hard limit for one serialized project template. A custom symbol is bounded
/// by the domain model, so a file above this size is either corrupt or not a
/// component at all.
pub const MAX_CUSTOM_TEMPLATE_BYTES: usize = 256 * 1024;

/// Longest file slug generated from a component display name.
const MAX_TEMPLATE_SLUG_CHARS: usize = 64;

/// Base names Windows reserves for devices. A project file may not use them,
/// even with a `.ron` extension.
const RESERVED_FILE_STEMS: [&str; 22] = [
    "con", "prn", "aux", "nul", "com1", "com2", "com3", "com4", "com5", "com6", "com7", "com8",
    "com9", "lpt1", "lpt2", "lpt3", "lpt4", "lpt5", "lpt6", "lpt7", "lpt8", "lpt9",
];

/// Where a library entry came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TemplateSource {
    /// Shipped with the engine.
    #[default]
    BuiltIn,
    /// Authored in the project's `ElectricalAssets/` directory.
    Project,
}

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
    /// Provenance of this entry.
    pub source: TemplateSource,
    /// File backing a project entry, when it was loaded from disk.
    pub source_path: Option<PathBuf>,
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

    /// True when this template is owned by the project rather than the engine.
    pub fn is_project_template(&self) -> bool {
        self.source == TemplateSource::Project
    }
}

impl ComponentLibrary {
    /// Create a library with built-in basic components.
    pub fn default_library() -> Self {
        let mut library = Self {
            components: vec![
                builtin_template(
                    "Resistor",
                    "Passive",
                    "Standard resistor",
                    Some("library/resistor.png"),
                    vec!["resistor", "ohm", "passive"],
                    true,
                    ElectronicComponent::resistor("10k"),
                ),
                builtin_template(
                    "Capacitor",
                    "Passive",
                    "Standard capacitor",
                    Some("library/capacitor.png"),
                    vec!["capacitor", "cap", "passive"],
                    false,
                    ElectronicComponent::capacitor("100nF"),
                ),
                builtin_template(
                    "LED",
                    "Diode",
                    "Light-emitting diode",
                    Some("library/led.png"),
                    vec!["led", "diode", "light"],
                    true,
                    ElectronicComponent::led(),
                ),
                builtin_template(
                    "Magnet",
                    "Magnet",
                    "Electromagnetic component",
                    Some("library/magnet.png"),
                    vec!["magnet", "electromagnetic"],
                    false,
                    ElectronicComponent::magnet("0.5T"),
                ),
                builtin_template(
                    "Battery",
                    "Power",
                    "DC Voltage Source",
                    Some("library/battery.png"),
                    vec!["battery", "source", "power"],
                    true,
                    ElectronicComponent::dc_source(9.0),
                ),
                builtin_template(
                    "Ground",
                    "Power",
                    "0V Reference",
                    Some("library/ground.png"),
                    vec!["ground", "gnd", "0v"],
                    true,
                    ElectronicComponent::ground(),
                ),
            ],
        };

        library.load_registered_extensions();
        library
    }

    /// Load external components from a project-relative `ElectricalAssets/`
    /// directory. The base path is provided by the caller so the engine never
    /// falls back to the current working directory.
    ///
    /// Loading is idempotent: previously loaded project entries are replaced,
    /// files that collide with a built-in name are skipped and duplicate slugs
    /// are ignored, so refreshing after a save never stacks copies.
    ///
    /// Missing directories are ignored silently; use `export_default_templates_to`
    /// when you want to scaffold a new project.
    pub fn load_external_assets_from(&mut self, base: &Path) {
        self.components
            .retain(|template| template.source != TemplateSource::Project);
        let assets_dir = base.join(ELECTRICAL_ASSETS_DIR);
        if !assets_dir.is_dir() {
            return;
        }

        let Ok(entries) = std::fs::read_dir(&assets_dir) else {
            return;
        };
        let mut known_slugs: HashSet<String> = self
            .components
            .iter()
            .filter_map(|template| template_slug(&template.name))
            .collect();
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().and_then(|extension| extension.to_str()) != Some("ron") {
                continue;
            }
            let Ok(metadata) = entry.metadata() else {
                continue;
            };
            if metadata.len() as usize > MAX_CUSTOM_TEMPLATE_BYTES {
                continue;
            }
            let Ok(contents) = std::fs::read_to_string(&path) else {
                continue;
            };
            let Ok(template) = ron::from_str::<ElectronicComponent>(&contents) else {
                continue;
            };
            let file_stem = path
                .file_stem()
                .and_then(|stem| stem.to_str())
                .map(|stem| stem.to_string())
                .unwrap_or_else(|| template.designator.clone());
            let name = if template.library_name.trim().is_empty() {
                file_stem
            } else {
                template.library_name.trim().to_string()
            };
            let Some(slug) = template_slug(&name) else {
                continue;
            };
            if !known_slugs.insert(slug) {
                continue;
            }
            self.components.push(ComponentTemplate {
                name,
                category: template.category.clone(),
                description: format!("Loaded from {}", path.display()),
                icon_asset: None,
                keywords: Vec::new(),
                favorite: false,
                datasheet: template.datasheet.clone(),
                template,
                source: TemplateSource::Project,
                source_path: Some(path),
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
        for tmpl in self
            .components
            .iter()
            .filter(|template| template.source == TemplateSource::BuiltIn)
        {
            let ron_str =
                ron::ser::to_string_pretty(&tmpl.template, ron::ser::PrettyConfig::default())
                    .map_err(|error| std::io::Error::new(std::io::ErrorKind::InvalidData, error))?;
            let file_name = format!("{}.ron", tmpl.name.replace(' ', "_"));
            std::fs::write(assets_dir.join(file_name), ron_str)?;
        }
        Ok(())
    }

    /// Persists one custom component as `<base>/ElectricalAssets/<slug>.ron`.
    ///
    /// The template is validated before any write, built-in names are reserved,
    /// and the slug can never contain a path separator. Call
    /// [`Self::load_external_assets_from`] afterwards to refresh the live
    /// library; this method intentionally does not mutate `self` because the
    /// caller owns project lifetime and UI invalidation.
    pub fn save_custom_template(
        &self,
        base: &Path,
        template: &ElectronicComponent,
        overwrite: bool,
    ) -> Result<PathBuf, String> {
        template.validate_custom()?;
        let display_name = template_display_name(template);
        let slug = template_slug(&display_name)
            .ok_or_else(|| "the component name cannot be used as a file name".to_string())?;
        if slug.contains('/') || slug.contains('\\') || slug.contains("..") {
            return Err("the component name resolves outside ElectricalAssets".to_string());
        }
        if self.components.iter().any(|candidate| {
            candidate.source == TemplateSource::BuiltIn
                && template_slug(&candidate.name).as_deref() == Some(slug.as_str())
        }) {
            return Err(format!(
                "'{display_name}' collides with a built-in component"
            ));
        }
        let slug_exists = self.components.iter().any(|candidate| {
            candidate.source == TemplateSource::Project
                && template_slug(&candidate.name).as_deref() == Some(slug.as_str())
        });
        if slug_exists && !overwrite {
            return Err(format!("'{display_name}' already exists"));
        }

        let assets_dir = base.join(ELECTRICAL_ASSETS_DIR);
        let path = assets_dir.join(format!("{slug}.ron"));
        if path.parent() != Some(assets_dir.as_path()) {
            return Err("the component path escapes ElectricalAssets".to_string());
        }
        if path.exists() && !overwrite {
            return Err(format!("'{display_name}' already exists"));
        }

        let serialized = ron::ser::to_string_pretty(template, ron::ser::PrettyConfig::default())
            .map_err(|error| format!("could not serialize component: {error}"))?;
        if serialized.len() > MAX_CUSTOM_TEMPLATE_BYTES {
            return Err(format!(
                "the serialized component exceeds {MAX_CUSTOM_TEMPLATE_BYTES} bytes"
            ));
        }
        std::fs::create_dir_all(&assets_dir)
            .map_err(|error| format!("could not create ElectricalAssets: {error}"))?;
        if path.exists() {
            let assets_canonical = std::fs::canonicalize(&assets_dir)
                .map_err(|error| format!("ElectricalAssets is unavailable: {error}"))?;
            let path_canonical = std::fs::canonicalize(&path)
                .map_err(|error| format!("component file is unavailable: {error}"))?;
            if !path_canonical.starts_with(&assets_canonical) {
                return Err("the component path leaves ElectricalAssets".to_string());
            }
        }
        std::fs::write(&path, serialized)
            .map_err(|error| format!("could not write {}: {error}", path.display()))?;
        Ok(path)
    }

    /// Removes one project template file. Built-in entries are never removable
    /// and the canonicalized target must stay inside `ElectricalAssets/`.
    pub fn remove_custom_template(&self, base: &Path, name: &str) -> Result<(), String> {
        let template = self
            .components
            .iter()
            .find(|candidate| candidate.source == TemplateSource::Project && candidate.name == name)
            .ok_or_else(|| format!("'{name}' is not a project component"))?;
        let assets_dir = base.join(ELECTRICAL_ASSETS_DIR);
        let assets_canonical = std::fs::canonicalize(&assets_dir)
            .map_err(|error| format!("ElectricalAssets is unavailable: {error}"))?;
        let path = template.source_path.clone().unwrap_or_else(|| {
            let slug = template_slug(&template.name).unwrap_or_else(|| "component".to_string());
            assets_dir.join(format!("{slug}.ron"))
        });
        if path.extension().and_then(|extension| extension.to_str()) != Some("ron") {
            return Err("only project component files can be removed".to_string());
        }
        let canonical = std::fs::canonicalize(&path)
            .map_err(|error| format!("component file is unavailable: {error}"))?;
        if !canonical.starts_with(&assets_canonical) {
            return Err("the component file is outside ElectricalAssets".to_string());
        }
        std::fs::remove_file(&canonical)
            .map_err(|error| format!("could not remove {}: {error}", canonical.display()))
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

fn builtin_template(
    name: &str,
    category: &str,
    description: &str,
    icon_asset: Option<&'static str>,
    keywords: Vec<&str>,
    favorite: bool,
    template: ElectronicComponent,
) -> ComponentTemplate {
    ComponentTemplate {
        name: name.to_string(),
        category: category.to_string(),
        description: description.to_string(),
        icon_asset,
        keywords: keywords.into_iter().map(str::to_string).collect(),
        favorite,
        datasheet: None,
        template,
        source: TemplateSource::BuiltIn,
        source_path: None,
    }
}

fn template_display_name(template: &ElectronicComponent) -> String {
    let name = template.library_name.trim();
    if !name.is_empty() {
        return name.to_string();
    }
    let value = template.value.trim();
    if !value.is_empty() {
        return value.to_string();
    }
    template.designator.trim().to_string()
}

/// Stable, filesystem-safe slug. Only ASCII letters, digits and single dashes
/// survive, so the result can never contain a path component.
pub fn template_slug(name: &str) -> Option<String> {
    let mut slug = String::new();
    let mut pending_dash = false;
    for character in name.chars() {
        if character.is_ascii_alphanumeric() {
            if pending_dash && !slug.is_empty() {
                slug.push('-');
            }
            pending_dash = false;
            slug.push(character.to_ascii_lowercase());
            if slug.len() >= MAX_TEMPLATE_SLUG_CHARS {
                break;
            }
        } else {
            pending_dash = true;
        }
    }
    let slug = slug.trim_end_matches('-').to_string();
    if slug.is_empty() {
        return None;
    }
    if RESERVED_FILE_STEMS.contains(&slug.as_str()) {
        return Some(format!("component-{slug}"));
    }
    Some(slug)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::symbol::SymbolPreset;
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

    fn custom_component() -> ElectronicComponent {
        let mut custom = ElectronicComponent::resistor("47k");
        custom.designator = "U".to_string();
        custom.library_name = "Op Amp".to_string();
        custom.category = "Analog".to_string();
        custom.sim_model = crate::component::SimModel::Unmodeled;
        custom.symbol = Some(SymbolPreset::Passive.symbol_for(2));
        custom
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
        assert!(library
            .components
            .iter()
            .all(|template| template.source == TemplateSource::BuiltIn));
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

    #[test]
    fn save_custom_template_round_trips_without_duplicate_entries() {
        let base = unique_dir("roundtrip");
        let library = ComponentLibrary::default_library();
        let path = library
            .save_custom_template(&base, &custom_component(), false)
            .expect("save custom template");
        let assets_dir = base.join(ELECTRICAL_ASSETS_DIR);
        assert_eq!(path.parent(), Some(assets_dir.as_path()));
        assert!(path.is_file());

        let mut reloaded = ComponentLibrary::default_library();
        reloaded.load_external_assets_from(&base);
        reloaded.load_external_assets_from(&base);
        let matches = reloaded
            .components
            .iter()
            .filter(|template| template.name == "Op Amp")
            .count();
        assert_eq!(matches, 1);
        let entry = reloaded
            .components
            .iter()
            .find(|template| template.name == "Op Amp")
            .expect("project entry");
        assert_eq!(entry.source, TemplateSource::Project);
        entry
            .template
            .validate_custom()
            .expect("valid reloaded part");
        assert!(entry.template.symbol.is_some());

        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn save_custom_template_rejects_built_in_collisions_and_traversal() {
        let base = unique_dir("collision");
        let library = ComponentLibrary::default_library();

        let mut collision = custom_component();
        collision.library_name = "Resistor".to_string();
        assert!(library
            .save_custom_template(&base, &collision, true)
            .is_err());

        let mut traversal = custom_component();
        traversal.library_name = "../escape".to_string();
        let path = library
            .save_custom_template(&base, &traversal, false)
            .expect("slug sanitizes traversal");
        assert_eq!(
            path.file_name().and_then(|name| name.to_str()),
            Some("escape.ron")
        );
        assert!(path.starts_with(base.join(ELECTRICAL_ASSETS_DIR)));

        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn remove_custom_template_never_touches_built_ins() {
        let base = unique_dir("remove");
        let library = ComponentLibrary::default_library();
        assert!(library.remove_custom_template(&base, "Resistor").is_err());

        let mut reloaded = ComponentLibrary::default_library();
        let path = reloaded
            .save_custom_template(&base, &custom_component(), false)
            .expect("save custom template");
        reloaded.load_external_assets_from(&base);
        reloaded
            .remove_custom_template(&base, "Op Amp")
            .expect("project template is removable");
        assert!(!path.exists());

        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn template_slugs_are_stable_and_path_safe() {
        assert_eq!(template_slug("Op Amp"), Some("op-amp".to_string()));
        assert_eq!(template_slug("../escape"), Some("escape".to_string()));
        assert_eq!(template_slug("  "), None);
        assert_eq!(template_slug("A/B\\C"), Some("a-b-c".to_string()));
        assert_eq!(template_slug("CON"), Some("component-con".to_string()));
    }
}
