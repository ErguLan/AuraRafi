//! Tool registry - exposes every engine operation as an invocable tool.
//!
//! This enables AI agents to discover and call engine functions
//! through a standardized JSON schema interface.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Definition of a callable tool (engine operation).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolDefinition {
    /// Machine-readable name (e.g. "create_entity").
    pub name: String,
    /// Category for grouping.
    pub category: String,
    /// Human-readable description.
    pub description: String,
    /// Parameters with their types and descriptions.
    pub parameters: Vec<ToolParameter>,
    /// Return type description.
    pub returns: String,
}

/// A parameter for a tool.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolParameter {
    pub name: String,
    pub param_type: String,
    pub description: String,
    pub required: bool,
    /// Possible enum values (if applicable).
    pub enum_values: Option<Vec<String>>,
}

/// Registry of all available tools. Populated at engine startup.
pub struct ToolRegistry {
    tools: HashMap<String, ToolDefinition>,
}

impl ToolRegistry {
    pub fn new() -> Self {
        Self {
            tools: HashMap::new(),
        }
    }

    /// Register a tool definition.
    pub fn register(&mut self, tool: ToolDefinition) {
        tracing::debug!("Registered tool: {}", tool.name);
        self.tools.insert(tool.name.clone(), tool);
    }

    /// Get a tool by name.
    pub fn get(&self, name: &str) -> Option<&ToolDefinition> {
        self.tools.get(name)
    }

    /// List all registered tools.
    pub fn list(&self) -> Vec<&ToolDefinition> {
        self.tools.values().collect()
    }

    /// List tools by category.
    pub fn by_category(&self, category: &str) -> Vec<&ToolDefinition> {
        self.tools
            .values()
            .filter(|t| t.category == category)
            .collect()
    }

    /// Export all tools as JSON schema (for AI consumption).
    pub fn to_json_schema(&self) -> serde_json::Value {
        let tools: Vec<&ToolDefinition> = self.tools.values().collect();
        serde_json::json!({
            "tools": tools
        })
    }

    /// Create a registry with default engine tools.
    pub fn with_defaults() -> Self {
        let mut registry = Self::new();

        // Scene tools
        registry.register(ToolDefinition {
            name: "create_entity".to_string(),
            category: "scene".to_string(),
            description: "Create a new entity in the scene".to_string(),
            parameters: vec![
                ToolParameter {
                    name: "name".to_string(),
                    param_type: "string".to_string(),
                    description: "Name for the entity".to_string(),
                    required: true,
                    enum_values: None,
                },
                ToolParameter {
                    name: "entity_type".to_string(),
                    param_type: "string".to_string(),
                    description: "Type of entity to create".to_string(),
                    required: true,
                    enum_values: Some(vec![
                        "cube".into(), "sphere".into(), "cylinder".into(),
                        "plane".into(), "sprite".into(), "light".into(),
                        "camera".into(), "empty".into(),
                    ]),
                },
            ],
            returns: "Entity ID".to_string(),
        });

        registry.register(ToolDefinition {
            name: "set_property".to_string(),
            category: "scene".to_string(),
            description: "Set a property on an entity".to_string(),
            parameters: vec![
                ToolParameter {
                    name: "entity_id".to_string(),
                    param_type: "string".to_string(),
                    description: "Target entity ID".to_string(),
                    required: true,
                    enum_values: None,
                },
                ToolParameter {
                    name: "property".to_string(),
                    param_type: "string".to_string(),
                    description: "Property name".to_string(),
                    required: true,
                    enum_values: Some(vec![
                        "position".into(), "rotation".into(), "scale".into(),
                        "name".into(), "visible".into(), "color".into(),
                    ]),
                },
                ToolParameter {
                    name: "value".to_string(),
                    param_type: "any".to_string(),
                    description: "New value".to_string(),
                    required: true,
                    enum_values: None,
                },
            ],
            returns: "Success boolean".to_string(),
        });

        registry.register(ToolDefinition {
            name: "import_asset".to_string(),
            category: "asset".to_string(),
            description: "Import an asset file into the project".to_string(),
            parameters: vec![
                ToolParameter {
                    name: "path".to_string(),
                    param_type: "string".to_string(),
                    description: "File path to import".to_string(),
                    required: true,
                    enum_values: None,
                },
            ],
            returns: "Asset ID".to_string(),
        });

        registry.register(ToolDefinition {
            name: "place_component".to_string(),
            category: "electronics".to_string(),
            description: "Place an electronic component on the schematic".to_string(),
            parameters: vec![
                ToolParameter {
                    name: "component_type".to_string(),
                    param_type: "string".to_string(),
                    description: "Type of component".to_string(),
                    required: true,
                    enum_values: Some(vec![
                        "resistor".into(), "capacitor".into(), "led".into(),
                        "transistor".into(), "ic".into(),
                    ]),
                },
                ToolParameter {
                    name: "value".to_string(),
                    param_type: "string".to_string(),
                    description: "Component value (e.g. 10k, 100nF)".to_string(),
                    required: false,
                    enum_values: None,
                },
                ToolParameter {
                    name: "position".to_string(),
                    param_type: "vec2".to_string(),
                    description: "Position on schematic grid".to_string(),
                    required: false,
                    enum_values: None,
                },
            ],
            returns: "Component ID".to_string(),
        });

        registry.register(ToolDefinition {
            name: "run_electrical_test".to_string(),
            category: "electronics".to_string(),
            description: "Run electrical connectivity test on the schematic".to_string(),
            parameters: vec![],
            returns: "List of issues".to_string(),
        });

        registry.register(ToolDefinition {
            name: "add_wire".to_string(),
            category: "electronics".to_string(),
            description: "Connect two points on the schematic with a wire".to_string(),
            parameters: vec![
                ToolParameter {
                    name: "start".to_string(),
                    param_type: "vec2".to_string(),
                    description: "Start position on schematic grid".to_string(),
                    required: true,
                    enum_values: None,
                },
                ToolParameter {
                    name: "end".to_string(),
                    param_type: "vec2".to_string(),
                    description: "End position on schematic grid".to_string(),
                    required: true,
                    enum_values: None,
                },
                ToolParameter {
                    name: "net_name".to_string(),
                    param_type: "string".to_string(),
                    description: "Net label for the wire".to_string(),
                    required: false,
                    enum_values: None,
                },
            ],
            returns: "Wire ID".to_string(),
        });

        registry.register(ToolDefinition {
            name: "remove_component".to_string(),
            category: "electronics".to_string(),
            description: "Remove an electronic component by its designator".to_string(),
            parameters: vec![
                ToolParameter {
                    name: "designator".to_string(),
                    param_type: "string".to_string(),
                    description: "Component designator (e.g. R1, C2)".to_string(),
                    required: true,
                    enum_values: None,
                },
            ],
            returns: "Success boolean".to_string(),
        });

        registry.register(ToolDefinition {
            name: "set_component_value".to_string(),
            category: "electronics".to_string(),
            description: "Change the value of an electronic component".to_string(),
            parameters: vec![
                ToolParameter {
                    name: "designator".to_string(),
                    param_type: "string".to_string(),
                    description: "Component designator".to_string(),
                    required: true,
                    enum_values: None,
                },
                ToolParameter {
                    name: "value".to_string(),
                    param_type: "string".to_string(),
                    description: "New value (e.g. 10k, 100nF)".to_string(),
                    required: true,
                    enum_values: None,
                },
            ],
            returns: "Success boolean".to_string(),
        });

        registry.register(ToolDefinition {
            name: "rotate_component".to_string(),
            category: "electronics".to_string(),
            description: "Rotate an electronic component by 90 degrees".to_string(),
            parameters: vec![
                ToolParameter {
                    name: "designator".to_string(),
                    param_type: "string".to_string(),
                    description: "Component designator".to_string(),
                    required: true,
                    enum_values: None,
                },
            ],
            returns: "New rotation angle".to_string(),
        });

        registry.register(ToolDefinition {
            name: "run_simulation".to_string(),
            category: "electronics".to_string(),
            description: "Run DC simulation on the current schematic".to_string(),
            parameters: vec![],
            returns: "Simulation results with node voltages and branch currents".to_string(),
        });

        registry.register(ToolDefinition {
            name: "get_simulation_results".to_string(),
            category: "electronics".to_string(),
            description: "Get the results of the last DC simulation".to_string(),
            parameters: vec![],
            returns: "Node voltages and component currents".to_string(),
        });

        registry.register(ToolDefinition {
            name: "run_drc".to_string(),
            category: "electronics".to_string(),
            description: "Run Design Rule Check on the schematic".to_string(),
            parameters: vec![],
            returns: "DRC report with errors, warnings, and info".to_string(),
        });

        registry.register(ToolDefinition {
            name: "export_schematic".to_string(),
            category: "electronics".to_string(),
            description: "Export schematic in the specified format".to_string(),
            parameters: vec![
                ToolParameter {
                    name: "format".to_string(),
                    param_type: "string".to_string(),
                    description: "Export format".to_string(),
                    required: true,
                    enum_values: Some(vec![
                        "netlist".into(), "bom_csv".into(), "svg".into(),
                    ]),
                },
            ],
            returns: "Exported content string".to_string(),
        });

        registry.register(ToolDefinition {
            name: "generate_bom".to_string(),
            category: "electronics".to_string(),
            description: "Generate Bill of Materials in CSV format".to_string(),
            parameters: vec![],
            returns: "BOM CSV content".to_string(),
        });

        registry.register(ToolDefinition {
            name: "get_netlist".to_string(),
            category: "electronics".to_string(),
            description: "Generate and return the netlist of the current schematic".to_string(),
            parameters: vec![],
            returns: "Netlist with nets and component connections".to_string(),
        });

        registry.register(ToolDefinition {
            name: "export_project".to_string(),
            category: "project".to_string(),
            description: "Export the project".to_string(),
            parameters: vec![
                ToolParameter {
                    name: "format".to_string(),
                    param_type: "string".to_string(),
                    description: "Export format".to_string(),
                    required: true,
                    enum_values: Some(vec![
                        "netlist".into(), "bom".into(), "svg".into(), "executable".into(),
                    ]),
                },
            ],
            returns: "Export path".to_string(),
        });

        registry
    }
}

impl Default for ToolRegistry {
    fn default() -> Self {
        Self::with_defaults()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_registry_has_tools() {
        let registry = ToolRegistry::with_defaults();
        assert!(!registry.list().is_empty());
        assert!(registry.get("create_entity").is_some());
        assert!(registry.get("place_component").is_some());
    }
}
