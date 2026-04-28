//! Open-source extension hooks for electronics.
//!
//! This keeps user-defined components and electrical rules out of the core
//! schematic/editor code while still letting them plug into the engine.

use std::sync::{OnceLock, RwLock};

use crate::drc::DrcIssue;
use crate::library::{ComponentLibrary, ComponentTemplate};
use crate::schematic::Schematic;

/// A custom electrical validation rule that can be injected by source mods.
pub trait ElectricalRule: Send + Sync {
    /// Stable identifier for logs and diagnostics.
    fn id(&self) -> &str;

    /// Run the rule against a schematic and return any findings.
    fn check(&self, schematic: &Schematic) -> Vec<DrcIssue>;
}

/// Lightweight summary for UI/debugging.
#[derive(Debug, Clone, Copy, Default)]
pub struct ElectricalExtensionSummary {
    pub component_templates: usize,
    pub drc_rules: usize,
}

/// Registry for electronics-specific additions.
#[derive(Default)]
pub struct ElectricalExtensionRegistry {
    component_templates: Vec<ComponentTemplate>,
    drc_rules: Vec<Box<dyn ElectricalRule>>,
}

impl ElectricalExtensionRegistry {
    pub fn register_component_template(&mut self, template: ComponentTemplate) {
        self.component_templates.push(template);
    }

    pub fn register_drc_rule(&mut self, rule: Box<dyn ElectricalRule>) {
        self.drc_rules.push(rule);
    }

    pub fn extend_library(&self, library: &mut ComponentLibrary) {
        library
            .components
            .extend(self.component_templates.iter().cloned());
    }

    pub fn run_drc_rules(&self, schematic: &Schematic) -> Vec<DrcIssue> {
        let mut issues = Vec::new();
        for rule in &self.drc_rules {
            issues.extend(rule.check(schematic));
        }
        issues
    }

    pub fn summary(&self) -> ElectricalExtensionSummary {
        ElectricalExtensionSummary {
            component_templates: self.component_templates.len(),
            drc_rules: self.drc_rules.len(),
        }
    }

    #[cfg(test)]
    pub fn clear(&mut self) {
        self.component_templates.clear();
        self.drc_rules.clear();
    }
}

fn global_registry() -> &'static RwLock<ElectricalExtensionRegistry> {
    static REGISTRY: OnceLock<RwLock<ElectricalExtensionRegistry>> = OnceLock::new();
    REGISTRY.get_or_init(|| RwLock::new(ElectricalExtensionRegistry::default()))
}

/// Register a source-mod component template so it appears in the schematic library.
pub fn register_component_template(template: ComponentTemplate) {
    let mut registry = global_registry()
        .write()
        .expect("electrical extension registry poisoned");
    registry.register_component_template(template);
}

/// Register a source-mod DRC/ERC rule.
pub fn register_drc_rule(rule: Box<dyn ElectricalRule>) {
    let mut registry = global_registry()
        .write()
        .expect("electrical extension registry poisoned");
    registry.register_drc_rule(rule);
}

/// Merge registered extension components into a live library instance.
pub fn extend_library_with_registered_extensions(library: &mut ComponentLibrary) {
    let registry = global_registry()
        .read()
        .expect("electrical extension registry poisoned");
    registry.extend_library(library);
}

/// Execute all registered external DRC rules.
pub fn run_registered_drc_rules(schematic: &Schematic) -> Vec<DrcIssue> {
    let registry = global_registry()
        .read()
        .expect("electrical extension registry poisoned");
    registry.run_drc_rules(schematic)
}

/// Get counts for external electrical contributions.
pub fn registered_extension_summary() -> ElectricalExtensionSummary {
    let registry = global_registry()
        .read()
        .expect("electrical extension registry poisoned");
    registry.summary()
}

#[cfg(test)]
pub fn clear_registered_extensions() {
    let mut registry = global_registry()
        .write()
        .expect("electrical extension registry poisoned");
    registry.clear();
}