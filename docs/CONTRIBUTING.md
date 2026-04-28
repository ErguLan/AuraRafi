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

# Validate the editor path first
cargo check -p raf_editor

# Build all crates
cargo build --workspace

# Run the editor
cargo run -p aura_rafi_editor

# Run tests
cargo test --workspace

# Optional lint pass
cargo clippy --workspace --all-targets
```

There is no repository CI workflow yet, so local validation matters. Before submitting changes, at minimum run the commands that affect the part of the workspace you touched.

### Project Structure

See [ARCHITECTURE.md](ARCHITECTURE.md) for a detailed breakdown of the
codebase structure and design decisions.

When reading the project, prefer this order:

1. `CHANGELOG.md` for the most current implementation milestones.
2. `docs/ARCHITECTURE.md` for system boundaries and prepared-vs-connected context.
3. `README.md` for the high-level project-facing summary.

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
- Inline tests are the standard style in this repository; do not assume the absence of `tests/` folders means the crate is untested
- For editor-heavy changes that are hard to unit test, still validate with the narrowest possible `cargo check` target and document any manual verification performed

### Performance

AuraRafi is designed to run on low-end hardware. Keep these principles in mind:

- Avoid unnecessary allocations in hot paths
- Prefer `Vec` over `HashMap` for small collections
- Use `glam` for math operations (SIMD-optimized)
- Profile before optimizing - use `cargo flamegraph` if needed
- Consider the "Potato" (Level 0) quality preset as the baseline
- Be explicit about whether a feature is active today, editor-only, or architecture prepared for later integration

## Areas for Contribution

### High Priority

- **Runtime Connection**: Connect scene editing and node execution to a real game runtime/play loop
- **Rendering Pipeline**: Keep improving the CPU-first viewport while wiring the optional GPU path responsibly
- **PCB Workflow**: Continue PCB routing, DRC, footprint coverage, and real Gerber layer emission
- **Asset Pipeline**: Keep polishing thumbnails, import flows, and project-level asset feedback
- **Documentation Accuracy**: Keep README/docs aligned with real implementation status, especially around prepared systems vs live systems

### Medium Priority

- **Hot Reload**: Expand from core watcher infrastructure into clearer editor-facing flows
- **Keyboard Shortcuts**: Grow customization beyond the current experimental surface
- **Runtime/Editor Bridge**: Define how project settings, render presets, and node graphs enter execution mode
- **Testing Expansion**: Add more behavior-focused tests for editor-adjacent logic where practical
- **Repository Hygiene**: Keep temporary scripts and debug artifacts out of tracked source state

### Future

- **3D Rendering**: Full PBR pipeline with materials and lighting
- **Physics**: Basic collision detection and rigid body simulation
- **Audio**: Sound playback system
- **Scripting**: Node graph compilation or hybrid execution beyond the current interpreter base
- **Networking**: Client/server multiplayer implementation
- **AI Integration**: Connect to LLM providers for tool-calling

## Reporting Issues

When reporting bugs, please include:

1. Steps to reproduce the issue
2. Expected behavior
3. Actual behavior
4. System information (OS, GPU, Rust version)
5. Relevant log output from the console panel

If the issue is documentation drift, mention exactly which file is stale and which code path contradicts it.

## License

By contributing, you agree that your contributions will be dual-licensed
under the MIT and Apache 2.0 licenses, matching the project license.
