use crate::manager::get_data_dir;
use crate::models::ClipboardItem;
use crate::win32::{get_clipboard_sequence_num, read_clipboard_text};
use chrono::Utc;
use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, RwLock};
use std::time::Duration;

pub struct ClipboardWatcher {
    pub file_path: Arc<RwLock<PathBuf>>,
    pub items: Arc<RwLock<Vec<ClipboardItem>>>,
    pub is_running: Arc<AtomicBool>,
}

impl ClipboardWatcher {
    pub fn new() -> Self {
        let base_dir = get_data_dir();
        let file_path = base_dir.join("clipboard_history.json");
        let mut items = Vec::new();

        if file_path.exists() {
            if let Ok(data) = fs::read_to_string(&file_path) {
                if let Ok(list) = serde_json::from_str::<Vec<ClipboardItem>>(&data) {
                    items = list;
                }
            }
        }

        Self {
            file_path: Arc::new(RwLock::new(file_path)),
            items: Arc::new(RwLock::new(items)),
            is_running: Arc::new(AtomicBool::new(false)),
        }
    }

    pub fn reload_from_dir(&self, new_dir: &std::path::Path) {
        let new_file = new_dir.join("clipboard_history.json");
        *self.file_path.write().unwrap() = new_file.clone();
        let mut items = Vec::new();
        if new_file.exists() {
            if let Ok(data) = fs::read_to_string(&new_file) {
                if let Ok(list) = serde_json::from_str::<Vec<ClipboardItem>>(&data) {
                    items = list;
                }
            }
        }
        *self.items.write().unwrap() = items;
    }

    pub fn start(&self) {
        if self.is_running.swap(true, Ordering::SeqCst) {
            return;
        }

        let items_clone = self.items.clone();
        let is_running_clone = self.is_running.clone();
        let file_path = self.file_path.clone();

        std::thread::spawn(move || {
            let mut last_seq = get_clipboard_sequence_num();
            let mut last_content = String::new();

            while is_running_clone.load(Ordering::SeqCst) {
                std::thread::sleep(Duration::from_millis(400));

                let cur_seq = get_clipboard_sequence_num();
                if cur_seq != last_seq {
                    last_seq = cur_seq;

                    if let Some(text) = read_clipboard_text() {
                        let trimmed = text.trim();
                        if !trimmed.is_empty() && trimmed != last_content {
                            last_content = trimmed.to_string();

                            let mut list = items_clone.write().unwrap();

                            // Deduplicate if already top item
                            if let Some(first) = list.first() {
                                if first.content == text {
                                    continue;
                                }
                            }

                            let id = format!("{:x}", rand::random::<u32>());
                            let item = ClipboardItem {
                                id,
                                content: text.clone(),
                                timestamp: Utc::now(),
                                pinned: false,
                                length: text.chars().count(),
                            };

                            list.insert(0, item);

                            // Cap at 300 entries (keeping pinned items)
                            if list.len() > 300 {
                                if let Some(last_unpinned) = list.iter().rposition(|it| !it.pinned)
                                {
                                    list.remove(last_unpinned);
                                }
                            }

                            if let Ok(json) = serde_json::to_string_pretty(&*list) {
                                let p = file_path.read().unwrap().clone();
                                let _ = fs::write(&p, json);
                            }
                        }
                    }
                }
            }
        });
    }

    pub fn stop(&self) {
        self.is_running.store(false, Ordering::SeqCst);
    }

    pub fn list(&self, search: &str, pinned_only: bool) -> Vec<ClipboardItem> {
        let q = search.trim().to_lowercase();
        let list = self.items.read().unwrap();
        list.iter()
            .filter(|it| {
                if pinned_only && !it.pinned {
                    return false;
                }
                if q.is_empty() {
                    true
                } else {
                    it.content.to_lowercase().contains(&q)
                }
            })
            .cloned()
            .collect()
    }

    pub fn save(&self) {
        let list = self.items.read().unwrap();
        if let Ok(json) = serde_json::to_string_pretty(&*list) {
            let p = self.file_path.read().unwrap().clone();
            let _ = fs::write(&p, json);
        }
    }

    pub fn toggle_pin(&self, id: &str) -> bool {
        let mut found = false;
        {
            let mut list = self.items.write().unwrap();
            if let Some(it) = list.iter_mut().find(|it| it.id == id) {
                it.pinned = !it.pinned;
                found = true;
            }
        }
        if found {
            self.save();
        }
        found
    }

    pub fn delete(&self, id: &str) -> bool {
        let mut found = false;
        {
            let mut list = self.items.write().unwrap();
            if let Some(pos) = list.iter().position(|it| it.id == id) {
                list.remove(pos);
                found = true;
            }
        }
        if found {
            self.save();
        }
        found
    }

    pub fn clear_unpinned(&self) -> usize {
        let cleared = {
            let mut list = self.items.write().unwrap();
            let initial_len = list.len();
            list.retain(|it| it.pinned);
            initial_len - list.len()
        };
        if cleared > 0 {
            self.save();
        }
        cleared
    }
}
