mod common;
use common::*;
use grapnel_engine::Command;

#[test]
fn unmapped_keys_pass_through() {
    let mut t = t(&rule("a", "\"b\"", "", ""));
    assert_eq!(t.tap("z"), (pass(), pass()));
}

#[test]
fn hold_is_default_and_repeats() {
    let mut t = t(&rule("a", "\"b\"", "", ""));
    assert_eq!(t.down("a"), eaten("+b"));
    assert_eq!(t.down("a"), eaten("+b"));
    assert_eq!(t.up("a"), eaten("-b"));
}

#[test]
fn tap_press_sends_immediately() {
    let mut t = t(&rule("a", "\"b\"", "press = \"tap\"", ""));
    assert_eq!(t.tap("a"), (eaten("+b -b"), eaten("")));
}

#[test]
fn hold_taps_earlier_chords_and_holds_the_last() {
    let mut t = t(&rule("a", "\"b c\"", "", ""));
    assert_eq!(t.down("a"), eaten("+b -b +c"));
    assert_eq!(t.down("a"), eaten("+c"));
    assert_eq!(t.up("a"), eaten("-c"));
}

#[test]
fn repeat_sends_only_the_last_key() {
    let mut t = t(&rule("a", "\"C-b\"", "", ""));
    assert_eq!(t.down("a"), eaten("+LCtrl +b"));
    assert_eq!(t.down("a"), eaten("+b"));
    assert_eq!(t.up("a"), eaten("-b -LCtrl"));
    // A tap action repeats only its last chord.
    let mut t = t_tap();
    assert_eq!(t.down("a"), eaten("+b -b +LCtrl +c -c -LCtrl"));
    assert_eq!(t.down("a"), eaten("+LCtrl +c -c -LCtrl"));
}

fn t_tap() -> T {
    t(&rule("a", "\"b C-c\"", "press = \"tap\"", ""))
}

#[test]
fn held_modifier_is_neutralized_and_restored() {
    let mut t = t(&rule("C-a", "\"Home\"", "", ""));
    assert_eq!(t.down("LCtrl"), pass());
    assert_eq!(t.down("a"), eaten("-LCtrl +Home"));
    assert_eq!(t.up("a"), eaten("-Home +LCtrl"));
    assert_eq!(t.up("LCtrl"), pass());
}

#[test]
fn output_modifiers_are_pressed() {
    let mut t = t(&rule("a", "\"C-b\"", "press = \"tap\"", ""));
    assert_eq!(t.down("a"), eaten("+LCtrl +b -b -LCtrl"));
}

#[test]
fn existing_right_modifier_satisfies_output() {
    let mut t = t(&rule("C-a", "\"C-b\"", "", ""));
    t.down("RCtrl");
    assert_eq!(t.down("a"), eaten("+b"));
    assert_eq!(t.up("a"), eaten("-b"));
}

#[test]
fn alt_release_and_repress_are_masked() {
    let mut t = t(&rule("M-a", "\"b\"", "press = \"tap\"", ""));
    t.down("LAlt");
    assert_eq!(t.down("a"), eaten("+vk:0xE8 -vk:0xE8 -LAlt +b -b +LAlt +vk:0xE8 -vk:0xE8"));
}

#[test]
fn modifiers_must_match_exactly() {
    let mut t = t(&rule("C-a", "\"b\"", "", ""));
    t.down("LCtrl");
    t.down("LShift");
    assert_eq!(t.down("a"), pass());
}

#[test]
fn text_run_and_mouse_steps() {
    let steps = r#"{ text = "あ{arg}" }, { mouse_move = [1, 2] }, { mouse_move_to = [3, 4] }, { sleep = 5 }, { run = "p{arg}", args = ["x"] }"#;
    let mut t = t(&rule("C-a", steps, "", ""));
    t.down("LCtrl");
    let r = t.down("a");
    let mut expected = keys("-LCtrl");
    expected.extend([
        Command::Text("あ".into()),
        Command::Key { key: k("LCtrl"), down: true },
        Command::MouseMove { x: 1, y: 2, absolute: false },
        Command::MouseMove { x: 3, y: 4, absolute: true },
        Command::Sleep(5),
    ]);
    let (later, before) = r.commands.split_last().unwrap();
    assert_eq!(before, expected);
    let Command::Resume(later) = later.clone() else { panic!("{later:?}") };
    assert_eq!(t.e.resume(later), [Command::Run { program: "p".into(), args: vec!["x".into()] }]);
    // Repeat does not re-run programs.
    assert_eq!(t.down("a"), eaten(""));
}

#[test]
fn targets_limit_rules() {
    let mut t = t(&rule("a", "\"b\"", "targets = [\"code\"]", "[targets.code]\napp = \"code.exe\""));
    assert_eq!(t.down("a"), pass());
    t.up("a");
    t.app("Code.exe");
    assert_eq!(t.down("a"), eaten("+b"));
}

#[test]
fn action_impls_pick_first_matching() {
    let cfg = "[targets.code]\napp = \"code.exe\"\n[keymap]\na = { do = \"x\", press = \"tap\" }\n\
               [actions]\nx = { code = \"b\", \"*\" = \"c\" }";
    let mut t = t(cfg);
    assert_eq!(t.tap("a").0, eaten("+c -c"));
    t.app("code.exe");
    assert_eq!(t.tap("a").0, eaten("+b -b"));
}

#[test]
fn fallback_when_no_impl_matches() {
    let base = "[targets.code]\napp = \"code.exe\"\n[actions]\nx = { code = \"b\" }\n[keymap.\"C-x a\"]\ndo = \"x\"\n";
    let mut t1 = t(&format!("{base}fallback = \"z\""));
    t1.tap("LCtrl");
    t1.down("LCtrl");
    t1.tap("x");
    t1.up("LCtrl");
    assert_eq!(t1.tap("a"), (eaten("+z -z"), eaten("")));
    // Default fallback replays the original input and holds the trigger.
    let mut t2 = t(base);
    t2.down("LCtrl");
    t2.tap("x");
    t2.up("LCtrl");
    assert_eq!(t2.down("a"), eaten("+LCtrl +x -x -LCtrl +a"));
    assert_eq!(t2.up("a"), eaten("-a"));
}

#[test]
fn scancode_keys() {
    let mut t = t(&rule("Zenkaku", "\"Esc\"", "", ""));
    assert_eq!(t.down("sc:0x29"), eaten("+Esc"));
}

#[test]
fn steps_after_sleep_see_modifiers_changed_meanwhile() {
    let mut t =
        t(&rule("C-a", r#""b", { sleep = 5 }, "c", { call = "d" }"#, "", "[actions]\nd = [{ sleep = 1 }, \"e\"]"));
    t.down("LCtrl");
    let r = t.down("a");
    assert_eq!(r.commands[..4], keys("-LCtrl +b -b +LCtrl"));
    let Some(Command::Resume(later)) = r.commands.last().cloned() else { panic!("{:?}", r.commands) };
    // Ctrl is released during the sleep, so `c` needs no Ctrl dance and Ctrl is not pressed again.
    t.up("LCtrl");
    let mut out = t.e.resume(later);
    let Some(Command::Resume(later)) = out.pop() else { panic!("{out:?}") };
    let mut expected = keys("+c -c");
    expected.push(Command::Sleep(1));
    assert_eq!(out, expected);
    assert_eq!(t.e.resume(later), keys("+e -e"));
}

#[test]
fn set_text_waits_like_sleep() {
    let mut t = t(&rule("a", r#"{ set_text = ">{arg}x" }, "b""#, "", ""));
    let r = t.down("a");
    let [Command::SetText { text, into: None }, Command::Resume(later)] = &r.commands[..] else {
        panic!("{:?}", r.commands)
    };
    assert_eq!(text, ">x");
    assert_eq!(t.e.resume(later.clone()), keys("+b -b"));
}

#[test]
fn steps_after_sleep_keep_the_window_they_started_for() {
    let extra = "[targets.code]\napp = \"code.exe\"\n[actions]\nd = { code = \"x\", \"*\" = \"y\" }";
    let mut t = t(&rule("a", r#"{ sleep = 5 }, { call = "d" }"#, "", extra));
    t.app("code.exe");
    let Some(Command::Resume(later)) = t.down("a").commands.last().cloned() else { panic!() };
    t.app("other.exe");
    assert_eq!(t.e.resume(later), keys("+x -x"));
}
