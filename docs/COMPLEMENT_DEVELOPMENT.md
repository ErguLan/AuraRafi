# AuraRafi Open Source Complements (Mods) API

Welcome to the **AuraRafi Modding System**. Instead of modifying the Engine's core loop, you can inject tools, panels, or background tasks using the **Complement API**.

## Core Philosophy
We recycle the exact same Tool API (JSON Schemas) used by the AI integrations (`OpenClaw`, `Claude`, etc.). 
If the AI can spawn an entity or place a resistor, **your complement can too**. 

## Creating a Complement

1. Create a `.rs` file in `crates/raf_render/src/complements/` (e.g. `complement_weather.rs`).
2. Implement the `EngineComplement` Trait explicitly declaring your domain (Game/Electronics) and presentation.

### 1. Minimal Implementation Example

```rust
use raf_core::complement::{
    ComplementContext, ComplementDomain, ComplementPresentation, EngineComplement,
};
use egui::Ui;

pub struct WeatherComplement {
    intensity: f32,
}

impl WeatherComplement {
    pub fn new() -> Self {
        Self { intensity: 0.5 }
    }
}

impl EngineComplement for WeatherComplement {
    fn id(&self) -> &str {
        "weather_mod"
    }

    fn name(&self) -> &str {
        "Weather Control"
    }

    fn domain(&self) -> ComplementDomain {
        // Only visible in Video Game projects
        ComplementDomain::Games 
    }

    fn presentation(&self) -> ComplementPresentation {
        // Renders alongside the AI Chat tab
        ComplementPresentation::BottomTab
    }

    fn draw_ui(&mut self, ui: &mut Ui, context: &mut ComplementContext) {
        ui.label("Rain Intensity:");
        if ui.add(egui::Slider::new(&mut self.intensity, 0.0..=1.0)).changed() {
            // Future implementation:
            // context.tools.call("set_weather", ...)
        }
    }
}
```

## Registering Your Complement

Once written, your complement must be registered. Keep an eye on `crates/raf_editor/src/app.rs` (or where the `ComplementRegistry` is initialized):

```rust
let mut registry = ComplementRegistry::new();
registry.register(Box::new(WeatherComplement::new()));
```

## New Electronics Mod Options

Complements are still the main open-source extension entry point, but Electronics mods now have another layer available.

If your complement targets circuits, schematics, or electronics workflows, you can now also inject:

- Extra component templates into the electrical library
- Extra DRC/ERC rules into the electrical validation pass

This means you no longer need to hard-patch built-in component lists or built-in rule functions just to add your own electrical theory, school rules, or project-specific parts.

## Easy Electronics Mod Flow

### 1. Create your complement

Make your Rust file and implement `EngineComplement` as usual.

Use `ComplementDomain::Electronics` if the mod should only appear for electronics projects.

### 2. Register electrical parts if needed

If your mod adds new parts, register them through `raf_electronics::register_component_template(...)`.

```rust
use raf_electronics::{
    register_component_template,
    ComponentTemplate,
    ElectronicComponent,
};

register_component_template(ComponentTemplate {
    name: "Thermistor NTC".to_string(),
    category: "Sensors".to_string(),
    description: "Added by open-source complement".to_string(),
    template: ElectronicComponent::resistor("10k"),
});
```

Those templates are merged into `ComponentLibrary` automatically when the schematic editor boots its library.

### 3. Register custom electrical rules if needed

If your mod adds extra validation logic, register a rule through `raf_electronics::register_drc_rule(...)`.

```rust
use raf_electronics::{register_drc_rule, DrcIssue, DrcSeverity, ElectricalRule, Schematic};

struct SchoolRule;

impl ElectricalRule for SchoolRule {
    fn id(&self) -> &str {
        "school_voltage_limit"
    }

    fn check(&self, schematic: &Schematic) -> Vec<DrcIssue> {
        let _ = schematic;
        vec![DrcIssue {
            severity: DrcSeverity::Info,
            rule: self.id().to_string(),
            message: "Example extra rule from a complement".to_string(),
            components: vec![],
            location: None,
        }]
    }
}

register_drc_rule(Box::new(SchoolRule));
```

When `run_drc(...)` executes, the engine now runs its internal checks first and then appends all registered external findings.

### 4. Use complements for UI, use electronics extensions for domain logic

The clean split now is:

- `EngineComplement`: panel, tab, floating window, lifecycle, per-domain visibility
- `raf_electronics` extension hooks: parts, rules, domain-specific electrical additions

That split exists so the editor UI does not become the storage place for community electrical logic.

## What Is Best For Each Mod Type?

- If you want a new panel or tool window: use a complement.
- If you want a new schematic component: register a `ComponentTemplate`.
- If you want a new ERC/DRC theory or validation rule: register an `ElectricalRule`.
- If you want pure data without code: ship `.ron` component files through `ElectricalAssets/`.

## Current Practical Limitation

Today the electrical extension registry is designed first for Rust/source mods inside the engine workspace.

So the easiest path right now is:

1. Rust complement for UI or lifecycle
2. `raf_electronics` registration for parts/rules
3. Optional `.ron` assets for data-driven content

## Presentation vs Domain Isolation
- **Domain Seclusion**: If a user loads an `Electronics` project, all `ComplementDomain::Games` tabs will strictly hide themselves. Use `ComplementDomain::Universal` if your tool handles both.
- **Presentation**: `Headless` complements will never invoke `draw_ui(...)`. They only run `on_update(...)` in the background tick.

## Tool Registry (Coming Soon)
The `ComplementContext` will shortly bind directly to the JSON Tool Schema. This allows executing macro operations safely. 
```rust
// Expected Mod API behavior:
context.tools.call("create_entity", serde_json::json!({
    "name": "StormCloud",
    "position": [0.0, 100.0, 0.0]
}));
```
