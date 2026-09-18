//! Command → `SendInput`. Planning is pure so it can be tested without injecting anything.

use grapnel_engine::Command;
use grapnel_keys::{Key, MouseButton, WheelDir};
use windows::Win32::UI::Input::KeyboardAndMouse::*;
use windows::Win32::UI::WindowsAndMessaging::SetCursorPos;

/// `dwExtraInfo` of every event we inject, so the hook can ignore it.
pub const INJECT_TAG: usize = 0x4752_4150;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Raw {
    Key { vk: u16, scan: u16, flags: KEYBD_EVENT_FLAGS },
    Mouse { flags: MOUSE_EVENT_FLAGS, data: i32, dx: i32, dy: i32 },
    CursorTo { x: i32, y: i32 },
}

const WHEEL_DELTA: i32 = 120;

fn mouse(flags: MOUSE_EVENT_FLAGS, data: i32) -> Raw {
    Raw::Mouse { flags, data, dx: 0, dy: 0 }
}

/// `scan_of(vk)` returns the scan code, `0xE0xx` for extended keys.
pub fn plan(cmds: &[Command], scan_of: &dyn Fn(u8) -> u16) -> Vec<Raw> {
    let mut out = Vec::new();
    for cmd in cmds {
        match cmd {
            Command::Key { key, down } => plan_key(key, *down, scan_of, &mut out),
            Command::Text(text) => {
                for c in text.chars().filter(|&c| c != '\r') {
                    if c == '\n' {
                        plan_key(&Key::Vk(0x0D), true, scan_of, &mut out);
                        plan_key(&Key::Vk(0x0D), false, scan_of, &mut out);
                        continue;
                    }
                    let mut buf = [0u16; 2];
                    for &unit in c.encode_utf16(&mut buf).iter() {
                        out.push(Raw::Key { vk: 0, scan: unit, flags: KEYEVENTF_UNICODE });
                        out.push(Raw::Key { vk: 0, scan: unit, flags: KEYEVENTF_UNICODE | KEYEVENTF_KEYUP });
                    }
                }
            }
            Command::MouseMove { x, y, absolute: false } => {
                out.push(Raw::Mouse { flags: MOUSEEVENTF_MOVE, data: 0, dx: *x, dy: *y })
            }
            Command::MouseMove { x, y, absolute: true } => out.push(Raw::CursorTo { x: *x, y: *y }),
            _ => {}
        }
    }
    out
}

fn plan_key(key: &Key, down: bool, scan_of: &dyn Fn(u8) -> u16, out: &mut Vec<Raw>) {
    let up = if down { KEYBD_EVENT_FLAGS(0) } else { KEYEVENTF_KEYUP };
    let ext = |sc: u16| if sc & 0xFF00 == 0xE000 { KEYEVENTF_EXTENDEDKEY } else { KEYBD_EVENT_FLAGS(0) };
    match key {
        Key::Vk(vk) => {
            let sc = scan_of(*vk);
            out.push(Raw::Key { vk: *vk as u16, scan: sc & 0xFF, flags: up | ext(sc) });
        }
        Key::Sc(sc) => out.push(Raw::Key { vk: 0, scan: sc & 0xFF, flags: up | ext(*sc) | KEYEVENTF_SCANCODE }),
        Key::Mouse(b) => {
            let (d, u, data) = match b {
                MouseButton::Left => (MOUSEEVENTF_LEFTDOWN, MOUSEEVENTF_LEFTUP, 0),
                MouseButton::Right => (MOUSEEVENTF_RIGHTDOWN, MOUSEEVENTF_RIGHTUP, 0),
                MouseButton::Middle => (MOUSEEVENTF_MIDDLEDOWN, MOUSEEVENTF_MIDDLEUP, 0),
                MouseButton::X1 => (MOUSEEVENTF_XDOWN, MOUSEEVENTF_XUP, 1),
                MouseButton::X2 => (MOUSEEVENTF_XDOWN, MOUSEEVENTF_XUP, 2),
            };
            out.push(mouse(if down { d } else { u }, data));
        }
        Key::Wheel(dir) if down => out.push(match dir {
            WheelDir::Up => mouse(MOUSEEVENTF_WHEEL, WHEEL_DELTA),
            WheelDir::Down => mouse(MOUSEEVENTF_WHEEL, -WHEEL_DELTA),
            WheelDir::Right => mouse(MOUSEEVENTF_HWHEEL, WHEEL_DELTA),
            WheelDir::Left => mouse(MOUSEEVENTF_HWHEEL, -WHEEL_DELTA),
        }),
        Key::Wheel(_) | Key::Gesture(..) | Key::Pad(_) => {}
    }
}

fn os_scan(vk: u8) -> u16 {
    unsafe { MapVirtualKeyW(vk as u32, MAPVK_VK_TO_VSC_EX) as u16 }
}

/// Injects the input commands in `cmds`; other commands are ignored.
pub fn send(cmds: &[Command]) {
    let mut batch: Vec<INPUT> = Vec::new();
    for raw in plan(cmds, &os_scan) {
        let input = match raw {
            Raw::Key { vk, scan, flags } => INPUT {
                r#type: INPUT_KEYBOARD,
                Anonymous: INPUT_0 {
                    ki: KEYBDINPUT {
                        wVk: VIRTUAL_KEY(vk),
                        wScan: scan,
                        dwFlags: flags,
                        time: 0,
                        dwExtraInfo: INJECT_TAG,
                    },
                },
            },
            Raw::Mouse { flags, data, dx, dy } => INPUT {
                r#type: INPUT_MOUSE,
                Anonymous: INPUT_0 {
                    mi: MOUSEINPUT { dx, dy, mouseData: data as u32, dwFlags: flags, time: 0, dwExtraInfo: INJECT_TAG },
                },
            },
            Raw::CursorTo { x, y } => {
                flush(&mut batch);
                if let Err(e) = unsafe { SetCursorPos(x, y) } {
                    log::warn!("SetCursorPos failed: {e}");
                }
                continue;
            }
        };
        batch.push(input);
    }
    flush(&mut batch);
}

fn flush(batch: &mut Vec<INPUT>) {
    if batch.is_empty() {
        return;
    }
    let sent = unsafe { SendInput(batch, std::mem::size_of::<INPUT>() as i32) };
    if sent as usize != batch.len() {
        log::warn!("SendInput sent {sent}/{} events (blocked by UIPI?)", batch.len());
    }
    batch.clear();
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scan(vk: u8) -> u16 {
        if vk == 0x24 { 0xE047 } else { 0x1E }
    }

    fn key(k: &str, down: bool) -> Command {
        Command::Key { key: grapnel_keys::parse_key(k).unwrap(), down }
    }

    #[test]
    fn keys_scancodes_and_extended() {
        let raw = plan(&[key("a", true), key("Home", false), key("sc:0xE01D", true)], &scan);
        assert_eq!(
            raw,
            [
                Raw::Key { vk: 0x41, scan: 0x1E, flags: KEYBD_EVENT_FLAGS(0) },
                Raw::Key { vk: 0x24, scan: 0x47, flags: KEYEVENTF_KEYUP | KEYEVENTF_EXTENDEDKEY },
                Raw::Key { vk: 0, scan: 0x1D, flags: KEYEVENTF_EXTENDEDKEY | KEYEVENTF_SCANCODE },
            ]
        );
    }

    #[test]
    fn text_uses_unicode_and_surrogates() {
        let raw = plan(&[Command::Text("あ😀\n".into())], &scan);
        let units: Vec<u16> = raw
            .iter()
            .filter_map(|r| match r {
                Raw::Key { scan, flags, .. } if *flags == KEYEVENTF_UNICODE => Some(*scan),
                _ => None,
            })
            .collect();
        assert_eq!(units, [0x3042, 0xD83D, 0xDE00]);
        assert_eq!(raw.len(), 8);
        assert!(matches!(raw[6], Raw::Key { vk: 0x0D, .. }));
    }

    #[test]
    fn mouse_buttons_wheel_and_moves() {
        let cmds = [
            key("X2Button", true),
            key("WheelLeft", true),
            key("WheelUp", false),
            Command::MouseMove { x: 3, y: -4, absolute: false },
            Command::MouseMove { x: 5, y: 6, absolute: true },
        ];
        assert_eq!(
            plan(&cmds, &scan),
            [
                mouse(MOUSEEVENTF_XDOWN, 2),
                mouse(MOUSEEVENTF_HWHEEL, -120),
                Raw::Mouse { flags: MOUSEEVENTF_MOVE, data: 0, dx: 3, dy: -4 },
                Raw::CursorTo { x: 5, y: 6 },
            ]
        );
    }
}
