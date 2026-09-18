//! Foreground window information and change notifications.

use grapnel_config::WindowInfo;
use std::cell::Cell;
use windows::Win32::Foundation::{CloseHandle, HWND, LPARAM, WPARAM};
use windows::Win32::System::Threading::{
    OpenProcess, PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION, QueryFullProcessImageNameW,
};
use windows::Win32::UI::Accessibility::{HWINEVENTHOOK, SetWinEventHook, UnhookWinEvent};
use windows::Win32::UI::WindowsAndMessaging::*;
use windows::core::PWSTR;

/// `wParam` of the notification message.
pub const CHANGED_FOREGROUND: usize = 0;
pub const CHANGED_FOCUS: usize = 1;
pub const CHANGED_TITLE: usize = 2;

thread_local! {
    static NOTIFY: Cell<(isize, u32)> = const { Cell::new((0, 0)) };
}

fn class_name(hwnd: HWND) -> String {
    let mut buf = [0u16; 256];
    let n = unsafe { GetClassNameW(hwnd, &mut buf) };
    String::from_utf16_lossy(&buf[..n.max(0) as usize])
}

fn exe_path(pid: u32) -> String {
    let Ok(h) = (unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid) }) else {
        return String::new();
    };
    let mut buf = [0u16; 1024];
    let mut len = buf.len() as u32;
    let ok = unsafe { QueryFullProcessImageNameW(h, PROCESS_NAME_WIN32, PWSTR(buf.as_mut_ptr()), &mut len) };
    let _ = unsafe { CloseHandle(h) };
    if ok.is_ok() { String::from_utf16_lossy(&buf[..len as usize]) } else { String::new() }
}

/// Reads everything except the UI Automation fields.
pub fn foreground_info() -> WindowInfo {
    let hwnd = unsafe { GetForegroundWindow() };
    if hwnd.is_invalid() {
        return WindowInfo::default();
    }
    let mut title = [0u16; 512];
    let n = unsafe { GetWindowTextW(hwnd, &mut title) };
    let mut pid = 0;
    let tid = unsafe { GetWindowThreadProcessId(hwnd, Some(&mut pid)) };
    let mut gui = GUITHREADINFO { cbSize: size_of::<GUITHREADINFO>() as u32, ..Default::default() };
    let control = match unsafe { GetGUIThreadInfo(tid, &mut gui) } {
        Ok(()) if !gui.hwndFocus.is_invalid() => class_name(gui.hwndFocus),
        _ => String::new(),
    };
    let exe_path = exe_path(pid);
    WindowInfo {
        exe_name: exe_path.rsplit('\\').next().unwrap_or_default().to_string(),
        exe_path,
        title: String::from_utf16_lossy(&title[..n.max(0) as usize]),
        class: class_name(hwnd),
        control,
        ..Default::default()
    }
}

/// Active WinEvent hooks; dropping removes them.
pub struct Watch(Vec<HWINEVENTHOOK>);

impl Drop for Watch {
    fn drop(&mut self) {
        for h in &self.0 {
            let _ = unsafe { UnhookWinEvent(*h) };
        }
    }
}

/// Posts `msg` to `hwnd` with a `CHANGED_*` wParam when the foreground window, focus or title changes.
/// Must be called on a thread that pumps messages.
pub fn watch(hwnd: HWND, msg: u32) -> Watch {
    NOTIFY.with(|n| n.set((hwnd.0 as isize, msg)));
    let events = [EVENT_SYSTEM_FOREGROUND, EVENT_OBJECT_FOCUS, EVENT_OBJECT_NAMECHANGE];
    let hooks = events
        .iter()
        .map(|&e| unsafe { SetWinEventHook(e, e, None, Some(on_event), 0, 0, WINEVENT_OUTOFCONTEXT) })
        .filter(|h| !h.is_invalid())
        .collect();
    Watch(hooks)
}

unsafe extern "system" fn on_event(_: HWINEVENTHOOK, event: u32, hwnd: HWND, obj: i32, _: i32, _: u32, _: u32) {
    let kind = match event {
        EVENT_SYSTEM_FOREGROUND => CHANGED_FOREGROUND,
        EVENT_OBJECT_FOCUS => CHANGED_FOCUS,
        EVENT_OBJECT_NAMECHANGE if obj == OBJID_WINDOW.0 && hwnd == unsafe { GetForegroundWindow() } => CHANGED_TITLE,
        _ => return,
    };
    let (target, msg) = NOTIFY.with(Cell::get);
    let _ = unsafe { PostMessageW(Some(HWND(target as _)), msg, WPARAM(kind), LPARAM(0)) };
}
