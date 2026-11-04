use crate::jiggler::JigglerEngine;
use crate::manager::ProcessManager;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};
use windows_sys::Win32::Foundation::{HINSTANCE, HWND, LPARAM, LRESULT, WPARAM};
use windows_sys::Win32::System::Threading::GetCurrentThreadId;
use windows_sys::Win32::UI::Input::KeyboardAndMouse::{
    GetAsyncKeyState, VK_CONTROL, VK_LWIN, VK_MENU, VK_RWIN, VK_SHIFT,
};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    CallNextHookEx, GetMessageW, PostThreadMessageW, SetWindowsHookExW, UnhookWindowsHookEx,
    KBDLLHOOKSTRUCT, MSG, WH_KEYBOARD_LL, WM_KEYDOWN, WM_SYSKEYDOWN, WM_USER,
};

pub const WM_RELOAD_HOTKEYS: u32 = WM_USER + 101;
pub const WM_STOP_HOTKEYS: u32 = WM_USER + 102;

#[derive(Clone, Debug)]
pub struct ParsedShortcut {
    pub target: String,
    pub original: String,
    pub ctrl: bool,
    pub alt: bool,
    pub shift: bool,
    pub win: bool,
    pub keys: Vec<u32>,
}

#[derive(Clone, Debug)]
struct KeyRecord {
    vk: u32,
    time: Instant,
    alt: bool,
    ctrl: bool,
    shift: bool,
    win: bool,
}

struct HotkeyState {
    pm: Arc<ProcessManager>,
    jiggler: Arc<JigglerEngine>,
    shortcuts: Vec<ParsedShortcut>,
    recent_keys: Vec<KeyRecord>,
    last_trigger: HashMap<String, Instant>,
}

static HOTKEY_STATE: Mutex<Option<HotkeyState>> = Mutex::new(None);

pub fn parse_shortcut(shortcut_str: &str, target: &str) -> Option<ParsedShortcut> {
    let raw = shortcut_str.trim();
    if raw.is_empty() {
        return None;
    }

    let parts: Vec<&str> = raw.split('+').collect();
    let mut ctrl = false;
    let mut alt = false;
    let mut shift = false;
    let mut win = false;
    let mut keys = Vec::new();

    for part in parts {
        let p = part.trim().to_lowercase();
        match p.as_str() {
            "ctrl" | "control" => ctrl = true,
            "alt" => alt = true,
            "shift" => shift = true,
            "win" | "super" | "cmd" => win = true,
            other => {
                let u = other.to_uppercase();
                if u.len() == 1 {
                    let b = u.as_bytes()[0];
                    if (b'0'..=b'9').contains(&b) || (b'A'..=b'Z').contains(&b) {
                        keys.push(b as u32);
                    }
                } else if u.starts_with('F') && u.len() >= 2 {
                    if let Ok(num) = u[1..].parse::<u32>() {
                        if (1..=12).contains(&num) {
                            keys.push(0x6F + num);
                        }
                    }
                } else {
                    match u.as_str() {
                        "SPACE" => keys.push(0x20),
                        "TAB" => keys.push(0x09),
                        "ENTER" | "RETURN" => keys.push(0x0D),
                        "ESC" | "ESCAPE" => keys.push(0x1B),
                        _ => {}
                    }
                }
            }
        }
    }

    if !keys.is_empty() {
        Some(ParsedShortcut {
            target: target.to_string(),
            original: shortcut_str.to_string(),
            ctrl,
            alt,
            shift,
            win,
            keys,
        })
    } else {
        None
    }
}

fn load_shortcuts(pm: &Arc<ProcessManager>, jiggler: &Arc<JigglerEngine>) -> Vec<ParsedShortcut> {
    let mut list = Vec::new();

    // 1. Service Shortcuts
    let services = {
        let map = pm.services.read().unwrap();
        let mut svc_list = Vec::new();
        for s_lock in map.values() {
            let s = s_lock.lock().unwrap();
            if let Some(ref sc) = s.config.shortcut {
                if !sc.trim().is_empty() {
                    svc_list.push((s.config.name.clone(), sc.clone()));
                }
            }
        }
        svc_list
    };

    for (name, sc_str) in services {
        if let Some(parsed) = parse_shortcut(&sc_str, &name) {
            list.push(parsed);
        }
    }

    // 2. Mouse Jiggler Shortcut (e.g. "Ctrl+Alt+J")
    let jiggler_sc = jiggler.get_config().shortcut;
    if !jiggler_sc.trim().is_empty() {
        if let Some(parsed) = parse_shortcut(&jiggler_sc, "__mouse_jiggler__") {
            list.push(parsed);
        }
    }

    list
}

unsafe extern "system" fn low_level_keyboard_proc(
    code: i32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    unsafe {
        if code >= 0 && (wparam == WM_KEYDOWN as WPARAM || wparam == WM_SYSKEYDOWN as WPARAM) {
            let kbd = *(lparam as *const KBDLLHOOKSTRUCT);
            let mut raw_vk = kbd.vkCode as u32;

            // Normalize Numpad 0..9 to standard '0'..'9' (0x30..0x39)
            if (0x60..=0x69).contains(&raw_vk) {
                raw_vk = 0x30 + (raw_vk - 0x60);
            }

            // Filter out modifier keys themselves (tracked via GetAsyncKeyState)
            let is_modifier_key = matches!(
                raw_vk,
                0x10 | 0xA0 | 0xA1 | // Shift
                0x11 | 0xA2 | 0xA3 | // Ctrl
                0x12 | 0xA4 | 0xA5 | // Alt / Menu
                0x5B | 0x5C          // Win
            );

            if !is_modifier_key {
                let is_alt = ((GetAsyncKeyState(VK_MENU as i32) as u16 & 0x8000) != 0)
                    || ((kbd.flags & 0x20) != 0);
                let is_ctrl = (GetAsyncKeyState(VK_CONTROL as i32) as u16 & 0x8000) != 0;
                let is_shift = (GetAsyncKeyState(VK_SHIFT as i32) as u16 & 0x8000) != 0;
                let is_win = ((GetAsyncKeyState(VK_LWIN as i32) as u16 & 0x8000) != 0)
                    || ((GetAsyncKeyState(VK_RWIN as i32) as u16 & 0x8000) != 0);

                let mut matched_target: Option<String> = None;
                let mut pm_opt = None;
                let mut jiggler_opt = None;

                if let Ok(mut guard) = HOTKEY_STATE.lock() {
                    if let Some(state) = guard.as_mut() {
                        // Retain keys pressed in the last 1200ms
                        state
                            .recent_keys
                            .retain(|k| k.time.elapsed() < Duration::from_millis(1200));

                        state.recent_keys.push(KeyRecord {
                            vk: raw_vk,
                            time: Instant::now(),
                            alt: is_alt,
                            ctrl: is_ctrl,
                            shift: is_shift,
                            win: is_win,
                        });

                        for sc in &state.shortcuts {
                            let n = sc.keys.len();
                            if state.recent_keys.len() < n {
                                continue;
                            }

                            let tail = &state.recent_keys[state.recent_keys.len() - n..];
                            let keys_match = tail
                                .iter()
                                .zip(sc.keys.iter())
                                .all(|(rec, &expected_vk)| rec.vk == expected_vk);

                            if !keys_match {
                                continue;
                            }

                            let modifiers_match = if n == 1 {
                                tail[0].ctrl == sc.ctrl
                                    && tail[0].alt == sc.alt
                                    && tail[0].shift == sc.shift
                                    && tail[0].win == sc.win
                            } else {
                                let had_ctrl = tail.iter().any(|k| k.ctrl);
                                let had_alt = tail.iter().any(|k| k.alt);
                                let had_shift = tail.iter().any(|k| k.shift);
                                let had_win = tail.iter().any(|k| k.win);

                                (sc.ctrl == had_ctrl || (!sc.ctrl && !is_ctrl))
                                    && (sc.alt == had_alt || (!sc.alt && !is_alt))
                                    && (sc.shift == had_shift || (!sc.shift && !is_shift))
                                    && (sc.win == had_win || (!sc.win && !is_win))
                            };

                            if modifiers_match {
                                let now = Instant::now();
                                let should_trigger = match state.last_trigger.get(&sc.target) {
                                    Some(&prev) => {
                                        now.duration_since(prev) > Duration::from_millis(400)
                                    }
                                    None => true,
                                };

                                if should_trigger {
                                    state.last_trigger.insert(sc.target.clone(), now);
                                    matched_target = Some(sc.target.clone());
                                    pm_opt = Some(state.pm.clone());
                                    jiggler_opt = Some(state.jiggler.clone());
                                }
                                state.recent_keys.clear();
                                break;
                            }
                        }
                    }
                }

                if let Some(target) = matched_target {
                    if let (Some(pm), Some(jiggler)) = (pm_opt, jiggler_opt) {
                        thread::spawn(move || {
                            if target == "__mouse_jiggler__" {
                                jiggler.toggle();
                            } else {
                                pm.toggle_service(&target);
                            }
                        });
                    }
                }
            }
        }

        CallNextHookEx(std::ptr::null_mut(), code, wparam, lparam)
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

            // Initialize global state
            let shortcuts = load_shortcuts(&pm, &jiggler);
            {
                let mut guard = HOTKEY_STATE.lock().unwrap();
                *guard = Some(HotkeyState {
                    pm: pm.clone(),
                    jiggler: jiggler.clone(),
                    shortcuts,
                    recent_keys: Vec::new(),
                    last_trigger: HashMap::new(),
                });
            }

            // Install WH_KEYBOARD_LL hook
            let hook = unsafe {
                SetWindowsHookExW(
                    WH_KEYBOARD_LL,
                    Some(low_level_keyboard_proc),
                    0 as HINSTANCE,
                    0,
                )
            };

            if hook.is_null() {
                eprintln!("[Hotkeys] Failed to install WH_KEYBOARD_LL hook");
                return;
            }

            // Win32 message loop
            unsafe {
                let mut msg: MSG = std::mem::zeroed();
                while GetMessageW(&mut msg, 0 as HWND, 0, 0) > 0 {
                    match msg.message {
                        WM_RELOAD_HOTKEYS => {
                            let updated = load_shortcuts(&pm, &jiggler);
                            if let Ok(mut guard) = HOTKEY_STATE.lock() {
                                if let Some(state) = guard.as_mut() {
                                    state.shortcuts = updated;
                                    state.recent_keys.clear();
                                }
                            }
                        }
                        WM_STOP_HOTKEYS => {
                            break;
                        }
                        _ => {}
                    }
                }

                UnhookWindowsHookEx(hook);
                if let Ok(mut guard) = HOTKEY_STATE.lock() {
                    *guard = None;
                }
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
