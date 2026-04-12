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
