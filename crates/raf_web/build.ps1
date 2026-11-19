#requires -Version 5.1
[CmdletBinding()]
param()

$ErrorActionPreference = 'Stop'

$crateDir = $PSScriptRoot
$repoRoot = (Resolve-Path (Join-Path $crateDir '..\..')).Path

function Stop-WithMessage {
    param([string]$Message)
    Write-Host ''
    Write-Host 'raf_web build failed' -ForegroundColor Red
    Write-Host $Message -ForegroundColor Red
    Write-Host ''
    exit 1
}

$cargo = Get-Command cargo -ErrorAction SilentlyContinue
if ($null -eq $cargo) {
    Stop-WithMessage 'cargo was not found on PATH. Install the Rust toolchain from https://rustup.rs and reopen the terminal.'
}

$rustup = Get-Command rustup -ErrorAction SilentlyContinue
if ($null -ne $rustup) {
    $installedTargets = & rustup target list --installed 2>$null
    if ($LASTEXITCODE -ne 0) {
        Stop-WithMessage 'rustup is installed but its target list could not be read. Run: rustup target list --installed'
    }
    if (-not ($installedTargets -contains 'wasm32-unknown-unknown')) {
        Stop-WithMessage 'The wasm32-unknown-unknown target is not installed. Run: rustup target add wasm32-unknown-unknown'
    }
}
else {
    Write-Host 'warning: rustup was not found. cargo will report an error if the wasm32-unknown-unknown target is missing.' -ForegroundColor Yellow
}

Push-Location $repoRoot
try {
    Write-Host 'Building raf_web for wasm32-unknown-unknown (release)...'
    & cargo build -p raf_web --target wasm32-unknown-unknown --release
    if ($LASTEXITCODE -ne 0) {
        Stop-WithMessage 'cargo build failed. Scroll up for the compiler error. Shared-crate platform gates are documented in docs/WASM.md.'
    }

    $metadataJson = (& cargo metadata --no-deps --format-version 1) -join "`n"
    if ($LASTEXITCODE -ne 0 -or [string]::IsNullOrWhiteSpace($metadataJson)) {
        Stop-WithMessage 'cargo metadata failed, so the output directory could not be located.'
    }
}
finally {
    Pop-Location
}

$targetDirectory = ($metadataJson | ConvertFrom-Json).target_directory
if ([string]::IsNullOrWhiteSpace($targetDirectory)) {
    Stop-WithMessage 'cargo metadata did not report a target directory.'
}

$wasmPath = Join-Path $targetDirectory 'wasm32-unknown-unknown\release\raf_web.wasm'
if (-not (Test-Path $wasmPath)) {
    Stop-WithMessage "The expected WebAssembly artifact was not found: $wasmPath"
}

$bindgen = Get-Command wasm-bindgen -ErrorAction SilentlyContinue
if ($null -eq $bindgen) {
    Stop-WithMessage 'wasm-bindgen was not found on PATH. Install the CLI version that matches Cargo.lock: cargo install wasm-bindgen-cli --version <locked version>'
}

$lockedVersion = $null
$lockPath = Join-Path $repoRoot 'Cargo.lock'
if (Test-Path $lockPath) {
    $lockLines = Get-Content $lockPath
    for ($index = 0; $index -lt $lockLines.Count; $index++) {
        if ($lockLines[$index] -eq 'name = "wasm-bindgen"') {
            $limit = [Math]::Min($index + 6, $lockLines.Count)
            for ($next = $index + 1; $next -lt $limit; $next++) {
                if ($lockLines[$next] -match '^version = "([^"]+)"') {
                    $lockedVersion = $Matches[1]
                    break
                }
            }
            break
        }
    }
}

$cliVersionOutput = (& wasm-bindgen --version 2>$null) -join ' '
if ($LASTEXITCODE -ne 0) {
    Stop-WithMessage 'wasm-bindgen is on PATH but could not report its version.'
}
$cliVersion = $null
if ($cliVersionOutput -match '([0-9]+\.[0-9]+\.[0-9]+)') {
    $cliVersion = $Matches[1]
}
if ($lockedVersion -and $cliVersion -and ($lockedVersion -ne $cliVersion)) {
    Stop-WithMessage "wasm-bindgen CLI $cliVersion does not match Cargo.lock ($lockedVersion). Run: cargo install wasm-bindgen-cli --version $lockedVersion --force"
}

$pkgDir = Join-Path $crateDir 'pkg'
New-Item -ItemType Directory -Path $pkgDir -Force | Out-Null

Write-Host 'Generating JavaScript bindings...'
& wasm-bindgen --target web --out-dir $pkgDir --out-name raf_web $wasmPath
if ($LASTEXITCODE -ne 0) {
    Stop-WithMessage 'wasm-bindgen failed. Check the CLI version and the artifact path printed above.'
}

Write-Host ''
Write-Host 'WASM build complete.' -ForegroundColor Green
Write-Host "  wasm:     $wasmPath"
Write-Host "  bindings: $pkgDir"
Write-Host ''
Write-Host 'Serve the demo over HTTP (file:// cannot import a WebAssembly module):'
Write-Host "  python -m http.server 8000 --directory `"$crateDir`""
Write-Host '  Then open http://localhost:8000/index.html'
Write-Host ''
