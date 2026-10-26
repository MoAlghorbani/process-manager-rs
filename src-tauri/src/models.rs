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
// 6. FFmpeg Media Studio Models
// ==========================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FFmpegEngineInfo {
    pub available: bool,
    pub path: String,
    pub ffprobe_path: String,
    pub version: String,
    pub is_embedded: bool,
    pub has_nvenc: bool,
    pub has_qsv: bool,
    pub has_amf: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MediaStreamInfo {
    pub index: usize,
    pub codec_type: String, // "video", "audio", "subtitle"
    pub codec_name: String,
    #[serde(default)]
    pub resolution: Option<String>,
    #[serde(default)]
    pub fps: Option<f64>,
    #[serde(default)]
    pub sample_rate: Option<String>,
    #[serde(default)]
    pub channels: Option<usize>,
    #[serde(default)]
    pub bitrate: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MediaProbeResult {
    pub file_path: String,
    pub file_name: String,
    pub file_size_mb: f64,
    pub format: String,
    pub duration_sec: f64,
    pub duration_formatted: String,
    #[serde(default)]
    pub bitrate_kbps: Option<u64>,
    pub streams: Vec<MediaStreamInfo>,
    pub has_video: bool,
    pub has_audio: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VolumeDetectResult {
    pub max_volume_db: f64,
    pub mean_volume_db: f64,
    pub recommended_boost_db: f64,
    pub recommended_boost_multiplier: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MediaTaskConfig {
    pub task_type: String, // "video_to_mp3", "convert_video", "convert_audio", "convert_image", "boost_volume"
    pub input_path: String,
    #[serde(default)]
    pub output_dir: Option<String>,
    #[serde(default)]
    pub custom_filename: Option<String>,
    #[serde(default = "default_mp3")]
    pub target_format: String,
    #[serde(default)]
    pub audio_bitrate: Option<String>, // "320k", "256k", "192k", "128k"
    #[serde(default)]
    pub video_codec: Option<String>,   // "libx264", "libx265", "libvpx-vp9", "copy"
    #[serde(default)]
    pub resolution: Option<String>,    // "original", "1920x1080", "1280x720", "854x480"
    #[serde(default)]
    pub preset: Option<String>,        // "ultrafast", "fast", "medium", "slow"
    #[serde(default)]
    pub volume_multiplier: Option<f64>, // e.g. 1.5, 2.0, 3.0
    #[serde(default)]
    pub volume_db: Option<f64>,        // e.g. 6.0
    #[serde(default)]
    pub use_loudnorm: bool,
    #[serde(default)]
    pub image_quality: Option<u32>,    // 1-100
    #[serde(default)]
    pub image_scale: Option<String>,   // "original", "50%", "1920x1080"
}

fn default_mp3() -> String {
    "mp3".to_string()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MediaJobProgress {
    pub job_id: String,
    pub status: String, // "idle", "running", "completed", "failed", "cancelled"
    pub task_type: String,
    pub input_path: String,
    pub output_path: String,
    pub progress_percent: f64,
    pub current_time: String,
    pub total_duration: String,
    pub speed: String,
    #[serde(default)]
    pub fps: Option<f64>,
    #[serde(default)]
    pub error_msg: Option<String>,
    #[serde(default)]
    pub output_size_mb: Option<f64>,
    #[serde(default)]
    pub elapsed_sec: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MediaHistoryItem {
    pub id: String,
    pub task_type: String,
    pub input_file: String,
    pub output_file: String,
    pub output_size_mb: f64,
    pub duration_formatted: String,
    pub completed_at: DateTime<Utc>,
    pub format: String,
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
    pub has_media: bool,
    pub services_count: usize,
    pub todos_count: usize,
    pub credentials_count: usize,
    pub clips_count: usize,
}
