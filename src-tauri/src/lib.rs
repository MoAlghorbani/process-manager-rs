pub mod cli;
pub mod clipboard;
pub mod commands;
pub mod credentials;
pub mod embedded;
pub mod ffmpeg;
pub mod hotkeys;
pub mod jiggler;
pub mod manager;
pub mod models;
pub mod todo;
pub mod win32;
pub mod ytdl;

use crate::clipboard::ClipboardWatcher;
use crate::commands::AppState;
use crate::credentials::CredentialStore;
use crate::ffmpeg::FFmpegEngine;
use crate::hotkeys::HotkeyManager;
use crate::jiggler::JigglerEngine;
use crate::manager::ProcessManager;
use crate::todo::TodoStore;
use crate::ytdl::YtDlpEngine;
use std::sync::Arc;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    // Background extraction of embedded tools (ffmpeg, ffprobe, yt-dlp) if not already present
    std::thread::spawn(|| {
        embedded::ensure_embedded_tools();
    });

    let pm = Arc::new(ProcessManager::new());
    let jiggler = Arc::new(JigglerEngine::new());
    let cred_store = Arc::new(CredentialStore::new());
    let clipboard_watcher = Arc::new(ClipboardWatcher::new());
    let todo_store = Arc::new(TodoStore::new());
    let ytdl_engine = Arc::new(YtDlpEngine::new());
    let ffmpeg_engine = Arc::new(FFmpegEngine::new());
    let hotkey_mgr = Arc::new(HotkeyManager::new(pm.clone(), jiggler.clone()));

    // Start background watchers & hotkey listener
    clipboard_watcher.start();
    todo_store.start_notification_worker();
    hotkey_mgr.start();

    let state = AppState {
        pm: pm.clone(),
        jiggler: jiggler.clone(),
        cred_store: cred_store.clone(),
        clipboard_watcher: clipboard_watcher.clone(),
        todo_store: todo_store.clone(),
        ytdl_engine: ytdl_engine.clone(),
        ffmpeg_engine: ffmpeg_engine.clone(),
        hotkey_mgr: hotkey_mgr.clone(),
    };

    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .manage(state)
        .invoke_handler(tauri::generate_handler![
            // Process Supervisor
            commands::get_manager_summary,
            commands::start_service,
            commands::stop_service,
            commands::restart_service,
            commands::toggle_service,
            commands::add_service,
            commands::update_service,
            commands::delete_service,
            commands::start_all,
            commands::stop_all,
            commands::restart_all,
            commands::start_group,
            commands::stop_group,
            commands::restart_group,
            commands::rename_group,
            commands::move_service,
            commands::dissolve_group,
            commands::get_logs,
            commands::clear_logs,
            commands::detect_venv,
            commands::request_elevation,
            // Jiggler
            commands::get_jiggler_config,
            commands::save_jiggler_config,
            commands::get_jiggler_stats,
            commands::toggle_jiggler,
            commands::start_jiggler,
            commands::stop_jiggler,
            // Credentials & Clipboard
            commands::list_credentials,
            commands::add_credential,
            commands::update_credential,
            commands::delete_credential,
            commands::generate_password,
            commands::list_clipboard,
            commands::toggle_clip_pin,
            commands::delete_clip,
            commands::clear_unpinned_clips,
            commands::copy_to_clipboard,
            // Tasks / Kanban
            commands::list_todos,
            commands::add_todo,
            commands::update_todo_status,
            commands::toggle_todo,
            commands::delete_todo,
            commands::clear_completed_todos,
            // Downloader
            commands::get_ytdl_config,
            commands::save_ytdl_config,
            commands::get_download_history,
            commands::fetch_video_metadata,
            commands::start_download,
            commands::cancel_download,
            commands::open_folder,
            commands::open_file,
            commands::open_url,
            // Port Killer & Scavenger
            commands::get_active_ports,
            commands::kill_port_task,
            commands::kill_process_by_pid,
            // Media Studio (FFmpeg)
            commands::get_ffmpeg_info,
            commands::probe_media_file,
            commands::detect_media_volume,
            commands::start_media_conversion,
            commands::cancel_media_conversion,
            commands::get_media_progress,
            commands::get_media_history,
            commands::clear_media_history,
            commands::pick_media_file,
            commands::pick_media_directory,
            commands::open_media_path,
            commands::reveal_media_in_explorer,
            // Storage Settings
            commands::get_storage_info,
            commands::inspect_storage_dir,
            commands::pick_storage_folder,
            commands::set_storage_dir,
        ])
        .run(tauri::generate_context!())
        .expect("error while running Process Manager application");
}
