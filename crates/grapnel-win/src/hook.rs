//! Low-level keyboard and mouse hooks. The installing thread must pump messages.

use crate::send::INJECT_TAG;
use grapnel_engine::Event;
use grapnel_keys::{Key, MouseButton, WheelDir};
use std::cell::{Cell, RefCell};
use std::sync::atomic::{AtomicU32, Ordering};
use windows::Win32::Foundation::{LPARAM, LRESULT, POINT, WPARAM};
use windows::Win32::System::SystemInformation::GetTickCount;
use windows::Win32::UI::Input::KeyboardAndMouse::*;
use windows::Win32::UI::WindowsAndMessaging::*;

type Handler = Box<dyn FnMut(Event) -> bool>;

/// Tick count of the last keyboard / mouse hook call, to notice hooks Windows removed.
static LAST_KB: AtomicU32 = AtomicU32::new(0);
static LAST_MOUSE: AtomicU32 = AtomicU32::new(0);
/// Input time of the last probe key-up, to tell the probe from real activity.
static PROBE_INPUT: AtomicU32 = AtomicU32::new(0);

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

fn tick() -> u32 {
    unsafe { GetTickCount() }
}

/// Tick of the last input the system saw, from any device, injected input included.
pub fn last_input() -> u32 {
    let mut info = LASTINPUTINFO { cbSize: size_of::<LASTINPUTINFO>() as u32, dwTime: 0 };
    let _ = unsafe { GetLastInputInfo(&mut info) };
    info.dwTime
}

/// Windows removes a low-level hook without notice when its callback times out. This injects a
/// key-up of the unassigned VK 0xFF and a zero mouse move, which both hooks pass on, and returns
/// the tick to give [`answered`], or `None` if the input was blocked (UIPI).
pub fn probe() -> Option<u32> {
    let at = tick();
    let ki =
        KEYBDINPUT { wVk: VIRTUAL_KEY(0xFF), dwFlags: KEYEVENTF_KEYUP, dwExtraInfo: INJECT_TAG, ..Default::default() };
    // Windows drops a zero relative move, so move to where the cursor already is.
    let mut pt = POINT::default();
    let _ = unsafe { GetCursorPos(&mut pt) };
    let metric = |m| unsafe { GetSystemMetrics(m) } as i64;
    let norm = |p: i32, origin, size: i64| ((p as i64 - origin) * 65536 + size - 1) / size.max(1);
    let mi = MOUSEINPUT {
        dx: norm(pt.x, metric(SM_XVIRTUALSCREEN), metric(SM_CXVIRTUALSCREEN)) as i32,
        dy: norm(pt.y, metric(SM_YVIRTUALSCREEN), metric(SM_CYVIRTUALSCREEN)) as i32,
        dwFlags: MOUSEEVENTF_MOVE | MOUSEEVENTF_ABSOLUTE | MOUSEEVENTF_VIRTUALDESK,
        dwExtraInfo: INJECT_TAG,
        ..Default::default()
    };
    let inputs = [
        // The key-up goes last, so its time (see `probe_input`) is the probe's last input.
        INPUT { r#type: INPUT_MOUSE, Anonymous: INPUT_0 { mi } },
        INPUT { r#type: INPUT_KEYBOARD, Anonymous: INPUT_0 { ki } },
    ];
    (unsafe { SendInput(&inputs, size_of::<INPUT>() as i32) } == 2).then_some(at)
}

/// The [`last_input`] tick of the last answered probe.
pub fn probe_input() -> u32 {
    PROBE_INPUT.load(Ordering::Relaxed)
}

/// Whether both hooks have run since the [`probe`] at tick `at`.
pub fn answered(at: u32) -> bool {
    [&LAST_KB, &LAST_MOUSE].iter().all(|t| since(t.load(Ordering::Relaxed), at))
}

/// Whether tick `t` is at or after tick `at`, across the 49.7-day wrap.
fn since(t: u32, at: u32) -> bool {
    t.wrapping_sub(at) as i32 >= 0
}

/// Re-entrant calls (e.g. while the handler is running) pass through.
fn dispatch(ev: Event) -> bool {
    HANDLER.with(|h| h.try_borrow_mut().ok().and_then(|mut h| h.as_mut().map(|f| f(ev))).unwrap_or(false))
}

unsafe extern "system" fn kb_proc(code: i32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    LAST_KB.store(tick(), Ordering::Relaxed);
    if code == HC_ACTION as i32 {
        let k = unsafe { &*(lp.0 as *const KBDLLHOOKSTRUCT) };
        if k.dwExtraInfo == INJECT_TAG && k.vkCode == 0xFF {
            PROBE_INPUT.store(k.time, Ordering::Relaxed);
        } else if k.dwExtraInfo != INJECT_TAG {
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
    LAST_MOUSE.store(tick(), Ordering::Relaxed);
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
        let k = |s| grapnel_keys::parse_key(s).unwrap();
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

    #[test]
    fn ticks_compare_across_the_wrap() {
        assert!(since(1500, 1000) && since(1000, 1000));
        assert!(!since(900, 1000));
        assert!(since(700, u32::MAX - 500));
    }
}
