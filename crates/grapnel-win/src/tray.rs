//! Notification-area icon, context menu and balloons.

use crate::wide;
use windows::Win32::Foundation::{HWND, LPARAM, POINT, WPARAM};
use windows::Win32::UI::Shell::*;
use windows::Win32::UI::WindowsAndMessaging::*;

const ID: u32 = 1;

pub struct Tray {
    hwnd: HWND,
}

fn copy(dst: &mut [u16], s: &str) {
    let src = wide(s);
    let n = src.len().min(dst.len()) - 1;
    dst[..n].copy_from_slice(&src[..n]);
    dst[n] = 0;
}

impl Tray {
    fn data(&self) -> NOTIFYICONDATAW {
        NOTIFYICONDATAW { cbSize: size_of::<NOTIFYICONDATAW>() as u32, hWnd: self.hwnd, uID: ID, ..Default::default() }
    }

    /// Adds the icon. Clicks arrive at `hwnd` as `msg` with the mouse message in `lParam`.
    pub fn add(hwnd: HWND, msg: u32, tip: &str) -> windows::core::Result<Tray> {
        let tray = Tray { hwnd };
        let mut d = tray.data();
        d.uFlags = NIF_MESSAGE | NIF_ICON | NIF_TIP;
        d.uCallbackMessage = msg;
        d.hIcon = unsafe { LoadIconW(None, IDI_APPLICATION)? };
        copy(&mut d.szTip, tip);
        unsafe { Shell_NotifyIconW(NIM_ADD, &d).ok()? };
        Ok(tray)
    }

    /// Updates the tooltip; a paused state shows a warning icon.
    pub fn set_state(&self, tip: &str, paused: bool) {
        let mut d = self.data();
        d.uFlags = NIF_ICON | NIF_TIP;
        let icon = if paused { IDI_WARNING } else { IDI_APPLICATION };
        d.hIcon = unsafe { LoadIconW(None, icon).unwrap_or_default() };
        copy(&mut d.szTip, tip);
        let _ = unsafe { Shell_NotifyIconW(NIM_MODIFY, &d) };
    }

    pub fn balloon(&self, title: &str, text: &str, error: bool) {
        let mut d = self.data();
        d.uFlags = NIF_INFO;
        d.dwInfoFlags = if error { NIIF_ERROR } else { NIIF_INFO };
        copy(&mut d.szInfoTitle, title);
        copy(&mut d.szInfo, text);
        let _ = unsafe { Shell_NotifyIconW(NIM_MODIFY, &d) };
    }
}

/// Shows a menu for `hwnd` at the cursor and returns the chosen id. Items are `(id, label, checked)`.
/// Runs a modal loop, so hold no `RefCell` borrows across it.
pub fn menu(hwnd: HWND, items: &[(u32, &str, bool)]) -> Option<u32> {
    unsafe {
        let menu = CreatePopupMenu().ok()?;
        for &(id, label, checked) in items {
            let flags = if checked { MF_STRING | MF_CHECKED } else { MF_STRING };
            let label = wide(label);
            let _ = AppendMenuW(menu, flags, id as usize, windows::core::PCWSTR(label.as_ptr()));
        }
        let mut pt = POINT::default();
        let _ = GetCursorPos(&mut pt);
        let _ = SetForegroundWindow(hwnd);
        let flags = TPM_RETURNCMD | TPM_NONOTIFY | TPM_RIGHTBUTTON;
        let id = TrackPopupMenu(menu, flags, pt.x, pt.y, None, hwnd, None).0 as u32;
        let _ = PostMessageW(Some(hwnd), WM_NULL, WPARAM(0), LPARAM(0));
        let _ = DestroyMenu(menu);
        (id != 0).then_some(id)
    }
}

impl Drop for Tray {
    fn drop(&mut self) {
        let _ = unsafe { Shell_NotifyIconW(NIM_DELETE, &self.data()) };
    }
}
