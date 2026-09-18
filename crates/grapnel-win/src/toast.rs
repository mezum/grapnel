//! A short message in the bottom-left corner of the foreground monitor (like Emacs' echo area).
//! It never takes focus, lets clicks through and hides itself after a while.

use crate::{wide, window};
use std::cell::{Cell, RefCell};
use windows::Win32::Foundation::{COLORREF, HWND, LPARAM, LRESULT, RECT, WPARAM};
use windows::Win32::Graphics::Gdi::*;
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::WindowsAndMessaging::*;
use windows::core::{PCWSTR, w};

const CLASS: PCWSTR = w!("grapnel_toast");
const PAD: i32 = 8;
const MARGIN: i32 = 12;
const HIDE_TIMER: usize = 1;

thread_local! {
    static HWND_: Cell<isize> = const { Cell::new(0) };
    static FONT: Cell<isize> = const { Cell::new(0) };
    static TEXT: RefCell<Vec<u16>> = const { RefCell::new(Vec::new()) };
}

/// The system message font, 1.5 times larger; created once and kept for the process.
fn font() -> HGDIOBJ {
    let mut font = FONT.with(Cell::get);
    if font == 0 {
        let mut m = NONCLIENTMETRICSW { cbSize: size_of::<NONCLIENTMETRICSW>() as u32, ..Default::default() };
        let p = Some(&mut m as *mut _ as *mut _);
        let _ = unsafe { SystemParametersInfoW(SPI_GETNONCLIENTMETRICS, m.cbSize, p, Default::default()) };
        m.lfMessageFont.lfHeight = m.lfMessageFont.lfHeight * 3 / 2;
        font = unsafe { CreateFontIndirectW(&m.lfMessageFont) }.0 as isize;
        FONT.with(|f| f.set(font));
    }
    HGDIOBJ(font as _)
}

fn text_rect(hdc: HDC, text: &mut [u16]) -> RECT {
    let mut rc = RECT::default();
    unsafe { DrawTextW(hdc, text, &mut rc, DT_CALCRECT | DT_SINGLELINE | DT_NOPREFIX) };
    rc
}

unsafe extern "system" fn proc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    unsafe {
        match msg {
            WM_PAINT => {
                let mut ps = PAINTSTRUCT::default();
                let hdc = BeginPaint(hwnd, &mut ps);
                let mut rc = RECT::default();
                let _ = GetClientRect(hwnd, &mut rc);
                let brush = CreateSolidBrush(COLORREF(0x00302C28)); // BGR
                FillRect(hdc, &rc, brush);
                let _ = DeleteObject(brush.into());
                SelectObject(hdc, font());
                SetBkMode(hdc, TRANSPARENT);
                SetTextColor(hdc, COLORREF(0x00F0F0F0));
                rc.left += PAD;
                TEXT.with(|t| DrawTextW(hdc, &mut t.borrow_mut(), &mut rc, DT_SINGLELINE | DT_VCENTER | DT_NOPREFIX));
                let _ = EndPaint(hwnd, &ps);
                LRESULT(0)
            }
            WM_TIMER if wp.0 == HIDE_TIMER => {
                let _ = KillTimer(Some(hwnd), HIDE_TIMER);
                let _ = ShowWindow(hwnd, SW_HIDE);
                LRESULT(0)
            }
            WM_NCHITTEST => LRESULT(HTTRANSPARENT as isize),
            _ => DefWindowProcW(hwnd, msg, wp, lp),
        }
    }
}

fn window() -> windows::core::Result<HWND> {
    let existing = HWND_.with(Cell::get);
    if existing != 0 {
        return Ok(HWND(existing as _));
    }
    unsafe {
        let instance = GetModuleHandleW(None)?.into();
        let wc = WNDCLASSW { lpfnWndProc: Some(proc), hInstance: instance, lpszClassName: CLASS, ..Default::default() };
        RegisterClassW(&wc);
        let ex = WS_EX_TOPMOST | WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE | WS_EX_TRANSPARENT;
        let hwnd = CreateWindowExW(ex, CLASS, None, WS_POPUP, 0, 0, 0, 0, None, None, Some(instance), None)?;
        HWND_.with(|h| h.set(hwnd.0 as isize));
        Ok(hwnd)
    }
}

/// Shows `text` for `ms` milliseconds, replacing any message still shown.
pub fn show(text: &str, ms: u32) -> windows::core::Result<()> {
    let hwnd = window()?;
    let mut buf = wide(text);
    buf.pop(); // DrawTextW takes the length from the slice
    unsafe {
        let hdc = GetDC(Some(hwnd));
        SelectObject(hdc, font());
        let rc = text_rect(hdc, &mut buf);
        ReleaseDC(Some(hwnd), hdc);
        TEXT.with(|t| *t.borrow_mut() = buf);
        let (w, h) = (rc.right - rc.left + 2 * PAD, rc.bottom - rc.top + PAD);
        let area = window::foreground_work_area();
        let (x, y) = (area.left + MARGIN, area.bottom - h - MARGIN);
        let flags = SWP_NOACTIVATE | SWP_SHOWWINDOW;
        SetWindowPos(hwnd, Some(HWND_TOPMOST), x, y, w, h, flags)?;
        let _ = InvalidateRect(Some(hwnd), None, true);
        SetTimer(Some(hwnd), HIDE_TIMER, ms, None);
    }
    Ok(())
}
