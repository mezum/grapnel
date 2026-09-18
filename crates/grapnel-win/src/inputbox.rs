//! A small top-most text prompt. Enter confirms, Esc cancels.
//! The owning thread's message loop must call [`pretranslate`] for every message.
// ponytail: no cancel on focus loss; add a WM_ACTIVATE handler if the lingering box annoys.

use crate::wide;
use std::cell::RefCell;
use windows::Win32::Foundation::LRESULT;
use windows::Win32::Foundation::{HWND, LPARAM, WPARAM};
use windows::Win32::Graphics::Gdi::{COLOR_WINDOW, DEFAULT_GUI_FONT, GetStockObject, HBRUSH};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::System::Threading::{AttachThreadInput, GetCurrentThreadId};
use windows::Win32::UI::Input::KeyboardAndMouse::{SetFocus, VK_ESCAPE, VK_RETURN};
use windows::Win32::UI::WindowsAndMessaging::*;
use windows::core::{PCWSTR, w};

struct Open {
    frame: HWND,
    edit: HWND,
    token: usize,
}

thread_local! {
    static OPEN: RefCell<Option<Open>> = const { RefCell::new(None) };
}

const CLASS: PCWSTR = w!("grapnel_inputbox");

unsafe extern "system" fn proc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    unsafe { DefWindowProcW(hwnd, msg, wp, lp) }
}

fn register() {
    let wc = WNDCLASSW {
        lpfnWndProc: Some(proc),
        hInstance: unsafe { GetModuleHandleW(None).unwrap_or_default().into() },
        lpszClassName: CLASS,
        hbrBackground: HBRUSH((COLOR_WINDOW.0 + 1) as _),
        ..Default::default()
    };
    unsafe { RegisterClassW(&wc) }; // fails harmlessly if already registered
}

/// Forces our window to the foreground even though another app currently owns it.
fn force_foreground(hwnd: HWND) {
    unsafe {
        let fg = GetWindowThreadProcessId(GetForegroundWindow(), None);
        let me = GetCurrentThreadId();
        let attached = fg != me && AttachThreadInput(fg, me, true).as_bool();
        let _ = SetForegroundWindow(hwnd);
        if attached {
            let _ = AttachThreadInput(fg, me, false);
        }
    }
}

/// Opens a prompt, replacing any open one. `token` is returned with the result.
pub fn open(prompt: &str, token: usize) -> windows::core::Result<()> {
    close();
    register();
    let (w, h) = (480, 64);
    unsafe {
        let x = (GetSystemMetrics(SM_CXSCREEN) - w) / 2;
        let y = GetSystemMetrics(SM_CYSCREEN) / 3;
        let title = wide(prompt);
        let frame = CreateWindowExW(
            WS_EX_TOPMOST | WS_EX_TOOLWINDOW,
            CLASS,
            PCWSTR(title.as_ptr()),
            WS_POPUP | WS_CAPTION | WS_BORDER,
            x,
            y,
            w,
            h,
            None,
            None,
            None,
            None,
        )?;
        let mut rc = Default::default();
        let _ = GetClientRect(frame, &mut rc);
        let style = WINDOW_STYLE(WS_CHILD.0 | WS_VISIBLE.0 | ES_AUTOHSCROLL as u32);
        let edit = CreateWindowExW(
            WS_EX_CLIENTEDGE,
            w!("EDIT"),
            None,
            style,
            4,
            4,
            rc.right - 8,
            rc.bottom - 8,
            Some(frame),
            None,
            None,
            None,
        )?;
        SendMessageW(edit, WM_SETFONT, Some(WPARAM(GetStockObject(DEFAULT_GUI_FONT).0 as usize)), Some(LPARAM(1)));
        let _ = ShowWindow(frame, SW_SHOW);
        force_foreground(frame);
        let _ = SetFocus(Some(edit));
        OPEN.with(|o| *o.borrow_mut() = Some(Open { frame, edit, token }));
    }
    Ok(())
}

pub fn close() {
    if let Some(o) = OPEN.with(|o| o.borrow_mut().take()) {
        let _ = unsafe { DestroyWindow(o.frame) };
    }
}

/// True while the prompt is the foreground window (its keystrokes should not be remapped).
pub fn is_foreground() -> bool {
    OPEN.with(|o| o.borrow().as_ref().is_some_and(|o| unsafe { GetForegroundWindow() } == o.frame))
}

/// Handles Enter/Esc. Returns `(token, Some(text) | None)` when the prompt finishes;
/// the message should then not be dispatched.
pub fn pretranslate(msg: &MSG) -> Option<(usize, Option<String>)> {
    let (edit, token) = OPEN.with(|o| o.borrow().as_ref().map(|o| (o.edit, o.token)))?;
    let key = |vk: u16| msg.hwnd == edit && msg.message == WM_KEYDOWN && msg.wParam.0 == vk as usize;
    let result = if key(VK_RETURN.0) {
        let mut buf = vec![0u16; 4096];
        let n = unsafe { GetWindowTextW(edit, &mut buf) };
        Some(String::from_utf16_lossy(&buf[..n.max(0) as usize]))
    } else if key(VK_ESCAPE.0) {
        None
    } else {
        return None;
    };
    close();
    Some((token, result))
}
