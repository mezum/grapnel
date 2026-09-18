//! Regression tests for sequences found in review.
mod common;
use common::*;

#[test]
fn tap_repeat_reruns_with_current_modifiers() {
    let mut t = t(&rule("C-a", "\"b\"", "press = \"tap\"", ""));
    t.down("LCtrl");
    assert_eq!(t.down("a"), eaten("-LCtrl +b -b +LCtrl"));
    t.up("LCtrl");
    assert_eq!(t.down("a"), eaten("+b -b"));
    assert_eq!(t.up("a"), eaten(""));
    assert_eq!(t.e.reset(), keys(""));
}

#[test]
fn replayed_key_keeps_passing_on_repeat_and_release() {
    let cfg = "[[rules]]\nkeys = \"x y\"\naction = \"b\"\n[[rules]]\nkeys = \"q\"\naction = \"b\"\n[[actions.b]]\ndo = [\"z\"]";
    let mut t = t(cfg);
    t.tap("x");
    assert_eq!(t.down("q"), eaten("+x -x +q"));
    assert_eq!(t.down("q"), eaten("+q"));
    assert_eq!(t.up("q"), eaten("-q"));
}

#[test]
fn shared_hold_output_released_by_last_trigger() {
    let cfg =
        "[[rules]]\nkeys = \"a\"\naction = \"x\"\n[[rules]]\nkeys = \"b\"\naction = \"x\"\n[[actions.x]]\ndo = [\"x\"]";
    let mut t = t(cfg);
    assert_eq!(t.down("a"), eaten("+x"));
    assert_eq!(t.down("b"), eaten("+x"));
    assert_eq!(t.up("a"), eaten(""));
    assert_eq!(t.up("b"), eaten("-x"));
}

#[test]
fn hold_repeat_follows_physical_modifiers_like_a_keyboard() {
    let mut t = t(&rule("C-a", "\"C-b\"", "", ""));
    t.down("LCtrl");
    assert_eq!(t.down("a"), eaten("+b"));
    t.up("LCtrl");
    assert_eq!(t.down("a"), eaten("+b"));
    assert_eq!(t.up("a"), eaten("-b"));
}

#[test]
fn standalone_alt_output_is_masked() {
    let mut t = t(&rule("a", "\"LAlt\"", "press = \"tap\"", ""));
    assert_eq!(t.down("a"), eaten("+LAlt +vk:0xE8 -vk:0xE8 -LAlt"));
}

#[test]
fn reset_keeps_physical_modifiers() {
    let mut t = t(&rule("C-a", "\"b\"", "", ""));
    t.down("LCtrl");
    assert_eq!(t.e.reset(), keys(""));
    assert_eq!(t.down("a"), eaten("-LCtrl +b"));
}

#[test]
fn any_other_key_activates_pending_user_modifier() {
    let mods = "[modifiers.Mu]\nkey = \"Muhenkan\"\ntap = \"Esc\"\n[modifiers.Nu]\nkey = \"Henkan\"";
    let mut t = t(mods);
    t.down("Muhenkan");
    assert_eq!(t.tap("Henkan"), (eaten(""), eaten("+Henkan -Henkan")));
    assert_eq!(t.up("Muhenkan"), eaten(""));
    t.down("Muhenkan");
    t.tap("LShift");
    assert_eq!(t.up("Muhenkan"), eaten(""));
}

#[test]
fn expired_prefix_is_resolved_before_next_key() {
    let mut t1 = t(&rule("x y", "\"z\"", "timeout_ms = 100\non_mismatch = \"discard\"", ""));
    t1.tap("x");
    t1.now = 101;
    assert_eq!(t1.down("y"), pass());
    let mut t2 = t(&rule("x y", "\"z\"", "timeout_ms = 100", ""));
    t2.tap("x");
    t2.now = 101;
    assert_eq!(t2.down("y"), eaten("+x -x +y"));
    assert_eq!(t2.up("y"), eaten("-y"));
}
