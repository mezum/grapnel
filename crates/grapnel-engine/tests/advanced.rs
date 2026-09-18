mod common;
use common::*;
use grapnel_config::ControlCmd;
use grapnel_engine::{Command, Fault, Reaction};

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
    let mut t1 = t(&rule("C-x t", "\"b\"", "", "[keymap.\"C-x\".options]\non_mismatch = \"discard\""));
    cx(&mut t1);
    let r = Reaction { consume: true, commands: vec![undefined("C-x q")] };
    assert_eq!(t1.tap("q"), (r, eaten("")));
    let mut t2 =
        t(&rule("C-x t", "\"b\"", "", "[keymap.\"C-x\".options]\non_mismatch = \"fallback\"\nfallback = \"Esc\""));
    cx(&mut t2);
    let mut r = eaten("+Esc -Esc");
    r.commands.insert(0, undefined("C-x q"));
    assert_eq!(t2.tap("q"), (r, eaten("")));
}

#[test]
fn chord_timeout() {
    let mut t = t(&rule("C-x t", "\"b\"", "", "[keymap.\"C-x\".options]\ntimeout_ms = 100"));
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
               [modes.normal.keymap]\ni = [{ mode = \"default\" }]";
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
    let cfg = "[keymap]\na = \"x\"\n\
               [actions]\nx = [{ call = \"y\", arg = \"1{arg}\" }, { input = \"?\", then = \"y\" }, { control = \"exit\" }]\n\
               y = [{ text = \"<{arg}>\" }, { call = \"z\" }]\n\
               z = [{ text = \"{arg}\" }]";
    let mut t = t(cfg);
    let y = 1;
    assert_eq!(
        t.down("a").commands,
        vec![
            Command::Text("<1>".into()),
            Command::Text("1".into()),
            Command::InputBox { prompt: "?".into(), then: Some(y), position: grapnel_config::InputPosition::Center },
            Command::Control(ControlCmd::Exit),
        ]
    );
    assert_eq!(t.e.invoke(y, "q", &t.win), vec![Command::Text("<q>".into()), Command::Text("q".into())]);
}

#[test]
fn invoking_unknown_action_reports_error() {
    let mut t = t("[actions]\nx = []");
    assert!(matches!(t.e.invoke(5, "", &t.win).as_slice(), [Command::Error(_)]));
}

#[test]
fn recursive_call_reports_error() {
    let mut t = t("[actions]\nx = [{ call = \"x\" }]");
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

#[test]
fn unmapped_input_leaves_a_mode_with_unmapped_to() {
    let cfg = "[modes.mark]\nunmapped_to = \"default\"\n\
               [modes.mark.keymap]\n\"C-f\" = \"S-Right\"\n\
               [modes.default.keymap]\n\"C-Space\" = [{ mode = \"mark\" }]";
    let mut t = t(cfg);
    t.down("LCtrl");
    assert_eq!(t.down("Space").commands, vec![Command::ModeChanged("mark".into())]);
    t.up("Space");
    assert!(t.down("f").consume);
    t.up("f");
    assert_eq!(t.e.mode_name(), "mark");
    t.up("LCtrl");
    // Modifiers alone do not leave; any other unmapped input does, and still passes through.
    assert_eq!(t.tap("LShift"), (pass(), pass()));
    assert_eq!(t.down("z"), Reaction { consume: false, commands: vec![Command::ModeChanged("default".into())] });
    assert_eq!(t.e.mode_name(), "default");
    assert_eq!(t.up("z"), pass());
}

const MARK: &str = "[modes.mark]\nunmapped_to = \"default\"\nhold = \"S\"\n\
    [modes.mark.keymap]\nRight = \"S-Right\"\n\
    [modes.default.keymap]\n\"C-Space\" = [{ mode = \"mark\" }]";

#[test]
fn mode_hold_keeps_modifiers_down_while_in_the_mode() {
    let mut t = t(MARK);
    t.down("LCtrl");
    assert_eq!(t.down("Space").commands, [vec![Command::ModeChanged("mark".into())], keys("+LShift")].concat());
    t.up("Space");
    assert_eq!(t.up("LCtrl"), pass());
    assert_eq!(t.down("Right"), eaten("+Right"));
    assert_eq!(t.up("Right"), eaten("-Right"));
    // A physical Shift release does not end the held Shift.
    assert_eq!(t.down("LShift"), pass());
    assert_eq!(t.up("LShift"), Reaction { consume: false, commands: keys("+LShift") });
    // Leaving the mode releases Shift before the key that caused it.
    let mut expected = vec![Command::ModeChanged("default".into())];
    expected.extend(keys("-LShift +x"));
    assert_eq!(t.down("x"), Reaction { consume: true, commands: expected });
    assert_eq!(t.up("x"), eaten("-x"));
}

#[test]
fn reset_leaves_a_holding_mode() {
    let mut t = t(MARK);
    t.down("LCtrl");
    t.tap("Space");
    t.up("LCtrl");
    assert_eq!(t.e.reset(), keys("-LShift"));
    assert_eq!(t.e.mode_name(), "default");
}

#[test]
fn input_box_without_then_and_invoking_by_name() {
    let mut t = t(
        "[keymap]\n\"M-x\" = [{ input = \"M-x\", position = \"bottom\" }]\n[actions]\nhello = [{ text = \"hi {arg}\" }]",
    );
    t.down("LAlt");
    let out = t.down("x").commands;
    assert_eq!(
        out,
        vec![Command::InputBox { prompt: "M-x".into(), then: None, position: grapnel_config::InputPosition::Bottom }]
    );
    t.up("x");
    t.up("LAlt");
    assert_eq!(t.e.invoke_named("hello", "", &t.win), vec![Command::Text("hi ".into())]);
    assert!(
        matches!(t.e.invoke_named("nope", "", &t.win).as_slice(), [Command::Error(Fault::UnknownAction(e))] if e == "nope")
    );
}

#[test]
fn replay_and_timeout_notices() {
    // replay sends the keys on, so it is not reported.
    let mut t1 = t(&rule("C-x t", "\"b\"", "", ""));
    t1.down("LCtrl");
    t1.tap("x");
    t1.up("LCtrl");
    assert!(!t1.down("q").commands.iter().any(|c| matches!(c, Command::Notice(_))));
    // A discarded timeout is reported.
    let mut t2 =
        t(&rule("C-x t", "\"b\"", "", "[keymap.\"C-x\".options]\non_mismatch = \"discard\"\ntimeout_ms = 100"));
    t2.down("LCtrl");
    t2.tap("x");
    assert_eq!(t2.e.tick(100), vec![timed_out("C-x")]);
}
