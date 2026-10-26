use crate::manager::get_data_dir;
use crate::models::CredentialItem;
use chrono::Utc;
use rand::seq::SliceRandom;
use std::fs;
use std::path::PathBuf;
use std::sync::RwLock;

pub struct CredentialStore {
    pub file_path: RwLock<PathBuf>,
    pub items: RwLock<Vec<CredentialItem>>,
}

impl CredentialStore {
    pub fn new() -> Self {
        let base_dir = get_data_dir();
        let file_path = base_dir.join("credentials.json");
        let mut items = Vec::new();

        if file_path.exists() {
            if let Ok(data) = fs::read_to_string(&file_path) {
                if let Ok(list) = serde_json::from_str::<Vec<CredentialItem>>(&data) {
                    items = list;
                }
            }
        }

        Self {
            file_path: RwLock::new(file_path),
            items: RwLock::new(items),
        }
    }

    pub fn reload_from_dir(&self, new_dir: &std::path::Path) {
        let new_file = new_dir.join("credentials.json");
        *self.file_path.write().unwrap() = new_file.clone();
        let mut items = Vec::new();
        if new_file.exists() {
            if let Ok(data) = fs::read_to_string(&new_file) {
                if let Ok(list) = serde_json::from_str::<Vec<CredentialItem>>(&data) {
                    items = list;
                }
            }
        }
        *self.items.write().unwrap() = items;
    }

    pub fn list(&self, search: &str) -> Vec<CredentialItem> {
        let q = search.trim().to_lowercase();
        let items = self.items.read().unwrap();
        if q.is_empty() {
            let mut list = items.clone();
            list.sort_by(|a, b| a.platform.to_lowercase().cmp(&b.platform.to_lowercase()));
            list
        } else {
            let mut list: Vec<CredentialItem> = items
                .iter()
                .filter(|it| {
                    it.platform.to_lowercase().contains(&q)
                        || it.username.to_lowercase().contains(&q)
                        || it.url.as_deref().unwrap_or("").to_lowercase().contains(&q)
                        || it.notes.as_deref().unwrap_or("").to_lowercase().contains(&q)
                })
                .cloned()
                .collect();
            list.sort_by(|a, b| a.platform.to_lowercase().cmp(&b.platform.to_lowercase()));
            list
        }
    }

    pub fn save(&self) {
        let items = self.items.read().unwrap();
        if let Ok(json) = serde_json::to_string_pretty(&*items) {
            let p = self.file_path.read().unwrap().clone();
            let _ = fs::write(&p, json);
        }
    }

    pub fn add(
        &self,
        platform: &str,
        username: &str,
        password: &str,
        url: Option<String>,
        notes: Option<String>,
    ) -> CredentialItem {
        let now = Utc::now();
        let id = format!("{:x}", rand::random::<u32>());
        let item = CredentialItem {
            id,
            platform: platform.trim().to_string(),
            username: username.trim().to_string(),
            password: password.to_string(),
            url: url.filter(|u| !u.trim().is_empty()),
            notes: notes.filter(|n| !n.trim().is_empty()),
            created_at: now,
            updated_at: now,
        };

        {
            let mut items = self.items.write().unwrap();
            items.push(item.clone());
        }
        self.save();
        item
    }

    pub fn update(&self, id: &str, updated: CredentialItem) -> bool {
        let mut found = false;
        {
            let mut items = self.items.write().unwrap();
            if let Some(pos) = items.iter().position(|it| it.id == id) {
                let mut it = updated;
                it.updated_at = Utc::now();
                items[pos] = it;
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
            let mut items = self.items.write().unwrap();
            if let Some(pos) = items.iter().position(|it| it.id == id) {
                items.remove(pos);
                found = true;
            }
        }
        if found {
            self.save();
        }
        found
    }

    pub fn generate_password(
        length: usize,
        upper: bool,
        lower: bool,
        digits: bool,
        symbols: bool,
    ) -> String {
        let mut charset: Vec<u8> = Vec::new();
        if lower {
            charset.extend(b"abcdefghijklmnopqrstuvwxyz");
        }
        if upper {
            charset.extend(b"ABCDEFGHIJKLMNOPQRSTUVWXYZ");
        }
        if digits {
            charset.extend(b"0123456789");
        }
        if symbols {
            charset.extend(b"!@#$%^&*()-_=+[]{}<>?");
        }
        if charset.is_empty() {
            charset.extend(b"abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789");
        }

        let mut rng = rand::thread_rng();
        let len = length.clamp(8, 64);
        (0..len)
            .map(|_| *charset.choose(&mut rng).unwrap() as char)
            .collect()
    }
}
