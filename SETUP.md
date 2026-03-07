# How to Run AuraRafi

You need two things: **Rust** and **MinGW** (on Windows). That's it.

## 1. Install Rust

Go to [rustup.rs](https://rustup.rs) and run the installer. When it asks,
pick the **GNU** toolchain (not MSVC). If you already have Rust:

```
rustup default stable-x86_64-pc-windows-gnu
```

## 2. Install MinGW (Windows only)

```
winget install BrechtSanders.WinLibs.POSIX.UCRT
```

Close and reopen your terminal after installing.
On Linux/Mac skip this, your system already has gcc.

## 3. Run the editor

```
cd ProyectRaf
cargo run -p aura_rafi_editor
```

First time takes a few minutes (~200 deps). After that it opens instantly.

## That's it

No Visual Studio, no 6GB downloads. Just Rust + MinGW.

If you get a dlltool error, restart your terminal so it picks up the new PATH.
