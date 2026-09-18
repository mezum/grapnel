use super::*;

fn seq(s: &str) -> KeySeq {
    parse_seq(s, &["Mu"]).unwrap()
}

#[test]
fn emacs_style_sequence() {
    let s = seq("C-x t 0");
    assert_eq!(s.0.len(), 3);
    assert_eq!(s.0[0], Chord { mods: Mods::CTRL, key: Key::Vk(b'X') });
    assert_eq!(s.0[1], Chord { mods: Mods::NONE, key: Key::Vk(b'T') });
    assert_eq!(s.0[2], Chord { mods: Mods::NONE, key: Key::Vk(b'0') });
}

#[test]
fn modifiers_combine_and_minus_key() {
    assert_eq!(seq("C-M-S-W-a").0[0].mods, Mods::REAL);
    assert_eq!(seq("C--").0[0], Chord { mods: Mods::CTRL, key: Key::Vk(0xBD) });
    assert_eq!(seq("-").0[0].key, Key::Vk(0xBD));
}

#[test]
fn user_modifier() {
    assert_eq!(seq("Mu-j").0[0], Chord { mods: Mods::user(0), key: Key::Vk(b'J') });
    assert!(parse_seq("Xx-j", &["Mu"]).is_err());
}

#[test]
fn names_are_case_insensitive() {
    assert_eq!(seq("ENTER"), seq("enter"));
    assert_eq!(seq("muhenkan").0[0].key, Key::Vk(0x1D));
    assert_eq!(seq("f12").0[0].key, Key::Vk(0x7B));
    assert_eq!(seq("Num7").0[0].key, Key::Vk(0x67));
}

#[test]
fn raw_codes() {
    assert_eq!(seq("vk:0x41").0[0].key, Key::Vk(0x41));
    assert_eq!(seq("sc:0xE01D").0[0].key, Key::Sc(0xE01D));
    assert!(parse_key("vk:0x100").is_err());
    assert!(parse_key("sc:zz").is_err());
}

#[test]
fn mouse_pad_and_gesture() {
    assert_eq!(seq("Mu-LButton").0[0].key, Key::Mouse(MouseButton::Left));
    assert_eq!(seq("WheelUp").0[0].key, Key::Wheel(WheelDir::Up));
    assert_eq!(seq("Pad.lb").0[0].key, Key::Pad(PadButton::LB));
    assert_eq!(seq("RButton:UL").0[0].key, Key::Gesture(MouseButton::Right, vec![Dir::U, Dir::L]));
    assert!(parse_key("RButton:").is_err());
    assert!(parse_key("RButton:X").is_err());
    assert_eq!(seq(":").0[0].key, Key::Vk(0xBA));
}

#[test]
fn unknown_and_empty() {
    assert!(parse_key("Foo").is_err());
    assert!(parse_key("F25").is_err());
    assert_eq!(seq(""), KeySeq::default());
}

#[test]
fn format_round_trips() {
    for s in [
        "C-x t 0",
        "C-M-S-W-a",
        "Mu-j",
        "C--",
        "F12 Num7 Enter",
        "RButton:UL",
        "Pad.LB",
        "vk:0xE8",
        "sc:0x7B",
        "Zenkaku",
        "WheelDown",
    ] {
        assert_eq!(format_seq(&seq(s), &["Mu"]), s);
    }
}

#[test]
fn real_mod_detection() {
    assert_eq!(parse_key("LCtrl").unwrap().real_mod(), Some(Mods::CTRL));
    assert_eq!(parse_key("RWin").unwrap().real_mod(), Some(Mods::WIN));
    assert_eq!(parse_key("a").unwrap().real_mod(), None);
}
