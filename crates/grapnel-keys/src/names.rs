//! Key name tables. Symbol names follow the JIS layout.

use crate::{Key, MouseButton, PadButton, WheelDir};

/// Named keyboard keys. Scan-code based entries are keys whose VK varies with IME state.
const NAMED: &[(&str, Key)] = &[
    ("Enter", Key::Vk(0x0D)),
    ("Esc", Key::Vk(0x1B)),
    ("Tab", Key::Vk(0x09)),
    ("Space", Key::Vk(0x20)),
    ("Backspace", Key::Vk(0x08)),
    ("Delete", Key::Vk(0x2E)),
    ("Insert", Key::Vk(0x2D)),
    ("Home", Key::Vk(0x24)),
    ("End", Key::Vk(0x23)),
    ("PageUp", Key::Vk(0x21)),
    ("PageDown", Key::Vk(0x22)),
    ("Up", Key::Vk(0x26)),
    ("Down", Key::Vk(0x28)),
    ("Left", Key::Vk(0x25)),
    ("Right", Key::Vk(0x27)),
    ("CapsLock", Key::Vk(0x14)),
    ("PrintScreen", Key::Vk(0x2C)),
    ("ScrollLock", Key::Vk(0x91)),
    ("Pause", Key::Vk(0x13)),
    ("Apps", Key::Vk(0x5D)),
    ("NumMul", Key::Vk(0x6A)),
    ("NumAdd", Key::Vk(0x6B)),
    ("NumSub", Key::Vk(0x6D)),
    ("NumDot", Key::Vk(0x6E)),
    ("NumDiv", Key::Vk(0x6F)),
    ("NumLock", Key::Vk(0x90)),
    ("Muhenkan", Key::Vk(0x1D)),
    ("Henkan", Key::Vk(0x1C)),
    ("Kana", Key::Sc(0x70)),
    ("Zenkaku", Key::Sc(0x29)),
    ("Eisu", Key::Sc(0x3A)),
    ("LCtrl", Key::Vk(0xA2)),
    ("RCtrl", Key::Vk(0xA3)),
    ("LAlt", Key::Vk(0xA4)),
    ("RAlt", Key::Vk(0xA5)),
    ("LShift", Key::Vk(0xA0)),
    ("RShift", Key::Vk(0xA1)),
    ("LWin", Key::Vk(0x5B)),
    ("RWin", Key::Vk(0x5C)),
    ("-", Key::Vk(0xBD)),
    ("^", Key::Vk(0xDE)),
    ("\\", Key::Vk(0xDC)),
    ("@", Key::Vk(0xC0)),
    ("[", Key::Vk(0xDB)),
    (";", Key::Vk(0xBB)),
    (":", Key::Vk(0xBA)),
    ("]", Key::Vk(0xDD)),
    (",", Key::Vk(0xBC)),
    (".", Key::Vk(0xBE)),
    ("/", Key::Vk(0xBF)),
    ("_", Key::Vk(0xE2)),
    ("Oem1", Key::Vk(0xBA)),
    ("OemPlus", Key::Vk(0xBB)),
    ("OemComma", Key::Vk(0xBC)),
    ("OemMinus", Key::Vk(0xBD)),
    ("OemPeriod", Key::Vk(0xBE)),
    ("Oem2", Key::Vk(0xBF)),
    ("Oem3", Key::Vk(0xC0)),
    ("Oem4", Key::Vk(0xDB)),
    ("Oem5", Key::Vk(0xDC)),
    ("Oem6", Key::Vk(0xDD)),
    ("Oem7", Key::Vk(0xDE)),
    ("Oem8", Key::Vk(0xDF)),
    ("Oem102", Key::Vk(0xE2)),
    ("WheelUp", Key::Wheel(WheelDir::Up)),
    ("WheelDown", Key::Wheel(WheelDir::Down)),
    ("WheelLeft", Key::Wheel(WheelDir::Left)),
    ("WheelRight", Key::Wheel(WheelDir::Right)),
];

pub(crate) const MOUSE: &[(&str, MouseButton)] = &[
    ("LButton", MouseButton::Left),
    ("RButton", MouseButton::Right),
    ("MButton", MouseButton::Middle),
    ("X1Button", MouseButton::X1),
    ("X2Button", MouseButton::X2),
];

const PAD: &[(&str, PadButton)] = &[
    ("A", PadButton::A),
    ("B", PadButton::B),
    ("X", PadButton::X),
    ("Y", PadButton::Y),
    ("LB", PadButton::LB),
    ("RB", PadButton::RB),
    ("LT", PadButton::LT),
    ("RT", PadButton::RT),
    ("Back", PadButton::Back),
    ("Start", PadButton::Start),
    ("LS", PadButton::LS),
    ("RS", PadButton::RS),
    ("Up", PadButton::Up),
    ("Down", PadButton::Down),
    ("Left", PadButton::Left),
    ("Right", PadButton::Right),
    ("LStickUp", PadButton::LStickUp),
    ("LStickDown", PadButton::LStickDown),
    ("LStickLeft", PadButton::LStickLeft),
    ("LStickRight", PadButton::LStickRight),
    ("RStickUp", PadButton::RStickUp),
    ("RStickDown", PadButton::RStickDown),
    ("RStickLeft", PadButton::RStickLeft),
    ("RStickRight", PadButton::RStickRight),
];

fn hex(s: &str) -> Option<u32> {
    u32::from_str_radix(s.strip_prefix("0x").or(s.strip_prefix("0X"))?, 16).ok()
}

/// Looks up a single key name (without gesture suffix). Case-insensitive.
pub(crate) fn lookup(name: &str) -> Option<Key> {
    let lower = name.to_ascii_lowercase();
    if let Some(v) = lower.strip_prefix("vk:") {
        return hex(v).filter(|&v| (1..=0xFE).contains(&v)).map(|v| Key::Vk(v as u8));
    }
    if let Some(v) = lower.strip_prefix("sc:") {
        return hex(v).filter(|&v| v > 0 && v <= 0xFFFF).map(|v| Key::Sc(v as u16));
    }
    if let Some(p) = lower.strip_prefix("pad.") {
        return PAD.iter().find(|(n, _)| n.eq_ignore_ascii_case(p)).map(|&(_, b)| Key::Pad(b));
    }
    if let [c] = lower.as_bytes()
        && c.is_ascii_alphanumeric()
    {
        return Some(Key::Vk(c.to_ascii_uppercase()));
    }
    if let Some(n) = lower.strip_prefix('f').and_then(|n| n.parse::<u8>().ok())
        && (1..=24).contains(&n)
        && !lower.starts_with("f0")
    {
        return Some(Key::Vk(0x6F + n));
    }
    if let Some(n) = lower.strip_prefix("num").and_then(|n| n.parse::<u8>().ok())
        && n <= 9
        && lower.len() == 4
    {
        return Some(Key::Vk(0x60 + n));
    }
    if let Some(&(_, b)) = MOUSE.iter().find(|(n, _)| n.eq_ignore_ascii_case(&lower)) {
        return Some(Key::Mouse(b));
    }
    NAMED.iter().find(|(n, _)| n.eq_ignore_ascii_case(&lower)).map(|(_, k)| k.clone())
}

/// Canonical name of a key.
pub(crate) fn name(key: &Key) -> String {
    match key {
        Key::Vk(v @ (b'0'..=b'9' | b'A'..=b'Z')) => (*v as char).to_ascii_lowercase().to_string(),
        Key::Vk(v @ 0x70..=0x87) => format!("F{}", v - 0x6F),
        Key::Vk(v @ 0x60..=0x69) => format!("Num{}", v - 0x60),
        Key::Mouse(b) => MOUSE.iter().find(|(_, m)| m == b).unwrap().0.to_string(),
        Key::Pad(b) => format!("Pad.{}", PAD.iter().find(|(_, p)| p == b).unwrap().0),
        Key::Gesture(b, dirs) => {
            let d: String = dirs.iter().map(|d| d.letter()).collect();
            format!("{}:{d}", name(&Key::Mouse(*b)))
        }
        k => match NAMED.iter().find(|(_, n)| n == k) {
            Some((n, _)) => n.to_string(),
            None => match k {
                Key::Vk(v) => format!("vk:0x{v:02X}"),
                Key::Sc(s) => format!("sc:0x{s:02X}"),
                _ => unreachable!(),
            },
        },
    }
}
