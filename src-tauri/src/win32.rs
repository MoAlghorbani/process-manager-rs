use std::collections::{HashMap, HashSet, VecDeque};
use std::ffi::OsStr;
use std::os::windows::ffi::OsStrExt;
use std::process::Command;
use std::ptr;
use std::time::Duration;
use windows_sys::Win32::Foundation::{
    CloseHandle, GetLastError, HANDLE, HWND, INVALID_HANDLE_VALUE,
};
use windows_sys::Win32::System::DataExchange::{
    CloseClipboard, GetClipboardData, GetClipboardSequenceNumber, OpenClipboard,
};
use windows_sys::Win32::System::Diagnostics::ToolHelp::{
    CreateToolhelp32Snapshot, Process32FirstW, Process32NextW, PROCESSENTRY32W,
    TH32CS_SNAPPROCESS,
};
use windows_sys::Win32::System::JobObjects::{
    AssignProcessToJobObject, CreateJobObjectW, JobObjectExtendedLimitInformation,
    SetInformationJobObject, TerminateJobObject, JOBOBJECT_EXTENDED_LIMIT_INFORMATION,
    JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
};
use windows_sys::Win32::System::Memory::{GlobalLock, GlobalUnlock};
use windows_sys::Win32::System::ProcessStatus::{GetProcessMemoryInfo, PROCESS_MEMORY_COUNTERS};
use windows_sys::Win32::System::Threading::{
    CreateMutexW, GetCurrentProcessId, OpenProcess,
    TerminateProcess, PROCESS_QUERY_INFORMATION,
    PROCESS_TERMINATE, PROCESS_VM_READ,
};
use windows_sys::Win32::UI::Input::KeyboardAndMouse::{
    keybd_event, mouse_event, GetLastInputInfo, KEYEVENTF_KEYUP,
    LASTINPUTINFO, MOUSEEVENTF_MOVE,
};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    FindWindowW, SetForegroundWindow, ShowWindow, SW_RESTORE,
};

pub const ES_CONTINUOUS: u32 = 0x80000000;
pub const ES_SYSTEM_REQUIRED: u32 = 0x00000001;
pub const ES_DISPLAY_REQUIRED: u32 = 0x00000002;

#[link(name = "kernel32")]
unsafe extern "system" {
    pub fn SetThreadExecutionState(esflags: u32) -> u32;
    pub fn GetTickCount() -> u32;
}

pub const CF_UNICODETEXT: u32 = 13;
pub const VK_F15: u8 = 0x7E;
pub const VK_SHIFT: u8 = 0x10;
pub const VK_CONTROL: u8 = 0x11;

pub fn to_wide_chars(s: &str) -> Vec<u16> {
    OsStr::new(s).encode_wide().chain(Some(0)).collect()
}

// ==========================================
// 1. Windows Job Object (Kernel Auto-Kill)
// ==========================================

pub struct SafeJobObject {
    handle: HANDLE,
}

unsafe impl Send for SafeJobObject {}
unsafe impl Sync for SafeJobObject {}

impl SafeJobObject {
    pub fn create_kill_on_close() -> Option<Self> {
        unsafe {
            let handle = CreateJobObjectW(ptr::null(), ptr::null());
            if handle.is_null() {
                return None;
            }

            let mut info: JOBOBJECT_EXTENDED_LIMIT_INFORMATION = std::mem::zeroed();
            info.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;

            let ok = SetInformationJobObject(
                handle,
                JobObjectExtendedLimitInformation,
                &info as *const _ as *const _,
                std::mem::size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>() as u32,
            );

            if ok == 0 {
                CloseHandle(handle);
                return None;
            }

            Some(Self { handle })
        }
    }

    pub fn assign_pid(&self, pid: u32) -> bool {
        unsafe {
            let proc_handle = OpenProcess(0x0101, 0, pid);
            if proc_handle.is_null() {
                return false;
            }
            let res = AssignProcessToJobObject(self.handle, proc_handle);
            CloseHandle(proc_handle);
            res != 0
        }
    }

    pub fn terminate(&self) {
        unsafe {
            if !self.handle.is_null() {
                TerminateJobObject(self.handle, 1);
            }
        }
    }
}

impl Drop for SafeJobObject {
    fn drop(&mut self) {
        unsafe {
            if !self.handle.is_null() {
                TerminateJobObject(self.handle, 1);
                CloseHandle(self.handle);
            }
        }
    }
}

// ==========================================
// 2. Process Tree & Process Memory
// ==========================================

pub fn get_process_memory_mb(pid: u32) -> f64 {
    if pid == 0 {
        return 0.0;
    }
    unsafe {
        let handle = OpenProcess(PROCESS_QUERY_INFORMATION | PROCESS_VM_READ, 0, pid);
        if handle.is_null() {
            return 0.0;
        }
        let mut counters: PROCESS_MEMORY_COUNTERS = std::mem::zeroed();
        counters.cb = std::mem::size_of::<PROCESS_MEMORY_COUNTERS>() as u32;

        let ok = GetProcessMemoryInfo(
            handle,
            &mut counters,
            std::mem::size_of::<PROCESS_MEMORY_COUNTERS>() as u32,
        );
        CloseHandle(handle);

        if ok != 0 {
            let mb = (counters.WorkingSetSize as f64) / (1024.0 * 1024.0);
            (mb * 10.0).round() / 10.0
        } else {
            0.0
        }
    }
}

pub fn get_child_pids(parent_pid: u32) -> Vec<u32> {
    if parent_pid == 0 {
        return Vec::new();
    }
    unsafe {
        let snapshot = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0);
        if snapshot == INVALID_HANDLE_VALUE {
            return Vec::new();
        }

        let mut entry: PROCESSENTRY32W = std::mem::zeroed();
        entry.dwSize = std::mem::size_of::<PROCESSENTRY32W>() as u32;

        if Process32FirstW(snapshot, &mut entry) == 0 {
            CloseHandle(snapshot);
            return Vec::new();
        }

        let mut tree: HashMap<u32, Vec<u32>> = HashMap::new();
        loop {
            let p = entry.th32ProcessID;
            let pp = entry.th32ParentProcessID;
            tree.entry(pp).or_default().push(p);

            if Process32NextW(snapshot, &mut entry) == 0 {
                break;
            }
        }
        CloseHandle(snapshot);

        let mut all_descendants = Vec::new();
        let mut queue = VecDeque::new();
        let mut visited = HashSet::new();
        queue.push_back(parent_pid);

        while let Some(curr) = queue.pop_front() {
            if let Some(children) = tree.get(&curr) {
                for &child in children {
                    if visited.insert(child) {
                        all_descendants.push(child);
                        queue.push_back(child);
                    }
                }
            }
        }

        all_descendants
    }
}

pub fn terminate_pid(pid: u32) -> bool {
    if pid == 0 {
        return false;
    }
    unsafe {
        let handle = OpenProcess(PROCESS_TERMINATE, 0, pid);
        if !handle.is_null() {
            let ok = TerminateProcess(handle, 1);
            CloseHandle(handle);
            return ok != 0;
        }
    }
    false
}

pub fn kill_process_tree(pid: u32) {
    if pid == 0 {
        return;
    }
    // 1. Terminate children first
    let children = get_child_pids(pid);
    for child in children {
        terminate_pid(child);
    }

    // 2. Terminate target PID
    let killed = terminate_pid(pid);

    // 3. Fallback taskkill only if process might still linger
    if !killed {
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            let mut cmd = Command::new("taskkill");
            cmd.args(["/F", "/T", "/PID", &pid.to_string()]);
            cmd.creation_flags(0x08000000); // CREATE_NO_WINDOW
            let _ = cmd.output();
        }
    }
}

// ==========================================
// 3. Port Scavenger (FreePort)
// ==========================================

pub fn is_protected_process_name(name: &str) -> bool {
    let lower = name.to_lowercase();
    let protected = [
        "system",
        "system idle process",
        "svchost.exe",
        "explorer.exe",
        "csrss.exe",
        "wininit.exe",
        "services.exe",
        "lsass.exe",
        "smss.exe",
        "pm.exe",
        "process_manager_rs.exe",
        "vmmem",
        "vmmemwsx",
    ];
    for p in protected {
        if lower == p || lower.contains("docker") || lower.contains("wsl") {
            return true;
        }
    }
    false
}

pub fn free_port(port: u16) -> usize {
    if port == 0 {
        return 0;
    }
    let output = match Command::new("netstat")
        .args(["-ano", "-p", "tcp"])
        .output()
    {
        Ok(o) => String::from_utf8_lossy(&o.stdout).to_string(),
        Err(_) => return 0,
    };

    let my_pid = unsafe { GetCurrentProcessId() };
    let mut killed = HashSet::new();

    for line in output.lines() {
        let line = line.trim();
        if !line.starts_with("TCP") {
            continue;
        }
        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.len() >= 5 {
            let local_addr = parts[1];
            let pid_str = parts[parts.len() - 1];

            // Extract port
            if let Some(idx) = local_addr.rfind(':') {
                if let Ok(p) = local_addr[idx + 1..].parse::<u16>() {
                    if p == port {
                        if let Ok(pid) = pid_str.parse::<u32>() {
                            if pid > 0 && pid != my_pid && !killed.contains(&pid) {
                                killed.insert(pid);
                                terminate_pid(pid);
                                kill_process_tree(pid);
                            }
                        }
                    }
                }
            }
        }
    }

    killed.len()
}

// ==========================================
// 4. Mouse Jiggler & Anti-Idle
// ==========================================

pub fn send_relative_mouse_move(dx: i32, dy: i32) {
    unsafe {
        mouse_event(MOUSEEVENTF_MOVE, dx, dy, 0, 0);
    }
}

pub fn send_key_tap(vk: u8) {
    unsafe {
        keybd_event(vk, 0, 0, 0);
        std::thread::sleep(Duration::from_millis(20));
        keybd_event(vk, 0, KEYEVENTF_KEYUP, 0);
    }
}

pub fn get_idle_seconds() -> f64 {
    unsafe {
        let mut lii: LASTINPUTINFO = std::mem::zeroed();
        lii.cbSize = std::mem::size_of::<LASTINPUTINFO>() as u32;

        let ret = GetLastInputInfo(&mut lii);
        if ret != 0 {
            let now_ms = GetTickCount();
            let elapsed_ms = now_ms.wrapping_sub(lii.dwTime);
            (elapsed_ms as f64) / 1000.0
        } else {
            0.0
        }
    }
}

pub fn set_keep_awake(enabled: bool) {
    unsafe {
        if enabled {
            SetThreadExecutionState(ES_CONTINUOUS | ES_SYSTEM_REQUIRED | ES_DISPLAY_REQUIRED);
        } else {
            SetThreadExecutionState(ES_CONTINUOUS);
        }
    }
}

// ==========================================
// 5. Windows Clipboard Sequence & Reading
// ==========================================

pub fn get_clipboard_sequence_num() -> u32 {
    unsafe { GetClipboardSequenceNumber() }
}

pub fn read_clipboard_text() -> Option<String> {
    unsafe {
        // Retry up to 3 times in case clipboard is temporarily locked by another app
        let mut opened = false;
        for _ in 0..3 {
            if OpenClipboard(ptr::null_mut()) != 0 {
                opened = true;
                break;
            }
            std::thread::sleep(Duration::from_millis(15));
        }
        if !opened {
            return None;
        }

        let h_data = GetClipboardData(CF_UNICODETEXT);
        if h_data.is_null() {
            CloseClipboard();
            return None;
        }

        let ptr = GlobalLock(h_data) as *const u16;
        if ptr.is_null() {
            CloseClipboard();
            return None;
        }

        let mut len = 0;
        while *ptr.add(len) != 0 && len < (1 << 22) {
            len += 1;
        }

        let slice = std::slice::from_raw_parts(ptr, len);
        let s = String::from_utf16_lossy(slice);

        GlobalUnlock(h_data);
        CloseClipboard();

        if s.is_empty() {
            None
        } else {
            Some(s)
        }
    }
}

// ==========================================
// 6. Admin Elevation & Single Instance
// ==========================================

pub fn is_admin() -> bool {
    let output = Command::new("net")
        .args(["session"])
        .output();
    if let Ok(out) = output {
        out.status.success()
    } else {
        false
    }
}

pub fn request_admin_elevation(args: &[&str]) -> Result<(), String> {
    let current_exe = std::env::current_exe().map_err(|e| e.to_string())?;
    let args_str = args.join(" ");

    let verb = to_wide_chars("runas");
    let file = to_wide_chars(&current_exe.to_string_lossy());
    let params = to_wide_chars(&args_str);

    unsafe {
        use windows_sys::Win32::UI::Shell::ShellExecuteW;
        let ret = ShellExecuteW(
            ptr::null_mut(),
            verb.as_ptr(),
            file.as_ptr(),
            params.as_ptr(),
            ptr::null(),
            1, // SW_SHOWNORMAL
        );
        if ret as isize <= 32 {
            Err(format!("Elevation failed with code {}", ret as isize))
        } else {
            Ok(())
        }
    }
}

pub fn check_single_instance(mutex_name: &str, window_title: &str) -> bool {
    unsafe {
        let name_w = to_wide_chars(mutex_name);
        let h_mutex = CreateMutexW(ptr::null(), 1, name_w.as_ptr());
        if GetLastError() == 183 {
            // ERROR_ALREADY_EXISTS
            let title_w = to_wide_chars(window_title);
            let mut hwnd: HWND = ptr::null_mut();
            for _ in 0..4 {
                hwnd = FindWindowW(ptr::null(), title_w.as_ptr());
                if !hwnd.is_null() {
                    break;
                }
                std::thread::sleep(Duration::from_millis(100));
            }

            if !hwnd.is_null() {
                ShowWindow(hwnd, SW_RESTORE);
                SetForegroundWindow(hwnd);
                return false; // Exit current instance
            } else {
                // Mutex held by orphaned zombie
                let my_pid = GetCurrentProcessId();
                let _ = Command::new("taskkill")
                    .args([
                        "/F",
                        "/IM",
                        "process_manager_rs.exe",
                        "/FI",
                        &format!("PID ne {}", my_pid),
                    ])
                    .output();
                std::thread::sleep(Duration::from_millis(300));
            }
        }
        let _ = h_mutex;
        true
    }
}
