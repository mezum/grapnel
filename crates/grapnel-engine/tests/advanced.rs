mod common;
use common::*;
use grapnel_config::ControlCmd;
use grapnel_engine::Command;

fn cx(t: &mut T) {
    t.down("LCtrl");
    assert_eq!(t.down("x"), eaten(""));
    assert_eq!(t.up("x"), eaten(""));
    t.up("LCtrl");
}

#[test]
fn chord_sequence_fires_on_last_key() {
    let mut t = t(&rule("C-x t 0", "\"b\"", "press = \"tap\"", ""));
    cx(&mut t);
    assert_eq!(t.tap("t"), (eaten(""), eaten("")));
    assert_eq!(t.down("0"), eaten("+b -b"));
}

#[test]
fn mismatch_replay_resends_prefix_and_current_key() {
    let mut t = t(&rule("C-x t", "\"b\"", "", ""));
    cx(&mut t);
    assert_eq!(t.down("q"), eaten("+LCtrl +x -x -LCtrl +q"));
    assert_eq!(t.up("q"), eaten("-q"));
}

#[test]
fn mismatch_discard_and_fallback() {
    let mut t1 = t(&rule("C-x t", "\"b\"", "on_mismatch = \"discard\"", ""));
    cx(&mut t1);
    assert_eq!(t1.tap("q"), (eaten(""), eaten("")));
    let mut t2 = t(&rule("C-x t", "\"b\"", "on_mismatch = \"fallback\"\nfallback = \"Esc\"", ""));
    cx(&mut t2);
    assert_eq!(t2.tap("q"), (eaten("+Esc -Esc"), eaten("")));
}

#[test]
fn chord_timeout() {
    let mut t = t(&rule("C-x t", "\"b\"", "timeout_ms = 100", ""));
    t.now = 10;
    cx(&mut t);
    assert_eq!(t.e.next_deadline(), Some(110));
    assert_eq!(t.e.tick(109), vec![]);
    assert_eq!(t.e.tick(110), keys("+LCtrl +x -x -LCtrl"));
    assert_eq!(t.e.next_deadline(), None);
    assert_eq!(t.down("t"), pass());
}

const MU: &str = "[modifiers.Mu]\nkey = \"Muhenkan\"\ntap_timeout_ms = 200";

#[test]
fn user_modifier_tap_and_hold() {
    let mut t = t(&rule("Mu-j", "\"Down\"", "", MU));
    assert_eq!(t.tap("Muhenkan"), (eaten(""), eaten("+Muhenkan -Muhenkan")));
    assert_eq!(t.down("Muhenkan"), eaten(""));
    assert_eq!(t.down("Muhenkan"), eaten(""));
    assert_eq!(t.down("j"), eaten("+Down"));
    assert_eq!(t.up("j"), eaten("-Down"));
    assert_eq!(t.up("Muhenkan"), eaten(""));
    // Without the modifier, j passes.
    assert_eq!(t.down("j"), pass());
}

#[test]
fn user_modifier_tap_timeout() {
    let mut t = t(&rule("Mu-j", "\"Down\"", "", MU));
    t.down("Muhenkan");
    t.now = 201;
    assert_eq!(t.up("Muhenkan"), eaten(""));
}

#[test]
fn modes_switch_and_block() {
    let cfg = "[settings]\ninitial_mode = \"normal\"\n[modes.normal]\nblock_unmapped = true\n\
               [[rules]]\nkeys = \"i\"\naction = \"ins\"\nmodes = [\"normal\"]\n\
               [[actions.ins]]\ndo = [{ mode = \"default\" }]";
    let mut t = t(cfg);
    assert_eq!(t.tap("z"), (eaten(""), eaten("")));
    assert_eq!(t.tap("LShift"), (pass(), pass()));
    assert_eq!(t.down("i").commands, vec![Command::ModeChanged("default".into())]);
    assert_eq!(t.e.mode_name(), "default");
    t.up("i");
    assert_eq!(t.tap("i"), (pass(), pass()));
}

#[test]
fn gesture_and_plain_click() {
    let mut t = t(&rule("RButton:UL", "\"b\"", "", "[settings]\ngesture_threshold = 10"));
    assert_eq!(t.down("RButton"), eaten(""));
    assert_eq!(t.mv(0, -6), pass());
    t.mv(1, -6);
    t.mv(-20, 3);
    t.mv(-20, 0);
    assert_eq!(t.up("RButton"), eaten("+b -b"));
    t.down("RButton");
    t.mv(3, 3);
    assert_eq!(t.up("RButton"), eaten("+RButton -RButton"));
    // Unknown gestures are dropped.
    t.down("RButton");
    t.mv(0, 30);
    assert_eq!(t.up("RButton"), eaten(""));
}

#[test]
fn wheel_with_modifier() {
    let mut t = t(&rule("Mu-WheelUp", "\"PageUp\"", "", MU));
    t.down("Muhenkan");
    assert_eq!(t.down("WheelUp"), eaten("+PageUp -PageUp"));
    assert_eq!(t.down("WheelUp"), eaten("+PageUp -PageUp"));
    t.up("Muhenkan");
    assert_eq!(t.down("WheelUp"), pass());
}

#[test]
fn call_input_and_control() {
    let cfg = "[[rules]]\nkeys = \"a\"\naction = \"x\"\n\
               [[actions.x]]\ndo = [{ call = \"y\", arg = \"1{arg}\" }, { input = \"?\", then = \"y\" }, { control = \"exit\" }]\n\
               [[actions.y]]\ndo = [{ text = \"<{arg}>\" }, { call = \"z\" }]\n\
               [[actions.z]]\ndo = [{ text = \"{arg}\" }]";
    let mut t = t(cfg);
    let y = 1;
    assert_eq!(
        t.down("a").commands,
        vec![
            Command::Text("<1>".into()),
            Command::Text("1".into()),
            Command::InputBox { prompt: "?".into(), then: y },
            Command::Control(ControlCmd::Exit),
        ]
    );
    assert_eq!(t.e.invoke(y, "q", &t.win), vec![Command::Text("<q>".into()), Command::Text("q".into())]);
}

#[test]
fn recursive_call_reports_error() {
    let mut t = t("[[actions.x]]\ndo = [{ call = \"x\" }]");
    let out = t.e.invoke(0, "", &t.win);
    assert!(matches!(out.as_slice(), [Command::Error(_)]), "{out:?}");
}

#[test]
fn reset_releases_holds() {
    let mut t = t(&rule("C-a", "\"Home\"", "", ""));
    t.down("LCtrl");
    t.down("a");
    assert_eq!(t.e.reset(), keys("-Home +LCtrl"));
    assert_eq!(t.up("a"), pass());
}
