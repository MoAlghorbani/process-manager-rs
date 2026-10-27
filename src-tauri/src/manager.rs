use crate::models::{GroupSummary, ManagerSummary, ServiceConfig, ServiceStatusReport};
use crate::win32::{
    free_port, get_process_memory_mb, is_admin, kill_process_tree, SafeJobObject,
};
use chrono::Utc;
use std::collections::HashMap;
use std::fs::{self, File, OpenOptions};
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::{Arc, Mutex, RwLock};
use std::time::{Duration, Instant};

pub fn get_storage_config_path() -> PathBuf {
    if let Ok(appdata) = std::env::var("APPDATA") {
        let p = PathBuf::from(appdata).join("ProcessManager");
        let _ = fs::create_dir_all(&p);
        return p.join("storage_config.json");
    }
    PathBuf::from("storage_config.json")
}

pub fn save_configured_data_dir(dir: &Path) -> Result<(), String> {
    let cfg_path = get_storage_config_path();
    let map = serde_json::json!({
        "data_dir": dir.to_string_lossy().to_string()
    });
    if let Ok(s) = serde_json::to_string_pretty(&map) {
        fs::write(&cfg_path, s).map_err(|e| format!("Failed to write storage config: {}", e))?;
    }
    Ok(())
}

pub fn get_data_dir() -> PathBuf {
    // 1. Check user-configured storage path in %APPDATA%\ProcessManager\storage_config.json
    let cfg_path = get_storage_config_path();
    if cfg_path.exists() {
        if let Ok(data) = fs::read_to_string(&cfg_path) {
            if let Ok(val) = serde_json::from_str::<serde_json::Value>(&data) {
                if let Some(dir_str) = val["data_dir"].as_str() {
                    let p = PathBuf::from(dir_str);
                    if p.exists() || fs::create_dir_all(&p).is_ok() {
                        return p;
                    }
                }
            }
        }
    }

    // 2. Check next to current executable
    if let Ok(exe) = std::env::current_exe() {
        if let Some(parent) = exe.parent() {
            if parent.join("services.json").exists() {
                return parent.to_path_buf();
            }
        }
    }

    // 3. Check CWD or parent
    if let Ok(cwd) = std::env::current_dir() {
        if cwd.join("services.json").exists() {
            return cwd;
        }
        if let Some(parent) = cwd.parent() {
            if parent.join("services.json").exists() {
                return parent.to_path_buf();
            }
        }
    }

    // 4. Check main PM repository folder E:\process_manager
    let main_pm = PathBuf::from("E:\\process_manager");
    if main_pm.join("services.json").exists() {
        return main_pm;
    }

    // 5. Default fallback: portable directory next to exe, or %APPDATA%\ProcessManager\data
    if let Ok(exe) = std::env::current_exe() {
        if let Some(parent) = exe.parent() {
            return parent.to_path_buf();
        }
    }

    if let Ok(appdata) = std::env::var("APPDATA") {
        let p = PathBuf::from(appdata).join("ProcessManager").join("data");
        let _ = fs::create_dir_all(&p);
        return p;
    }

    PathBuf::from(".")
}

pub struct ActiveProcess {
    pub pid: u32,
    pub job: Option<SafeJobObject>,
    pub child: Option<Child>,
    pub start_time: Instant,
    pub manual_stop: bool,
}

pub struct ManagedServiceState {
    pub config: ServiceConfig,
    pub active: Option<ActiveProcess>,
    pub status: String, // "stopped", "running", "crashed", "restarting"
    pub restarts: u32,
    pub last_exit_code: Option<i32>,
    pub log_file: PathBuf,
}

pub struct ProcessManager {
    pub base_dir: RwLock<PathBuf>,
    pub services_file: RwLock<PathBuf>,
    pub logs_dir: RwLock<PathBuf>,
    pub services: Arc<RwLock<HashMap<String, Arc<Mutex<ManagedServiceState>>>>>,
}

impl ProcessManager {
    pub fn new() -> Self {
        let base_dir = get_data_dir();
        let services_file = base_dir.join("services.json");
        let logs_dir = base_dir.join("logs");
        let _ = fs::create_dir_all(&logs_dir);

        let pm = Self {
            base_dir: RwLock::new(base_dir),
            services_file: RwLock::new(services_file),
            logs_dir: RwLock::new(logs_dir),
            services: Arc::new(RwLock::new(HashMap::new())),
        };

        pm.load_state();
        pm
    }

    pub fn reload_from_dir(&self, new_dir: &Path) {
        let services_file = new_dir.join("services.json");
        let logs_dir = new_dir.join("logs");
        let _ = fs::create_dir_all(&logs_dir);

        *self.base_dir.write().unwrap() = new_dir.to_path_buf();
        *self.services_file.write().unwrap() = services_file;
        *self.logs_dir.write().unwrap() = logs_dir;

        // Clear existing services in map that aren't running
        {
            let mut map = self.services.write().unwrap();
            map.retain(|_, s_lock| {
                let s = s_lock.lock().unwrap();
                s.status == "running" // keep running processes active
            });
        }

        self.load_state();
    }

    pub fn load_state(&self) {
        let services_file = self.services_file.read().unwrap().clone();
        if !services_file.exists() {
            return;
        }
        if let Ok(content) = fs::read_to_string(&services_file) {
            if let Ok(configs) = serde_json::from_str::<HashMap<String, ServiceConfig>>(&content) {
                let mut map = self.services.write().unwrap();
                let logs_dir = self.logs_dir.read().unwrap().clone();
                for (name, cfg) in configs {
                    if map.contains_key(&name) {
                        continue;
                    }
                    let log_file = logs_dir.join(format!("{}.log", name));
                    let service = ManagedServiceState {
                        config: cfg,
                        active: None,
                        status: "stopped".to_string(),
                        restarts: 0,
                        last_exit_code: None,
                        log_file,
                    };
                    map.insert(name, Arc::new(Mutex::new(service)));
                }
            }
        }
    }

    pub fn save_state(&self) {
        let configs = {
            let map = self.services.read().unwrap();
            let mut configs: HashMap<String, ServiceConfig> = HashMap::new();
            for (name, s_lock) in map.iter() {
                if let Ok(s) = s_lock.try_lock() {
                    let mut cfg = s.config.clone();
                    cfg.saved_status = Some(s.status.clone());
                    configs.insert(name.clone(), cfg);
                } else if let Ok(s) = s_lock.lock() {
                    let mut cfg = s.config.clone();
                    cfg.saved_status = Some(s.status.clone());
                    configs.insert(name.clone(), cfg);
                }
            }
            configs
        };

        if let Ok(json) = serde_json::to_string_pretty(&configs) {
            let services_file = self.services_file.read().unwrap().clone();
            let _ = fs::write(&services_file, json);
        }
    }

    pub fn detect_venv_in_dir(dir: &str) -> Option<String> {
        let dir_path = Path::new(dir);
        let candidates = [".venv", "venv", "env"];
        for c in candidates {
            let py_exe = dir_path.join(c).join("Scripts").join("python.exe");
            if py_exe.exists() {
                return Some(c.to_string());
            }
        }
        None
    }

    pub fn start_service(self: &Arc<Self>, name: &str) -> bool {
        let service_lock = {
            let map = self.services.read().unwrap();
            match map.get(name) {
                Some(s) => s.clone(),
                None => return false,
            }
        };

        let mut s = service_lock.lock().unwrap();
        if s.status == "running" && s.active.is_some() {
            return true;
        }

        s.status = "starting".to_string();

        // 1. Port scavenger
        if let Some(port) = s.config.port {
            if port > 0 {
                free_port(port);
                std::thread::sleep(Duration::from_millis(200));
            }
        }

        // 2. Log file preparation
        let log_file = s.log_file.clone();
        let mut log_writer = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&log_file)
            .ok();

        if let Some(f) = &mut log_writer {
            let now = Utc::now().format("%Y-%m-%d %H:%M:%S").to_string();
            let _ = writeln!(f, "\n[{}] === STARTING SERVICE: {} ===", now, s.config.name);
            let _ = writeln!(f, "Command: {} | CWD: {}", s.config.command, s.config.cwd);
        }

        // 3. Virtualenv detection
        let mut venv = s.config.venv.clone();
        if venv.is_none() {
            venv = Self::detect_venv_in_dir(&s.config.cwd);
        }

        // 4. Command construction via cmd.exe /C
        let mut cmd = Command::new("cmd.exe");
        cmd.args(["/C", &s.config.command]);

        if !s.config.cwd.is_empty() {
            cmd.current_dir(&s.config.cwd);
        } else {
            let base = self.base_dir.read().unwrap().clone();
            cmd.current_dir(base);
        }

        if let Some(v) = &venv {
            let venv_dir = if Path::new(v).is_absolute() {
                PathBuf::from(v)
            } else {
                Path::new(&s.config.cwd).join(v)
            };
            let scripts_dir = venv_dir.join("Scripts");
            if scripts_dir.exists() {
                cmd.env("VIRTUAL_ENV", &venv_dir);
                if let Ok(cur_path) = std::env::var("PATH") {
                    cmd.env("PATH", format!("{};{}", scripts_dir.display(), cur_path));
                }
                if let Some(f) = &mut log_writer {
                    let _ = writeln!(f, "Venv Activated: {}", venv_dir.display());
                }
            }
        }

        if let Some(p) = s.config.port {
            cmd.env("PORT", p.to_string());
        }

        if let Some(custom_envs) = &s.config.env {
            for (k, val) in custom_envs {
                cmd.env(k, val);
            }
        }

        // Standard handles to log file
        if let Some(f) = &log_writer {
            if let Ok(out_file) = f.try_clone() {
                cmd.stdout(Stdio::from(out_file));
            }
            if let Ok(err_file) = f.try_clone() {
                cmd.stderr(Stdio::from(err_file));
            }
        }

        // Windows process creation flags (CREATE_NEW_PROCESS_GROUP = 0x00000200)
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            cmd.creation_flags(0x00000200 | 0x08000000); // CREATE_NEW_PROCESS_GROUP | CREATE_NO_WINDOW
        }

        let child = match cmd.spawn() {
            Ok(c) => c,
            Err(e) => {
                s.status = "crashed".to_string();
                if let Some(f) = &mut log_writer {
                    let _ = writeln!(f, "Launch Failed: {}", e);
                }
                return false;
            }
        };

        let pid = child.id();
        let job = SafeJobObject::create_kill_on_close();
        if let Some(ref j) = job {
            j.assign_pid(pid);
        }

        s.active = Some(ActiveProcess {
            pid,
            job,
            child: Some(child),
            start_time: Instant::now(),
            manual_stop: false,
        });
        s.status = "running".to_string();

        // 5. Exit Watcher & Auto-Restart Goroutine/Task
        let pm_clone = self.clone();
        let service_name = name.to_string();
        let service_lock_clone = service_lock.clone();

        std::thread::spawn(move || {
            let mut child_to_wait = {
                let mut guard = service_lock_clone.lock().unwrap();
                if let Some(ref mut act) = guard.active {
                    act.child.take()
                } else {
                    None
                }
            };

            let exit_status = if let Some(ref mut c) = child_to_wait {
                c.wait().ok()
            } else {
                None
            };

            let (should_restart, was_manual) = {
                let mut guard = service_lock_clone.lock().unwrap();
                guard.active = None;
                if let Some(st) = exit_status {
                    guard.last_exit_code = st.code();
                }

                let manual = guard.status == "stopped";
                if manual {
                    guard.status = "stopped".to_string();
                    (false, true)
                } else {
                    guard.status = "crashed".to_string();
                    guard.restarts += 1;
                    (guard.config.auto_restart, false)
                }
            };

            if should_restart && !was_manual {
                {
                    let mut guard = service_lock_clone.lock().unwrap();
                    guard.status = "restarting".to_string();
                }
                std::thread::sleep(Duration::from_millis(1500));
                pm_clone.start_service(&service_name);
            }
        });

        true
    }

    pub fn stop_service(&self, name: &str) -> bool {
        let service_lock = {
            let map = self.services.read().unwrap();
            match map.get(name) {
                Some(s) => s.clone(),
                None => return false,
            }
        };

        // Scope the lock on `s` so it is DROPPED before terminating processes
        // and before calling save_state(), preventing self-deadlock.
        let (active, pid_opt) = {
            let mut s = service_lock.lock().unwrap();
            s.status = "stopped".to_string();
            let act = s.active.take();
            let pid = act.as_ref().map(|a| a.pid);
            (act, pid)
        };

        if let Some(mut act) = active {
            act.manual_stop = true;
            if let Some(ref job) = act.job {
                job.terminate();
            }
            if let Some(pid) = pid_opt {
                kill_process_tree(pid);
            }
            if let Some(ref mut child) = act.child {
                let _ = child.kill();
            }
        }

        self.save_state();
        true
    }

    pub fn restart_service(self: &Arc<Self>, name: &str) -> bool {
        self.stop_service(name);
        std::thread::sleep(Duration::from_millis(300));
        self.start_service(name)
    }

    pub fn toggle_service(self: &Arc<Self>, name: &str) -> bool {
        let is_running = {
            let map = self.services.read().unwrap();
            if let Some(s_lock) = map.get(name) {
                let s = s_lock.lock().unwrap();
                s.status == "running"
            } else {
                return false;
            }
        };

        if is_running {
            self.stop_service(name)
        } else {
            self.start_service(name)
        }
    }

    pub fn start_all(self: &Arc<Self>) {
        let names: Vec<String> = {
            let map = self.services.read().unwrap();
            map.keys().cloned().collect()
        };
        for name in names {
            self.start_service(&name);
        }
    }

    pub fn stop_all(&self) {
        let names: Vec<String> = {
            let map = self.services.read().unwrap();
            map.keys().cloned().collect()
        };
        for name in names {
            self.stop_service(&name);
        }
    }

    pub fn restart_all(self: &Arc<Self>) {
        let names: Vec<String> = {
            let map = self.services.read().unwrap();
            map.keys().cloned().collect()
        };
        for name in names {
            self.restart_service(&name);
        }
    }

    pub fn add_service(self: &Arc<Self>, cfg: ServiceConfig, auto_start: bool) -> bool {
        let name = cfg.name.clone();
        let log_file = self.logs_dir.read().unwrap().join(format!("{}.log", name));
        let state = ManagedServiceState {
            config: cfg,
            active: None,
            status: "stopped".to_string(),
            restarts: 0,
            last_exit_code: None,
            log_file,
        };

        {
            let mut map = self.services.write().unwrap();
            map.insert(name.clone(), Arc::new(Mutex::new(state)));
        }

        self.save_state();

        if auto_start {
            self.start_service(&name);
        }

        true
    }

    pub fn update_service(
        self: &Arc<Self>,
        old_name: &str,
        cfg: ServiceConfig,
        restart_if_running: bool,
    ) -> bool {
        let was_running = {
            let map = self.services.read().unwrap();
            if let Some(s_lock) = map.get(old_name) {
                let s = s_lock.lock().unwrap();
                s.status == "running"
            } else {
                return false;
            }
        };

        if was_running {
            self.stop_service(old_name);
        }

        {
            let mut map = self.services.write().unwrap();
            map.remove(old_name);
            let log_file = self.logs_dir.read().unwrap().join(format!("{}.log", cfg.name));
            let state = ManagedServiceState {
                config: cfg.clone(),
                active: None,
                status: "stopped".to_string(),
                restarts: 0,
                last_exit_code: None,
                log_file,
            };
            map.insert(cfg.name.clone(), Arc::new(Mutex::new(state)));
        }

        self.save_state();

        if was_running && restart_if_running {
            self.start_service(&cfg.name);
        }
        true
    }

    pub fn delete_service(&self, name: &str) -> bool {
        self.stop_service(name);
        let removed = {
            let mut map = self.services.write().unwrap();
            map.remove(name).is_some()
        };
        self.save_state();
        removed
    }

    // Dynamic Group Management
    pub fn start_group(self: &Arc<Self>, group: &str) {
        let names: Vec<String> = {
            let map = self.services.read().unwrap();
            map.values()
                .filter_map(|s_lock| {
                    let s = s_lock.lock().unwrap();
                    if s.config.group.eq_ignore_ascii_case(group) {
                        Some(s.config.name.clone())
                    } else {
                        None
                    }
                })
                .collect()
        };
        for n in names {
            self.start_service(&n);
        }
    }

    pub fn stop_group(&self, group: &str) {
        let names: Vec<String> = {
            let map = self.services.read().unwrap();
            map.values()
                .filter_map(|s_lock| {
                    let s = s_lock.lock().unwrap();
                    if s.config.group.eq_ignore_ascii_case(group) {
                        Some(s.config.name.clone())
                    } else {
                        None
                    }
                })
                .collect()
        };
        for n in names {
            self.stop_service(&n);
        }
    }

    pub fn restart_group(self: &Arc<Self>, group: &str) {
        let names: Vec<String> = {
            let map = self.services.read().unwrap();
            map.values()
                .filter_map(|s_lock| {
                    let s = s_lock.lock().unwrap();
                    if s.config.group.eq_ignore_ascii_case(group) {
                        Some(s.config.name.clone())
                    } else {
                        None
                    }
                })
                .collect()
        };
        for n in names {
            self.restart_service(&n);
        }
    }

    pub fn rename_group(&self, old_group: &str, new_group: &str) -> usize {
        let mut count = 0;
        let clean_new = new_group.trim();
        if clean_new.is_empty() {
            return 0;
        }

        {
            let map = self.services.read().unwrap();
            for s_lock in map.values() {
                let mut s = s_lock.lock().unwrap();
                if s.config.group.eq_ignore_ascii_case(old_group) {
                    s.config.group = clean_new.to_string();
                    count += 1;
                }
            }
        }

        if count > 0 {
            self.save_state();
        }
        count
    }

    pub fn move_service_to_group(&self, service_name: &str, new_group: &str) -> bool {
        let clean_new = new_group.trim();
        let found = {
            let map = self.services.read().unwrap();
            if let Some(s_lock) = map.get(service_name) {
                let mut s = s_lock.lock().unwrap();
                s.config.group = if clean_new.is_empty() {
                    "default".to_string()
                } else {
                    clean_new.to_string()
                };
                true
            } else {
                false
            }
        };

        if found {
            self.save_state();
        }
        found
    }

    pub fn dissolve_group(&self, group: &str) -> usize {
        self.rename_group(group, "default")
    }

    pub fn get_summary(&self) -> ManagerSummary {
        let admin = is_admin();
        let map = self.services.read().unwrap();
        let mut reports = Vec::new();
        let mut group_map: HashMap<String, (usize, usize, Vec<String>)> = HashMap::new();

        for s_lock in map.values() {
            let s = s_lock.lock().unwrap();
            let mut pid_val = None;
            let mut uptime_secs = 0;
            let mut ram = 0.0;

            if let Some(ref act) = s.active {
                pid_val = Some(act.pid);
                uptime_secs = act.start_time.elapsed().as_secs();
                ram = get_process_memory_mb(act.pid);
            }

            let report = ServiceStatusReport {
                name: s.config.name.clone(),
                group: s.config.group.clone(),
                command: s.config.command.clone(),
                cwd: s.config.cwd.clone(),
                port: s.config.port,
                auto_restart: s.config.auto_restart,
                venv: s.config.venv.clone(),
                shortcut: s.config.shortcut.clone(),
                pid: pid_val,
                status: s.status.clone(),
                restarts: s.restarts,
                uptime_seconds: uptime_secs,
                log_file: s.log_file.to_string_lossy().to_string(),
                last_exit_code: s.last_exit_code,
                memory_mb: ram,
            };

            let entry = group_map
                .entry(s.config.group.clone())
                .or_insert((0, 0, Vec::new()));
            entry.0 += 1;
            if s.status == "running" {
                entry.1 += 1;
            }
            entry.2.push(s.config.name.clone());

            reports.push(report);
        }

        // Sort by name
        reports.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));

        let mut groups = Vec::new();
        for (g_name, (tot, run, svcs)) in group_map {
            groups.push(GroupSummary {
                name: g_name,
                total: tot,
                running: run,
                services: svcs,
            });
        }
        groups.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));

        ManagerSummary {
            admin,
            services: reports,
            groups,
        }
    }

    pub fn get_logs(&self, name: &str, lines_count: usize) -> String {
        let log_file = self.logs_dir.read().unwrap().join(format!("{}.log", name));
        if !log_file.exists() {
            return "(No log file recorded yet)".to_string();
        }

        let file = match File::open(&log_file) {
            Ok(f) => f,
            Err(_) => return "(Failed to open log file)".to_string(),
        };

        let reader = BufReader::new(file);
        let lines: Vec<String> = reader.lines().filter_map(|l| l.ok()).collect();

        if lines.is_empty() {
            return "(Log file is empty)".to_string();
        }

        let take_count = if lines_count == 0 { 250 } else { lines_count };
        let start = if lines.len() > take_count {
            lines.len() - take_count
        } else {
            0
        };

        lines[start..].join("\n")
    }

    pub fn clear_logs(&self, name: &str) -> bool {
        let log_file = self.logs_dir.read().unwrap().join(format!("{}.log", name));
        if log_file.exists() {
            let _ = fs::write(&log_file, "");
            true
        } else {
            false
        }
    }
}
