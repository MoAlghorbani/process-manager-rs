use process_manager_rs_lib::cli::handle_cli;
use process_manager_rs_lib::clipboard::ClipboardWatcher;
use process_manager_rs_lib::credentials::CredentialStore;
use process_manager_rs_lib::manager::ProcessManager;
use process_manager_rs_lib::todo::TodoStore;
use process_manager_rs_lib::win32::check_single_instance;
use process_manager_rs_lib::ytdl::YtDlpEngine;
use std::sync::Arc;
use windows_sys::Win32::Foundation::HWND;
use windows_sys::Win32::UI::WindowsAndMessaging::{ShowWindow, SW_HIDE};

#[link(name = "kernel32")]
unsafe extern "system" {
    fn GetConsoleWindow() -> HWND;
}

fn main() {
    let args: Vec<String> = std::env::args().collect();

    // Check CLI first
    let pm = Arc::new(ProcessManager::new());
    let cred = Arc::new(CredentialStore::new());
    let clips = Arc::new(ClipboardWatcher::new());
    let todo = Arc::new(TodoStore::new());
    let ytdl = Arc::new(YtDlpEngine::new());

    if handle_cli(&args, &pm, &cred, &clips, &todo, &ytdl) {
        return;
    }

    // Background extraction of embedded tools (ffmpeg, ffprobe, yt-dlp) for Desktop GUI
    std::thread::spawn(|| {
        process_manager_rs_lib::embedded::ensure_embedded_tools();
    });

    // Hide console window when launching Desktop GUI
    unsafe {
        let hwnd = GetConsoleWindow();
        if !hwnd.is_null() {
            ShowWindow(hwnd, SW_HIDE);
        }
    }

    // Single-instance check for desktop GUI
    const MUTEX_NAME: &str = "Local\\ProcessManager_RustGUI_SingleInstance";
    const WIN_TITLE: &str = "Windows Process Manager & Service Supervisor (Rust)";

    if !check_single_instance(MUTEX_NAME, WIN_TITLE) {
        return;
    }

    process_manager_rs_lib::run();
}
