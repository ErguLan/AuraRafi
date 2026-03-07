# Contributing to AuraRafi

Thank you for your interest in contributing to AuraRafi! This document
provides guidelines and instructions for contributing.

## Getting Started

### Prerequisites

- **Rust** (stable, 2021 edition): [rustup.rs](https://rustup.rs)
- **Git**: For version control
- A code editor (VS Code, or any editor of your choice)

### Building

```bash
# Clone the repository
git clone https://github.com/AuraRafi/AuraRafi.git
cd AuraRafi

# Build all crates
cargo build

# Run the editor
cargo run -p aura_rafi_editor

# Run tests
cargo test --workspace
```

### Project Structure

See [ARCHITECTURE.md](ARCHITECTURE.md) for a detailed breakdown of the
codebase structure and design decisions.

## Development Guidelines

### Code Style

- Follow standard Rust formatting (`cargo fmt`)
- Run `cargo clippy` before submitting changes
- All code and comments must be written in **English**
- Use descriptive variable and function names
- Document public APIs with doc comments (`///`)

### Module Organization

Each crate follows a consistent structure:

```
crate_name/
  Cargo.toml
  src/
    lib.rs          Crate root with module declarations and re-exports
    module.rs       Individual modules
    subdir/
      mod.rs        Submodule root
      impl.rs       Implementation files
```

### Commit Messages

Use clear, descriptive commit messages:

```
feat(raf_render): add shadow mapping for Low quality preset
fix(raf_editor): correct hierarchy panel selection on delete
docs(raf_core): document CommandBus flush behavior
refactor(raf_nodes): extract pin validation to helper function
```

Format: `type(scope): description`

Types: `feat`, `fix`, `docs`, `refactor`, `test`, `chore`, `perf`

### Testing

- Write unit tests for all public functions
- Place tests in `#[cfg(test)] mod tests` at the bottom of each file
- Run the full test suite before submitting: `cargo test --workspace`

### Performance

AuraRafi is designed to run on low-end hardware. Keep these principles in mind:

- Avoid unnecessary allocations in hot paths
- Prefer `Vec` over `HashMap` for small collections
- Use `glam` for math operations (SIMD-optimized)
- Profile before optimizing - use `cargo flamegraph` if needed
- Consider the "Potato" (Level 0) quality preset as the baseline

## Areas for Contribution

### High Priority

- **Rendering Pipeline**: Implement actual wgpu render passes for meshes
- **Shader System**: WGSL shaders for basic lighting and materials
- **Node Editor**: Expand node types and implement graph execution
- **Schematic Editor**: Component rotation, net naming, design rule checks
- **Asset Pipeline**: Thumbnail generation, drag-and-drop import

### Medium Priority

- **i18n**: Implement Fluent-based localization (files already in deps)
- **Hot Reload**: Connect `notify` file watcher to asset browser
- **Scene Serialization**: Save/load complete scenes to RON files
- **Keyboard Shortcuts**: Comprehensive shortcut system
- **Undo/Redo**: Wire command bus to actual editor operations

### Future

- **3D Rendering**: Full PBR pipeline with materials and lighting
- **Physics**: Basic collision detection and rigid body simulation
- **Audio**: Sound playback system
- **Scripting**: Node graph compilation to executable logic
- **Networking**: Client/server multiplayer implementation
- **AI Integration**: Connect to LLM providers for tool-calling

## Reporting Issues

When reporting bugs, please include:

1. Steps to reproduce the issue
2. Expected behavior
3. Actual behavior
4. System information (OS, GPU, Rust version)
5. Relevant log output from the console panel

## License

By contributing, you agree that your contributions will be dual-licensed
under the MIT and Apache 2.0 licenses, matching the project license.
