use crate::clipboard::ClipboardWatcher;
use crate::credentials::CredentialStore;
use crate::manager::{save_configured_data_dir, ProcessManager};
use crate::models::{
    ClipboardItem, CredentialItem, DataStorageInfo, DownloadItem, FFmpegEngineInfo, JigglerConfig,
    JigglerStats, ManagerSummary, MediaHistoryItem, MediaJobProgress, MediaProbeResult,
    MediaTaskConfig, ServiceConfig, TodoItem, VideoMetadata, VolumeDetectResult, YtDlpConfig,
};
use crate::todo::TodoStore;
use crate::win32::request_admin_elevation;
use crate::ytdl::YtDlpEngine;
use chrono::{DateTime, Utc};
use std::path::Path;
use std::process::Command;
use std::sync::Arc;
use tauri::State;

pub struct AppState {
    pub pm: Arc<ProcessManager>,
    pub jiggler: Arc<crate::jiggler::JigglerEngine>,
    pub cred_store: Arc<CredentialStore>,
    pub clipboard_watcher: Arc<ClipboardWatcher>,
    pub todo_store: Arc<TodoStore>,
    pub ytdl_engine: Arc<YtDlpEngine>,
    pub ffmpeg_engine: Arc<crate::ffmpeg::FFmpegEngine>,
}

// ==========================================
// 1. Process Supervisor Commands
// ==========================================

#[tauri::command]
pub fn get_manager_summary(state: State<'_, AppState>) -> ManagerSummary {
    state.pm.get_summary()
}

#[tauri::command]
pub fn start_service(name: String, state: State<'_, AppState>) -> bool {
    state.pm.start_service(&name)
}

#[tauri::command]
pub fn stop_service(name: String, state: State<'_, AppState>) -> bool {
    state.pm.stop_service(&name)
}

#[tauri::command]
pub fn restart_service(name: String, state: State<'_, AppState>) -> bool {
    state.pm.restart_service(&name)
}

#[tauri::command]
pub fn add_service(config: ServiceConfig, auto_start: bool, state: State<'_, AppState>) -> bool {
    state.pm.add_service(config, auto_start)
}

#[tauri::command]
pub fn update_service(
    old_name: String,
    config: ServiceConfig,
    restart: bool,
    state: State<'_, AppState>,
) -> bool {
    state.pm.update_service(&old_name, config, restart)
}

#[tauri::command]
pub fn delete_service(name: String, state: State<'_, AppState>) -> bool {
    state.pm.delete_service(&name)
}

#[tauri::command]
pub fn start_all(state: State<'_, AppState>) {
    state.pm.start_all();
}

#[tauri::command]
pub fn stop_all(state: State<'_, AppState>) {
    state.pm.stop_all();
}

#[tauri::command]
pub fn restart_all(state: State<'_, AppState>) {
    state.pm.restart_all();
}

#[tauri::command]
pub fn start_group(group: String, state: State<'_, AppState>) {
    state.pm.start_group(&group);
}

#[tauri::command]
pub fn stop_group(group: String, state: State<'_, AppState>) {
    state.pm.stop_group(&group);
}

#[tauri::command]
pub fn restart_group(group: String, state: State<'_, AppState>) {
    state.pm.restart_group(&group);
}

#[tauri::command]
pub fn rename_group(old_group: String, new_group: String, state: State<'_, AppState>) -> usize {
    let count = state.pm.rename_group(&old_group, &new_group);
    state.todo_store.rename_project(&old_group, &new_group);
    count
}

#[tauri::command]
pub fn move_service(service: String, group: String, state: State<'_, AppState>) -> bool {
    state.pm.move_service_to_group(&service, &group)
}

#[tauri::command]
pub fn dissolve_group(group: String, state: State<'_, AppState>) -> usize {
    state.pm.dissolve_group(&group)
}

#[tauri::command]
pub fn get_logs(name: String, lines: usize, state: State<'_, AppState>) -> String {
    state.pm.get_logs(&name, lines)
}

#[tauri::command]
pub fn clear_logs(name: String, state: State<'_, AppState>) -> bool {
    state.pm.clear_logs(&name)
}

#[tauri::command]
pub fn detect_venv(cwd: String) -> Option<String> {
    ProcessManager::detect_venv_in_dir(&cwd)
}

#[tauri::command]
pub fn request_elevation() -> Result<(), String> {
    request_admin_elevation(&["gui"])
}

// ==========================================
// 2. Mouse Jiggler Commands
// ==========================================

#[tauri::command]
pub fn get_jiggler_config(state: State<'_, AppState>) -> JigglerConfig {
    state.jiggler.get_config()
}

#[tauri::command]
pub fn save_jiggler_config(config: JigglerConfig, state: State<'_, AppState>) {
    state.jiggler.save_config(config);
}

#[tauri::command]
pub fn get_jiggler_stats(state: State<'_, AppState>) -> JigglerStats {
    state.jiggler.get_stats()
}

#[tauri::command]
pub fn toggle_jiggler(state: State<'_, AppState>) -> bool {
    state.jiggler.toggle()
}

#[tauri::command]
pub fn start_jiggler(state: State<'_, AppState>) {
    state.jiggler.start();
}

#[tauri::command]
pub fn stop_jiggler(state: State<'_, AppState>) {
    state.jiggler.stop();
}

// ==========================================
// 3. Credentials & Clipboard Commands
// ==========================================

#[tauri::command]
pub fn list_credentials(search: String, state: State<'_, AppState>) -> Vec<CredentialItem> {
    state.cred_store.list(&search)
}

#[tauri::command]
pub fn add_credential(
    platform: String,
    username: String,
    password: String,
    url: Option<String>,
    notes: Option<String>,
    state: State<'_, AppState>,
) -> CredentialItem {
    state
        .cred_store
        .add(&platform, &username, &password, url, notes)
}

#[tauri::command]
pub fn update_credential(id: String, item: CredentialItem, state: State<'_, AppState>) -> bool {
    state.cred_store.update(&id, item)
}

#[tauri::command]
pub fn delete_credential(id: String, state: State<'_, AppState>) -> bool {
    state.cred_store.delete(&id)
}

#[tauri::command]
pub fn generate_password(
    length: usize,
    upper: bool,
    lower: bool,
    digits: bool,
    symbols: bool,
) -> String {
    CredentialStore::generate_password(length, upper, lower, digits, symbols)
}

#[tauri::command]
pub fn list_clipboard(
    search: String,
    pinned_only: bool,
    state: State<'_, AppState>,
) -> Vec<ClipboardItem> {
    state.clipboard_watcher.list(&search, pinned_only)
}

#[tauri::command]
pub fn toggle_clip_pin(id: String, state: State<'_, AppState>) -> bool {
    state.clipboard_watcher.toggle_pin(&id)
}

#[tauri::command]
pub fn delete_clip(id: String, state: State<'_, AppState>) -> bool {
    state.clipboard_watcher.delete(&id)
}

#[tauri::command]
pub fn clear_unpinned_clips(state: State<'_, AppState>) -> usize {
    state.clipboard_watcher.clear_unpinned()
}

#[tauri::command]
pub fn copy_to_clipboard(text: String) -> Result<(), String> {
    let mut cmd = Command::new("powershell");
    cmd.args(["-NoProfile", "-Command", "Set-Clipboard -Value $input"]);
    cmd.stdin(std::process::Stdio::piped());

    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x08000000); // CREATE_NO_WINDOW
    }

    if let Ok(mut child) = cmd.spawn() {
        if let Some(mut stdin) = child.stdin.take() {
            use std::io::Write;
            let _ = stdin.write_all(text.as_bytes());
        }
        let _ = child.wait();
        Ok(())
    } else {
        Err("Failed to copy to clipboard".to_string())
    }
}

// ==========================================
// 4. Tasks & Kanban Commands
// ==========================================

#[tauri::command]
pub fn list_todos(
    search: String,
    project: Option<String>,
    status: Option<String>,
    state: State<'_, AppState>,
) -> Vec<TodoItem> {
    state
        .todo_store
        .list(&search, project.as_deref(), status.as_deref())
}

#[tauri::command]
pub fn add_todo(
    title: String,
    project: Option<String>,
    priority: Option<String>,
    status: Option<String>,
    notify_at: Option<String>,
    state: State<'_, AppState>,
) -> TodoItem {
    let parsed_notify = notify_at.and_then(|s| DateTime::parse_from_rfc3339(&s).ok().map(|d| d.with_timezone(&Utc)));
    state
        .todo_store
        .add(&title, project, priority, status, parsed_notify)
}

#[tauri::command]
pub fn update_todo_status(id: String, status: String, state: State<'_, AppState>) -> bool {
    state.todo_store.update_status(&id, &status)
}

#[tauri::command]
pub fn toggle_todo(id: String, state: State<'_, AppState>) -> bool {
    state.todo_store.toggle(&id)
}

#[tauri::command]
pub fn delete_todo(id: String, state: State<'_, AppState>) -> bool {
    state.todo_store.delete(&id)
}

#[tauri::command]
pub fn clear_completed_todos(state: State<'_, AppState>) -> usize {
    state.todo_store.clear_completed()
}

// ==========================================
// 5. Downloader Commands
// ==========================================

#[tauri::command]
pub fn get_ytdl_config(state: State<'_, AppState>) -> YtDlpConfig {
    state.ytdl_engine.get_config()
}

#[tauri::command]
pub fn save_ytdl_config(config: YtDlpConfig, state: State<'_, AppState>) {
    state.ytdl_engine.save_config(config);
}

#[tauri::command]
pub fn get_download_history(state: State<'_, AppState>) -> Vec<DownloadItem> {
    state.ytdl_engine.get_history()
}

#[tauri::command]
pub async fn fetch_video_metadata(
    url: String,
    state: State<'_, AppState>,
) -> Result<VideoMetadata, String> {
    state.ytdl_engine.fetch_metadata(&url).await
}

#[tauri::command]
pub fn start_download(
    url: String,
    media_type: String,
    container: String,
    quality: String,
    custom_dir: Option<String>,
    is_playlist: bool,
    state: State<'_, AppState>,
) -> Result<DownloadItem, String> {
    state.ytdl_engine.start_download(
        &url,
        &media_type,
        &container,
        &quality,
        custom_dir,
        is_playlist,
    )
}

#[tauri::command]
pub fn cancel_download(state: State<'_, AppState>) -> bool {
    state.ytdl_engine.cancel_active_download()
}

#[tauri::command]
pub fn open_folder(path: String) -> Result<(), String> {
    let p = Path::new(&path);
    let target = if p.is_file() {
        p.parent().unwrap_or(p)
    } else {
        p
    };
    let _ = Command::new("explorer").arg(target).spawn();
    Ok(())
}

#[tauri::command]
pub fn open_file(path: String) -> Result<(), String> {
    let _ = Command::new("explorer").arg(&path).spawn();
    Ok(())
}

// ==========================================
// 6. FFmpeg Media Studio Commands
// ==========================================

#[tauri::command]
pub fn get_ffmpeg_info(state: State<'_, AppState>) -> FFmpegEngineInfo {
    state.ffmpeg_engine.get_engine_info()
}

#[tauri::command]
pub fn probe_media_file(file_path: String, state: State<'_, AppState>) -> Result<MediaProbeResult, String> {
    state.ffmpeg_engine.probe_file(&file_path)
}

#[tauri::command]
pub fn detect_media_volume(file_path: String, state: State<'_, AppState>) -> Result<VolumeDetectResult, String> {
    state.ffmpeg_engine.detect_volume(&file_path)
}

#[tauri::command]
pub fn start_media_conversion(
    config: MediaTaskConfig,
    state: State<'_, AppState>,
) -> Result<String, String> {
    state.ffmpeg_engine.start_conversion(config)
}

#[tauri::command]
pub fn cancel_media_conversion(state: State<'_, AppState>) -> bool {
    state.ffmpeg_engine.cancel_active_conversion()
}

#[tauri::command]
pub fn get_media_progress(state: State<'_, AppState>) -> MediaJobProgress {
    state.ffmpeg_engine.get_progress()
}

#[tauri::command]
pub fn get_media_history(state: State<'_, AppState>) -> Vec<MediaHistoryItem> {
    state.ffmpeg_engine.get_history()
}

#[tauri::command]
pub fn clear_media_history(state: State<'_, AppState>) -> bool {
    state.ffmpeg_engine.clear_history();
    true
}

#[tauri::command]
pub fn pick_media_file(filter_type: String, state: State<'_, AppState>) -> Option<String> {
    state.ffmpeg_engine.pick_media_file(&filter_type)
}

#[tauri::command]
pub fn pick_media_directory(state: State<'_, AppState>) -> Option<String> {
    state.ffmpeg_engine.pick_output_directory()
}

#[tauri::command]
pub fn open_media_path(path: String, state: State<'_, AppState>) -> bool {
    state.ffmpeg_engine.open_file_or_dir(&path);
    true
}

#[tauri::command]
pub fn reveal_media_in_explorer(file_path: String, state: State<'_, AppState>) -> bool {
    state.ffmpeg_engine.reveal_in_explorer(&file_path);
    true
}

// ==========================================
// 7. Data Storage Configuration Commands
// ==========================================

fn inspect_dir_info(dir: &Path) -> DataStorageInfo {
    let services_file = dir.join("services.json");
    let todos_file = dir.join("todos.json");
    let creds_file = dir.join("credentials.json");
    let clip_file = dir.join("clipboard_history.json");
    let jiggler_file = dir.join("jiggler_config.json");
    let ytdl_file = dir.join("ytdl_config.json");
    let downloads_file = dir.join("downloads.json");
    let media_file = dir.join("media_history.json");

    let has_services = services_file.exists();
    let has_todos = todos_file.exists();
    let has_credentials = creds_file.exists();
    let has_clipboard = clip_file.exists();
    let has_jiggler = jiggler_file.exists();
    let has_downloader = ytdl_file.exists() || downloads_file.exists();
    let has_media = media_file.exists();

    let mut services_count = 0;
    if has_services {
        if let Ok(c) = std::fs::read_to_string(&services_file) {
            if let Ok(list) = serde_json::from_str::<Vec<ServiceConfig>>(&c) {
                services_count = list.len();
            }
        }
    }

    let mut todos_count = 0;
    if has_todos {
        if let Ok(c) = std::fs::read_to_string(&todos_file) {
            if let Ok(list) = serde_json::from_str::<Vec<TodoItem>>(&c) {
                todos_count = list.len();
            }
        }
    }

    let mut credentials_count = 0;
    if has_credentials {
        if let Ok(c) = std::fs::read_to_string(&creds_file) {
            if let Ok(list) = serde_json::from_str::<Vec<CredentialItem>>(&c) {
                credentials_count = list.len();
            }
        }
    }

    let mut clips_count = 0;
    if has_clipboard {
        if let Ok(c) = std::fs::read_to_string(&clip_file) {
            if let Ok(list) = serde_json::from_str::<Vec<ClipboardItem>>(&c) {
                clips_count = list.len();
            }
        }
    }

    DataStorageInfo {
        current_dir: dir.to_string_lossy().to_string(),
        has_services,
        has_todos,
        has_credentials,
        has_clipboard,
        has_jiggler,
        has_downloader,
        has_media,
        services_count,
        todos_count,
        credentials_count,
        clips_count,
    }
}

#[tauri::command]
pub fn get_storage_info(state: State<'_, AppState>) -> DataStorageInfo {
    let current_dir = state.pm.base_dir.read().unwrap().clone();
    inspect_dir_info(&current_dir)
}

#[tauri::command]
pub fn inspect_storage_dir(path: String) -> Result<DataStorageInfo, String> {
    let p = Path::new(&path);
    if !p.exists() || !p.is_dir() {
        return Err("Directory does not exist".to_string());
    }
    Ok(inspect_dir_info(p))
}

#[tauri::command]
pub fn pick_storage_folder() -> Option<String> {
    let script = r#"Add-Type -AssemblyName System.Windows.Forms; $f = New-Object System.Windows.Forms.FolderBrowserDialog; $f.Description = 'Select Data Storage Folder'; if($f.ShowDialog() -eq [System.Windows.Forms.DialogResult]::OK){ Write-Output $f.SelectedPath }"#;

    let mut cmd = Command::new("powershell");
    cmd.args(["-NoProfile", "-Command", script]);

    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x08000000); // CREATE_NO_WINDOW
    }

    if let Ok(out) = cmd.output() {
        let path = String::from_utf8_lossy(&out.stdout).trim().to_string();
        if !path.is_empty() && Path::new(&path).is_dir() {
            return Some(path);
        }
    }
    None
}

#[tauri::command]
pub fn set_storage_dir(
    new_dir: String,
    copy_current: bool,
    state: State<'_, AppState>,
) -> Result<DataStorageInfo, String> {
    let target = Path::new(&new_dir);
    if !target.exists() {
        std::fs::create_dir_all(target)
            .map_err(|e| format!("Failed to create directory: {}", e))?;
    }

    let current_dir = state.pm.base_dir.read().unwrap().clone();

    if copy_current {
        let files_to_copy = [
            "services.json",
            "todos.json",
            "credentials.json",
            "clipboard_history.json",
            "jiggler_config.json",
            "ytdl_config.json",
            "downloads.json",
            "media_history.json",
        ];
        for f in &files_to_copy {
            let src = current_dir.join(f);
            let dst = target.join(f);
            if src.exists() && !dst.exists() {
                let _ = std::fs::copy(&src, &dst);
            }
        }
    }

    save_configured_data_dir(target)?;

    // Reload all engine stores
    state.pm.reload_from_dir(target);
    state.todo_store.reload_from_dir(target);
    state.cred_store.reload_from_dir(target);
    state.clipboard_watcher.reload_from_dir(target);
    state.jiggler.reload_from_dir(target);
    state.ytdl_engine.reload_from_dir(target);
    state.ffmpeg_engine.reload_from_dir(target);

    Ok(inspect_dir_info(target))
}

