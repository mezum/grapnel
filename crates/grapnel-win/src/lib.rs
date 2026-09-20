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
use windows::core::{PCWSTR, PWSTR};

/// NUL-terminated UTF-16.
pub fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(Some(0)).collect()
}

/// Whether Windows is in dark mode, checked the documented way: the theme is dark when its
/// foreground (text) colour is light.
/// <https://learn.microsoft.com/windows/apps/desktop/modernize/ui/apply-windows-themes>
pub fn dark_mode() -> bool {
    use windows::UI::ViewManagement::{UIColorType, UISettings};
    let Ok(c) = UISettings::new().and_then(|s| s.GetColorValue(UIColorType::Foreground)) else {
        return false;
    };
    // Perceived brightness, as that page computes it: whole numbers, weighted towards green.
    5 * c.G as u32 + 2 * c.R as u32 + c.B as u32 > 8 * 128
}

/// The user's first Windows display language (e.g. `"ja-JP"`), or `"en"` if unknown.
pub fn ui_language() -> String {
    use windows::Win32::Globalization::{GetUserPreferredUILanguages, MUI_LANGUAGE_NAME};
    let (mut count, mut len) = (0, 256);
    let mut buf = [0u16; 256];
    let ok =
        unsafe { GetUserPreferredUILanguages(MUI_LANGUAGE_NAME, &mut count, Some(PWSTR(buf.as_mut_ptr())), &mut len) };
    let first = buf.split(|&c| c == 0).next().unwrap_or_default();
    if ok.is_ok() && !first.is_empty() { String::from_utf16_lossy(first) } else { "en".into() }
}

pub fn error_box(text: &str) {
    let (text, title) = (wide(text), wide("grapnel"));
    unsafe { MessageBoxW(None, PCWSTR(text.as_ptr()), PCWSTR(title.as_ptr()), MB_OK | MB_ICONERROR) };
}
