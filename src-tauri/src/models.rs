use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

// ==========================================
// 1. Service Supervisor Models
// ==========================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServiceConfig {
    pub name: String,
    #[serde(default = "default_group")]
    pub group: String,
    pub command: String,
    #[serde(default)]
    pub cwd: String,
    #[serde(default)]
    pub port: Option<u16>,
    #[serde(default = "default_true")]
    pub auto_restart: bool,
    #[serde(default)]
    pub venv: Option<String>,
    #[serde(default)]
    pub env: Option<HashMap<String, String>>,
    #[serde(default)]
    pub saved_status: Option<String>,
    #[serde(default)]
    pub shortcut: Option<String>,
}

fn default_group() -> String {
    "default".to_string()
}

fn default_true() -> bool {
    true
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServiceStatusReport {
    pub name: String,
    pub group: String,
    pub command: String,
    pub cwd: String,
    pub port: Option<u16>,
    pub auto_restart: bool,
    pub venv: Option<String>,
    pub shortcut: Option<String>,
    pub pid: Option<u32>,
    pub status: String, // "stopped", "running", "crashed", "restarting"
    pub restarts: u32,
    pub uptime_seconds: u64,
    pub log_file: String,
    pub last_exit_code: Option<i32>,
    pub memory_mb: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GroupSummary {
    pub name: String,
    pub total: usize,
    pub running: usize,
    pub services: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ManagerSummary {
    pub admin: bool,
    pub services: Vec<ServiceStatusReport>,
    pub groups: Vec<GroupSummary>,
}

// ==========================================
// 2. Mouse Jiggler & Anti-Idle Models
// ==========================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JigglerConfig {
    pub shortcut: String,
    pub mouse_enabled: bool,
    pub mouse_interval_sec: u64,
    pub mouse_move_mode: String, // "zen", "gentle", "orbit", "api_only"
    pub key_enabled: bool,
    pub key_interval_sec: u64,
    pub key_choice: String, // "f15", "shift", "control"
    pub idle_guard_enabled: bool,
    pub idle_threshold_sec: u64,
    pub keep_awake_enabled: bool,
}

impl Default for JigglerConfig {
    fn default() -> Self {
        Self {
            shortcut: "Ctrl+Alt+J".to_string(),
            mouse_enabled: true,
            mouse_interval_sec: 10,
            mouse_move_mode: "orbit".to_string(),
            key_enabled: true,
            key_interval_sec: 30,
            key_choice: "f15".to_string(),
            idle_guard_enabled: false,
            idle_threshold_sec: 15,
            keep_awake_enabled: true,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JigglerStats {
    pub running: bool,
    pub total_mouse_moves: u64,
    pub total_key_taps: u64,
    pub next_mouse_move_sec: u64,
    pub next_key_tap_sec: u64,
    pub current_idle_sec: f64,
    pub last_action_msg: String,
}

impl Default for JigglerStats {
    fn default() -> Self {
        Self {
            running: false,
            total_mouse_moves: 0,
            total_key_taps: 0,
            next_mouse_move_sec: 0,
            next_key_tap_sec: 0,
            current_idle_sec: 0.0,
            last_action_msg: "Standby".to_string(),
        }
    }
}

// ==========================================
// 3. Credentials & Clipboard Models
// ==========================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CredentialItem {
    pub id: String,
    pub platform: String,
    pub username: String,
    pub password: String,
    #[serde(default)]
    pub url: Option<String>,
    #[serde(default)]
    pub notes: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClipboardItem {
    pub id: String,
    pub content: String,
    pub timestamp: DateTime<Utc>,
    pub pinned: bool,
    pub length: usize,
}

// ==========================================
// 4. Kanban Tasks & Todo Models
// ==========================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TodoItem {
    pub id: String,
    pub title: String,
    #[serde(default)]
    pub project: Option<String>,
    pub status: String, // "todo", "in_progress", "done"
    pub completed: bool,
    #[serde(default = "default_normal_priority")]
    pub priority: Option<String>, // "high", "normal", "low"
    #[serde(default)]
    pub notify_at: Option<DateTime<Utc>>,
    #[serde(default)]
    pub notified: Option<bool>,
    pub created_at: DateTime<Utc>,
    #[serde(default)]
    pub completed_at: Option<DateTime<Utc>>,
}

fn default_normal_priority() -> Option<String> {
    Some("normal".to_string())
}

// ==========================================
// 5. Downloader Models
// ==========================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct YtDlpConfig {
    pub output_dir: String,
    pub last_format: String,
    pub last_media_type: String,
    pub last_video_format: String,
    pub last_video_quality: String,
    pub last_audio_format: String,
    pub last_audio_quality: String,
    pub download_playlist: bool,
}

impl Default for YtDlpConfig {
    fn default() -> Self {
        let default_dir = dirs_fallback_downloads();
        Self {
            output_dir: default_dir,
            last_format: "video_best".to_string(),
            last_media_type: "video".to_string(),
            last_video_format: "mp4".to_string(),
            last_video_quality: "1080p".to_string(),
            last_audio_format: "mp3".to_string(),
            last_audio_quality: "320k".to_string(),
            download_playlist: true,
        }
    }
}

pub fn dirs_fallback_downloads() -> String {
    if let Ok(user_profile) = std::env::var("USERPROFILE") {
        let p = std::path::Path::new(&user_profile).join("Downloads");
        if p.exists() {
            return p.to_string_lossy().to_string();
        }
    }
    std::env::current_dir()
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or_else(|_| ".".to_string())
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VideoMetadata {
    pub id: String,
    pub title: String,
    pub uploader: String,
    pub duration: u64,
    pub duration_string: String,
    pub thumbnail: String,
    pub view_count: i64,
    pub webpage_url: String,
    pub max_resolution: Option<String>,
    pub available_resolutions: Vec<String>,
    pub is_playlist: bool,
    pub playlist_count: Option<usize>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DownloadItem {
    pub id: String,
    pub url: String,
    pub title: String,
    #[serde(default)]
    pub uploader: Option<String>,
    #[serde(default)]
    pub duration: Option<String>,
    #[serde(default)]
    pub thumbnail: Option<String>,
    #[serde(default)]
    pub media_type: Option<String>,
    #[serde(default)]
    pub container: Option<String>,
    #[serde(default)]
    pub quality: Option<String>,
    pub format: String,
    pub format_label: String,
    pub output_dir: String,
    #[serde(default)]
    pub file_path: Option<String>,
    #[serde(default)]
    pub file_size: Option<String>,
    pub status: String, // "queued", "fetching", "downloading", "completed", "error", "cancelled"
    pub progress: f64,
    #[serde(default)]
    pub speed: Option<String>,
    #[serde(default)]
    pub eta: Option<String>,
    #[serde(default)]
    pub total_size: Option<String>,
    #[serde(default)]
    pub log_snippet: Option<String>,
    #[serde(default)]
    pub error: Option<String>,
    pub created_at: DateTime<Utc>,
    #[serde(default)]
    pub completed_at: Option<DateTime<Utc>>,
    #[serde(default)]
    pub is_playlist: bool,
    #[serde(default)]
    pub playlist_title: Option<String>,
    #[serde(default)]
    pub current_item: usize,
    #[serde(default)]
    pub total_items: usize,
}

// ==========================================
// 6. LAN Chat Models
// ==========================================

fn default_transfer_port() -> u16 {
    38384
}

fn default_desktop_notifications() -> bool {
    true
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatConfig {
    pub user_id: String,
    pub username: String,
    pub hostname: String,
    pub status: String, // "online", "away", "busy"
    pub port: u16,
    #[serde(default = "default_transfer_port")]
    pub file_transfer_port: u16,
    pub broadcast_enabled: bool,
    pub sound_notifications: bool,
    #[serde(default = "default_desktop_notifications")]
    pub desktop_notifications: bool,
}

impl Default for ChatConfig {
    fn default() -> Self {
        let username = std::env::var("USERNAME")
            .or_else(|_| std::env::var("USER"))
            .unwrap_or_else(|_| "User".to_string());

        let hostname = std::env::var("COMPUTERNAME")
            .unwrap_or_else(|_| "Windows-PC".to_string());

        let user_id = format!(
            "{}-{}",
            username.to_lowercase().replace(' ', "_"),
            rand::random::<u16>()
        );

        Self {
            user_id,
            username,
            hostname,
            status: "online".to_string(),
            port: 38383,
            file_transfer_port: 38384,
            broadcast_enabled: true,
            sound_notifications: true,
            desktop_notifications: true,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PeerInfo {
    pub id: String,
    pub username: String,
    pub hostname: String,
    pub ip: String,
    pub status: String,
    pub last_seen: DateTime<Utc>,
    pub is_online: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileTransferInfo {
    pub transfer_id: String,
    pub file_name: String,
    pub file_size: u64,
    #[serde(default)]
    pub file_path: Option<String>,
    #[serde(default)]
    pub save_path: Option<String>,
    pub sender_id: String,
    pub sender_name: String,
    pub sender_host: String,
    pub recipient_id: String,
    pub sender_ip: String,
    pub transfer_port: u16,
    pub direction: String, // "outgoing", "incoming"
    pub status: String,    // "offered", "transferring", "completed", "declined", "cancelled", "failed"
    pub bytes_transferred: u64,
    pub progress: f64,
    pub speed_bps: u64,
    pub eta_sec: u64,
    #[serde(default)]
    pub error: Option<String>,
    pub created_at: DateTime<Utc>,
    #[serde(default)]
    pub completed_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PickedFileInfo {
    pub path: String,
    pub name: String,
    pub size: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatMessage {
    pub id: String,
    pub sender_id: String,
    pub sender_name: String,
    pub sender_host: String,
    pub recipient_id: String, // "all" for general room, or specific peer user_id
    pub content: String,
    pub timestamp: DateTime<Utc>,
    pub is_mine: bool,
    #[serde(default)]
    pub file_info: Option<FileTransferInfo>,
    #[serde(default)]
    pub buzz_type: Option<String>, // "hard" or "soft"
}

// ==========================================
// 7. Data Storage Configuration Models
// ==========================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DataStorageInfo {
    pub current_dir: String,
    pub has_services: bool,
    pub has_todos: bool,
    pub has_credentials: bool,
    pub has_clipboard: bool,
    pub has_jiggler: bool,
    pub has_downloader: bool,
    pub has_chat: bool,
    pub services_count: usize,
    pub todos_count: usize,
    pub credentials_count: usize,
    pub clips_count: usize,
    pub messages_count: usize,
}
