# Linux Build

This document describes the supported Linux build path for AuraRafi, the
reason for the target override, and the headless smoke test used by CI.

## Why an override is required

`.cargo/config.toml` is written for local Windows development:

```toml
[build]
target = "x86_64-pc-windows-gnu"
target-dir = "target_gnu"
```

Linux must not edit that file. Override both settings through the environment:

```bash
export CARGO_BUILD_TARGET=x86_64-unknown-linux-gnu
export CARGO_TARGET_DIR=target_gnu
```

`CARGO_BUILD_TARGET` takes precedence over `build.target`, and
`CARGO_TARGET_DIR` takes precedence over `build.target-dir`. `target_gnu` is
the only permitted in-repository target directory. Do not create `target_ai`,
`target_check`, `target_fix`, or any other `target_*` directory. If an isolated
build is needed, point `CARGO_TARGET_DIR` outside the repository.

## Packages

Ubuntu 24.04 is the CI distribution. Debian bookworm uses the same package
names for this set.

```bash
sudo apt-get update
sudo apt-get install -y --no-install-recommends \
  build-essential \
  pkg-config \
  libudev-dev \
  libxkbcommon-dev \
  libwayland-dev \
  libx11-xcb-dev \
  libxcb1-dev \
  libxrandr-dev \
  libxi-dev \
  libxcursor-dev \
  libfontconfig1-dev \
  cmake \
  clang \
  lld
```

Why each group is present:

| Package | Consumer |
| --- | --- |
| `pkg-config` | Wayland, udev, and other `-sys` build scripts |
| `libudev-dev` | hardware/serial discovery |
| `libxkbcommon-dev`, `libwayland-dev`, `libx11-xcb-dev`, `libxcb1-dev`, `libxrandr-dev`, `libxi-dev`, `libxcursor-dev` | Winit X11/Wayland host |
| `libfontconfig1-dev` | native text/toolkit build probes |
| `cmake`, `clang`, `lld` | native dependencies and linking |
| `build-essential` | host C compiler and linker |

Rust:

```bash
rustup toolchain install stable --component rustfmt
rustup target add x86_64-unknown-linux-gnu
```

## Check and test commands

Run from the repository root. The commands below are the same ones executed by
`.github/workflows/ci.yml`.

```bash
export CARGO_BUILD_TARGET=x86_64-unknown-linux-gnu
export CARGO_TARGET_DIR=target_gnu
export CARGO_BUILD_JOBS=2
export CARGO_INCREMENTAL=0
export RUST_BACKTRACE=1

cargo fmt --all -- --check

cargo check -p raf_core -p raf_runtime -p raf_render -p raf_ui -p raf_net
cargo check -p raf_player --bins
cargo check -p raf_editor
cargo check -p aura_rafi_editor

cargo test -p raf_core --lib
cargo test -p raf_net --lib
cargo test -p raf_runtime --lib
cargo test -p raf_render --lib
```

Why this shape:

- The checks catch Linux-specific `cfg` and dependency errors without paying
  the link cost of the full application.
- `-p raf_player --bins` covers the standalone player, not only its library.
- `-p raf_editor` and `-p aura_rafi_editor` cover the editor library and the
  native executable separately.
- The four test commands are focused library tests. `--workspace` is
  intentionally not used: unrelated in-flight areas must not block the Linux
  gate.

`cargo check` compiles but does not link. Some missing system libraries only
appear in a link step. The focused tests and the player build exercise the
linker.

## Build the player

```bash
cargo build -p raf_player --bin aurarafi-player
./target_gnu/x86_64-unknown-linux-gnu/debug/aurarafi-player --help
```

Release:

```bash
cargo build --release -p raf_player --bin aurarafi-player
```

The release profile uses fat LTO and one codegen unit, so the release link is
substantially slower than debug. Use debug for smoke checks.

The output layout with the target override is:

```text
target_gnu/x86_64-unknown-linux-gnu/debug/aurarafi-player
target_gnu/x86_64-unknown-linux-gnu/release/aurarafi-player
```

## Headless smoke

The v0.11 contract requires the player to accept a headless flag and print help
without a display, GPU, or project:

```bash
./target_gnu/x86_64-unknown-linux-gnu/debug/aurarafi-player --headless --help
./target_gnu/x86_64-unknown-linux-gnu/debug/aurarafi-player --help
```

Implementation status: the current player CLI accepts `--manifest`,
`--runtime`, and `--help`. `--headless` is the v0.11 implementation gate and is
tracked by the `headless-smoke` CI job. The documentation describes the
required behavior; it does not claim that the flag already exists.

A bounded frame run becomes available when `examples/headless/project/runtime.ron`
exists:

```bash
RAFI_HEADLESS=1 \
RAFI_MAX_FRAMES=3 \
RAFI_IDLE_TIMEOUT_SECONDS=10 \
timeout 120 ./target_gnu/x86_64-unknown-linux-gnu/debug/aurarafi-player \
  --headless \
  --manifest examples/headless/project/runtime.ron
```

CI skips that step while the fixture directory is absent.

## Container path

The production container is defined by the root `Dockerfile`. It builds with
`rust:1.83-bookworm`, installs the same development packages, and targets
`x86_64-unknown-linux-gnu` with `CARGO_TARGET_DIR=/build/target`.

```bash
docker build -t aurarafi-player:dev .
docker run --rm \
  -v /srv/aurarafi/project:/session/project:ro \
  -v /srv/aurarafi/out:/session/out \
  aurarafi-player:dev --help
```

The runtime image installs `mesa-vulkan-drivers`, `libvulkan1`, the X11/Wayland
runtime libraries, `ca-certificates`, and `tini`. It runs as uid 10001. Volume
layout and security rules are in [CLOUD_STREAMING.md](CLOUD_STREAMING.md).

## Windows-only areas

These are the known platform splits. They do not block the Linux build.

### Audio

`crates/raf_player/src/audio.rs` links `winmm` and implements PCM WAV playback
under `#[cfg(target_os = "windows")]`. On Linux the same module compiles a
fallback that reports native audio output as unsupported. There is no Linux
audio backend yet; the player must not claim audio output on Linux.

### Native pickers and dialogs

`crates/raf_editor/src/folder_picker.rs` uses COM dialogs on Windows. Linux uses
`zenity` or `kdialog` when present (see `crates/raf_editor/src/folder_picker.rs`
and `docs/LOCAL_RUNTIME.md`). When neither helper is installed, the picker
returns no selection instead of failing the build.

### Accessibility

`crates/raf_editor/src/native_accessibility/` is gated by
`#[cfg(windows)]` in `crates/raf_editor/src/lib.rs`. It is not compiled on
Linux.

### Window handles and Win32 crates

The `windows`, `windows-core`, and `raw-window-handle` dependencies in
`crates/raf_editor/Cargo.toml` are under `[target.'cfg(windows)'.dependencies]`.
They are not built on Linux.

### Icon embedding

`editor/build.rs` embeds `editor/icon.ico` with `winresource`. The build script
returns early when `CARGO_CFG_WINDOWS` is absent. `winresource` is currently an
unconditional build dependency, but it compiles on Linux and the early return
keeps it inert. Do not move it under
`[target.'cfg(windows)'.build-dependencies]` without also gating the
`winresource` code in `editor/build.rs` with `#[cfg(windows)]`; otherwise the
Linux build script fails on an unresolved import.

### Graphics configuration

`.cargo/config.toml` selects the Windows GNU target and `target_gnu`. Linux
overrides both through the environment as shown above. The file must not be
edited for Linux.

## Troubleshooting

| Symptom | Cause | Fix |
| --- | --- | --- |
| `failed to run custom build command for ...` mentioning `libudev` | missing udev headers | install `libudev-dev` |
| Wayland or XKB `pkg-config` failure | missing window-system headers | install the packages in the table above |
| `linker 'cc' not found` or unresolved native symbols | missing compiler/linker | install `build-essential`, `clang`, `lld` |
| Cargo tries `x86_64-pc-windows-gnu` | override not exported | `export CARGO_BUILD_TARGET=x86_64-unknown-linux-gnu` |
| New `target_*` directory appears | custom target directory | use `target_gnu` or an external `CARGO_TARGET_DIR` |
| Window creation fails with no display | no X11/Wayland session | use `--headless` for server runs |
| No Vulkan adapter | missing driver or device | install `mesa-vulkan-drivers` or use the CPU recovery path |
| Test link fails but `cargo check` passes | `check` does not link | install the missing runtime/development package |

Do not work around a missing system package by editing workspace manifests or
`.cargo/config.toml`. Fix the host environment; CI uses the package list above.

## CI mapping

`.github/workflows/ci.yml` runs three jobs:

| Job | Purpose |
| --- | --- |
| `linux` | formatting, Linux checks, focused tests, player artifact |
| `headless-smoke` | downloads the artifact, checks help, runs a fixture when present |
| `wasm` | non-blocking `raf_web` wasm32 check until the crate exists |

The Linux job exports `CARGO_BUILD_TARGET`, `CARGO_TARGET_DIR`,
`CARGO_BUILD_JOBS=2`, `CARGO_INCREMENTAL=0`, and `RUST_BACKTRACE=1`, so a local
reproduction should use the same values.
