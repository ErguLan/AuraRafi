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
    /// Factory function to create an instance.
    pub create: fn() -> ElectronicComponent,
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
                    create: || ElectronicComponent::resistor("10k"),
                },
                ComponentTemplate {
                    name: "Capacitor".to_string(),
                    category: "Passive".to_string(),
                    description: "Standard capacitor".to_string(),
                    create: || ElectronicComponent::capacitor("100nF"),
                },
                ComponentTemplate {
                    name: "LED".to_string(),
                    category: "Diode".to_string(),
                    description: "Light-emitting diode".to_string(),
                    create: ElectronicComponent::led,
                },
            ],
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
