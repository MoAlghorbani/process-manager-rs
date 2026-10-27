use crate::jiggler::JigglerEngine;
use crate::manager::ProcessManager;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::thread;
use windows_sys::Win32::Foundation::HWND;
use windows_sys::Win32::System::Threading::GetCurrentThreadId;
use windows_sys::Win32::UI::Input::KeyboardAndMouse::{RegisterHotKey, UnregisterHotKey};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    GetMessageW, PeekMessageW, PostThreadMessageW, MSG, WM_HOTKEY, WM_USER,
};

pub const WM_RELOAD_HOTKEYS: u32 = WM_USER + 101;
pub const WM_STOP_HOTKEYS: u32 = WM_USER + 102;

const MOD_ALT: u32 = 0x0001;
const MOD_CONTROL: u32 = 0x0002;
const MOD_SHIFT: u32 = 0x0004;
const MOD_WIN: u32 = 0x0008;
const MOD_NOREPEAT: u32 = 0x4000;

/// Translates user shortcut string (e.g. "Ctrl+Alt+1", "Ctrl+Alt+J", "F5")
/// to Win32 modifiers and Virtual Key code.
pub fn parse_shortcut(shortcut_str: &str) -> Option<(u32, u32)> {
    let raw = shortcut_str.trim();
    if raw.is_empty() {
        return None;
    }

    let parts: Vec<&str> = raw.split('+').collect();
    let mut modifiers: u32 = MOD_NOREPEAT;
    let mut vk: u32 = 0;

    for part in parts {
        let p = part.trim().to_lowercase();
        match p.as_str() {
            "ctrl" | "control" => modifiers |= MOD_CONTROL,
            "alt" => modifiers |= MOD_ALT,
            "shift" => modifiers |= MOD_SHIFT,
            "win" | "super" | "cmd" => modifiers |= MOD_WIN,
            other => {
                let u = other.to_uppercase();
                if u.len() == 1 {
                    let b = u.as_bytes()[0];
                    if (b'0'..=b'9').contains(&b) || (b'A'..=b'Z').contains(&b) {
                        vk = b as u32;
                    }
                } else if u.starts_with('F') && u.len() >= 2 {
                    if let Ok(num) = u[1..].parse::<u32>() {
                        if (1..=12).contains(&num) {
                            vk = 0x6F + num; // F1 is 0x70, F2 is 0x71, etc.
                        }
                    }
                } else {
                    match u.as_str() {
                        "SPACE" => vk = 0x20,
                        "TAB" => vk = 0x09,
                        "ENTER" | "RETURN" => vk = 0x0D,
                        "ESC" | "ESCAPE" => vk = 0x1B,
                        _ => {}
                    }
                }
            }
        }
    }

    if vk != 0 {
        Some((modifiers, vk))
    } else {
        None
    }
}

pub struct HotkeyManager {
    thread_id: Arc<Mutex<Option<u32>>>,
    pm: Arc<ProcessManager>,
    jiggler: Arc<JigglerEngine>,
}

impl HotkeyManager {
    pub fn new(pm: Arc<ProcessManager>, jiggler: Arc<JigglerEngine>) -> Self {
        Self {
            thread_id: Arc::new(Mutex::new(None)),
            pm,
            jiggler,
        }
    }

    pub fn start(&self) {
        let thread_id_arc = self.thread_id.clone();
        let pm = self.pm.clone();
        let jiggler = self.jiggler.clone();

        thread::spawn(move || {
            let tid = unsafe { GetCurrentThreadId() };
            {
                let mut guard = thread_id_arc.lock().unwrap();
                *guard = Some(tid);
            }

            // Force creation of Win32 message queue for this background thread
            unsafe {
                let mut dummy_msg: MSG = std::mem::zeroed();
                PeekMessageW(&mut dummy_msg, 0 as HWND, 0, 0, 0);
            }

            let mut hotkey_map: HashMap<i32, String> = HashMap::new();
            let mut registered_ids: Vec<i32> = Vec::new();

            let unregister_all = |registered_ids: &mut Vec<i32>| {
                for &id in registered_ids.iter() {
                    unsafe {
                        UnregisterHotKey(0 as HWND, id);
                    }
                }
                registered_ids.clear();
            };

            let register_all = |registered_ids: &mut Vec<i32>, hotkey_map: &mut HashMap<i32, String>| {
                unregister_all(registered_ids);
                hotkey_map.clear();

                let mut current_id: i32 = 1000;

                // 1. Register Service Shortcuts
                let services = {
                    let map = pm.services.read().unwrap();
                    let mut list = Vec::new();
                    for s_lock in map.values() {
                        let s = s_lock.lock().unwrap();
                        if let Some(ref sc) = s.config.shortcut {
                            if !sc.trim().is_empty() {
                                list.push((s.config.name.clone(), sc.clone()));
                            }
                        }
                    }
                    list
                };

                for (name, sc_str) in services {
                    if let Some((mods, vk)) = parse_shortcut(&sc_str) {
                        current_id += 1;
                        let ok = unsafe { RegisterHotKey(0 as HWND, current_id, mods, vk) };
                        if ok != 0 {
                            hotkey_map.insert(current_id, name);
                            registered_ids.push(current_id);
                        }
                    }
                }

                // 2. Register Mouse Jiggler Shortcut (e.g. "Ctrl+Alt+J")
                let jiggler_sc = jiggler.get_config().shortcut;
                if !jiggler_sc.trim().is_empty() {
                    if let Some((mods, vk)) = parse_shortcut(&jiggler_sc) {
                        current_id += 1;
                        let ok = unsafe { RegisterHotKey(0 as HWND, current_id, mods, vk) };
                        if ok != 0 {
                            hotkey_map.insert(current_id, "__mouse_jiggler__".to_string());
                            registered_ids.push(current_id);
                        }
                    }
                }
            };

            register_all(&mut registered_ids, &mut hotkey_map);

            // Win32 Message Loop
            unsafe {
                let mut msg: MSG = std::mem::zeroed();
                while GetMessageW(&mut msg, 0 as HWND, 0, 0) > 0 {
                    match msg.message {
                        WM_HOTKEY => {
                            let hk_id = msg.wParam as i32;
                            if let Some(target) = hotkey_map.get(&hk_id).cloned() {
                                if target == "__mouse_jiggler__" {
                                    jiggler.toggle();
                                } else {
                                    pm.toggle_service(&target);
                                }
                            }
                        }
                        WM_RELOAD_HOTKEYS => {
                            register_all(&mut registered_ids, &mut hotkey_map);
                        }
                        WM_STOP_HOTKEYS => {
                            unregister_all(&mut registered_ids);
                            break;
                        }
                        _ => {}
                    }
                }
                unregister_all(&mut registered_ids);
            }
        });
    }

    pub fn reload(&self) {
        let guard = self.thread_id.lock().unwrap();
        if let Some(tid) = *guard {
            unsafe {
                PostThreadMessageW(tid, WM_RELOAD_HOTKEYS, 0, 0);
            }
        }
    }

    pub fn stop(&self) {
        let guard = self.thread_id.lock().unwrap();
        if let Some(tid) = *guard {
            unsafe {
                PostThreadMessageW(tid, WM_STOP_HOTKEYS, 0, 0);
            }
        }
    }
}
