# Process Manager (Rust Edition) — `pm_rs`

A high-performance, native Windows desktop supervisor, task manager, and local network collaboration cockpit written in **Rust** (Tauri v2 + Win32 Kernel APIs).

- **Location:** `E:\process_manager_rs` (also accessible via junction at `E:\process_manager\rust`)
- **Backend Language:** Rust (`src-tauri/`)
- **Desktop Engine:** Tauri v2 (native Windows WebView2 hardware acceleration)
- **GUI Frontend:** Vanilla HTML5, CSS3 (Solid Dark Black, Neutral Gray, and Crimson Red flat theme with zero gradients), and Vanilla JS (`src/`)
- **Binary Output:** `pm_rs.exe` (Lightweight ~6.4 MB standalone executable with embedded Web GUI and zero bloat).
- **Data Compatibility:** 100% interoperable with `services.json`, `todos.json`, `credentials.json`, `clipboard_history.json`, `ytdl_config.json`, `downloads.json`, `jiggler_config.json`, `chat_history.json`, and `chat_config.json`.

---

## Supported Feature Suites

### 1. ⚡ Process Supervisor & Service Engine (`manager.rs`, `win32.rs`)
- **Kernel Safety with Job Objects:** Every process is bound to a native Windows Job Object (`JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE = 0x2000`). When supervisor stops or terminates a service, all child and grandchild workers (`cmd.exe`, `node.exe`, `python.exe`) are killed at the OS kernel level without orphans.
- **Port Scavenger (`free_port`):** Automatically scans for processes listening on target TCP ports, checks against protected processes (`svchost.exe`, `lsass.exe`, `docker.exe`, `wsl.exe`), walks the descendant process tree (`CreateToolhelp32Snapshot`), and terminates them cleanly.
- **Python Virtualenv Auto-Detection:** Automatically discovers `.venv`, `venv`, and `env` folders, sets `VIRTUAL_ENV`, and prepends to `PATH`.
- **Auto-Restart Engine:** Background async task automatically restarts unexpected process failures after a 1500ms backoff sleep.
- **Real-Time RAM Monitoring:** Uses native `GetProcessMemoryInfo` (`PROCESS_MEMORY_COUNTERS`) to report live working set in MB.
- **Dynamic Group Management:** Batch actions (`Start All`, `Stop All`, `Restart All`), group renames with cascading task updates in Kanban, service reassignments, and group dissolution.
- **Resizable Split Log Viewer:** Interactive drag handle with divider, preset sizing buttons (Collapse, Half, Expand), file size badges, and an intelligent **Auto-Scroll Freeze Guard** that protects against viewport jumps when reading history or selecting text.

### 2. 💬 LAN P2P Chat & Large File Transfers (`chat.rs`)
- **Zero-Config P2P Discovery:** Automatic local network peer discovery via UDP heartbeats on port 38383 without requiring a central server.
- **Rooms & 1-on-1 Messaging:** Supports both public broadcast (`# 🌐 General LAN`) and private direct peer conversations.
- **50GB+ Zero-RAM Streaming TCP Transfers:**
  - Streams files of arbitrary size (50GB+ ISOs, VM disks, backups) directly from disk to network socket using a constant 512KB chunk buffer (< 1MB RAM usage).
  - Native line rate throughput (saturates 1Gbps, 2.5Gbps, or 10Gbps LANs).
  - Interactive transfer cards with live progress bars, transfer speed (MB/s), ETA countdown, and collision-safe `.part` saving with `[📂 Open File]` and `[📁 Show in Folder]`.
- **⚡ Hard Buzz & 🔔 Soft Buzz:**
  - **Hard Buzz:** Violent multi-axis screen vibration animation (`750ms`), aggressive dual-pulse electric buzzer sound synthesized via Web Audio API, native Windows taskbar flashing (`FlashWindow`), and urgent Windows desktop toast.
  - **Soft Buzz:** Gentle subtle nudge vibration (`450ms`), soft double chime audio alert, taskbar flash, and friendly notification.
  - Triggerable via UI menu (`⚡ Buzz ▾`) or slash commands (`/hard`, `/buzz`, `/soft`, `/nudge`) with 4-second anti-spam rate limiting.
- **Native Windows Desktop Toasts:** Integrated Windows Runtime `ToastNotificationManager` alerts for incoming chat messages, file offers, and transfer completion even when the application is minimized.

### 3. 🐭 Mouse Jiggler & Anti-Idle Suite (`jiggler.rs`)
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

### 4. 🔐 Credentials Vault & Clipboard History (`credentials.rs`, `clipboard.rs`)
- **Accounts Vault:** JSON storage in `credentials.json` (`Platform`, `Username`, `Password`, `URL`, `Notes`).
  - 1-click `[📋 User]` and `[📋 Pass]` clipboard copying.
  - Masked password visibility toggle (`👁️`).
  - Cryptographically secure password generator (`rand::random` across custom length, uppercase, lowercase, digits, symbols).
- **Permanent Clipboard History:**
  - Background polling task checks `GetClipboardSequenceNumber` every 400ms.
  - Extracts UTF-16 Unicode text via `OpenClipboard`, `GetClipboardData(CF_UNICODETEXT)`, and `GlobalLock`.
  - Automatic deduplication and permanent persistence up to 300 entries in `clipboard_history.json`.
  - Search, star/pin (`⭐`), delete, and batch clear unpinned clips.

### 5. 📝 Kanban Task Board (`todo.rs`)
- **3-Column Workflow:** `📋 TO DO`, `🚧 IN PROGRESS`, `✅ DONE`.
- **Data Model:** `TodoItem` matching `todos.json` with project tags, priorities (`HIGH`, `NORMAL`, `LOW`), and due timestamps.
- **Service Group Integration:** Projects automatically sync with active service groups.
- **Scheduled Desktop Notifications:** Background async worker checks `notify_at` on incomplete tasks and triggers native Windows notifications.

### 6. 📥 YouTube & Media Downloader (`ytdl.rs`)
- **Media Engine:** Leverages `yt-dlp.exe` with lightweight footprint.
- **Full Playlist Support:** Auto-detects pure playlists (`/playlist?list=...`) vs video links with playlist references (`watch?v=...&list=...`). Organizes downloaded files into `OutputDir/<Playlist Title>/<index> - <Track Title>.<ext>`.
- **Quality & Format Matrix:**
  - **Video:** `MP4`, `MKV`, `WEBM` across qualities `Best Available`, `4K (2160p)`, `2K (1440p)`, `1080p`, `720p`, `480p`, `360p`.
  - **Audio:** `MP3`, `M4A`, `FLAC`, `WAV`, `OPUS` across bitrates `320 kbps`, `256 kbps`, `192 kbps`, `128 kbps`.
- **Live Telemetry:** Real-time progress reporting speed (e.g. `4.5 MiB/s`), ETA countdown, downloaded file size, and dynamic progress bar.
- **Job Object Safety:** `yt-dlp` processes attach to a Windows Job Object so cancellation terminates all child processes instantly.

### 7. 📁 Dynamic Data Storage Location & Multi-User Import
- **In-App Storage Selector:** Direct access via the top-bar header pill button (`📁 Storage Folder`).
- **Team / Multi-User Data Sharing:** Users can point to any local folder, network drive, or shared network share (e.g. `\\server\shared_pm_data` or `D:\TeamData`).
- **Live Folder Manifest & Inspection:** Automatically scans the target folder before applying, showing a live checklist of all data files:
  - `services.json` (with service count)
  - `todos.json` (with task count)
  - `credentials.json` (with account count)
  - `clipboard_history.json` (with clips count)
  - `chat_history.json` and `chat_config.json`
  - `jiggler_config.json`, `ytdl_config.json`, `downloads.json`
- **Instant Hot-Reload:** Switches directories in memory and reloads all stores without restarting the app or killing running services.
- **Automatic Migration:** Offers to copy current files over if the target folder is empty or missing configurations.

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
.\pm_rs.exe        # Direct executable launch
```
