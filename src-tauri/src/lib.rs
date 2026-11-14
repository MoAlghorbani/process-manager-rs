pub mod chat;
pub mod cli;
pub mod clipboard;
pub mod commands;
pub mod credentials;
pub mod hotkeys;
pub mod jiggler;
pub mod manager;
pub mod media;
pub mod models;
pub mod todo;
pub mod win32;
pub mod ytdl;

use crate::chat::ChatEngine;
use crate::clipboard::ClipboardWatcher;
use crate::commands::AppState;
use crate::credentials::CredentialStore;
use crate::hotkeys::HotkeyManager;
use crate::jiggler::JigglerEngine;
use crate::manager::ProcessManager;
use crate::media::MediaServer;
use crate::todo::TodoStore;
use crate::ytdl::YtDlpEngine;
use std::sync::Arc;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let pm = Arc::new(ProcessManager::new());
    let jiggler = Arc::new(JigglerEngine::new());
    let cred_store = Arc::new(CredentialStore::new());
    let clipboard_watcher = Arc::new(ClipboardWatcher::new());
    let todo_store = Arc::new(TodoStore::new());
    let ytdl_engine = Arc::new(YtDlpEngine::new());
    let chat_engine = Arc::new(ChatEngine::new());
    let hotkey_mgr = Arc::new(HotkeyManager::new(pm.clone(), jiggler.clone()));
    let media_server = Arc::new(MediaServer::new());

    // Start background watchers & hotkey listener
    clipboard_watcher.start();
    todo_store.start_notification_worker();
    chat_engine.start();
    hotkey_mgr.start();
    media_server.start();

    let state = AppState {
        pm: pm.clone(),
        jiggler: jiggler.clone(),
        cred_store: cred_store.clone(),
        clipboard_watcher: clipboard_watcher.clone(),
        todo_store: todo_store.clone(),
        ytdl_engine: ytdl_engine.clone(),
        chat_engine: chat_engine.clone(),
        hotkey_mgr: hotkey_mgr.clone(),
        media_server: media_server.clone(),
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
            commands::open_live_cmd,
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
            // LAN Chat
            commands::get_chat_config,
            commands::save_chat_config,
            commands::get_chat_peers,
            commands::get_chat_messages,
            commands::send_chat_message,
            commands::send_chat_typing,
            commands::send_chat_buzz,
            commands::get_chat_typing_status,
            commands::clear_chat_history,
            // LAN File Transfers
            commands::pick_file_to_send,
            commands::pick_save_file_path,
            commands::send_file_offer,
            commands::accept_file_transfer,
            commands::decline_file_transfer,
            commands::cancel_file_transfer,
            commands::get_file_transfers,
            commands::send_desktop_notification,
            commands::bring_app_to_front,
            // Storage Settings
            commands::get_storage_info,
            commands::inspect_storage_dir,
            commands::pick_storage_folder,
            commands::set_storage_dir,
            // Media Player & Workspace
            commands::get_media_server_port,
            commands::pick_media_workspace,
            commands::scan_media_workspace,
            commands::get_saved_media_workspace,
            commands::save_media_workspace,
        ])
        .run(tauri::generate_context!())
        .expect("error while running Process Manager application");
}
