use serde::Deserialize;

// Keep the editor-facing names for compatibility, but make the core catalog
// types authoritative. This prevents the headless CLI/MCP and the in-editor
// Agent from drifting into two different schemas.
pub use raf_core::{
    CapabilityDefinition as CommandDefinition, CapabilityParameter as CommandParameter,
};

#[derive(Debug, Clone, Deserialize)]
pub struct CommandCatalog {
    pub version: u32,
    pub commands: Vec<CommandDefinition>,
}

impl CommandCatalog {
    pub fn load_builtin() -> Result<Self, String> {
        let raw = include_str!("../../../../assets/commands/catalog.json");
        serde_json::from_str(raw).map_err(|error| format!("command catalog: {error}"))
    }

    pub fn builtin() -> Self {
        Self::load_builtin().unwrap_or_else(|error| Self {
            version: 0,
            commands: vec![CommandDefinition {
                name: "help".to_string(),
                aliases: vec![],
                domain: "shared".to_string(),
                category: "system".to_string(),
                description_key: "commands.help.desc".to_string(),
                parameters: vec![],
                examples: vec![format!("/help # {error}")],
            }],
        })
    }

    pub fn command_names(&self) -> Vec<String> {
        let mut names = Vec::new();
        for command in &self.commands {
            names.push(format!("/{}", command.name));
            for alias in &command.aliases {
                names.push(format!("/{}", alias));
            }
        }
        names.sort();
        names.dedup();
        names
    }

    pub fn find(&self, name: &str) -> Option<&CommandDefinition> {
        let normalized = normalize_command_name(name);
        self.commands.iter().find(|command| {
            normalize_command_name(&command.name) == normalized
                || command
                    .aliases
                    .iter()
                    .any(|alias| normalize_command_name(alias) == normalized)
        })
    }

    pub fn by_domain<'a>(&'a self, domain: &'a str) -> impl Iterator<Item = &'a CommandDefinition> {
        self.commands
            .iter()
            .filter(move |command| command.domain == "shared" || command.domain == domain)
    }
}

fn normalize_command_name(name: &str) -> String {
    name.trim()
        .trim_start_matches('/')
        .to_ascii_lowercase()
        .replace('-', "_")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builtin_catalog_loads() {
        let catalog = CommandCatalog::load_builtin().unwrap();
        assert!(catalog.version >= 1);
        assert!(catalog.find("game.add").is_some());
        assert!(catalog.find("undo").is_some());
        assert!(catalog.find("redo").is_some());
        assert!(catalog.find("transaction.undo").is_some());
    }
}
