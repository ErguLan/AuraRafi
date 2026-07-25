@echo off
setlocal
set "RAFUI_ROOT=%~dp0.."
cargo run --manifest-path "%RAFUI_ROOT%\Cargo.toml" -p aura_rafi_editor -- --rafui-studio-preview %*
exit /b %ERRORLEVEL%
