use crate::manager::get_data_dir;
use crate::models::{
    FFmpegEngineInfo, MediaHistoryItem, MediaJobProgress, MediaProbeResult, MediaStreamInfo,
    MediaTaskConfig, VolumeDetectResult,
};
use crate::win32::SafeJobObject;
use chrono::Utc;
use regex::Regex;
use std::fs;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::{Arc, Mutex, RwLock};

#[derive(Clone)]
pub struct FFmpegEngine {
    pub base_dir: Arc<RwLock<PathBuf>>,
    pub history_file: Arc<RwLock<PathBuf>>,
    pub history: Arc<RwLock<Vec<MediaHistoryItem>>>,
    pub active_job: Arc<Mutex<Option<(String, Arc<SafeJobObject>, u32)>>>, // job_id, job, pid
    pub active_progress: Arc<RwLock<MediaJobProgress>>,
}

impl FFmpegEngine {
    pub fn new() -> Self {
        let base_dir = get_data_dir();
        let history_file = base_dir.join("media_history.json");
        let mut history = Vec::new();

        if history_file.exists() {
            if let Ok(data) = fs::read_to_string(&history_file) {
                if let Ok(list) = serde_json::from_str::<Vec<MediaHistoryItem>>(&data) {
                    history = list;
                }
            }
        }

        let initial_progress = MediaJobProgress {
            job_id: String::new(),
            status: "idle".to_string(),
            task_type: String::new(),
            input_path: String::new(),
            output_path: String::new(),
            progress_percent: 0.0,
            current_time: "00:00:00".to_string(),
            total_duration: "00:00:00".to_string(),
            speed: "-".to_string(),
            fps: None,
            error_msg: None,
            output_size_mb: None,
            elapsed_sec: 0,
        };

        Self {
            base_dir: Arc::new(RwLock::new(base_dir)),
            history_file: Arc::new(RwLock::new(history_file)),
            history: Arc::new(RwLock::new(history)),
            active_job: Arc::new(Mutex::new(None)),
            active_progress: Arc::new(RwLock::new(initial_progress)),
        }
    }

    pub fn reload_from_dir(&self, new_dir: &Path) {
        let new_base = new_dir.to_path_buf();
        let new_hist = new_base.join("media_history.json");

        let mut history = Vec::new();
        if new_hist.exists() {
            if let Ok(data) = fs::read_to_string(&new_hist) {
                if let Ok(list) = serde_json::from_str::<Vec<MediaHistoryItem>>(&data) {
                    history = list;
                }
            }
        }
        {
            let mut guard = self.history.write().unwrap();
            *guard = history;
        }

        *self.base_dir.write().unwrap() = new_base;
        *self.history_file.write().unwrap() = new_hist;
    }

    pub fn save_history(&self) {
        let list = self.history.read().unwrap();
        let hist_path = self.history_file.read().unwrap().clone();
        if let Ok(json) = serde_json::to_string_pretty(&*list) {
            let _ = fs::write(&hist_path, json);
        }
    }

    pub fn locate_ffmpeg(&self) -> Result<PathBuf, String> {
        // 0. Check embedded tools in %LOCALAPPDATA%\ProcessManager\bin
        crate::embedded::ensure_embedded_tools();
        let embedded_bin = crate::embedded::get_embedded_bin_dir().join("ffmpeg.exe");
        if embedded_bin.exists() {
            return Ok(embedded_bin);
        }

        // 1. Check embedded bin folder: <base_dir>/bin/ffmpeg.exe
        let base_dir = self.base_dir.read().unwrap().clone();
        let embedded = base_dir.join("bin").join("ffmpeg.exe");
        if embedded.exists() {
            return Ok(embedded);
        }

        // 2. Check current_exe parent directory and parent/bin
        if let Ok(exe) = std::env::current_exe() {
            if let Some(parent) = exe.parent() {
                let bin = parent.join("bin").join("ffmpeg.exe");
                if bin.exists() {
                    return Ok(bin);
                }
                let root = parent.join("ffmpeg.exe");
                if root.exists() {
                    return Ok(root);
                }
            }
        }

        // 3. Check fixed project path E:\process_manager_rs\bin\ffmpeg.exe
        let fixed = PathBuf::from("E:\\process_manager_rs\\bin\\ffmpeg.exe");
        if fixed.exists() {
            return Ok(fixed);
        }

        // 4. Check WinGet Gyan full build
        if let Ok(localappdata) = std::env::var("LOCALAPPDATA") {
            let winget = PathBuf::from(&localappdata)
                .join("Microsoft")
                .join("WinGet")
                .join("Links")
                .join("ffmpeg.exe");
            if winget.exists() {
                return Ok(winget);
            }
        }

        // 5. Check PATH
        if let Ok(paths) = std::env::var("PATH") {
            for p in std::env::split_paths(&paths) {
                let cand = p.join("ffmpeg.exe");
                if cand.exists() {
                    return Ok(cand);
                }
            }
        }

        Err("ffmpeg.exe was not found. Please ensure it is present in the application bin/ folder.".to_string())
    }

    pub fn locate_ffprobe(&self) -> Result<PathBuf, String> {
        // 0. Check embedded tools in %LOCALAPPDATA%\ProcessManager\bin
        crate::embedded::ensure_embedded_tools();
        let embedded_bin = crate::embedded::get_embedded_bin_dir().join("ffprobe.exe");
        if embedded_bin.exists() {
            return Ok(embedded_bin);
        }

        // 1. Embedded bin folder
        let base_dir = self.base_dir.read().unwrap().clone();
        let embedded = base_dir.join("bin").join("ffprobe.exe");
        if embedded.exists() {
            return Ok(embedded);
        }

        if let Ok(exe) = std::env::current_exe() {
            if let Some(parent) = exe.parent() {
                let bin = parent.join("bin").join("ffprobe.exe");
                if bin.exists() {
                    return Ok(bin);
                }
                let root = parent.join("ffprobe.exe");
                if root.exists() {
                    return Ok(root);
                }
            }
        }

        let fixed = PathBuf::from("E:\\process_manager_rs\\bin\\ffprobe.exe");
        if fixed.exists() {
            return Ok(fixed);
        }

        if let Ok(localappdata) = std::env::var("LOCALAPPDATA") {
            let winget = PathBuf::from(&localappdata)
                .join("Microsoft")
                .join("WinGet")
                .join("Links")
                .join("ffprobe.exe");
            if winget.exists() {
                return Ok(winget);
            }
        }

        if let Ok(paths) = std::env::var("PATH") {
            for p in std::env::split_paths(&paths) {
                let cand = p.join("ffprobe.exe");
                if cand.exists() {
                    return Ok(cand);
                }
            }
        }

        Err("ffprobe.exe was not found.".to_string())
    }

    pub fn get_engine_info(&self) -> FFmpegEngineInfo {
        let ffmpeg_res = self.locate_ffmpeg();
        let ffprobe_res = self.locate_ffprobe();

        let available = ffmpeg_res.is_ok();
        let path = ffmpeg_res.map(|p| p.to_string_lossy().to_string()).unwrap_or_default();
        let ffprobe_path = ffprobe_res.map(|p| p.to_string_lossy().to_string()).unwrap_or_default();

        let mut version = "Unknown".to_string();
        let mut has_nvenc = false;
        let mut has_qsv = false;
        let mut has_amf = false;
        let mut is_embedded = false;

        if !path.is_empty() {
            if path.contains("bin\\ffmpeg.exe") || path.contains("bin/ffmpeg.exe") {
                is_embedded = true;
            }

            let mut cmd = Command::new(&path);
            cmd.arg("-version");
            #[cfg(windows)]
            {
                use std::os::windows::process::CommandExt;
                cmd.creation_flags(0x08000000); // CREATE_NO_WINDOW
            }

            if let Ok(out) = cmd.output() {
                let stdout = String::from_utf8_lossy(&out.stdout);
                if let Some(first_line) = stdout.lines().next() {
                    version = first_line.to_string();
                }
                has_nvenc = stdout.contains("nvenc") || stdout.contains("cuda");
                has_qsv = stdout.contains("vpl") || stdout.contains("qsv");
                has_amf = stdout.contains("amf");
            }
        }

        FFmpegEngineInfo {
            available,
            path,
            ffprobe_path,
            version,
            is_embedded,
            has_nvenc,
            has_qsv,
            has_amf,
        }
    }

    pub fn probe_file(&self, file_path: &str) -> Result<MediaProbeResult, String> {
        let ffprobe = self.locate_ffprobe()?;
        let path = Path::new(file_path);
        if !path.exists() {
            return Err(format!("File does not exist: {}", file_path));
        }

        let file_name = path
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| "Unknown".to_string());

        let file_size_mb = match fs::metadata(path) {
            Ok(meta) => (meta.len() as f64) / (1024.0 * 1024.0),
            Err(_) => 0.0,
        };

        let mut cmd = Command::new(&ffprobe);
        cmd.args([
            "-v", "quiet",
            "-print_format", "json",
            "-show_format",
            "-show_streams",
            file_path,
        ]);

        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            cmd.creation_flags(0x08000000); // CREATE_NO_WINDOW
        }

        let output = cmd
            .output()
            .map_err(|e| format!("Failed to run ffprobe: {}", e))?;

        if !output.status.success() {
            return Err("ffprobe failed to inspect file format".to_string());
        }

        let json_str = String::from_utf8_lossy(&output.stdout);
        let parsed: serde_json::Value =
            serde_json::from_str(&json_str).map_err(|e| format!("Failed to parse probe JSON: {}", e))?;

        let format_obj = &parsed["format"];
        let format_name = format_obj["format_long_name"]
            .as_str()
            .or_else(|| format_obj["format_name"].as_str())
            .unwrap_or("Unknown")
            .to_string();

        let duration_sec = format_obj["duration"]
            .as_str()
            .and_then(|d| d.parse::<f64>().ok())
            .unwrap_or(0.0);

        let bitrate_kbps = format_obj["bit_rate"]
            .as_str()
            .and_then(|b| b.parse::<u64>().ok())
            .map(|b| b / 1000);

        let duration_formatted = format_seconds(duration_sec);

        let mut streams = Vec::new();
        let mut has_video = false;
        let mut has_audio = false;

        if let Some(stream_arr) = parsed["streams"].as_array() {
            for (i, s) in stream_arr.iter().enumerate() {
                let codec_type = s["codec_type"].as_str().unwrap_or("unknown").to_string();
                let codec_name = s["codec_name"].as_str().unwrap_or("unknown").to_string();

                let mut resolution = None;
                let mut fps = None;
                let mut sample_rate = None;
                let mut channels = None;
                let mut stream_bitrate = None;

                if codec_type == "video" {
                    has_video = true;
                    if let (Some(w), Some(h)) = (s["width"].as_i64(), s["height"].as_i64()) {
                        resolution = Some(format!("{}x{}", w, h));
                    }
                    if let Some(r_fps) = s["r_frame_rate"].as_str() {
                        if let Some((num, den)) = r_fps.split_once('/') {
                            if let (Ok(n), Ok(d)) = (num.parse::<f64>(), den.parse::<f64>()) {
                                if d > 0.0 {
                                    fps = Some((n / d * 100.0).round() / 100.0);
                                }
                            }
                        }
                    }
                } else if codec_type == "audio" {
                    has_audio = true;
                    if let Some(sr) = s["sample_rate"].as_str() {
                        sample_rate = Some(format!("{} Hz", sr));
                    }
                    if let Some(ch) = s["channels"].as_u64() {
                        channels = Some(ch as usize);
                    }
                    if let Some(br) = s["bit_rate"].as_str() {
                        if let Ok(b) = br.parse::<u64>() {
                            stream_bitrate = Some(format!("{} kbps", b / 1000));
                        }
                    }
                }

                streams.push(MediaStreamInfo {
                    index: i,
                    codec_type,
                    codec_name,
                    resolution,
                    fps,
                    sample_rate,
                    channels,
                    bitrate: stream_bitrate,
                });
            }
        }

        Ok(MediaProbeResult {
            file_path: file_path.to_string(),
            file_name,
            file_size_mb: (file_size_mb * 100.0).round() / 100.0,
            format: format_name,
            duration_sec,
            duration_formatted,
            bitrate_kbps,
            streams,
            has_video,
            has_audio,
        })
    }

    pub fn detect_volume(&self, file_path: &str) -> Result<VolumeDetectResult, String> {
        let ffmpeg = self.locate_ffmpeg()?;
        let path = Path::new(file_path);
        if !path.exists() {
            return Err("Input file does not exist".to_string());
        }

        let mut cmd = Command::new(&ffmpeg);
        cmd.args([
            "-i", file_path,
            "-af", "volumedetect",
            "-vn", "-sn", "-dn",
            "-f", "null",
            "NUL",
        ]);

        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            cmd.creation_flags(0x08000000); // CREATE_NO_WINDOW
        }

        let output = cmd
            .output()
            .map_err(|e| format!("Failed to analyze volume: {}", e))?;

        let stderr = String::from_utf8_lossy(&output.stderr);

        let max_vol_re = Regex::new(r"max_volume:\s+(-?\d+\.?\d*)\s+dB").unwrap();
        let mean_vol_re = Regex::new(r"mean_volume:\s+(-?\d+\.?\d*)\s+dB").unwrap();

        let max_volume_db = max_vol_re
            .captures(&stderr)
            .and_then(|c| c[1].parse::<f64>().ok())
            .unwrap_or(0.0);

        let mean_volume_db = mean_vol_re
            .captures(&stderr)
            .and_then(|c| c[1].parse::<f64>().ok())
            .unwrap_or(-24.0);

        // Calculate safe recommended boost (headroom before 0.0 dB clipping)
        let recommended_boost_db = if max_volume_db < 0.0 {
            (-max_volume_db * 10.0).floor() / 10.0 // e.g. -6.4 dB -> +6.4 dB
        } else {
            0.0
        };

        // Linear multiplier: factor = 10^(dB / 20)
        let recommended_boost_multiplier = 10f64.powf(recommended_boost_db / 20.0);
        let rounded_multiplier = (recommended_boost_multiplier * 100.0).round() / 100.0;

        Ok(VolumeDetectResult {
            max_volume_db,
            mean_volume_db,
            recommended_boost_db,
            recommended_boost_multiplier: rounded_multiplier,
        })
    }

    pub fn start_conversion(self: &Arc<Self>, config: MediaTaskConfig) -> Result<String, String> {
        // Ensure no active conversion is already running
        {
            let guard = self.active_job.lock().unwrap();
            if guard.is_some() {
                return Err("A media conversion task is already in progress. Please wait or cancel it.".to_string());
            }
        }

        let ffmpeg = self.locate_ffmpeg()?;
        let in_path = Path::new(&config.input_path);
        if !in_path.exists() {
            return Err("Input file does not exist".to_string());
        }

        // Determine destination output file
        let out_dir = match &config.output_dir {
            Some(d) if !d.trim().is_empty() => PathBuf::from(d),
            _ => in_path.parent().unwrap_or_else(|| Path::new(".")).to_path_buf(),
        };
        let _ = fs::create_dir_all(&out_dir);

        let stem = in_path
            .file_stem()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_else(|| "output".to_string());

        let target_ext = match config.task_type.as_str() {
            "video_to_mp3" => "mp3".to_string(),
            _ => config.target_format.trim_start_matches('.').to_string(),
        };

        let file_name = if let Some(custom) = &config.custom_filename {
            if !custom.trim().is_empty() {
                custom.clone()
            } else {
                format!("{}_{}.{}", stem, config.task_type, target_ext)
            }
        } else {
            match config.task_type.as_str() {
                "video_to_mp3" => format!("{}.mp3", stem),
                "boost_volume" => format!("{}_boosted.{}", stem, target_ext),
                _ => format!("{}_converted.{}", stem, target_ext),
            }
        };

        let out_path = out_dir.join(&file_name);
        let out_path_str = out_path.to_string_lossy().to_string();

        // Query probe duration for percentage calculation
        let probe_info = self.probe_file(&config.input_path).ok();
        let total_duration_sec = probe_info.as_ref().map(|p| p.duration_sec).unwrap_or(0.0);
        let total_duration_formatted = format_seconds(total_duration_sec);

        // Build FFmpeg command arguments
        let mut cmd = Command::new(&ffmpeg);
        cmd.arg("-y"); // overwrite output
        cmd.args(["-i", &config.input_path]);

        match config.task_type.as_str() {
            "video_to_mp3" => {
                let bitrate = config.audio_bitrate.as_deref().unwrap_or("320k");
                cmd.args(["-vn", "-c:a", "libmp3lame", "-b:a", bitrate]);
            }

            "boost_volume" => {
                let has_video = probe_info.as_ref().map(|p| p.has_video).unwrap_or(false);

                // Build audio filter
                let filter_str = if config.use_loudnorm {
                    "loudnorm=I=-16:TP=-1.5:LRA=11".to_string()
                } else if let Some(mult) = config.volume_multiplier {
                    format!("volume={:.2}", mult)
                } else if let Some(db) = config.volume_db {
                    format!("volume={:.1}dB", db)
                } else {
                    "volume=1.5".to_string()
                };

                cmd.args(["-af", &filter_str]);

                if has_video {
                    // Fast stream copy for video stream, re-encode audio only!
                    cmd.args(["-c:v", "copy", "-c:a", "aac", "-b:a", "192k"]);
                } else {
                    // Audio only
                    match target_ext.as_str() {
                        "wav" => {
                            cmd.args(["-c:a", "pcm_s16le"]);
                        }
                        "flac" => {
                            cmd.args(["-c:a", "flac"]);
                        }
                        "opus" => {
                            cmd.args(["-c:a", "libopus", "-b:a", "160k"]);
                        }
                        _ => {
                            cmd.args(["-c:a", "libmp3lame", "-b:a", "320k"]);
                        }
                    }
                }
            }

            "convert_video" => {
                if target_ext == "gif" {
                    cmd.args([
                        "-vf",
                        "fps=15,scale=480:-1:flags=lanczos,split[s0][s1];[s0]palettegen[p];[s1][p]paletteuse",
                        "-loop",
                        "0",
                    ]);
                } else {
                    let codec = config.video_codec.as_deref().unwrap_or("libx264");
                    let preset = config.preset.as_deref().unwrap_or("fast");

                    if codec == "copy" {
                        cmd.args(["-c", "copy"]);
                    } else {
                        cmd.args(["-c:v", codec, "-preset", preset]);

                        if codec == "libx264" {
                            cmd.args(["-crf", "22", "-c:a", "aac", "-b:a", "192k"]);
                        } else if codec == "libx265" {
                            cmd.args(["-crf", "26", "-c:a", "aac", "-b:a", "192k"]);
                        } else if codec == "libvpx-vp9" {
                            cmd.args(["-crf", "30", "-b:v", "0", "-c:a", "libopus"]);
                        }

                        // Resolution
                        if let Some(res) = &config.resolution {
                            if res != "original" && !res.is_empty() {
                                cmd.args(["-vf", &format!("scale={}", res)]);
                            }
                        }
                    }
                }
            }

            "convert_audio" => {
                let bitrate = config.audio_bitrate.as_deref().unwrap_or("320k");
                match target_ext.as_str() {
                    "mp3" => {
                        cmd.args(["-c:a", "libmp3lame", "-b:a", bitrate]);
                    }
                    "wav" => {
                        cmd.args(["-c:a", "pcm_s16le"]);
                    }
                    "flac" => {
                        cmd.args(["-c:a", "flac"]);
                    }
                    "aac" | "m4a" => {
                        cmd.args(["-c:a", "aac", "-b:a", bitrate]);
                    }
                    "ogg" => {
                        cmd.args(["-c:a", "libvorbis", "-q:a", "6"]);
                    }
                    "opus" => {
                        cmd.args(["-c:a", "libopus", "-b:a", "160k"]);
                    }
                    _ => {
                        cmd.args(["-c:a", "libmp3lame", "-b:a", "320k"]);
                    }
                }
            }

            "convert_image" => {
                if target_ext == "webp" {
                    let q = config.image_quality.unwrap_or(85);
                    cmd.args(["-c:v", "libwebp", "-quality", &q.to_string()]);
                } else if target_ext == "jpg" || target_ext == "jpeg" {
                    cmd.args(["-q:v", "2"]);
                } else if target_ext == "ico" {
                    cmd.args(["-vf", "scale=256:256"]);
                }

                if let Some(scale) = &config.image_scale {
                    if scale != "original" && !scale.is_empty() {
                        if scale.ends_with('%') {
                            let pct_str = scale.trim_end_matches('%');
                            if let Ok(pct) = pct_str.parse::<f64>() {
                                let factor = pct / 100.0;
                                cmd.args(["-vf", &format!("scale=iw*{:.2}:ih*{:.2}", factor, factor)]);
                            }
                        } else {
                            cmd.args(["-vf", &format!("scale={}", scale)]);
                        }
                    }
                }
            }

            _ => {
                return Err("Unsupported conversion task type".to_string());
            }
        }

        // Attach progress pipe
        cmd.args(["-progress", "pipe:1", "-nostats", &out_path_str]);

        cmd.stdout(Stdio::piped());
        cmd.stderr(Stdio::piped());

        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            cmd.creation_flags(0x08000000); // CREATE_NO_WINDOW
        }

        let mut child = cmd.spawn().map_err(|e| format!("Failed to spawn ffmpeg: {}", e))?;
        let pid = child.id();
        let job = SafeJobObject::create_kill_on_close();
        if let Some(ref j) = job {
            j.assign_pid(pid);
        }

        let job_arc = Arc::new(job.unwrap());
        let job_id = format!("{:x}", rand::random::<u32>());

        {
            let mut guard = self.active_job.lock().unwrap();
            *guard = Some((job_id.clone(), job_arc.clone(), pid));
        }

        let initial_prog = MediaJobProgress {
            job_id: job_id.clone(),
            status: "running".to_string(),
            task_type: config.task_type.clone(),
            input_path: config.input_path.clone(),
            output_path: out_path_str.clone(),
            progress_percent: 0.0,
            current_time: "00:00:00".to_string(),
            total_duration: total_duration_formatted.clone(),
            speed: "0x".to_string(),
            fps: None,
            error_msg: None,
            output_size_mb: None,
            elapsed_sec: 0,
        };

        {
            let mut p = self.active_progress.write().unwrap();
            *p = initial_prog;
        }

        let stdout = child.stdout.take();
        let stderr = child.stderr.take();
        let engine_clone = self.clone();
        let task_type_str = config.task_type.clone();
        let input_file_str = in_path.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
        let output_file_str = file_name.clone();

        // Run progress monitor thread
        std::thread::spawn(move || {
            let start_instant = std::time::Instant::now();

            if let Some(out) = stdout {
                let reader = BufReader::new(out);
                for line in reader.lines().filter_map(|l| l.ok()) {
                    let mut prog = engine_clone.active_progress.write().unwrap();
                    prog.elapsed_sec = start_instant.elapsed().as_secs();

                    if let Some((k, v)) = line.split_once('=') {
                        let key = k.trim();
                        let val = v.trim();

                        match key {
                            "out_time" => {
                                if let Some(dot_idx) = val.find('.') {
                                    prog.current_time = val[..dot_idx].to_string();
                                } else {
                                    prog.current_time = val.to_string();
                                }
                            }
                            "out_time_us" => {
                                if let Ok(us) = val.parse::<u64>() {
                                    let cur_secs = us as f64 / 1_000_000.0;
                                    if total_duration_sec > 0.0 {
                                        let pct = (cur_secs / total_duration_sec) * 100.0;
                                        prog.progress_percent = (pct.min(100.0) * 10.0).round() / 10.0;
                                    }
                                }
                            }
                            "speed" => {
                                prog.speed = val.to_string();
                            }
                            "fps" => {
                                if let Ok(f) = val.parse::<f64>() {
                                    prog.fps = Some(f);
                                }
                            }
                            "progress" if val == "end" => {
                                prog.progress_percent = 100.0;
                            }
                            _ => {}
                        }
                    }
                }
            }

            let status = child.wait().ok();
            let mut final_status = "completed";
            let mut err_msg = None;

            if let Some(st) = status {
                if !st.success() {
                    final_status = "failed";
                    if let Some(err_reader) = stderr {
                        let lines: Vec<String> = BufReader::new(err_reader)
                            .lines()
                            .filter_map(|l| l.ok())
                            .collect();
                        err_msg = Some(lines.join("\n"));
                    }
                }
            }

            let output_size_mb = if Path::new(&out_path_str).exists() {
                fs::metadata(&out_path_str)
                    .map(|m| (m.len() as f64 / (1024.0 * 1024.0) * 100.0).round() / 100.0)
                    .ok()
            } else {
                None
            };

            // Update progress state
            {
                let mut p = engine_clone.active_progress.write().unwrap();
                p.status = final_status.to_string();
                if final_status == "completed" {
                    p.progress_percent = 100.0;
                }
                p.output_size_mb = output_size_mb;
                p.error_msg = err_msg;
            }

            // Clear active job pointer
            {
                let mut guard = engine_clone.active_job.lock().unwrap();
                *guard = None;
            }

            // Record to history if completed
            if final_status == "completed" {
                let history_item = MediaHistoryItem {
                    id: format!("{:x}", rand::random::<u32>()),
                    task_type: task_type_str,
                    input_file: input_file_str,
                    output_file: output_file_str,
                    output_size_mb: output_size_mb.unwrap_or(0.0),
                    duration_formatted: total_duration_formatted,
                    completed_at: Utc::now(),
                    format: target_ext.to_string().to_uppercase(),
                };

                {
                    let mut hist = engine_clone.history.write().unwrap();
                    hist.insert(0, history_item);
                    if hist.len() > 100 {
                        hist.truncate(100);
                    }
                }
                engine_clone.save_history();
            }
        });

        Ok(job_id)
    }

    pub fn cancel_active_conversion(&self) -> bool {
        let mut guard = self.active_job.lock().unwrap();
        if let Some((_, job, pid)) = guard.take() {
            job.terminate();
            let _ = Command::new("taskkill")
                .args(["/F", "/PID", &pid.to_string()])
                .output();

            let mut p = self.active_progress.write().unwrap();
            p.status = "cancelled".to_string();
            p.error_msg = Some("Conversion was cancelled by user".to_string());
            true
        } else {
            false
        }
    }

    pub fn get_progress(&self) -> MediaJobProgress {
        self.active_progress.read().unwrap().clone()
    }

    pub fn get_history(&self) -> Vec<MediaHistoryItem> {
        self.history.read().unwrap().clone()
    }

    pub fn clear_history(&self) {
        {
            let mut hist = self.history.write().unwrap();
            hist.clear();
        }
        self.save_history();
    }

    pub fn pick_media_file(&self, filter_type: &str) -> Option<String> {
        let filter_str = match filter_type {
            "video" => "Video Files (*.mp4;*.mkv;*.avi;*.mov;*.webm;*.flv;*.ts;*.wmv)|*.mp4;*.mkv;*.avi;*.mov;*.webm;*.flv;*.ts;*.wmv|All Files (*.*)|*.*",
            "audio" => "Audio Files (*.mp3;*.wav;*.flac;*.aac;*.ogg;*.opus;*.m4a)|*.mp3;*.wav;*.flac;*.aac;*.ogg;*.opus;*.m4a|All Files (*.*)|*.*",
            "image" => "Image Files (*.png;*.jpg;*.jpeg;*.webp;*.bmp;*.gif;*.ico;*.tiff)|*.png;*.jpg;*.jpeg;*.webp;*.bmp;*.gif;*.ico;*.tiff|All Files (*.*)|*.*",
            _ => "Media Files (*.mp4;*.mkv;*.mp3;*.wav;*.png;*.jpg;*.webp)|*.mp4;*.mkv;*.avi;*.mov;*.webm;*.flv;*.mp3;*.wav;*.flac;*.aac;*.ogg;*.opus;*.png;*.jpg;*.jpeg;*.webp;*.bmp;*.gif;*.tiff|All Files (*.*)|*.*",
        };

        let script = format!(
            r#"Add-Type -AssemblyName System.Windows.Forms; $f = New-Object System.Windows.Forms.OpenFileDialog; $f.Title = 'Select Media File'; $f.Filter = '{}'; if($f.ShowDialog() -eq [System.Windows.Forms.DialogResult]::OK){{ Write-Output $f.FileName }}"#,
            filter_str
        );

        let mut cmd = Command::new("powershell");
        cmd.args(["-NoProfile", "-Command", &script]);

        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            cmd.creation_flags(0x08000000); // CREATE_NO_WINDOW
        }

        if let Ok(out) = cmd.output() {
            let path = String::from_utf8_lossy(&out.stdout).trim().to_string();
            if !path.is_empty() && Path::new(&path).exists() {
                return Some(path);
            }
        }
        None
    }

    pub fn pick_output_directory(&self) -> Option<String> {
        let script = r#"Add-Type -AssemblyName System.Windows.Forms; $f = New-Object System.Windows.Forms.FolderBrowserDialog; $f.Description = 'Select Output Directory'; if($f.ShowDialog() -eq [System.Windows.Forms.DialogResult]::OK){ Write-Output $f.SelectedPath }"#;

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

    pub fn open_file_or_dir(&self, target_path: &str) {
        let p = Path::new(target_path);
        if !p.exists() {
            return;
        }

        let _ = Command::new("explorer").arg(target_path).spawn();
    }

    pub fn reveal_in_explorer(&self, file_path: &str) {
        let p = Path::new(file_path);
        if !p.exists() {
            return;
        }

        let _ = Command::new("explorer")
            .arg(format!("/select,{}", file_path))
            .spawn();
    }
}

fn format_seconds(seconds: f64) -> String {
    let s = seconds as u64;
    let h = s / 3600;
    let m = (s % 3600) / 60;
    let sec = s % 60;
    if h > 0 {
        format!("{:02}:{:02}:{:02}", h, m, sec)
    } else {
        format!("{:02}:{:02}", m, sec)
    }
}
