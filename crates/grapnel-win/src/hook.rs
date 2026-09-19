//! Low-level keyboard and mouse hooks. The installing thread must pump messages.

use crate::send::INJECT_TAG;
use grapnel_engine::Event;
use grapnel_keys::{Key, MouseButton, WheelDir};
use std::cell::{Cell, RefCell};
use windows::Win32::Foundation::{LPARAM, LRESULT, POINT, WPARAM};
use windows::Win32::UI::WindowsAndMessaging::*;

type Handler = Box<dyn FnMut(Event) -> bool>;

thread_local! {
    static HANDLER: RefCell<Option<Handler>> = RefCell::new(None);
    static SCANCODES: RefCell<Vec<u16>> = const { RefCell::new(Vec::new()) };
    static LAST_PT: Cell<Option<(i32, i32)>> = const { Cell::new(None) };
}

/// Sets the per-thread callback. It returns `true` to swallow the event and must not pump messages.
pub fn set_handler(f: impl FnMut(Event) -> bool + 'static) {
    HANDLER.with(|h| *h.borrow_mut() = Some(Box::new(f)));
}

/// Keys with these scan codes are reported as `Key::Sc` instead of `Key::Vk`.
pub fn set_scancodes(codes: Vec<u16>) {
    SCANCODES.with(|s| *s.borrow_mut() = codes);
}

/// Installed hooks; dropping removes them.
pub struct Hooks(HHOOK, HHOOK);

impl Drop for Hooks {
    fn drop(&mut self) {
        unsafe {
            let _ = UnhookWindowsHookEx(self.0);
            let _ = UnhookWindowsHookEx(self.1);
        }
    }
}

pub fn install() -> windows::core::Result<Hooks> {
    LAST_PT.with(|p| p.set(None));
    unsafe {
        let kb = SetWindowsHookExW(WH_KEYBOARD_LL, Some(kb_proc), None, 0)?;
        match SetWindowsHookExW(WH_MOUSE_LL, Some(mouse_proc), None, 0) {
            Ok(ms) => Ok(Hooks(kb, ms)),
            Err(e) => {
                let _ = UnhookWindowsHookEx(kb);
                Err(e)
            }
        }
    }
}

/// Re-entrant calls (e.g. while the handler is running) pass through.
fn dispatch(ev: Event) -> bool {
    HANDLER.with(|h| h.try_borrow_mut().ok().and_then(|mut h| h.as_mut().map(|f| f(ev))).unwrap_or(false))
}

unsafe extern "system" fn kb_proc(code: i32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    if code == HC_ACTION as i32 {
        let k = unsafe { &*(lp.0 as *const KBDLLHOOKSTRUCT) };
        if k.dwExtraInfo != INJECT_TAG {
            let ext = if k.flags.contains(LLKHF_EXTENDED) { 0xE000 } else { 0 };
            let sc = k.scanCode as u16 | ext;
            let key = match SCANCODES.with(|s| s.borrow().contains(&sc)) {
                true => Key::Sc(sc),
                false => Key::Vk(k.vkCode as u8),
            };
            let ev = if k.flags.contains(LLKHF_UP) { Event::Up(key) } else { Event::Down(key) };
            if dispatch(ev) {
                return LRESULT(1);
            }
        }
    }
    unsafe { CallNextHookEx(None, code, wp, lp) }
}

/// Maps a mouse hook message to an event. `data` is `MSLLHOOKSTRUCT::mouseData`.
fn mouse_event(msg: u32, data: u32, pt: (i32, i32), last: Option<(i32, i32)>) -> Option<Event> {
    let hi = (data >> 16) as u16;
    let x = if hi == 2 { MouseButton::X2 } else { MouseButton::X1 };
    let button = |b| Some(Key::Mouse(b));
    let (key, down) = match msg {
        WM_LBUTTONDOWN => (button(MouseButton::Left), true),
        WM_LBUTTONUP => (button(MouseButton::Left), false),
        WM_RBUTTONDOWN => (button(MouseButton::Right), true),
        WM_RBUTTONUP => (button(MouseButton::Right), false),
        WM_MBUTTONDOWN => (button(MouseButton::Middle), true),
        WM_MBUTTONUP => (button(MouseButton::Middle), false),
        WM_XBUTTONDOWN => (button(x), true),
        WM_XBUTTONUP => (button(x), false),
        WM_MOUSEWHEEL if hi as i16 > 0 => (Some(Key::Wheel(WheelDir::Up)), true),
        WM_MOUSEWHEEL => (Some(Key::Wheel(WheelDir::Down)), true),
        WM_MOUSEHWHEEL if hi as i16 > 0 => (Some(Key::Wheel(WheelDir::Right)), true),
        WM_MOUSEHWHEEL => (Some(Key::Wheel(WheelDir::Left)), true),
        WM_MOUSEMOVE => {
            let (lx, ly) = last?;
            return Some(Event::MouseMove { dx: pt.0 - lx, dy: pt.1 - ly });
        }
        _ => (None, false),
    };
    key.map(|k| if down { Event::Down(k) } else { Event::Up(k) })
}

unsafe extern "system" fn mouse_proc(code: i32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    if code == HC_ACTION as i32 {
        let m = unsafe { &*(lp.0 as *const MSLLHOOKSTRUCT) };
        let POINT { x, y } = m.pt;
        let msg = wp.0 as u32;
        let last = if msg == WM_MOUSEMOVE { LAST_PT.with(|p| p.replace(Some((x, y)))) } else { None };
        if m.dwExtraInfo != INJECT_TAG && mouse_event(msg, m.mouseData, (x, y), last).is_some_and(dispatch) {
            return LRESULT(1);
        }
    }
    unsafe { CallNextHookEx(None, code, wp, lp) }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mouse_messages() {
        let k = |s| grapnel_keys::parse_key(s, Default::default()).unwrap();
        assert_eq!(mouse_event(WM_RBUTTONDOWN, 0, (0, 0), None), Some(Event::Down(k("RButton"))));
        assert_eq!(mouse_event(WM_XBUTTONUP, 2 << 16, (0, 0), None), Some(Event::Up(k("X2Button"))));
        assert_eq!(
            mouse_event(WM_MOUSEWHEEL, (-120i16 as u16 as u32) << 16, (0, 0), None),
            Some(Event::Down(k("WheelDown")))
        );
        assert_eq!(mouse_event(WM_MOUSEHWHEEL, 120 << 16, (0, 0), None), Some(Event::Down(k("WheelRight"))));
        assert_eq!(mouse_event(WM_MOUSEMOVE, 0, (5, 7), Some((1, 10))), Some(Event::MouseMove { dx: 4, dy: -3 }));
        assert_eq!(mouse_event(WM_MOUSEMOVE, 0, (5, 7), None), None);
    }
}
