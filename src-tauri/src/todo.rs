use crate::manager::get_data_dir;
use crate::models::TodoItem;
use chrono::{DateTime, Utc};
use std::fs;
use std::path::PathBuf;
use std::sync::{Arc, RwLock};
use std::time::Duration;

pub struct TodoStore {
    pub file_path: Arc<RwLock<PathBuf>>,
    pub items: Arc<RwLock<Vec<TodoItem>>>,
}

impl TodoStore {
    pub fn new() -> Self {
        let base_dir = get_data_dir();
        let file_path = base_dir.join("todos.json");
        let mut items = Vec::new();

        if file_path.exists() {
            if let Ok(data) = fs::read_to_string(&file_path) {
                if let Ok(list) = serde_json::from_str::<Vec<TodoItem>>(&data) {
                    items = list;
                }
            }
        }

        Self {
            file_path: Arc::new(RwLock::new(file_path)),
            items: Arc::new(RwLock::new(items)),
        }
    }

    pub fn reload_from_dir(&self, new_dir: &std::path::Path) {
        let new_file = new_dir.join("todos.json");
        *self.file_path.write().unwrap() = new_file.clone();
        let mut items = Vec::new();
        if new_file.exists() {
            if let Ok(data) = fs::read_to_string(&new_file) {
                if let Ok(list) = serde_json::from_str::<Vec<TodoItem>>(&data) {
                    items = list;
                }
            }
        }
        *self.items.write().unwrap() = items;
    }

    pub fn list(
        &self,
        search: &str,
        project_filter: Option<&str>,
        status_filter: Option<&str>,
    ) -> Vec<TodoItem> {
        let q = search.trim().to_lowercase();
        let items = self.items.read().unwrap();

        items
            .iter()
            .filter(|it| {
                if let Some(status) = status_filter {
                    if !it.status.eq_ignore_ascii_case(status) {
                        return false;
                    }
                }
                if let Some(proj) = project_filter {
                    if !proj.is_empty() && !proj.eq_ignore_ascii_case("all") {
                        if !it
                            .project
                            .as_deref()
                            .unwrap_or("General")
                            .eq_ignore_ascii_case(proj)
                        {
                            return false;
                        }
                    }
                }
                if q.is_empty() {
                    true
                } else {
                    it.title.to_lowercase().contains(&q)
                        || it
                            .project
                            .as_deref()
                            .unwrap_or("")
                            .to_lowercase()
                            .contains(&q)
                }
            })
            .cloned()
            .collect()
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
        title: &str,
        project: Option<String>,
        priority: Option<String>,
        status: Option<String>,
        notify_at: Option<DateTime<Utc>>,
    ) -> TodoItem {
        let now = Utc::now();
        let id = format!("{:06x}", rand::random::<u32>() & 0xFFFFFF);
        let st = status.unwrap_or_else(|| "todo".to_string());
        let completed = st == "done";

        let item = TodoItem {
            id,
            title: title.trim().to_string(),
            project: project.filter(|p| !p.trim().is_empty()),
            status: st,
            completed,
            priority: priority.filter(|p| !p.trim().is_empty()),
            notify_at,
            notified: Some(false),
            created_at: now,
            completed_at: if completed { Some(now) } else { None },
        };

        {
            let mut list = self.items.write().unwrap();
            list.push(item.clone());
        }
        self.save();
        item
    }

    pub fn update_status(&self, id: &str, new_status: &str) -> bool {
        let mut found = false;
        let now = Utc::now();
        {
            let mut list = self.items.write().unwrap();
            if let Some(it) = list.iter_mut().find(|it| it.id == id) {
                it.status = new_status.to_string();
                it.completed = new_status == "done";
                if it.completed {
                    it.completed_at = Some(now);
                } else {
                    it.completed_at = None;
                }
                found = true;
            }
        }
        if found {
            self.save();
        }
        found
    }

    pub fn toggle(&self, id: &str) -> bool {
        let mut found = false;
        let now = Utc::now();
        {
            let mut list = self.items.write().unwrap();
            if let Some(it) = list.iter_mut().find(|it| it.id == id) {
                if it.completed {
                    it.completed = false;
                    it.status = "todo".to_string();
                    it.completed_at = None;
                } else {
                    it.completed = true;
                    it.status = "done".to_string();
                    it.completed_at = Some(now);
                }
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

    pub fn clear_completed(&self) -> usize {
        let cleared = {
            let mut list = self.items.write().unwrap();
            let initial = list.len();
            list.retain(|it| !it.completed && it.status != "done");
            initial - list.len()
        };
        if cleared > 0 {
            self.save();
        }
        cleared
    }

    pub fn rename_project(&self, old_project: &str, new_project: &str) -> usize {
        let mut count = 0;
        {
            let mut list = self.items.write().unwrap();
            for it in list.iter_mut() {
                if let Some(ref p) = it.project {
                    if p.eq_ignore_ascii_case(old_project) {
                        it.project = Some(new_project.to_string());
                        count += 1;
                    }
                }
            }
        }
        if count > 0 {
            self.save();
        }
        count
    }

    pub fn start_notification_worker(&self) {
        let items_clone = self.items.clone();
        let file_path = self.file_path.clone();

        std::thread::spawn(move || {
            loop {
                std::thread::sleep(Duration::from_secs(10));

                let now = Utc::now();
                let mut triggered = Vec::new();

                {
                    let mut list = items_clone.write().unwrap();
                    for it in list.iter_mut() {
                        if !it.completed && it.status != "done" {
                            if let Some(notify_at) = it.notify_at {
                                if now >= notify_at && it.notified != Some(true) {
                                    it.notified = Some(true);
                                    triggered.push((it.id.clone(), it.title.clone()));
                                }
                            }
                        }
                    }
                }

                if !triggered.is_empty() {
                    // Save notification status
                    {
                        let list = items_clone.read().unwrap();
                        if let Ok(json) = serde_json::to_string_pretty(&*list) {
                            let p = file_path.read().unwrap().clone();
                            let _ = fs::write(&p, json);
                        }
                    }

                    // Windows notification via PowerShell
                    for (_, title) in triggered {
                        let script = format!(
                            r#"[Windows.UI.Notifications.ToastNotificationManager, Windows.UI.Notifications, ContentType = WindowsRuntime] | Out-Null
$template = [Windows.UI.Notifications.ToastNotificationManager]::GetTemplateContent([Windows.UI.Notifications.ToastTemplateType]::ToastText02)
$textNodes = $template.GetElementsByTagName("text")
$textNodes.Item(0).AppendChild($template.CreateTextNode("Process Manager Reminder")) | Out-Null
$textNodes.Item(1).AppendChild($template.CreateTextNode("{}")) | Out-Null
$toast = [Windows.UI.Notifications.ToastNotification]::new($template)
[Windows.UI.Notifications.ToastNotificationManager]::CreateToastNotifier("ProcessManager").Show($toast)"#,
                            title.replace('"', "\\\"")
                        );

                        let _ = std::process::Command::new("powershell")
                            .args(["-NoProfile", "-Command", &script])
                            .output();
                    }
                }
            }
        });
    }
}
