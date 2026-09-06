# AuraRafi Open Source Complements (Mods) API

Welcome to the **AuraRafi Modding System**. Instead of modifying the Engine's core loop, you can inject tools, panels, or background tasks using the **Complement API**.

## Current status

`EngineComplement` and `ComplementRegistry` are defined in
`crates/raf_core/src/complement.rs` as an extension contract. The current native
composition path does not mount a live `ComplementRegistry` automatically, so
this document describes the prepared API and its required integration work; it
does not promise that a complement appears in the editor by simply adding a
file.

## Core Philosophy
Complements are a separate extension contract from the Agent transport. They
may call shared command/domain APIs when an explicit host integration grants
that access, but they do not automatically inherit the Agent's JSON tool
registry or provider connections.

## Creating a Complement

1. Place the source in the crate that owns the extension boundary; do not put a
   general complement in `crates/raf_render/src/complements/`, which is a
   renderer-internal namespace.
2. Import the contract from `raf_core::complement` and implement the
   `EngineComplement` trait with an explicit domain and presentation.

### 1. Minimal Implementation Example

```rust
use raf_core::complement::{
    ComplementContext, ComplementDomain, ComplementPresentation, EngineComplement,
};
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

    fn draw_ui(&mut self, context: &mut ComplementContext) {
        // The complement contract exposes domain state only. A retained RafUI
        // surface should bind a typed action to this state through the host.
        let _ = context;
        let _current_intensity = self.intensity;
    }
}
```

## Registering Your Complement

Once written, a host that explicitly owns a `ComplementRegistry` must register
it at that host's composition boundary. The current native application does not
perform this registration yet; do not paste this into `native_application.rs`
without first mounting the registry into the workbench lifecycle:

```rust
let mut registry = ComplementRegistry::new();
registry.register(Box::new(WeatherComplement::new()));
```

## New Electronics Mod Options

The electronics domain has its own active extension layer, independent of the
prepared UI complement registry.

If your complement targets circuits, schematics, or electronics workflows, you can now also inject:

- Extra component templates into the electrical library
- Extra DRC/ERC rules into the electrical validation pass

This means you no longer need to hard-patch built-in component lists or built-in
rule functions just to add your own electrical theory, school rules, or
project-specific parts.

## Easy Electronics Mod Flow

### 1. Create your complement

Make your Rust file and implement `EngineComplement` only if a host has mounted
the complement lifecycle.

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

## Current practical limitation

Today the electrical extension registry is designed first for Rust/source mods
inside the engine workspace. The electrical hooks are loaded by
`ComponentLibrary::default_library()` and the DRC path; the general UI
`ComplementRegistry` remains prepared rather than automatically mounted.

So the easiest path right now is:

1. Rust complement for UI or lifecycle
2. `raf_electronics` registration for parts/rules
3. Optional `.ron` assets for data-driven content

## Presentation vs Domain Isolation
- **Domain Seclusion**: If a user loads an `Electronics` project, all `ComplementDomain::Games` tabs will strictly hide themselves. Use `ComplementDomain::Universal` if your tool handles both.
- **Presentation**: `Headless` complements will never invoke `draw_ui(...)`. They only run `on_update(...)` in the background tick.

## Agent tool registry boundary

`ComplementContext` does not currently bind directly to the Agent JSON tool
schema. If macro operations are added later, they must use the shared command
protocol, explicit permissions and the native host lifecycle; the example
below is an unmounted design sketch, not a supported API:
```rust
// Expected Mod API behavior:
context.tools.call("create_entity", serde_json::json!({
    "name": "StormCloud",
    "position": [0.0, 100.0, 0.0]
}));
```
