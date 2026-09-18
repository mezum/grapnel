//! User modifiers with `as`: keys without a rule are sent with the emulated real modifiers.
mod common;
use common::*;

const CMD: &str = "[modifiers.Cmd]\nkey = \"F19\"\ntap = \"\"\nas = \"C\"";

#[test]
fn unmapped_keys_get_the_emulated_modifier() {
    let mut t = t(&rule("Cmd-S-]", "\"C-Tab\"", "", CMD));
    assert_eq!(t.down("F19"), eaten(""));
    assert_eq!(t.down("c"), eaten("+LCtrl +c"));
    assert_eq!(t.down("c"), eaten("+c"));
    assert_eq!(t.up("c"), eaten("-c -LCtrl"));
    assert_eq!(t.up("F19"), eaten(""));
    // Without Cmd nothing changes.
    assert_eq!(t.down("c"), pass());
}

#[test]
fn physical_modifiers_combine_with_the_emulated_one() {
    let mut t = t(&rule("Cmd-S-]", "\"C-Tab\"", "", CMD));
    t.down("F19");
    t.down("LShift");
    assert_eq!(t.down("a"), eaten("+LCtrl +a"));
    assert_eq!(t.up("a"), eaten("-a -LCtrl"));
}

#[test]
fn rules_take_precedence() {
    let mut t = t(&rule("Cmd-S-]", "\"C-Tab\"", "", CMD));
    t.down("F19");
    t.down("LShift");
    assert_eq!(t.down("]"), eaten("+LCtrl -LShift +Tab"));
    assert_eq!(t.up("]"), eaten("-Tab -LCtrl +LShift"));
}

#[test]
fn wheel_and_clicks_are_emulated_too() {
    let mut t = t(&rule("Cmd-S-]", "\"C-Tab\"", "", CMD));
    t.down("F19");
    assert_eq!(t.down("WheelUp"), eaten("+LCtrl +WheelUp -LCtrl"));
    assert_eq!(t.down("LButton"), eaten("+LCtrl +LButton"));
    assert_eq!(t.up("LButton"), eaten("-LButton -LCtrl"));
}
