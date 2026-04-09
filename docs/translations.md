# Internationalization (i18n) System - AuraRafi Engine

This document provides a detailed explanation of the AuraRafi engine's dynamic translation system, how to extend it, and best practices for keeping the code free from hardcoded UI strings.

## System Architecture

The AuraRafi i18n system is designed to be lightweight, "potato-friendly," and highly scalable. It is built upon three main pillars:

1.  **Resource Files (JSON)**: Located in `crates/raf_core/locales/`. These files contain key-value pairs for each supported language.
2.  **`raf_core::i18n` Crate**: The lookup engine that handles the JSON data (embedded in the binary) and provides the `t()` function.
3.  **`Language` Enum**: Defined in `raf_core::config`, this serves as the context selector for all translation queries.

---

## Technical Overview

### 1. Defining Strings
Strings do not live in the Rust source code. They reside in `en.json` (English) and `es.json` (Spanish).

**Example JSON structure:**
```json
{
  "app.hierarchy": "HIERARCHY",
  "app.no_entities": "No entities in scene"
}
```

### 2. The `t()` Function
This is the core of the system. Its signature is:
```rust
pub fn t(key: &str, lang: Language) -> &str
```
-   **key**: The unique identifier for the string (e.g., `"app.name"`).
-   **lang**: The `Language` enum value (e.g., `Language::English`).

Internally, `raf_core` uses the `include_str!` macro to embed the JSON files into the executable at compile time. This ensures zero disk latency when switching languages. If a key is missing in the target language, the system fallbacks to English or returns the key itself as a safety measure.

---

## Tutorial: How to Add a New Translation

When creating a new feature (e.g., a "Simulation" panel), follow these steps:

### Step 1: Add the key to the JSON files
Open `crates/raf_core/locales/en.json`:
```json
"sim.start": "Start Simulation",
"sim.stop": "Stop Simulation"
```

Open `crates/raf_core/locales/es.json`:
```json
"sim.start": "Iniciar Simulación",
"sim.stop": "Detener Simulación"
```

### Step 2: Implement it in Rust
In your `.rs` file, ensure you have access to the current language (usually passed from `settings.language`) and call the `t()` function:

```rust
use raf_core::i18n::t;
use raf_core::config::Language;

fn draw_ui(ui: &mut egui::Ui, lang: Language) {
    if ui.button(t("sim.start", lang)).clicked() {
        // Implementation logic
    }
}
```

---

## Advanced Usage & Best Practices

### Contextual Language Passing
To maintain clean code, editor panels (like `PropertiesPanel`) receive the language as an argument in their `show()` methods. **Never** attempt to read the global configuration file directly inside a UI panel; always propagate the language context from the main `app.rs`.

### Dynamic Formatting
If you need to insert variables into a translated string (e.g., "Project 'MyGame' saved"), use a base key and format it in Rust:
```rust
let msg = format!("{} '{}' {}", t("app.project", lang), name, t("app.saved", lang));
```

### Why We Moved Away from `if/else`?
Previously, the engine used manual checks:
```rust
let label = if is_es { "Guardar" } else { "Save" }; // BAD PRACTICE ❌
```
This approach is flawed because:
1.  **Maintainability**: Translation logic becomes scattered across hundreds of files.
2.  **Scalability**: Adding a third language (e.g., Japanese) would require manual edits to every single UI component.
3.  **Binary Size**: It increases source code complexity.

The new system is much cleaner:
```rust
let label = t("app.save", lang); // BEST PRACTICE ✅
```
Adding a new language now only requires creating a new `.json` file. The Rust code remains untouched.

---

## Troubleshooting
-   **Missing Keys**: If the UI displays the raw key (e.g., `app.missing_label`), it means the key is missing from the JSON files or was misspelled.
-   **Applying Changes**: Since JSON files are embedded at compile time, you must recompile the project (`cargo run`) to see changes made to the translation files.

---
*AuraRafi v0.4.0 Documentation - The "Zero-If" Localization System.*
