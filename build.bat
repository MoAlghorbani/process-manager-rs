@echo off
echo ========================================================
echo   Building Process Manager (Rust Desktop Edition)
echo ========================================================
cargo build --release --manifest-path src-tauri\Cargo.toml
if %ERRORLEVEL% NEQ 0 (
    echo [ERROR] Build failed with error code %ERRORLEVEL%.
    exit /b %ERRORLEVEL%
)

copy /Y "src-tauri\target\release\process_manager_rs.exe" "pm_rs.exe"
echo [SUCCESS] Standalone binary compiled to pm_rs.exe.
