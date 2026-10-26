use crate::manager::get_data_dir;
use crate::models::{JigglerConfig, JigglerStats};
use crate::win32::{
    get_idle_seconds, send_key_tap, send_relative_mouse_move, set_keep_awake, VK_CONTROL, VK_F15,
    VK_SHIFT,
};
use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, RwLock};
use std::time::Duration;

pub struct JigglerEngine {
    pub config_file: RwLock<PathBuf>,
    pub config: RwLock<JigglerConfig>,
    pub stats: Arc<Mutex<JigglerStats>>,
    pub is_running: Arc<AtomicBool>,
}

impl JigglerEngine {
    pub fn new() -> Self {
        let base_dir = get_data_dir();
        let config_file = base_dir.join("jiggler_config.json");
        let mut config = JigglerConfig::default();

        if config_file.exists() {
            if let Ok(c) = fs::read_to_string(&config_file) {
                if let Ok(loaded) = serde_json::from_str::<JigglerConfig>(&c) {
                    config = loaded;
                }
            }
        }

        Self {
            config_file: RwLock::new(config_file),
            config: RwLock::new(config),
            stats: Arc::new(Mutex::new(JigglerStats::default())),
            is_running: Arc::new(AtomicBool::new(false)),
        }
    }

    pub fn reload_from_dir(&self, new_dir: &std::path::Path) {
        let new_file = new_dir.join("jiggler_config.json");
        *self.config_file.write().unwrap() = new_file.clone();
        if new_file.exists() {
            if let Ok(c) = fs::read_to_string(&new_file) {
                if let Ok(loaded) = serde_json::from_str::<JigglerConfig>(&c) {
                    *self.config.write().unwrap() = loaded;
                }
            }
        }
    }

    pub fn get_config(&self) -> JigglerConfig {
        self.config.read().unwrap().clone()
    }

    pub fn save_config(&self, cfg: JigglerConfig) {
        {
            let mut guard = self.config.write().unwrap();
            *guard = cfg.clone();
        }
        if let Ok(json) = serde_json::to_string_pretty(&cfg) {
            let p = self.config_file.read().unwrap().clone();
            let _ = fs::write(&p, json);
        }
    }

    pub fn get_stats(&self) -> JigglerStats {
        let mut s = self.stats.lock().unwrap().clone();
        s.running = self.is_running.load(Ordering::SeqCst);
        s.current_idle_sec = get_idle_seconds();
        s
    }

    pub fn start(&self) {
        if self.is_running.swap(true, Ordering::SeqCst) {
            return; // Already running
        }

        let is_running_clone = self.is_running.clone();
        let stats_clone = self.stats.clone();
        let cfg = self.get_config();

        // Enforce keep awake
        set_keep_awake(cfg.keep_awake_enabled);

        std::thread::spawn(move || {
            let mut mouse_countdown = cfg.mouse_interval_sec;
            let mut key_countdown = cfg.key_interval_sec;

            {
                let mut st = stats_clone.lock().unwrap();
                st.running = true;
                st.last_action_msg = "Anti-Idle Active".to_string();
            }

            while is_running_clone.load(Ordering::SeqCst) {
                std::thread::sleep(Duration::from_secs(1));

                if !is_running_clone.load(Ordering::SeqCst) {
                    break;
                }

                let current_idle = get_idle_seconds();

                // Check idle guard
                let user_active = cfg.idle_guard_enabled
                    && current_idle < (cfg.idle_threshold_sec as f64);

                if user_active {
                    let mut st = stats_clone.lock().unwrap();
                    st.last_action_msg = "Paused (User Typing / Mouse Active)".to_string();
                    continue;
                }

                if mouse_countdown > 0 {
                    mouse_countdown -= 1;
                }
                if key_countdown > 0 {
                    key_countdown -= 1;
                }

                // Mouse action
                if cfg.mouse_enabled && mouse_countdown == 0 {
                    match cfg.mouse_move_mode.as_str() {
                        "zen" => {
                            send_relative_mouse_move(1, 0);
                            std::thread::sleep(Duration::from_millis(20));
                            send_relative_mouse_move(-1, 0);
                        }
                        "gentle" => {
                            send_relative_mouse_move(20, 0);
                            std::thread::sleep(Duration::from_millis(100));
                            send_relative_mouse_move(-20, 0);
                        }
                        "orbit" => {
                            // Circular arc (20 steps)
                            let radius = 28.0;
                            let steps = 20;
                            for i in 0..steps {
                                let angle = (i as f64 / steps as f64) * 2.0 * std::f64::consts::PI;
                                let dx = (angle.cos() * radius) as i32;
                                let dy = (angle.sin() * radius) as i32;
                                send_relative_mouse_move(dx / 5, dy / 5);
                                std::thread::sleep(Duration::from_millis(15));
                            }
                        }
                        "api_only" => {
                            set_keep_awake(true);
                        }
                        _ => {
                            send_relative_mouse_move(1, 0);
                            send_relative_mouse_move(-1, 0);
                        }
                    }

                    mouse_countdown = cfg.mouse_interval_sec;
                    let mut st = stats_clone.lock().unwrap();
                    st.total_mouse_moves += 1;
                    st.last_action_msg = format!("Nudged Mouse ({})", cfg.mouse_move_mode);
                }

                // Key tap action
                if cfg.key_enabled && key_countdown == 0 {
                    let vk = match cfg.key_choice.as_str() {
                        "shift" => VK_SHIFT,
                        "control" => VK_CONTROL,
                        _ => VK_F15,
                    };
                    send_key_tap(vk);

                    key_countdown = cfg.key_interval_sec;
                    let mut st = stats_clone.lock().unwrap();
                    st.total_key_taps += 1;
                    st.last_action_msg = format!("Tapped Ghost Key ({})", cfg.key_choice);
                }

                {
                    let mut st = stats_clone.lock().unwrap();
                    st.next_mouse_move_sec = mouse_countdown;
                    st.next_key_tap_sec = key_countdown;
                }
            }

            // Restore sleep settings
            set_keep_awake(false);
            {
                let mut st = stats_clone.lock().unwrap();
                st.running = false;
                st.last_action_msg = "Stopped".to_string();
            }
        });
    }

    pub fn stop(&self) {
        self.is_running.store(false, Ordering::SeqCst);
    }

    pub fn toggle(&self) -> bool {
        if self.is_running.load(Ordering::SeqCst) {
            self.stop();
            false
        } else {
            self.start();
            true
        }
    }
}
