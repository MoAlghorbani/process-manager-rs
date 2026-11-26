use crate::manager::get_data_dir;
use crate::models::{CloudflaredStatus, PublicServerConfig, PublicServerStatus};
use crate::win32::{kill_process_tree, SafeJobObject};
use chrono::Utc;
use regex::Regex;
use std::collections::{HashMap, VecDeque};
use std::fs;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, RwLock};
use std::time::Instant;

pub struct ServerRuntime {
    pub status: String, // "stopped", "connecting", "online", "error"
    pub public_url: Option<String>,
    pub pid: Option<u32>,
    pub start_time: Option<Instant>,
    pub error_message: Option<String>,
    pub logs: VecDeque<String>,
    pub manual_stop: Arc<AtomicBool>,
    pub job: Option<Arc<SafeJobObject>>,
    pub child: Option<Arc<Mutex<Option<Child>>>>,
}

impl ServerRuntime {
    pub fn new() -> Self {
        Self {
            status: "stopped".to_string(),
            public_url: None,
            pid: None,
            start_time: None,
            error_message: None,
            logs: VecDeque::with_capacity(1000),
            manual_stop: Arc::new(AtomicBool::new(false)),
            job: None,
            child: None,
        }
    }
}

pub struct TunnelEngine {
    pub base_dir: Arc<RwLock<PathBuf>>,
    pub config_file: Arc<RwLock<PathBuf>>,
    pub servers: Arc<RwLock<Vec<PublicServerConfig>>>,
    pub runtimes: Arc<RwLock<HashMap<String, Arc<Mutex<ServerRuntime>>>>>,
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

impl TunnelEngine {
    pub fn new() -> Self {
        let base_dir = get_data_dir();
        let config_file = base_dir.join("public_servers.json");

        let mut servers = Vec::new();
        if config_file.exists() {
            if let Ok(data) = fs::read_to_string(&config_file) {
                if let Ok(loaded) = serde_json::from_str::<Vec<PublicServerConfig>>(&data) {
                    servers = loaded;
                }
            }
        }

        let mut runtimes = HashMap::new();
        for s in &servers {
            runtimes.insert(s.id.clone(), Arc::new(Mutex::new(ServerRuntime::new())));
        }

        Self {
            base_dir: Arc::new(RwLock::new(base_dir)),
            config_file: Arc::new(RwLock::new(config_file)),
            servers: Arc::new(RwLock::new(servers)),
            runtimes: Arc::new(RwLock::new(runtimes)),
        }
    }

    pub fn start_auto_start_servers(&self) {
        let to_start: Vec<String> = {
            let servers = self.servers.read().unwrap();
            servers
                .iter()
                .filter(|s| s.auto_start)
                .map(|s| s.id.clone())
                .collect()
        };

        for id in to_start {
            let _ = self.start_server(&id);
        }
    }

    pub fn reload_from_dir(&self, new_dir: &Path) {
        let new_base = new_dir.to_path_buf();
        let new_cfg = new_base.join("public_servers.json");

        let mut servers = Vec::new();
        if new_cfg.exists() {
            if let Ok(data) = fs::read_to_string(&new_cfg) {
                if let Ok(loaded) = serde_json::from_str::<Vec<PublicServerConfig>>(&data) {
                    servers = loaded;
                }
            }
        }

        {
            let mut s_guard = self.servers.write().unwrap();
            *s_guard = servers.clone();
        }

        {
            let mut r_guard = self.runtimes.write().unwrap();
            for s in &servers {
                if !r_guard.contains_key(&s.id) {
                    r_guard.insert(s.id.clone(), Arc::new(Mutex::new(ServerRuntime::new())));
                }
            }
        }

        *self.base_dir.write().unwrap() = new_base;
        *self.config_file.write().unwrap() = new_cfg;
    }

    pub fn locate_cloudflared(&self) -> Result<PathBuf, String> {
        // 1. Next to current executable or in bin/
        if let Ok(exe) = std::env::current_exe() {
            if let Some(parent) = exe.parent() {
                let local = parent.join("cloudflared.exe");
                if local.exists() {
                    return Ok(local);
                }
                let in_bin = parent.join("bin").join("cloudflared.exe");
                if in_bin.exists() {
                    return Ok(in_bin);
                }
            }
        }

        // 2. Base storage directory
        let base_dir = self.base_dir.read().unwrap().clone();
        let local_storage = base_dir.join("cloudflared.exe");
        if local_storage.exists() {
            return Ok(local_storage);
        }

        // 3. Standard Program Files
        let pf_x86 = PathBuf::from(r"C:\Program Files (x86)\cloudflared\cloudflared.exe");
        if pf_x86.exists() {
            return Ok(pf_x86);
        }

        let pf_64 = PathBuf::from(r"C:\Program Files\cloudflared\cloudflared.exe");
        if pf_64.exists() {
            return Ok(pf_64);
        }

        // 4. WinGet links
        if let Ok(local_app_data) = std::env::var("LOCALAPPDATA") {
            let winget = Path::new(&local_app_data)
                .join("Microsoft")
                .join("WinGet")
                .join("Links")
                .join("cloudflared.exe");
            if winget.exists() {
                return Ok(winget);
            }
            let cf_local = Path::new(&local_app_data)
                .join("cloudflared")
                .join("cloudflared.exe");
            if cf_local.exists() {
                return Ok(cf_local);
            }
        }

        // 5. User Profile
        if let Ok(profile) = std::env::var("USERPROFILE") {
            let prof_cf = Path::new(&profile).join("cloudflared.exe");
            if prof_cf.exists() {
                return Ok(prof_cf);
            }
        }

        // 6. System PATH
        if let Some(p) = find_in_path("cloudflared") {
            return Ok(p);
        }

        Err("cloudflared.exe not found. Install via 'winget install --id Cloudflare.cloudflared' or place cloudflared.exe in PATH or application directory.".to_string())
    }

    pub fn get_cloudflared_status(&self) -> CloudflaredStatus {
        match self.locate_cloudflared() {
            Ok(p) => {
                let mut version = None;
                let mut cmd = Command::new(&p);
                cmd.arg("--version");
                #[cfg(windows)]
                {
                    use std::os::windows::process::CommandExt;
                    cmd.creation_flags(0x08000000); // CREATE_NO_WINDOW
                }
                if let Ok(out) = cmd.output() {
                    let s = String::from_utf8_lossy(&out.stdout);
                    // Match e.g. "cloudflared version 2026.8.3"
                    let re = Regex::new(r"cloudflared\s+version\s+([^\s\(\)]+)").unwrap();
                    if let Some(caps) = re.captures(&s) {
                        if let Some(m) = caps.get(1) {
                            version = Some(m.as_str().to_string());
                        }
                    } else if !s.trim().is_empty() {
                        version = Some(s.lines().next().unwrap_or("").trim().to_string());
                    }
                }

                CloudflaredStatus {
                    installed: true,
                    path: Some(p.to_string_lossy().to_string()),
                    version,
                }
            }
            Err(_) => CloudflaredStatus {
                installed: false,
                path: None,
                version: None,
            },
        }
    }

    pub fn normalize_url(url: &str) -> String {
        let trimmed = url.trim();
        if trimmed.is_empty() {
            return "http://localhost:8080".to_string();
        }
        // If it's purely a port number like "8080"
        if trimmed.chars().all(|c| c.is_ascii_digit()) {
            return format!("http://localhost:{}", trimmed);
        }
        if trimmed.starts_with("http://") || trimmed.starts_with("https://") {
            trimmed.to_string()
        } else {
            format!("http://{}", trimmed)
        }
    }

    pub fn save_servers(&self) {
        let servers = self.servers.read().unwrap();
        let path = self.config_file.read().unwrap().clone();
        if let Ok(json) = serde_json::to_string_pretty(&*servers) {
            let _ = fs::write(path, json);
        }
    }

    pub fn list_servers(&self) -> Vec<PublicServerStatus> {
        let servers = self.servers.read().unwrap().clone();
        let runtimes = self.runtimes.read().unwrap();

        servers
            .into_iter()
            .map(|s| {
                if let Some(r_lock) = runtimes.get(&s.id) {
                    let r = r_lock.lock().unwrap();
                    let uptime = r
                        .start_time
                        .map(|t| t.elapsed().as_secs())
                        .unwrap_or(0);

                    PublicServerStatus {
                        id: s.id,
                        name: s.name,
                        local_url: s.local_url,
                        public_url: r.public_url.clone(),
                        status: r.status.clone(),
                        auto_start: s.auto_start,
                        pid: r.pid,
                        error_message: r.error_message.clone(),
                        uptime_seconds: uptime,
                        created_at: s.created_at,
                    }
                } else {
                    PublicServerStatus {
                        id: s.id,
                        name: s.name,
                        local_url: s.local_url,
                        public_url: None,
                        status: "stopped".to_string(),
                        auto_start: s.auto_start,
                        pid: None,
                        error_message: None,
                        uptime_seconds: 0,
                        created_at: s.created_at,
                    }
                }
            })
            .collect()
    }

    pub fn add_server(
        &self,
        name: String,
        local_url: String,
        auto_start: bool,
    ) -> Result<PublicServerStatus, String> {
        let norm_url = Self::normalize_url(&local_url);
        let id = format!("{:06x}", rand::random::<u32>() & 0xFFFFFF);

        let s_name = if name.trim().is_empty() {
            let stripped = norm_url
                .trim_start_matches("http://")
                .trim_start_matches("https://");
            format!("Server {}", stripped)
        } else {
            name.trim().to_string()
        };

        let new_config = PublicServerConfig {
            id: id.clone(),
            name: s_name.clone(),
            local_url: norm_url.clone(),
            auto_start,
            created_at: Utc::now().to_rfc3339(),
        };

        {
            let mut s_guard = self.servers.write().unwrap();
            s_guard.push(new_config.clone());
        }
        self.save_servers();

        let runtime = Arc::new(Mutex::new(ServerRuntime::new()));
        {
            let mut r_guard = self.runtimes.write().unwrap();
            r_guard.insert(id.clone(), runtime);
        }

        // Start if requested
        if auto_start {
            let _ = self.start_server(&id);
        }

        // Return current status
        let runtimes = self.runtimes.read().unwrap();
        let r_lock = runtimes.get(&id).unwrap();
        let r = r_lock.lock().unwrap();

        Ok(PublicServerStatus {
            id,
            name: s_name,
            local_url: norm_url,
            public_url: r.public_url.clone(),
            status: r.status.clone(),
            auto_start,
            pid: r.pid,
            error_message: r.error_message.clone(),
            uptime_seconds: 0,
            created_at: new_config.created_at,
        })
    }

    pub fn update_server(
        &self,
        id: &str,
        name: String,
        local_url: String,
        auto_start: bool,
    ) -> Result<PublicServerStatus, String> {
        let norm_url = Self::normalize_url(&local_url);
        let mut was_running = false;

        let (old_url, created_at) = {
            let mut servers = self.servers.write().unwrap();
            if let Some(s) = servers.iter_mut().find(|s| s.id == id) {
                let old = s.local_url.clone();
                s.name = if name.trim().is_empty() {
                    s.name.clone()
                } else {
                    name.trim().to_string()
                };
                s.local_url = norm_url.clone();
                s.auto_start = auto_start;
                (old, s.created_at.clone())
            } else {
                return Err("Server not found".to_string());
            }
        };
        self.save_servers();

        if let Some(r_lock) = self.runtimes.read().unwrap().get(id) {
            let r = r_lock.lock().unwrap();
            if r.status == "online" || r.status == "connecting" {
                was_running = true;
            }
        }

        if was_running && old_url != norm_url {
            let _ = self.restart_server(id);
        }

        let runtimes = self.runtimes.read().unwrap();
        let r_lock = runtimes.get(id).unwrap();
        let r = r_lock.lock().unwrap();

        Ok(PublicServerStatus {
            id: id.to_string(),
            name,
            local_url: norm_url,
            public_url: r.public_url.clone(),
            status: r.status.clone(),
            auto_start,
            pid: r.pid,
            error_message: r.error_message.clone(),
            uptime_seconds: r.start_time.map(|t| t.elapsed().as_secs()).unwrap_or(0),
            created_at,
        })
    }

    pub fn delete_server(&self, id: &str) -> Result<bool, String> {
        // Stop if running
        let _ = self.stop_server(id);

        let found = {
            let mut servers = self.servers.write().unwrap();
            if let Some(pos) = servers.iter().position(|s| s.id == id) {
                servers.remove(pos);
                true
            } else {
                false
            }
        };

        if found {
            self.save_servers();
            let mut runtimes = self.runtimes.write().unwrap();
            runtimes.remove(id);
            Ok(true)
        } else {
            Err("Server not found".to_string())
        }
    }

    pub fn start_server(&self, id: &str) -> Result<bool, String> {
        let (local_url, _name) = {
            let servers = self.servers.read().unwrap();
            if let Some(s) = servers.iter().find(|s| s.id == id) {
                (s.local_url.clone(), s.name.clone())
            } else {
                return Err("Server not found".to_string());
            }
        };

        let runtime_arc = {
            let mut r_guard = self.runtimes.write().unwrap();
            r_guard
                .entry(id.to_string())
                .or_insert_with(|| Arc::new(Mutex::new(ServerRuntime::new())))
                .clone()
        };

        {
            let r = runtime_arc.lock().unwrap();
            if r.status == "online" || r.status == "connecting" {
                return Ok(true);
            }
        }

        let cloudflared_path = match self.locate_cloudflared() {
            Ok(p) => p,
            Err(e) => {
                let mut r = runtime_arc.lock().unwrap();
                r.status = "error".to_string();
                r.error_message = Some(e.clone());
                return Err(e);
            }
        };

        let manual_stop = Arc::new(AtomicBool::new(false));
        let job = SafeJobObject::create_kill_on_close().map(Arc::new);

        let mut cmd = Command::new(&cloudflared_path);
        cmd.args(["tunnel", "--url", &local_url]);
        cmd.stdout(Stdio::piped());
        cmd.stderr(Stdio::piped());

        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            cmd.creation_flags(0x00000200 | 0x08000000); // CREATE_NEW_PROCESS_GROUP | CREATE_NO_WINDOW
        }

        let mut child = match cmd.spawn() {
            Ok(c) => c,
            Err(e) => {
                let mut r = runtime_arc.lock().unwrap();
                r.status = "error".to_string();
                r.error_message = Some(format!("Failed to start cloudflared: {}", e));
                return Err(format!("Failed to spawn cloudflared: {}", e));
            }
        };

        let pid = child.id();
        if let Some(ref j) = job {
            j.assign_pid(pid);
        }

        let stdout = child.stdout.take();
        let stderr = child.stderr.take();
        let child_arc = Arc::new(Mutex::new(Some(child)));

        {
            let mut r = runtime_arc.lock().unwrap();
            r.status = "connecting".to_string();
            r.public_url = None;
            r.pid = Some(pid);
            r.start_time = Some(Instant::now());
            r.error_message = None;
            r.manual_stop = manual_stop.clone();
            r.job = job;
            r.child = Some(child_arc.clone());
            r.logs.push_back(format!(
                "[{}] Starting cloudflared tunnel for {} (PID: {})...",
                Utc::now().format("%Y-%m-%d %H:%M:%S"),
                local_url,
                pid
            ));
        }

        let url_regex = Regex::new(r"https://[a-zA-Z0-9-]+\.trycloudflare\.com").unwrap();

        // 1. Stdout reader thread
        if let Some(out) = stdout {
            let r_clone = runtime_arc.clone();
            let re_clone = url_regex.clone();
            std::thread::spawn(move || {
                let reader = BufReader::new(out);
                for line_res in reader.lines() {
                    if let Ok(line) = line_res {
                        let mut r = r_clone.lock().unwrap();
                        if r.logs.len() >= 1000 {
                            r.logs.pop_front();
                        }
                        r.logs.push_back(line.clone());

                        if r.public_url.is_none() {
                            if let Some(m) = re_clone.find(&line) {
                                let found_url = m.as_str().to_string();
                                r.public_url = Some(found_url.clone());
                                r.status = "online".to_string();
                                r.logs.push_back(format!(
                                    "[{}] ★ Public tunnel established: {}",
                                    Utc::now().format("%Y-%m-%d %H:%M:%S"),
                                    found_url
                                ));
                            }
                        }
                    } else {
                        break;
                    }
                }
            });
        }

        // 2. Stderr reader thread (cloudflared quick tunnels write most logs here)
        if let Some(err) = stderr {
            let r_clone = runtime_arc.clone();
            let re_clone = url_regex.clone();
            std::thread::spawn(move || {
                let reader = BufReader::new(err);
                for line_res in reader.lines() {
                    if let Ok(line) = line_res {
                        let mut r = r_clone.lock().unwrap();
                        if r.logs.len() >= 1000 {
                            r.logs.pop_front();
                        }
                        r.logs.push_back(line.clone());

                        if r.public_url.is_none() {
                            if let Some(m) = re_clone.find(&line) {
                                let found_url = m.as_str().to_string();
                                r.public_url = Some(found_url.clone());
                                r.status = "online".to_string();
                                r.logs.push_back(format!(
                                    "[{}] ★ Public tunnel established: {}",
                                    Utc::now().format("%Y-%m-%d %H:%M:%S"),
                                    found_url
                                ));
                            }
                        }
                    } else {
                        break;
                    }
                }
            });
        }

        // 3. Process monitor wait thread
        {
            let r_clone = runtime_arc.clone();
            let manual_stop_clone = manual_stop.clone();
            std::thread::spawn(move || {
                let mut child_opt = {
                    let mut guard = child_arc.lock().unwrap();
                    guard.take()
                };

                if let Some(ref mut c) = child_opt {
                    let exit_status = c.wait();
                    let was_manual = manual_stop_clone.load(Ordering::SeqCst);
                    let mut r = r_clone.lock().unwrap();
                    r.pid = None;
                    r.public_url = None;
                    r.start_time = None;

                    if was_manual {
                        r.status = "stopped".to_string();
                        r.error_message = None;
                        r.logs.push_back(format!(
                            "[{}] Tunnel stopped by user.",
                            Utc::now().format("%Y-%m-%d %H:%M:%S")
                        ));
                    } else {
                        r.status = "error".to_string();
                        let msg = match exit_status {
                            Ok(st) => format!("Tunnel exited with status: {}", st),
                            Err(e) => format!("Tunnel terminated with error: {}", e),
                        };
                        r.error_message = Some(msg.clone());
                        r.logs.push_back(format!(
                            "[{}] {}",
                            Utc::now().format("%Y-%m-%d %H:%M:%S"),
                            msg
                        ));
                    }
                }
            });
        }

        Ok(true)
    }

    pub fn stop_server(&self, id: &str) -> Result<bool, String> {
        let runtime_arc = {
            let runtimes = self.runtimes.read().unwrap();
            runtimes.get(id).cloned()
        };

        if let Some(r_arc) = runtime_arc {
            let pid_opt = {
                let mut r = r_arc.lock().unwrap();
                r.manual_stop.store(true, Ordering::SeqCst);
                let pid = r.pid;
                r.status = "stopped".to_string();
                r.public_url = None;
                r.pid = None;
                r.start_time = None;
                if let Some(ref job) = r.job {
                    job.terminate();
                }
                r.job = None;
                pid
            };

            if let Some(pid) = pid_opt {
                kill_process_tree(pid);
            }

            Ok(true)
        } else {
            Err("Server not found".to_string())
        }
    }

    pub fn restart_server(&self, id: &str) -> Result<bool, String> {
        let _ = self.stop_server(id);
        std::thread::sleep(std::time::Duration::from_millis(500));
        self.start_server(id)
    }

    pub fn start_all(&self) -> usize {
        let ids: Vec<String> = {
            let servers = self.servers.read().unwrap();
            servers.iter().map(|s| s.id.clone()).collect()
        };
        let mut count = 0;
        for id in ids {
            if self.start_server(&id).is_ok() {
                count += 1;
            }
        }
        count
    }

    pub fn stop_all(&self) -> usize {
        let ids: Vec<String> = {
            let servers = self.servers.read().unwrap();
            servers.iter().map(|s| s.id.clone()).collect()
        };
        let mut count = 0;
        for id in ids {
            if self.stop_server(&id).is_ok() {
                count += 1;
            }
        }
        count
    }

    pub fn get_logs(&self, id: &str) -> Result<String, String> {
        let runtimes = self.runtimes.read().unwrap();
        if let Some(r_arc) = runtimes.get(id) {
            let r = r_arc.lock().unwrap();
            let logs: Vec<String> = r.logs.iter().cloned().collect();
            Ok(logs.join("\n"))
        } else {
            Err("Server not found".to_string())
        }
    }

    pub fn clear_logs(&self, id: &str) -> Result<bool, String> {
        let runtimes = self.runtimes.read().unwrap();
        if let Some(r_arc) = runtimes.get(id) {
            let mut r = r_arc.lock().unwrap();
            r.logs.clear();
            Ok(true)
        } else {
            Err("Server not found".to_string())
        }
    }
}
