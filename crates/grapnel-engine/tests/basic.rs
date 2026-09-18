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
fn multi_step_is_always_tap_and_repeats_input_steps() {
    let mut t = t(&rule("a", "\"b c\"", "", ""));
    assert_eq!(t.down("a"), eaten("+b -b +c -c"));
    assert_eq!(t.down("a"), eaten("+b -b +c -c"));
    assert_eq!(t.up("a"), eaten(""));
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
        Command::Run { program: "p".into(), args: vec!["x".into()] },
    ]);
    assert_eq!(r.commands, expected);
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
    let cfg = "[targets.code]\napp = \"code.exe\"\n[[rules]]\nkeys = \"a\"\naction = \"x\"\npress = \"tap\"\n\
               [[actions.x]]\nwhen = [\"code\"]\ndo = [\"b\"]\n[[actions.x]]\ndo = [\"c\"]";
    let mut t = t(cfg);
    assert_eq!(t.tap("a").0, eaten("+c -c"));
    t.app("code.exe");
    assert_eq!(t.tap("a").0, eaten("+b -b"));
}

#[test]
fn fallback_when_no_impl_matches() {
    let base = "[targets.code]\napp = \"code.exe\"\n[[rules]]\nkeys = \"C-x a\"\naction = \"x\"\n";
    let action = "[[actions.x]]\nwhen = [\"code\"]\ndo = [\"b\"]";
    let mut t1 = t(&format!("{base}fallback = \"z\"\n{action}"));
    t1.tap("LCtrl");
    t1.down("LCtrl");
    t1.tap("x");
    t1.up("LCtrl");
    assert_eq!(t1.tap("a"), (eaten("+z -z"), eaten("")));
    // Default fallback replays the original input and holds the trigger.
    let mut t2 = t(&format!("{base}{action}"));
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
