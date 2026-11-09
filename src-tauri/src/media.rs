use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicU16, Ordering};
use std::sync::Arc;
use tokio::io::{AsyncReadExt, AsyncSeekExt, AsyncWriteExt};
use tokio::net::TcpListener;

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct MediaItem {
    pub id: String,
    pub name: String,
    pub rel_path: String,
    pub abs_path: String,
    pub size: u64,
    pub size_formatted: String,
    pub media_type: String, // "video" or "audio"
    pub extension: String,
    pub folder: String,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct MediaScanResponse {
    pub root_path: String,
    pub total_count: usize,
    pub video_count: usize,
    pub audio_count: usize,
    pub total_size_formatted: String,
    pub items: Vec<MediaItem>,
}

pub fn format_file_size(bytes: u64) -> String {
    const KB: u64 = 1024;
    const MB: u64 = KB * 1024;
    const GB: u64 = MB * 1024;

    if bytes >= GB {
        format!("{:.2} GB", bytes as f64 / GB as f64)
    } else if bytes >= MB {
        format!("{:.1} MB", bytes as f64 / MB as f64)
    } else if bytes >= KB {
        format!("{:.0} KB", bytes as f64 / KB as f64)
    } else {
        format!("{} B", bytes)
    }
}

pub fn get_mime_type(ext: &str) -> &'static str {
    match ext {
        "mp4" | "m4v" => "video/mp4",
        "webm" => "video/webm",
        "mkv" => "video/webm",
        "mov" => "video/quicktime",
        "avi" => "video/x-msvideo",
        "wmv" => "video/x-ms-wmv",
        "flv" => "video/x-flv",
        "ts" => "video/mp2t",
        "mp3" => "audio/mpeg",
        "wav" => "audio/wav",
        "ogg" => "audio/ogg",
        "flac" => "audio/flac",
        "aac" => "audio/aac",
        "m4a" => "audio/mp4",
        "opus" => "audio/opus",
        "wma" => "audio/x-ms-wma",
        _ => "application/octet-stream",
    }
}

pub fn url_decode(s: &str) -> String {
    let mut res = Vec::new();
    let bytes = s.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            if let Ok(val) = u8::from_str_radix(
                std::str::from_utf8(&bytes[i + 1..i + 3]).unwrap_or(""),
                16,
            ) {
                res.push(val);
                i += 3;
                continue;
            }
        } else if bytes[i] == b'+' {
            res.push(b' ');
            i += 1;
            continue;
        }
        res.push(bytes[i]);
        i += 1;
    }
    String::from_utf8_lossy(&res).to_string()
}

pub fn get_media_config_path() -> PathBuf {
    if let Ok(appdata) = std::env::var("APPDATA") {
        let p = PathBuf::from(appdata).join("ProcessManager");
        let _ = fs::create_dir_all(&p);
        return p.join("media_workspace.json");
    }
    PathBuf::from("media_workspace.json")
}

pub fn save_media_folder(path: &str) -> Result<(), String> {
    let cfg_path = get_media_config_path();
    let map = serde_json::json!({
        "media_dir": path
    });
    if let Ok(s) = serde_json::to_string_pretty(&map) {
        fs::write(&cfg_path, s).map_err(|e| format!("Failed to save media workspace: {}", e))?;
    }
    Ok(())
}

pub fn get_saved_media_folder() -> Option<String> {
    let cfg_path = get_media_config_path();
    if cfg_path.exists() {
        if let Ok(data) = fs::read_to_string(&cfg_path) {
            if let Ok(val) = serde_json::from_str::<serde_json::Value>(&data) {
                if let Some(s) = val["media_dir"].as_str() {
                    let p = Path::new(s);
                    if p.exists() && p.is_dir() {
                        return Some(s.to_string());
                    }
                }
            }
        }
    }
    None
}

pub fn pick_media_folder() -> Option<String> {
    let script = r#"Add-Type -AssemblyName System.Windows.Forms; $f = New-Object System.Windows.Forms.FolderBrowserDialog; $f.Description = 'Select Media Workspace Folder (Videos & Audios)'; if($f.ShowDialog() -eq [System.Windows.Forms.DialogResult]::OK){ Write-Output $f.SelectedPath }"#;

    let mut cmd = Command::new("powershell");
    cmd.args(["-NoProfile", "-Command", script]);

    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x08000000); // CREATE_NO_WINDOW
    }

    if let Ok(output) = cmd.output() {
        if output.status.success() {
            let res = String::from_utf8_lossy(&output.stdout).trim().to_string();
            if !res.is_empty() {
                return Some(res);
            }
        }
    }
    None
}

pub fn scan_media_folder(root: &str) -> Result<MediaScanResponse, String> {
    let root_p = Path::new(root);
    if !root_p.exists() || !root_p.is_dir() {
        return Err("Selected media folder does not exist or is not a directory".to_string());
    }

    let mut items = Vec::new();
    let mut stack = vec![root_p.to_path_buf()];

    while let Some(current_dir) = stack.pop() {
        let entries = match fs::read_dir(&current_dir) {
            Ok(e) => e,
            Err(_) => continue,
        };

        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                    if name.starts_with('.')
                        || name.eq_ignore_ascii_case("node_modules")
                        || name.eq_ignore_ascii_case("target")
                        || name.eq_ignore_ascii_case("$recycle.bin")
                        || name.eq_ignore_ascii_case("system volume information")
                    {
                        continue;
                    }
                }
                stack.push(path);
            } else if path.is_file() {
                if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
                    let ext_lower = ext.to_lowercase();
                    let is_video = matches!(
                        ext_lower.as_str(),
                        "mp4" | "mkv" | "webm" | "avi" | "mov" | "m4v" | "wmv" | "flv" | "ts"
                    );
                    let is_audio = matches!(
                        ext_lower.as_str(),
                        "mp3" | "wav" | "ogg" | "flac" | "aac" | "m4a" | "wma" | "opus"
                    );

                    if is_video || is_audio {
                        let name = path
                            .file_name()
                            .and_then(|n| n.to_str())
                            .unwrap_or("unknown")
                            .to_string();
                        let rel_path = path
                            .strip_prefix(root_p)
                            .unwrap_or(&path)
                            .to_string_lossy()
                            .to_string();
                        let folder = path
                            .parent()
                            .and_then(|p| p.strip_prefix(root_p).ok())
                            .map(|p| p.to_string_lossy().to_string())
                            .unwrap_or_default();
                        let size = entry.metadata().map(|m| m.len()).unwrap_or(0);
                        let size_formatted = format_file_size(size);

                        items.push(MediaItem {
                            id: rel_path.clone(),
                            name,
                            rel_path,
                            abs_path: path.to_string_lossy().to_string(),
                            size,
                            size_formatted,
                            media_type: if is_video {
                                "video".into()
                            } else {
                                "audio".into()
                            },
                            extension: ext_lower,
                            folder: if folder.is_empty() {
                                "/".into()
                            } else {
                                folder.replace('\\', "/")
                            },
                        });
                    }
                }
            }
        }
    }

    // Sort items alphabetically by folder, then name
    items.sort_by(|a, b| {
        let f = a.folder.to_lowercase().cmp(&b.folder.to_lowercase());
        if f == std::cmp::Ordering::Equal {
            a.name.to_lowercase().cmp(&b.name.to_lowercase())
        } else {
            f
        }
    });

    let video_count = items.iter().filter(|i| i.media_type == "video").count();
    let audio_count = items.iter().filter(|i| i.media_type == "audio").count();
    let total_size: u64 = items.iter().map(|i| i.size).sum();

    Ok(MediaScanResponse {
        root_path: root.to_string(),
        total_count: items.len(),
        video_count,
        audio_count,
        total_size_formatted: format_file_size(total_size),
        items,
    })
}

#[derive(Clone)]
pub struct MediaServer {
    port: Arc<AtomicU16>,
}

impl MediaServer {
    pub fn new() -> Self {
        Self {
            port: Arc::new(AtomicU16::new(0)),
        }
    }

    pub fn get_port(&self) -> u16 {
        self.port.load(Ordering::Relaxed)
    }

    pub fn start(&self) {
        let port_atomic = self.port.clone();

        std::thread::Builder::new()
            .name("media-streaming-server".to_string())
            .spawn(move || {
                let rt = match tokio::runtime::Builder::new_multi_thread()
                    .enable_all()
                    .build()
                {
                    Ok(r) => r,
                    Err(e) => {
                        eprintln!("[MediaServer] Failed to create Tokio runtime: {}", e);
                        return;
                    }
                };

                rt.block_on(async move {
            let listener = match TcpListener::bind("127.0.0.1:0").await {
                Ok(l) => l,
                Err(e) => {
                    eprintln!("[MediaServer] Failed to bind local streaming socket: {}", e);
                    return;
                }
            };

            let bound_port = match listener.local_addr() {
                Ok(addr) => addr.port(),
                Err(e) => {
                    eprintln!("[MediaServer] Failed to get local port: {}", e);
                    return;
                }
            };

            port_atomic.store(bound_port, Ordering::Relaxed);
            println!("[MediaServer] Streaming server listening on http://127.0.0.1:{}", bound_port);

            while let Ok((mut socket, _)) = listener.accept().await {
                tokio::spawn(async move {
                    let mut buf = vec![0u8; 4096];
                    let n = match socket.read(&mut buf).await {
                        Ok(n) if n > 0 => n,
                        _ => return,
                    };
                    let req_text = String::from_utf8_lossy(&buf[..n]);
                    let first_line = req_text.lines().next().unwrap_or("");

                    // CORS Preflight
                    if first_line.starts_with("OPTIONS") {
                        let resp = "HTTP/1.1 204 No Content\r\n\
                            Access-Control-Allow-Origin: *\r\n\
                            Access-Control-Allow-Methods: GET, HEAD, OPTIONS\r\n\
                            Access-Control-Allow-Headers: Range, Content-Type\r\n\
                            \r\n";
                        let _ = socket.write_all(resp.as_bytes()).await;
                        return;
                    }

                    // Health check ping
                    if first_line.starts_with("GET /ping") {
                        let resp = "HTTP/1.1 200 OK\r\n\
                            Access-Control-Allow-Origin: *\r\n\
                            Content-Type: text/plain\r\n\
                            Content-Length: 4\r\n\r\npong";
                        let _ = socket.write_all(resp.as_bytes()).await;
                        return;
                    }

                    // Extract file path from: GET /stream?path={encoded_path} HTTP/1.1
                    let path_param = if let Some(idx) = first_line.find("?path=") {
                        let rest = &first_line[idx + 6..];
                        let end_idx = rest.find(' ').unwrap_or(rest.len());
                        url_decode(&rest[..end_idx])
                    } else {
                        "".to_string()
                    };

                    let file_path = Path::new(&path_param);
                    if !file_path.exists() || !file_path.is_file() {
                        let resp = "HTTP/1.1 404 Not Found\r\n\
                            Access-Control-Allow-Origin: *\r\n\
                            Content-Type: text/plain\r\n\
                            Content-Length: 9\r\n\r\nNot Found";
                        let _ = socket.write_all(resp.as_bytes()).await;
                        return;
                    }

                    let mut file = match tokio::fs::File::open(&file_path).await {
                        Ok(f) => f,
                        Err(_) => return,
                    };

                    let meta = match file.metadata().await {
                        Ok(m) => m,
                        Err(_) => return,
                    };
                    let total_len = meta.len();
                    let ext = file_path
                        .extension()
                        .and_then(|e| e.to_str())
                        .unwrap_or("")
                        .to_lowercase();
                    let mime = get_mime_type(&ext);

                    // Check for HTTP Range: bytes=start-end
                    let range_line = req_text
                        .lines()
                        .find(|l| l.to_lowercase().starts_with("range: bytes="));

                    if let Some(rline) = range_line {
                        let range_val = rline[13..].trim();
                        let parts: Vec<&str> = range_val.split('-').collect();
                        let start: u64 = parts[0].parse().unwrap_or(0);
                        let mut end: u64 = if parts.len() > 1 && !parts[1].is_empty() {
                            parts[1].parse().unwrap_or(total_len.saturating_sub(1))
                        } else {
                            // Serve in responsive 4MB chunks for fast video seek / buffer
                            (start + 4 * 1024 * 1024).min(total_len.saturating_sub(1))
                        };
                        if end >= total_len {
                            end = total_len.saturating_sub(1);
                        }

                        let chunk_len = if end >= start { end - start + 1 } else { 0 };
                        let header = format!(
                            "HTTP/1.1 206 Partial Content\r\n\
                            Content-Type: {}\r\n\
                            Content-Length: {}\r\n\
                            Content-Range: bytes {}-{}/{}\r\n\
                            Accept-Ranges: bytes\r\n\
                            Access-Control-Allow-Origin: *\r\n\
                            Access-Control-Allow-Headers: Range, Content-Type\r\n\
                            Cache-Control: no-cache\r\n\
                            Connection: keep-alive\r\n\r\n",
                            mime, chunk_len, start, end, total_len
                        );

                        if socket.write_all(header.as_bytes()).await.is_err() {
                            return;
                        }

                        if file.seek(std::io::SeekFrom::Start(start)).await.is_err() {
                            return;
                        }

                        let mut send_buf = vec![0u8; 65536];
                        let mut left = chunk_len;
                        while left > 0 {
                            let to_read = (left as usize).min(send_buf.len());
                            let n = match file.read(&mut send_buf[..to_read]).await {
                                Ok(n) if n > 0 => n,
                                _ => break,
                            };
                            if socket.write_all(&send_buf[..n]).await.is_err() {
                                break;
                            }
                            left -= n as u64;
                        }
                    } else {
                        // Full content response
                        let header = format!(
                            "HTTP/1.1 200 OK\r\n\
                            Content-Type: {}\r\n\
                            Content-Length: {}\r\n\
                            Accept-Ranges: bytes\r\n\
                            Access-Control-Allow-Origin: *\r\n\
                            Access-Control-Allow-Headers: Range, Content-Type\r\n\
                            Cache-Control: no-cache\r\n\
                            Connection: keep-alive\r\n\r\n",
                            mime, total_len
                        );

                        if socket.write_all(header.as_bytes()).await.is_err() {
                            return;
                        }

                        let _ = tokio::io::copy(&mut file, &mut socket).await;
                    }
                });
            }
                });
            })
            .expect("Failed to spawn media streaming server thread");
    }
}

