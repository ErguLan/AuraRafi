#!/usr/bin/env sh
# Build the raf_web browser canvas target.
set -eu

script_dir=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
repo_root=$(CDPATH= cd -- "$script_dir/../.." && pwd)

fail() {
    printf '\nraf_web build failed\n%s\n\n' "$1" >&2
    exit 1
}

command -v cargo >/dev/null 2>&1 ||
    fail "cargo was not found on PATH. Install the Rust toolchain from https://rustup.rs and reopen the terminal."

if command -v rustup >/dev/null 2>&1; then
    installed_targets=$(rustup target list --installed 2>/dev/null || true)
    if ! printf '%s\n' "$installed_targets" | grep -qx 'wasm32-unknown-unknown'; then
        fail "The wasm32-unknown-unknown target is not installed. Run: rustup target add wasm32-unknown-unknown"
    fi
else
    printf 'warning: rustup was not found. cargo will report an error if the wasm32-unknown-unknown target is missing.\n' >&2
fi

cd "$repo_root"

printf 'Building raf_web for wasm32-unknown-unknown (release)...\n'
cargo build -p raf_web --target wasm32-unknown-unknown --release ||
    fail "cargo build failed. Scroll up for the compiler error. Shared-crate platform gates are documented in docs/WASM.md."

metadata=$(cargo metadata --no-deps --format-version 1) ||
    fail "cargo metadata failed, so the output directory could not be located."

target_dir=$(printf '%s' "$metadata" | sed -n 's/.*"target_directory":"\([^"]*\)".*/\1/p' | sed 's|\\\\|/|g')
[ -n "$target_dir" ] || target_dir="$repo_root/target"

wasm_path="$target_dir/wasm32-unknown-unknown/release/raf_web.wasm"
[ -f "$wasm_path" ] || fail "The expected WebAssembly artifact was not found: $wasm_path"

command -v wasm-bindgen >/dev/null 2>&1 ||
    fail "wasm-bindgen was not found on PATH. Install the CLI version that matches Cargo.lock: cargo install wasm-bindgen-cli --version <locked version>"

locked_version=""
if [ -f "$repo_root/Cargo.lock" ]; then
    locked_version=$(awk '
        /^name = "wasm-bindgen"$/ { found = 1; next }
        found && /^version = / {
            gsub(/[" ]/, "", $3)
            print $3
            exit
        }
    ' "$repo_root/Cargo.lock")
fi
cli_version=$(wasm-bindgen --version 2>/dev/null | awk '{print $2}') ||
    fail "wasm-bindgen could not report its version."

if [ -n "$locked_version" ] && [ -n "$cli_version" ] && [ "$locked_version" != "$cli_version" ]; then
    fail "wasm-bindgen CLI $cli_version does not match Cargo.lock ($locked_version). Run: cargo install wasm-bindgen-cli --version $locked_version --force"
fi

pkg_dir="$script_dir/pkg"
mkdir -p "$pkg_dir"

printf 'Generating JavaScript bindings...\n'
wasm-bindgen --target web --out-dir "$pkg_dir" --out-name raf_web "$wasm_path" ||
    fail "wasm-bindgen failed. Check the CLI version and the artifact path printed above."

printf '\nWASM build complete.\n'
printf '  wasm:     %s\n' "$wasm_path"
printf '  bindings: %s\n\n' "$pkg_dir"
printf 'Serve the demo over HTTP (file:// cannot import a WebAssembly module):\n'
printf '  python3 -m http.server 8000 --directory "%s"\n' "$script_dir"
printf '  Then open http://localhost:8000/index.html\n\n'
