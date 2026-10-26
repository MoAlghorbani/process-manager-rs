@echo off
title Process Manager (Rust Edition)
cd /d "%~dp0"
if "%~1"=="" (
    if exist "pm_rs.exe" (
        start "" "pm_rs.exe"
    ) else if exist "src-tauri\target\release\process_manager_rs.exe" (
        start "" "src-tauri\target\release\process_manager_rs.exe"
    ) else if exist "src-tauri\target\debug\process_manager_rs.exe" (
        start "" "src-tauri\target\debug\process_manager_rs.exe"
    ) else (
        echo [ERROR] Executable not found. Running build...
        cargo run --manifest-path src-tauri\Cargo.toml
    )
) else (
    if exist "pm_rs.exe" (
        "pm_rs.exe" %*
    ) else if exist "src-tauri\target\release\process_manager_rs.exe" (
        "src-tauri\target\release\process_manager_rs.exe" %*
    ) else if exist "src-tauri\target\debug\process_manager_rs.exe" (
        "src-tauri\target\debug\process_manager_rs.exe" %*
    ) else (
        cargo run --manifest-path src-tauri\Cargo.toml -- %*
    )
)
