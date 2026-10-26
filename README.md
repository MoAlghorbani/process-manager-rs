# Process Manager (Rust Edition) — `pm_rs`

A high-performance, native Windows desktop supervisor, task manager, and productivity cockpit rewritten in **Rust** (Tauri v2 + Win32 Kernel APIs).

- **Location:** `E:\process_manager_rs` (also accessible via junction at `E:\process_manager\rust`)
- **Backend Language:** Rust 1.99.1 (`src-tauri/`)
- **Desktop Engine:** Tauri v2 (native Windows WebView2 hardware acceleration)
- **GUI Frontend:** Vanilla HTML5, CSS3 (Modern Glassmorphic Dark Design), and Vanilla JS (`src/`)
- **Binary Output:** `pm_rs.exe` (100% standalone monolithic executable with embedded Web GUI, `yt-dlp.exe`, `ffmpeg.exe`, and `ffprobe.exe` — zero extra folders or external dependencies required).
- **Embedded Tools Engine:** On first launch, `pm_rs.exe` silently extracts the embedded tools into `%LOCALAPPDATA%\ProcessManager\bin` in the background and runs them completely offline.
- **Data Compatibility:** 100% interoperable with existing `services.json`, `todos.json`, `credentials.json`, `clipboard_history.json`, `ytdl_config.json`, `downloads.json`, and `jiggler_config.json`.

---

## Supported Feature Suites

### 1. ⚡ Process Supervisor & Service Engine (`manager.rs`, `win32.rs`)
- **Kernel Safety with Job Objects:** Every process is bound to a native Windows Job Object (`JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE = 0x2000`). When supervisor stops or terminates a service, all child and grandchild workers (`cmd.exe`, `node.exe`, `python.exe`) are killed at the OS kernel level without orphans.
- **Port Scavenger (`free_port`):** Automatically scans for processes listening on target TCP ports, checks against protected processes (`svchost.exe`, `lsass.exe`, `docker.exe`, `wsl.exe`), walks the descendant process tree (`CreateToolhelp32Snapshot`), and terminates them cleanly.
- **Python Virtualenv Auto-Detection:** Automatically discovers `.venv`, `venv`, and `env` folders, sets `VIRTUAL_ENV`, and prepends to `PATH`.
- **Auto-Restart Engine:** Background async task automatically restarts unexpected process failures after a 1500ms backoff sleep.
- **Real-Time RAM Monitoring:** Uses native `GetProcessMemoryInfo` (`PROCESS_MEMORY_COUNTERS`) to report live working set in MB.
- **Dynamic Group Management:** Batch actions (`Start All`, `Stop All`, `Restart All`), group renames with cascading task updates in Kanban, service reassignments, and group dissolution.
- **Resizable Split Log Viewer:** Interactive drag handle with Slate-700 divider, preset sizing buttons (Collapse, Half, Expand), file size badges, and an intelligent **Auto-Scroll Freeze Guard** that protects against viewport jumps when reading history or selecting text.

### 2. 🐭 Mouse Jiggler & Anti-Idle Suite (`jiggler.rs`)
- **Prevents Away Status:** Keeps Slack, Teams, Discord, and Zoom active.
- **Movement Modes:**
  - `orbit`: Smooth circular arc (28px radius in 20 trigonometric steps).
  - `zen`: Invisible 1-pixel micro-nudge (`+1px` then `-1px`).
  - `gentle`: +20px displacement returning after 100ms.
  - `api_only`: `SetThreadExecutionState` keep-awake without mouse pointer motion.
- **Ghost Key Taps:** Injects `VK_F15 (0x7E)` via `keybd_event` (resets system idle timer without typing characters into active text fields).
- **Sleep Prevention:** Enforces `ES_CONTINUOUS | ES_SYSTEM_REQUIRED | ES_DISPLAY_REQUIRED`.
- **Idle Guard:** Reads `GetLastInputInfo` + `GetTickCount` to suspend jiggling when user is actively typing or moving the mouse.
- **Live Telemetry:** Real-time counters for mouse nudges, key taps, countdown timers, and system inactivity duration.

### 3. 🔐 Credentials Vault & Clipboard History (`credentials.rs`, `clipboard.rs`)
- **Accounts Vault:** JSON storage in `credentials.json` (`Platform`, `Username`, `Password`, `URL`, `Notes`).
  - 1-click `[📋 User]` and `[📋 Pass]` clipboard copying.
  - Masked password visibility toggle (`👁️`).
  - Cryptographically secure password generator (`rand::random` across custom length, uppercase, lowercase, digits, symbols).
- **Permanent Clipboard History:**
  - Background polling task checks `GetClipboardSequenceNumber` every 400ms.
  - Extracts UTF-16 Unicode text via `OpenClipboard`, `GetClipboardData(CF_UNICODETEXT)`, and `GlobalLock`.
  - Automatic deduplication and permanent persistence up to 300 entries in `clipboard_history.json`.
  - Search, star/pin (`⭐`), delete, and batch clear unpinned clips.

### 4. 📝 Kanban Task Board (`todo.rs`)
- **3-Column Workflow:** `📋 TO DO`, `🚧 IN PROGRESS`, `✅ DONE`.
- **Data Model:** `TodoItem` matching `todos.json` with project tags, priorities (`HIGH`, `NORMAL`, `LOW`), and due timestamps.
- **Service Group Integration:** Projects automatically sync with active service groups.
- **Scheduled Desktop Notifications:** Background async worker checks `notify_at` every 10 seconds on incomplete tasks and triggers native Windows notifications.

### 5. 📥 YouTube & Media Downloader (`ytdl.rs`)
- **Media Engine:** Leverages embedded/local `yt-dlp.exe` and system `ffmpeg`.
- **Full Playlist Support:** Auto-detects pure playlists (`/playlist?list=...`) vs video links with playlist references (`watch?v=...&list=...`). Organizes downloaded files into `OutputDir/<Playlist Title>/<index> - <Track Title>.<ext>`.
- **Quality & Format Matrix:**
  - **Video:** `MP4`, `MKV`, `WEBM` across qualities `Best Available`, `4K (2160p)`, `2K (1440p)`, `1080p`, `720p`, `480p`, `360p`.
  - **Audio:** `MP3`, `M4A`, `FLAC`, `WAV`, `OPUS` across bitrates `320 kbps`, `256 kbps`, `192 kbps`, `128 kbps`.
- **Live Telemetry:** Regex progress parser reporting speed (e.g. `4.5 MiB/s`), ETA countdown (`00:15`), downloaded file size (`42.1 MiB / 65.5 MiB`), and dynamic progress bar.
- **Job Object Safety:** `yt-dlp` and `ffmpeg` processes attach to a Windows Job Object so cancellation terminates all child processes instantly.

### 6. 🎬 Media Studio & Audio Booster (`ffmpeg.rs`)
- **Embedded Engine:** Ships self-contained with full builds of `ffmpeg.exe` and `ffprobe.exe` embedded in `bin/` — **no external download required** by the end user.
- **Hardware Acceleration:** Auto-detects NVIDIA NVENC / CUDA, Intel QSV, and AMD AMF hardware encoders.
- **5 Operating Modes:**
  1. 🎵 **Video to MP3:** High-fidelity audio extraction with selectable bitrates (`320 kbps (Audiophile)`, `256 kbps`, `192 kbps (Standard CD)`, `128 kbps (Compact)`).
  2. 🔊 **Audio Booster ("Make Sound Bigger"):**
     - Amplifies audio in standalone songs OR video files.
     - **Video Stream Copy Safety:** Video track is passed through via `-c:v copy` without re-encoding, boosting audio in seconds without loss of video quality.
     - Linear / Decibel Multipliers: Presets for `125% (+2 dB)`, `150% (+3.5 dB)`, `200% (+6 dB)`, `300% (+9.5 dB)`, `400% (+12 dB)` or custom slider (100% to 400%).
     - **EBU R128 Broadcast Normalization (`loudnorm`):** Intelligently maximizes loudness while preventing digital distortion.
     - **Peak Volume Headroom Analyzer (`volumedetect`):** Detects maximum peak dB and recommends the exact safe amplification level before clipping.
  3. 📹 **Video Converter:** Transcode between `MP4`, `MKV`, `WEBM`, `AVI`, `MOV`, and animated `GIF` with resolution scaling (`1080p`, `720p`, `480p`) and codecs (`H.264`, `H.265`, `VP9`, `Copy`).
  4. 🎧 **Audio Converter:** Convert between `MP3`, `WAV (Lossless PCM)`, `FLAC (Lossless Compressed)`, `AAC/M4A`, `OGG`, and `OPUS`.
  5. 🖼️ **Image Converter:** Convert between `WEBP`, `PNG`, `JPG`, `ICO (Windows Icon)`, `BMP`, and `TIFF` with quality and scaling options.
- **Live Progress HUD & History:** Real-time percentage, duration processed, speed multiplier, elapsed time, cancel button bound to Windows Job Object, and persistent history with quick "Open Result" / "Open Folder" actions.

### 7. 📁 Dynamic Data Storage Location & Multi-User Import
- **In-App Storage Selector:** Direct access via the top-bar header pill button (`📁 <Folder Name>`).
- **Team / Multi-User Data Sharing:** Users can point to any local folder, network drive, or shared network share (e.g. `\\server\shared_pm_data` or `D:\TeamData`).
- **Live Folder Manifest & Inspection:** Automatically scans the target folder before applying, showing a live checklist of all data files:
  - `services.json` (with service count)
  - `todos.json` (with task count)
  - `credentials.json` (with account count)
  - `clipboard_history.json` (with clips count)
  - `jiggler_config.json`, `ytdl_config.json`, `downloads.json`, `media_history.json`
- **Instant Hot-Reload:** Switches directories in memory and reloads all services, tasks, vault records, clipboard entries, and configs without restarting the app or killing running services.
- **Automatic Migration:** Offers to copy current files over if the target folder is empty or missing configurations.
- **User Preference Persistence:** Persists the selected directory across app launches in `%APPDATA%\ProcessManager\storage_config.json`.

---

## CLI Command Suite

You can execute any command directly from terminal without launching the GUI:

```powershell
# Service Status & Inspection
.\pm_rs.exe list                      # Table of all services with RAM, PID, Port, Group
.\pm_rs.exe groups                    # Overview of groups, member counts, and running state
.\pm_rs.exe logs <name> [-n <lines>]  # Tail last N log lines

# Lifecycle Controls
.\pm_rs.exe start <name|all>          # Start service(s)
.\pm_rs.exe stop <name|all>           # Stop service(s)
.\pm_rs.exe restart <name|all>        # Restart service(s)
.\pm_rs.exe delete <name>             # Unregister service

# Tools & Vault
.\pm_rs.exe creds [<search>]          # View credentials vault in terminal
.\pm_rs.exe clips [<search>]          # View clipboard history
.\pm_rs.exe todo                      # Kanban task overview
.\pm_rs.exe elevate                   # Request Administrator privileges via UAC
.\pm_rs.exe gui                       # Launch native Tauri desktop GUI
```

---

## Build & Run

### 1. Build Standalone Release Executable
```cmd
.\build.bat
```
Compiles `src-tauri` in release mode and copies the standalone executable to `pm_rs.exe`.

### 2. Launch Application
```cmd
.\run.bat          # Launches Desktop GUI
.\run.bat list     # Runs CLI subcommand
```
