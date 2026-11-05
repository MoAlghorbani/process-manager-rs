use crate::jiggler::JigglerEngine;
use crate::manager::ProcessManager;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};
use windows_sys::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM};
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
use windows_sys::Win32::System::Threading::GetCurrentThreadId;
use windows_sys::Win32::UI::Input::KeyboardAndMouse::{
    GetAsyncKeyState, VK_CONTROL, VK_LWIN, VK_MENU, VK_RWIN, VK_SHIFT,
};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    CallNextHookEx, GetMessageW, PeekMessageW, PostThreadMessageW, SetWindowsHookExW,
    UnhookWindowsHookEx, KBDLLHOOKSTRUCT, MSG, WH_KEYBOARD_LL, WM_KEYDOWN, WM_KEYUP,
    WM_SYSKEYDOWN, WM_SYSKEYUP, WM_USER,
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
    ctrl_down: bool,
    alt_down: bool,
    shift_down: bool,
    win_down: bool,
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
        if code >= 0 {
            let kbd = *(lparam as *const KBDLLHOOKSTRUCT);
            let mut raw_vk = kbd.vkCode as u32;

            // Normalize Numpad 0..9 to standard '0'..'9' (0x30..0x39)
            if (0x60..=0x69).contains(&raw_vk) {
                raw_vk = 0x30 + (raw_vk - 0x60);
            }

            let is_down = wparam == WM_KEYDOWN as WPARAM || wparam == WM_SYSKEYDOWN as WPARAM;
            let is_up = wparam == WM_KEYUP as WPARAM || wparam == WM_SYSKEYUP as WPARAM;

            let is_ctrl_key = matches!(raw_vk, 0x11 | 0xA2 | 0xA3);
            let is_alt_key = matches!(raw_vk, 0x12 | 0xA4 | 0xA5);
            let is_shift_key = matches!(raw_vk, 0x10 | 0xA0 | 0xA1);
            let is_win_key = matches!(raw_vk, 0x5B | 0x5C);
            let is_modifier = is_ctrl_key || is_alt_key || is_shift_key || is_win_key;

            if let Ok(mut guard) = HOTKEY_STATE.lock() {
                if let Some(state) = guard.as_mut() {
                    if is_down {
                        if is_ctrl_key {
                            state.ctrl_down = true;
                        }
                        if is_alt_key {
                            state.alt_down = true;
                        }
                        if is_shift_key {
                            state.shift_down = true;
                        }
                        if is_win_key {
                            state.win_down = true;
                        }
                    } else if is_up {
                        if is_ctrl_key {
                            state.ctrl_down = false;
                        }
                        if is_alt_key {
                            state.alt_down = false;
                        }
                        if is_shift_key {
                            state.shift_down = false;
                        }
                        if is_win_key {
                            state.win_down = false;
                        }
                    }

                    if is_down && !is_modifier {
                        let is_alt = state.alt_down
                            || ((GetAsyncKeyState(VK_MENU as i32) as u16 & 0x8000) != 0)
                            || ((kbd.flags & 0x20) != 0);
                        let is_ctrl = state.ctrl_down
                            || ((GetAsyncKeyState(VK_CONTROL as i32) as u16 & 0x8000) != 0);
                        let is_shift = state.shift_down
                            || ((GetAsyncKeyState(VK_SHIFT as i32) as u16 & 0x8000) != 0);
                        let is_win = state.win_down
                            || ((GetAsyncKeyState(VK_LWIN as i32) as u16 & 0x8000) != 0)
                            || ((GetAsyncKeyState(VK_RWIN as i32) as u16 & 0x8000) != 0);

                        // Retain keys pressed within last 1500ms
                        state
                            .recent_keys
                            .retain(|k| k.time.elapsed() < Duration::from_millis(1500));

                        state.recent_keys.push(KeyRecord {
                            vk: raw_vk,
                            time: Instant::now(),
                            alt: is_alt,
                            ctrl: is_ctrl,
                            shift: is_shift,
                            win: is_win,
                        });

                        let mut matched_target: Option<String> = None;
                        for sc in &state.shortcuts {
                            let n = sc.keys.len();
                            if state.recent_keys.len() < n {
                                continue;
                            }

                            let tail = &state.recent_keys[state.recent_keys.len() - n..];
                            let keys_match = tail
                                .iter()
                                .zip(sc.keys.iter())
                                .all(|(rec, &expected)| rec.vk == expected);

                            if !keys_match {
                                continue;
                            }

                            let modifiers_match = if n == 1 {
                                tail[0].ctrl == sc.ctrl
                                    && tail[0].alt == sc.alt
                                    && tail[0].shift == sc.shift
                                    && tail[0].win == sc.win
                            } else {
                                let had_ctrl = tail.iter().any(|k| k.ctrl) || is_ctrl;
                                let had_alt = tail.iter().any(|k| k.alt) || is_alt;
                                let had_shift = tail.iter().any(|k| k.shift) || is_shift;
                                let had_win = tail.iter().any(|k| k.win) || is_win;

                                (sc.ctrl == had_ctrl)
                                    && (sc.alt == had_alt)
                                    && (!sc.shift || had_shift)
                                    && (!sc.win || had_win)
                            };

                            if modifiers_match {
                                let now = Instant::now();
                                let should_trigger = match state.last_trigger.get(&sc.target) {
                                    Some(&prev) => {
                                        now.duration_since(prev) > Duration::from_millis(350)
                                    }
                                    None => true,
                                };

                                if should_trigger {
                                    state.last_trigger.insert(sc.target.clone(), now);
                                    matched_target = Some(sc.target.clone());
                                }
                                state.recent_keys.clear();
                                break;
                            }
                        }

                        if let Some(target) = matched_target {
                            let pm = state.pm.clone();
                            let jiggler = state.jiggler.clone();
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

            // Force Win32 message queue creation for this background thread
            unsafe {
                let mut dummy_msg: MSG = std::mem::zeroed();
                PeekMessageW(&mut dummy_msg, 0 as HWND, 0, 0, 0);
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
                    ctrl_down: false,
                    alt_down: false,
                    shift_down: false,
                    win_down: false,
                });
            }

            // Install WH_KEYBOARD_LL hook using the current executable module handle
            let hmod = unsafe { GetModuleHandleW(std::ptr::null()) };
            let hook = unsafe {
                SetWindowsHookExW(
                    WH_KEYBOARD_LL,
                    Some(low_level_keyboard_proc),
                    hmod,
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
