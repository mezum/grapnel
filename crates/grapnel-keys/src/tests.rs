use super::*;

fn seq(s: &str) -> KeySeq {
    parse_seq(s, &["Mu"], Layout::Jis).unwrap()
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
    assert!(parse_seq("Xx-j", &["Mu"], Layout::Jis).is_err());
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
    assert!(parse_key("vk:0x100", Layout::Jis).is_err());
    assert!(parse_key("sc:zz", Layout::Jis).is_err());
}

#[test]
fn mouse_pad_and_gesture() {
    assert_eq!(seq("Mu-LButton").0[0].key, Key::Mouse(MouseButton::Left));
    assert_eq!(seq("WheelUp").0[0].key, Key::Wheel(WheelDir::Up));
    assert_eq!(seq("Pad.lb").0[0].key, Key::Pad(PadButton::LB));
    assert_eq!(seq("RButton:UL").0[0].key, Key::Gesture(MouseButton::Right, vec![Dir::U, Dir::L]));
    assert!(parse_key("RButton:", Layout::Jis).is_err());
    assert!(parse_key("RButton:X", Layout::Jis).is_err());
    assert_eq!(seq(":").0[0].key, Key::Vk(0xBA));
}

#[test]
fn unknown_and_empty() {
    assert!(parse_key("Foo", Layout::Jis).is_err());
    assert!(parse_key("F25", Layout::Jis).is_err());
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
        "$ C-? M-_ Ro",
    ] {
        assert_eq!(format_seq(&seq(s), &["Mu"], Layout::Jis), s);
    }
}

#[test]
fn shifted_symbols_are_chords() {
    assert_eq!(seq("$"), seq("S-4"));
    assert_eq!(seq("C-?"), seq("C-S-/"));
    assert_eq!(seq("_").0[0], Chord { mods: Mods::SHIFT, key: Key::Vk(0xE2) });
    assert_eq!(format_seq(&seq("S-4 S-;"), &[], Layout::Jis), "$ +");
    assert!(parse_key("$", Layout::Jis).is_err());
}

#[test]
fn real_mod_detection() {
    assert_eq!(parse_key("LCtrl", Layout::Jis).unwrap().real_mod(), Some(Mods::CTRL));
    assert_eq!(parse_key("RWin", Layout::Jis).unwrap().real_mod(), Some(Mods::WIN));
    assert_eq!(parse_key("a", Layout::Jis).unwrap().real_mod(), None);
}

#[test]
fn symbols_follow_the_layout() {
    let us = |s| parse_seq(s, &[], Layout::Us).unwrap();
    assert_eq!(us(";").0[0].key, Key::Vk(0xBA));
    assert_eq!(us("@"), us("S-2"));
    assert_eq!(format_seq(&us("S-2 S-; ; C-S-'"), &[], Layout::Us), "@ : ; C-\"");
    let de = |s| parse_seq(s, &[], Layout::De).unwrap();
    assert_eq!(de("ß").0[0].key, Key::Vk(0xDB));
    assert_eq!(de("?"), de("S-ß"));
    assert_eq!(format_seq(&de("S-7 ü"), &[], Layout::De), "/ ü");
    let fr = |s| parse_seq(s, &[], Layout::Fr).unwrap();
    assert_eq!(fr("&").0[0].key, Key::Vk(b'1'));
    assert_eq!(format_seq(&fr("S-1 é"), &[], Layout::Fr), "S-1 2");
    assert!(parse_seq("ß", &[], Layout::Jis).is_err());
}

#[test]
fn duplicate_symbols_name_the_first_key() {
    // JIS types `\` on both the Yen and Ro keys; `\` names the Yen key, and Ro keeps its name.
    assert_eq!(parse_key("\\", Layout::Jis).unwrap(), Key::Vk(0xDC));
    assert_eq!(format_key(&Key::Vk(0xE2), Layout::Jis), "Ro");
}

#[test]
fn layout_names() {
    for l in Layout::ALL {
        assert_eq!(Layout::from_name(l.name()), Some(l));
    }
    assert_eq!(Layout::from_name("US"), Some(Layout::Us));
    assert_eq!(Layout::from_name("dvorak"), None);
}
