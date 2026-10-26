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
if not exist "bin\ffmpeg.exe" (
    echo [INFO] Copying embedded FFmpeg to bin\...
    mkdir "bin" 2>nul
    if exist "C:\Users\MohamedAlghurbani\AppData\Local\Microsoft\WinGet\Packages\Gyan.FFmpeg_Microsoft.Winget.Source_8wekyb3d8bbwe\ffmpeg-9.0-full_build\bin\ffmpeg.exe" (
        copy /Y "C:\Users\MohamedAlghurbani\AppData\Local\Microsoft\WinGet\Packages\Gyan.FFmpeg_Microsoft.Winget.Source_8wekyb3d8bbwe\ffmpeg-9.0-full_build\bin\ffmpeg.exe" "bin\ffmpeg.exe"
        copy /Y "C:\Users\MohamedAlghurbani\AppData\Local\Microsoft\WinGet\Packages\Gyan.FFmpeg_Microsoft.Winget.Source_8wekyb3d8bbwe\ffmpeg-9.0-full_build\bin\ffprobe.exe" "bin\ffprobe.exe"
    )
)
echo [SUCCESS] Standalone binary compiled to pm_rs.exe with embedded FFmpeg suite.
