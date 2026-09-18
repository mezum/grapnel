//! Win32 backend for grapnel.

pub mod hook;
pub mod inputbox;
pub mod pipe;
pub mod send;
pub mod toast;
pub mod tray;
pub mod uia;
pub mod window;

use windows::Win32::UI::WindowsAndMessaging::{MB_ICONERROR, MB_OK, MessageBoxW};
use windows::core::PCWSTR;

/// NUL-terminated UTF-16.
pub fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(Some(0)).collect()
}

pub fn error_box(text: &str) {
    let (text, title) = (wide(text), wide("grapnel"));
    unsafe { MessageBoxW(None, PCWSTR(text.as_ptr()), PCWSTR(title.as_ptr()), MB_OK | MB_ICONERROR) };
}
