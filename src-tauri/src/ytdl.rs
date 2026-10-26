use crate::manager::get_data_dir;
use crate::models::{DownloadItem, VideoMetadata, YtDlpConfig};
use crate::win32::SafeJobObject;
use chrono::Utc;
use regex::Regex;
use std::fs;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::{Arc, Mutex, RwLock};

pub struct YtDlpEngine {
    pub base_dir: Arc<RwLock<PathBuf>>,
    pub config_file: Arc<RwLock<PathBuf>>,
    pub history_file: Arc<RwLock<PathBuf>>,
    pub config: RwLock<YtDlpConfig>,
    pub history: Arc<RwLock<Vec<DownloadItem>>>,
    pub active_download: Arc<Mutex<Option<(String, Arc<SafeJobObject>, u32)>>>, // id, job, pid
}

impl YtDlpEngine {
    pub fn new() -> Self {
        let base_dir = get_data_dir();
        let config_file = base_dir.join("ytdl_config.json");
        let history_file = base_dir.join("downloads.json");

        let mut config = YtDlpConfig::default();
        if config_file.exists() {
            if let Ok(data) = fs::read_to_string(&config_file) {
                if let Ok(loaded) = serde_json::from_str::<YtDlpConfig>(&data) {
                    config = loaded;
                }
            }
        }

        let mut history = Vec::new();
        if history_file.exists() {
            if let Ok(data) = fs::read_to_string(&history_file) {
                if let Ok(list) = serde_json::from_str::<Vec<DownloadItem>>(&data) {
                    history = list;
                }
            }
        }

        Self {
            base_dir: Arc::new(RwLock::new(base_dir)),
            config_file: Arc::new(RwLock::new(config_file)),
            history_file: Arc::new(RwLock::new(history_file)),
            config: RwLock::new(config),
            history: Arc::new(RwLock::new(history)),
            active_download: Arc::new(Mutex::new(None)),
        }
    }

    pub fn reload_from_dir(&self, new_dir: &Path) {
        let new_base = new_dir.to_path_buf();
        let new_cfg = new_base.join("ytdl_config.json");
        let new_hist = new_base.join("downloads.json");

        if new_cfg.exists() {
            if let Ok(data) = fs::read_to_string(&new_cfg) {
                if let Ok(loaded) = serde_json::from_str::<YtDlpConfig>(&data) {
                    let mut guard = self.config.write().unwrap();
                    *guard = loaded;
                }
            }
        }

        let mut history = Vec::new();
        if new_hist.exists() {
            if let Ok(data) = fs::read_to_string(&new_hist) {
                if let Ok(list) = serde_json::from_str::<Vec<DownloadItem>>(&data) {
                    history = list;
                }
            }
        }
        {
            let mut guard = self.history.write().unwrap();
            *guard = history;
        }

        *self.base_dir.write().unwrap() = new_base;
        *self.config_file.write().unwrap() = new_cfg;
        *self.history_file.write().unwrap() = new_hist;
    }

    pub fn get_config(&self) -> YtDlpConfig {
        self.config.read().unwrap().clone()
    }

    pub fn save_config(&self, cfg: YtDlpConfig) {
        {
            let mut guard = self.config.write().unwrap();
            *guard = cfg.clone();
        }
        let cfg_path = self.config_file.read().unwrap().clone();
        if let Ok(json) = serde_json::to_string_pretty(&cfg) {
            let _ = fs::write(&cfg_path, json);
        }
    }

    pub fn get_history(&self) -> Vec<DownloadItem> {
        self.history.read().unwrap().clone()
    }

    pub fn save_history(&self) {
        let list = self.history.read().unwrap();
        let hist_path = self.history_file.read().unwrap().clone();
        if let Ok(json) = serde_json::to_string_pretty(&*list) {
            let _ = fs::write(&hist_path, json);
        }
    }
}

fn find_in_path(name: &str) -> Option<PathBuf> {
    if let Ok(paths) = std::env::var("PATH") {
        for p in std::env::split_paths(&paths) {
            let full = p.join(name);
            if full.exists() {
                return Some(full);
            }
            let with_exe = p.join(format!("{}.exe", name));
            if with_exe.exists() {
                return Some(with_exe);
            }
        }
    }
    None
}

impl YtDlpEngine {
    pub fn locate_ytdlp(&self) -> Result<PathBuf, String> {
        // 0. Check embedded tools in %LOCALAPPDATA%\ProcessManager\bin
        crate::embedded::ensure_embedded_tools();
        let embedded_bin = crate::embedded::get_embedded_bin_dir().join("yt-dlp.exe");
        if embedded_bin.exists() {
            return Ok(embedded_bin);
        }

        // 1. Check next to current exe
        if let Ok(exe) = std::env::current_exe() {
            if let Some(parent) = exe.parent() {
                let local = parent.join("yt-dlp.exe");
                if local.exists() {
                    return Ok(local);
                }
                let in_bin = parent.join("bin").join("yt-dlp.exe");
                if in_bin.exists() {
                    return Ok(in_bin);
                }
            }
        }

        // 2. Check base_dir (e.g. storage folder\yt-dlp.exe)
        let base_dir = self.base_dir.read().unwrap().clone();
        let local = base_dir.join("yt-dlp.exe");
        if local.exists() {
            return Ok(local);
        }

        // 3. Check %LOCALAPPDATA%\ProcessManager\bin\yt-dlp.exe
        if let Ok(local_app_data) = std::env::var("LOCALAPPDATA") {
            let cached = Path::new(&local_app_data)
                .join("ProcessManager")
                .join("bin")
                .join("yt-dlp.exe");
            if cached.exists() {
                return Ok(cached);
            }
        }

        // 4. Check system PATH
        if let Some(p) = find_in_path("yt-dlp") {
            return Ok(p);
        }

        // 5. Check e:\process_manager fallback directly
        let pm_direct = PathBuf::from("E:\\process_manager\\yt-dlp.exe");
        if pm_direct.exists() {
            return Ok(pm_direct);
        }

        Err("yt-dlp.exe not found. Place yt-dlp.exe in the application folder or install it in PATH.".to_string())
    }

    pub fn locate_ffmpeg(&self) -> Option<PathBuf> {
        // 0. Check embedded tools in %LOCALAPPDATA%\ProcessManager\bin
        crate::embedded::ensure_embedded_tools();
        let embedded_bin = crate::embedded::get_embedded_bin_dir().join("ffmpeg.exe");
        if embedded_bin.exists() {
            return Some(embedded_bin);
        }

        // 1. Check system PATH
        if let Some(p) = find_in_path("ffmpeg") {
            return Some(p);
        }

        // 2. Check WinGet links
        if let Ok(local_app_data) = std::env::var("LOCALAPPDATA") {
            let winget = Path::new(&local_app_data)
                .join("Microsoft")
                .join("WinGet")
                .join("Links")
                .join("ffmpeg.exe");
            if winget.exists() {
                return Some(winget);
            }
        }

        // 3. Check base_dir
        let base_dir = self.base_dir.read().unwrap().clone();
        let local = base_dir.join("ffmpeg.exe");
        if local.exists() {
            return Some(local);
        }

        None
    }

    pub fn is_playlist_url(url: &str) -> bool {
        url.contains("playlist?list=")
            || url.contains("&list=")
            || url.contains("?list=")
            || url.contains("/sets/")
    }

    pub async fn fetch_metadata(self: &Arc<Self>, raw_url: &str) -> Result<VideoMetadata, String> {
        let ytdlp = self.locate_ytdlp()?;
        let is_playlist = Self::is_playlist_url(raw_url);

        let mut cmd = Command::new(&ytdlp);
        cmd.arg("--dump-json");
        cmd.arg("--no-warnings");
        cmd.args(["--extractor-args", "youtube:player_client=android,ios,web"]);

        if is_playlist {
            cmd.arg("--flat-playlist");
        } else {
            cmd.arg("--no-playlist");
        }
        cmd.arg(raw_url);

        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            cmd.creation_flags(0x08000000); // CREATE_NO_WINDOW
        }

        let output = cmd.output().map_err(|e| format!("Failed to spawn yt-dlp: {}", e))?;
        if !output.status.success() {
            let err = String::from_utf8_lossy(&output.stderr);
            return Err(format!("yt-dlp error: {}", err));
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        let first_line = stdout.lines().next().unwrap_or("{}");

        if let Ok(v) = serde_json::from_str::<serde_json::Value>(first_line) {
            let id = v["id"].as_str().unwrap_or("").to_string();
            let title = v["title"]
                .as_str()
                .unwrap_or("Unknown Title")
                .to_string();
            let uploader = v["uploader"]
                .as_str()
                .or_else(|| v["channel"].as_str())
                .unwrap_or("Unknown Creator")
                .to_string();
            let duration = v["duration"].as_u64().unwrap_or(0);
            let thumbnail = v["thumbnail"].as_str().unwrap_or("").to_string();
            let view_count = v["view_count"].as_i64().unwrap_or(0);

            let dur_str = if duration >= 3600 {
                format!("{:02}:{:02}:{:02}", duration / 3600, (duration % 3600) / 60, duration % 60)
            } else {
                format!("{:02}:{:02}", duration / 60, duration % 60)
            };

            let mut available_res = Vec::new();
            if let Some(formats) = v["formats"].as_array() {
                for f in formats {
                    if let Some(h) = f["height"].as_i64() {
                        let res_tag = format!("{}p", h);
                        if !available_res.contains(&res_tag) {
                            available_res.push(res_tag);
                        }
                    }
                }
            }
            available_res.sort_by(|a, b| {
                let na: i64 = a.trim_end_matches('p').parse().unwrap_or(0);
                let nb: i64 = b.trim_end_matches('p').parse().unwrap_or(0);
                nb.cmp(&na)
            });

            let max_res = available_res.first().cloned();

            let playlist_count = if is_playlist {
                Some(stdout.lines().count())
            } else {
                None
            };

            Ok(VideoMetadata {
                id,
                title,
                uploader,
                duration,
                duration_string: dur_str,
                thumbnail,
                view_count,
                webpage_url: raw_url.to_string(),
                max_resolution: max_res,
                available_resolutions: available_res,
                is_playlist,
                playlist_count,
            })
        } else {
            Err("Failed to parse video metadata JSON".to_string())
        }
    }

    pub fn cancel_active_download(&self) -> bool {
        let mut active = self.active_download.lock().unwrap();
        if let Some((_, job, pid)) = active.take() {
            job.terminate();
            crate::win32::kill_process_tree(pid);
            true
        } else {
            false
        }
    }

    pub fn start_download(
        self: &Arc<Self>,
        raw_url: &str,
        media_type: &str,
        container: &str,
        quality: &str,
        custom_dir: Option<String>,
        is_playlist: bool,
    ) -> Result<DownloadItem, String> {
        let ytdlp = self.locate_ytdlp()?;
        let out_dir = custom_dir.unwrap_or_else(|| self.get_config().output_dir);
        let _ = fs::create_dir_all(&out_dir);

        let id = format!("{:08x}", rand::random::<u32>());
        let item = DownloadItem {
            id: id.clone(),
            url: raw_url.to_string(),
            title: "Fetching stream information...".to_string(),
            uploader: None,
            duration: None,
            thumbnail: None,
            media_type: Some(media_type.to_string()),
            container: Some(container.to_string()),
            quality: Some(quality.to_string()),
            format: format!("{}_{}", media_type, quality),
            format_label: format!("{} ({})", container.to_uppercase(), quality),
            output_dir: out_dir.clone(),
            file_path: None,
            file_size: None,
            status: "fetching".to_string(),
            progress: 0.0,
            speed: None,
            eta: None,
            total_size: None,
            log_snippet: None,
            error: None,
            created_at: Utc::now(),
            completed_at: None,
            is_playlist,
            playlist_title: None,
            current_item: 1,
            total_items: 1,
        };

        let mut args: Vec<String> = vec![
            "--newline".to_string(),
            "--no-colors".to_string(),
            "--no-warnings".to_string(),
            "--extractor-args".to_string(),
            "youtube:player_client=android,ios,web".to_string(),
        ];

        if let Some(ffmpeg) = self.locate_ffmpeg() {
            args.push("--ffmpeg-location".to_string());
            args.push(ffmpeg.to_string_lossy().to_string());
        }

        if is_playlist {
            args.push("-o".to_string());
            args.push(format!("{}/%(playlist_title)s/%(playlist_index)s - %(title)s.%(ext)s", out_dir));
            args.push("--yes-playlist".to_string());
        } else {
            args.push("-o".to_string());
            args.push(format!("{}/%(title)s.%(ext)s", out_dir));
            args.push("--no-playlist".to_string());
        }

        if media_type == "audio" {
            args.push("-x".to_string());
            args.push("--audio-format".to_string());
            args.push(container.to_string());
            if quality != "best" {
                args.push("--audio-quality".to_string());
                args.push(quality.to_string());
            }
        } else {
            // Video format & height
            let height = match quality {
                "4k" | "2160p" => 2160,
                "1440p" | "2k" => 1440,
                "1080p" => 1080,
                "720p" => 720,
                "480p" => 480,
                "360p" => 360,
                _ => 0,
            };

            if height > 0 {
                args.push("-f".to_string());
                args.push(format!(
                    "bestvideo[height<={}]+bestaudio/best[height<={}]/best",
                    height, height
                ));
            } else {
                args.push("-f".to_string());
                args.push("bestvideo+bestaudio/best".to_string());
            }

            args.push("--merge-output-format".to_string());
            args.push(container.to_string());
        }

        args.push(raw_url.to_string());

        let mut cmd = Command::new(&ytdlp);
        cmd.args(&args);
        cmd.stdout(Stdio::piped());
        cmd.stderr(Stdio::piped());

        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            cmd.creation_flags(0x00000200 | 0x08000000); // CREATE_NEW_PROCESS_GROUP | CREATE_NO_WINDOW
        }

        let mut child = cmd.spawn().map_err(|e| format!("Failed to start download: {}", e))?;
        let pid = child.id();

        let job = SafeJobObject::create_kill_on_close();
        if let Some(ref j) = job {
            j.assign_pid(pid);
        }

        let job_arc = Arc::new(job.unwrap());
        {
            let mut active = self.active_download.lock().unwrap();
            *active = Some((id.clone(), job_arc.clone(), pid));
        }

        let stdout = child.stdout.take();
        let stderr = child.stderr.take();
        let engine_clone = self.clone();
        let item_id = id.clone();
        let mut cur_item = item.clone();

        std::thread::spawn(move || {
            let progress_re = Regex::new(
                r"\[download\]\s+(\d+\.?\d*)%\s+of\s+~?(\S+)\s+at\s+(\S+)\s+ETA\s+(\S+)",
            )
            .unwrap();
            let dest_re = Regex::new(r"\[(?:download|Merger|ExtractAudio)\]\s+Destination:\s+(.+)").unwrap();
            let item_count_re = Regex::new(r"\[download\]\s+Downloading item\s+(\d+)\s+of\s+(\d+)").unwrap();

            if let Some(out) = stdout {
                let reader = BufReader::new(out);
                for line in reader.lines().filter_map(|l| l.ok()) {
                    cur_item.status = "downloading".to_string();

                    if let Some(caps) = progress_re.captures(&line) {
                        if let Ok(pct) = caps[1].parse::<f64>() {
                            cur_item.progress = pct;
                        }
                        cur_item.total_size = Some(caps[2].to_string());
                        cur_item.speed = Some(caps[3].to_string());
                        cur_item.eta = Some(caps[4].to_string());
                    }

                    if let Some(caps) = dest_re.captures(&line) {
                        let path_str = caps[1].trim().to_string();
                        cur_item.file_path = Some(path_str.clone());
                        let p = Path::new(&path_str);
                        if let Some(file_name) = p.file_stem() {
                            cur_item.title = file_name.to_string_lossy().to_string();
                        }
                    }

                    if let Some(caps) = item_count_re.captures(&line) {
                        if let Ok(cur) = caps[1].parse::<usize>() {
                            cur_item.current_item = cur;
                        }
                        if let Ok(tot) = caps[2].parse::<usize>() {
                            cur_item.total_items = tot;
                        }
                    }

                    cur_item.log_snippet = Some(line);
                }
            }

            let status = child.wait();
            let is_success = status.map(|s| s.success()).unwrap_or(false);

            if is_success {
                cur_item.status = "completed".to_string();
                cur_item.progress = 100.0;
                cur_item.completed_at = Some(Utc::now());

                // Read file size
                if let Some(ref fp) = cur_item.file_path {
                    if let Ok(meta) = fs::metadata(fp) {
                        let bytes = meta.len();
                        if bytes > 1024 * 1024 {
                            cur_item.file_size = Some(format!("{:.1} MiB", bytes as f64 / (1024.0 * 1024.0)));
                        } else {
                            cur_item.file_size = Some(format!("{:.1} KiB", bytes as f64 / 1024.0));
                        }
                    }
                }
            } else {
                cur_item.status = "error".to_string();
                if let Some(err) = stderr {
                    let err_reader = BufReader::new(err);
                    let err_lines: Vec<String> = err_reader.lines().filter_map(|l| l.ok()).collect();
                    cur_item.error = Some(err_lines.join("\n"));
                }
            }

            // Save to history
            {
                let mut hist = engine_clone.history.write().unwrap();
                hist.insert(0, cur_item.clone());
            }
            engine_clone.save_history();

            // Clear active download
            {
                let mut active = engine_clone.active_download.lock().unwrap();
                if let Some((ref act_id, _, _)) = *active {
                    if act_id == &item_id {
                        *active = None;
                    }
                }
            }
        });

        Ok(item)
    }
}

