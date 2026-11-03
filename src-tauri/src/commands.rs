use crate::chat::ChatEngine;
use crate::clipboard::ClipboardWatcher;
use crate::credentials::CredentialStore;
use crate::manager::{save_configured_data_dir, ProcessManager};
use crate::models::{
    ChatConfig, ChatMessage, ClipboardItem, CredentialItem, DataStorageInfo, DownloadItem,
    FileTransferInfo, JigglerConfig, JigglerStats, ManagerSummary, PeerInfo, PickedFileInfo,
    ServiceConfig, TodoItem, VideoMetadata, YtDlpConfig,
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
    pub chat_engine: Arc<ChatEngine>,
    pub hotkey_mgr: Arc<crate::hotkeys::HotkeyManager>,
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
pub fn toggle_service(name: String, state: State<'_, AppState>) -> bool {
    state.pm.toggle_service(&name)
}

#[tauri::command]
pub fn add_service(config: ServiceConfig, auto_start: bool, state: State<'_, AppState>) -> bool {
    let res = state.pm.add_service(config, auto_start);
    state.hotkey_mgr.reload();
    res
}

#[tauri::command]
pub fn update_service(
    old_name: String,
    config: ServiceConfig,
    restart: bool,
    state: State<'_, AppState>,
) -> bool {
    let res = state.pm.update_service(&old_name, config, restart);
    state.hotkey_mgr.reload();
    res
}

#[tauri::command]
pub fn delete_service(name: String, state: State<'_, AppState>) -> bool {
    let res = state.pm.delete_service(&name);
    state.hotkey_mgr.reload();
    res
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
pub fn open_live_cmd(name: String, state: State<'_, AppState>) -> Result<(), String> {
    let log_file = state.pm.logs_dir.read().unwrap().join(format!("{}.log", name));
    let log_path_str = log_file.to_string_lossy().to_string();

    #[cfg(windows)]
    {
        use std::process::Command;
        let safe_log_path = log_path_str.replace("'", "''");
        let ps_cmd = format!(
            "$host.UI.RawUI.WindowTitle = 'Live Console: {name}'; \
             Write-Host '==================================================' -ForegroundColor DarkCyan; \
             Write-Host '  LIVE STREAMING PROCESS LOG: {name}' -ForegroundColor Green; \
             Write-Host '  Log File: {path}' -ForegroundColor Gray; \
             Write-Host '  Press Ctrl+C to exit console viewer' -ForegroundColor DarkYellow; \
             Write-Host '==================================================' -ForegroundColor DarkCyan; \
             if (Test-Path '{path}') {{ \
                 Get-Content -Path '{path}' -Tail 200 -Wait \
             }} else {{ \
                 Write-Host 'Log file does not exist yet. Waiting...' -ForegroundColor Yellow; \
                 while (-not (Test-Path '{path}')) {{ Start-Sleep -Milliseconds 500 }}; \
                 Get-Content -Path '{path}' -Tail 200 -Wait \
             }}",
            name = name,
            path = safe_log_path
        );

        let res = Command::new("cmd.exe")
            .args(["/C", "start", &format!("Logs - {}", name), "powershell.exe", "-NoExit", "-Command", &ps_cmd])
            .spawn();

        match res {
            Ok(_) => Ok(()),
            Err(e) => Err(format!("Failed to spawn console window: {}", e)),
        }
    }

    #[cfg(not(windows))]
    {
        Ok(())
    }
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
    state.hotkey_mgr.reload();
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

#[tauri::command]
pub fn open_url(url: String) -> Result<(), String> {
    let target = if url.starts_with("http://") || url.starts_with("https://") {
        url
    } else {
        format!("http://{}", url)
    };

    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        let mut cmd = Command::new("rundll32");
        cmd.args(["url.dll,FileProtocolHandler", &target]);
        cmd.creation_flags(0x08000000); // CREATE_NO_WINDOW
        let _ = cmd.spawn().map_err(|e| e.to_string())?;
        Ok(())
    }
    #[cfg(not(windows))]
    {
        let _ = Command::new("open").arg(&target).spawn().map_err(|e| e.to_string())?;
        Ok(())
    }
}

// ==========================================
// Port Killer & Process Scavenger Commands
// ==========================================

#[tauri::command]
pub fn get_active_ports() -> Vec<crate::win32::PortProcessInfo> {
    crate::win32::get_active_listening_ports()
}

#[tauri::command]
pub fn kill_port_task(port: u16) -> usize {
    crate::win32::free_port(port)
}

#[tauri::command]
pub fn kill_process_by_pid(pid: u32) -> bool {
    crate::win32::kill_task_by_pid(pid)
}

// ==========================================
// 6. LAN Chat Commands
// ==========================================

#[tauri::command]
pub fn get_chat_config(state: State<'_, AppState>) -> ChatConfig {
    state.chat_engine.get_config()
}

#[tauri::command]
pub fn save_chat_config(config: ChatConfig, state: State<'_, AppState>) -> Result<(), String> {
    state.chat_engine.save_config(config);
    Ok(())
}

#[tauri::command]
pub fn get_chat_peers(state: State<'_, AppState>) -> Vec<PeerInfo> {
    state.chat_engine.get_peers()
}

#[tauri::command]
pub fn get_chat_messages(recipient: Option<String>, state: State<'_, AppState>) -> Vec<ChatMessage> {
    state.chat_engine.get_messages(recipient.as_deref())
}

#[tauri::command]
pub fn send_chat_message(
    recipient_id: String,
    content: String,
    state: State<'_, AppState>,
) -> Result<ChatMessage, String> {
    state.chat_engine.send_message(&recipient_id, &content)
}

#[tauri::command]
pub fn send_chat_typing(recipient_id: String, state: State<'_, AppState>) -> Result<(), String> {
    state.chat_engine.send_typing(&recipient_id);
    Ok(())
}

#[tauri::command]
pub fn get_chat_typing_status(recipient: String, state: State<'_, AppState>) -> Vec<String> {
    state.chat_engine.get_typing_users(&recipient)
}

#[tauri::command]
pub fn clear_chat_history(recipient: Option<String>, state: State<'_, AppState>) -> Result<(), String> {
    state.chat_engine.clear_history(recipient.as_deref());
    Ok(())
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
    let chat_file = dir.join("chat_history.json");

    let has_services = services_file.exists();
    let has_todos = todos_file.exists();
    let has_credentials = creds_file.exists();
    let has_clipboard = clip_file.exists();
    let has_jiggler = jiggler_file.exists();
    let has_downloader = ytdl_file.exists() || downloads_file.exists();
    let has_chat = chat_file.exists();

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

    let mut messages_count = 0;
    if has_chat {
        if let Ok(c) = std::fs::read_to_string(&chat_file) {
            if let Ok(list) = serde_json::from_str::<Vec<ChatMessage>>(&c) {
                messages_count = list.len();
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
        has_chat,
        services_count,
        todos_count,
        credentials_count,
        clips_count,
        messages_count,
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
            "chat_history.json",
            "chat_config.json",
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
    state.chat_engine.reload_from_dir(target);

    Ok(inspect_dir_info(target))
}

#[tauri::command]
pub fn pick_file_to_send() -> Option<PickedFileInfo> {
    let script = r#"Add-Type -AssemblyName System.Windows.Forms; $f = New-Object System.Windows.Forms.OpenFileDialog; $f.Title = 'Select File to Send over LAN'; $f.Filter = 'All Files (*.*)|*.*'; if($f.ShowDialog() -eq [System.Windows.Forms.DialogResult]::OK){ Write-Output $f.FileName }"#;

    let mut cmd = Command::new("powershell");
    cmd.args(["-NoProfile", "-Command", script]);

    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x08000000); // CREATE_NO_WINDOW
    }

    if let Ok(out) = cmd.output() {
        let raw = String::from_utf8_lossy(&out.stdout).trim().to_string();
        if !raw.is_empty() {
            let p = Path::new(&raw);
            if p.is_file() {
                if let Ok(meta) = std::fs::metadata(p) {
                    let name = p
                        .file_name()
                        .map(|n| n.to_string_lossy().to_string())
                        .unwrap_or_else(|| "file.bin".to_string());
                    return Some(PickedFileInfo {
                        path: raw,
                        name,
                        size: meta.len(),
                    });
                }
            }
        }
    }
    None
}

#[tauri::command]
pub fn pick_save_file_path(default_name: String) -> Option<String> {
    let safe_name = default_name.replace('\'', "''");
    let script = format!(
        "Add-Type -AssemblyName System.Windows.Forms; $f = New-Object System.Windows.Forms.SaveFileDialog; $f.Title = 'Save Received File'; $f.FileName = '{safe_name}'; $f.Filter = 'All Files (*.*)|*.*'; if($f.ShowDialog() -eq [System.Windows.Forms.DialogResult]::OK){{ Write-Output $f.FileName }}"
    );

    let mut cmd = Command::new("powershell");
    cmd.args(["-NoProfile", "-Command", &script]);

    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x08000000); // CREATE_NO_WINDOW
    }

    if let Ok(out) = cmd.output() {
        let raw = String::from_utf8_lossy(&out.stdout).trim().to_string();
        if !raw.is_empty() {
            return Some(raw);
        }
    }
    None
}

#[tauri::command]
pub fn send_file_offer(
    recipient_id: String,
    file_path: String,
    state: State<'_, AppState>,
) -> Result<FileTransferInfo, String> {
    state.chat_engine.send_file_offer(&recipient_id, &file_path)
}

#[tauri::command]
pub fn accept_file_transfer(
    transfer_id: String,
    save_path: Option<String>,
    state: State<'_, AppState>,
) -> Result<(), String> {
    state.chat_engine.accept_file_transfer(&transfer_id, save_path)
}

#[tauri::command]
pub fn decline_file_transfer(
    transfer_id: String,
    state: State<'_, AppState>,
) -> Result<(), String> {
    state.chat_engine.decline_file_transfer(&transfer_id)
}

#[tauri::command]
pub fn cancel_file_transfer(
    transfer_id: String,
    state: State<'_, AppState>,
) -> Result<(), String> {
    state.chat_engine.cancel_file_transfer(&transfer_id)
}

#[tauri::command]
pub fn get_file_transfers(
    state: State<'_, AppState>,
) -> Vec<FileTransferInfo> {
    state.chat_engine.get_file_transfers()
}

#[tauri::command]
pub fn send_desktop_notification(title: String, body: String) {
    crate::win32::show_desktop_notification(&title, &body);
}

#[tauri::command]
pub fn send_chat_buzz(
    recipient_id: String,
    buzz_type: String,
    state: State<'_, AppState>,
) -> Result<ChatMessage, String> {
    state.chat_engine.send_buzz(&recipient_id, &buzz_type)
}
