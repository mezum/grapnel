//! User modifiers with `as`: the emulated real modifiers are held while the user modifier is,
//! and are only lifted for rule output that does not want them.
mod common;
use common::*;
use grapnel_engine::Reaction;

const CMD: &str = "[modifiers.Cmd]\nkey = \"F19\"\ntap = \"\"\nas = \"C\"";

#[test]
fn emulated_modifier_is_held_with_the_user_modifier() {
    let mut t = t(&rule("Cmd-S-]", "\"C-Tab\"", "", CMD));
    assert_eq!(t.down("F19"), eaten("+LCtrl"));
    // Held F19 repeats like a held Ctrl.
    assert_eq!(t.down("F19"), eaten("+LCtrl"));
    assert_eq!(t.down("c"), pass());
    assert_eq!(t.down("c"), pass());
    assert_eq!(t.up("c"), pass());
    assert_eq!(t.up("F19"), eaten("-LCtrl"));
    assert_eq!(t.down("c"), pass());
}

#[test]
fn physical_modifiers_combine_with_the_emulated_one() {
    let mut t = t(&rule("Cmd-S-]", "\"C-Tab\"", "", CMD));
    t.down("F19");
    t.down("LShift");
    assert_eq!(t.down("a"), pass());
    assert_eq!(t.down("WheelUp"), pass());
    assert_eq!(t.down("LButton"), pass());
}

#[test]
fn rules_lift_only_what_they_do_not_want() {
    let mut t = t(&rule("Cmd-S-]", "\"C-Tab\"", "", CMD));
    t.down("F19");
    t.down("LShift");
    assert_eq!(t.down("]"), eaten("-LShift +Tab"));
    assert_eq!(t.down("]"), eaten("+Tab"));
    assert_eq!(t.up("]"), eaten("-Tab +LShift"));
    assert_eq!(t.up("LShift"), pass());
    assert_eq!(t.up("F19"), eaten("-LCtrl"));
}

#[test]
fn tap_lifts_the_emulated_modifier() {
    let mut t = t("[modifiers.Cmd]\nkey = \"F19\"\ntap = \"Esc\"\nas = \"C\"");
    assert_eq!(t.down("F19"), eaten("+LCtrl"));
    assert_eq!(t.up("F19"), eaten("-LCtrl +Esc -Esc"));
}

#[test]
fn reset_releases_the_emulated_modifier() {
    let mut t = t(CMD);
    t.down("F19");
    assert_eq!(t.e.reset(), keys("-LCtrl"));
}

#[test]
fn releasing_the_physical_modifier_keeps_the_emulated_one() {
    let mut t = t(CMD);
    t.down("F19");
    assert_eq!(t.down("LCtrl"), pass());
    assert_eq!(t.up("LCtrl"), Reaction { consume: false, commands: keys("+LCtrl") });
    assert_eq!(t.up("F19"), eaten("-LCtrl"));
}

#[test]
fn plain_user_modifier_leaves_held_output_alone() {
    let mut t = t(&rule("a", "\"C-b\"", "", "[modifiers.Mu]\nkey = \"F19\"\ntap = \"\""));
    assert_eq!(t.down("a"), eaten("+LCtrl +b"));
    assert_eq!(t.down("F19"), eaten(""));
    assert_eq!(t.down("F19"), eaten(""));
    assert_eq!(t.up("F19"), eaten(""));
    assert_eq!(t.down("a"), eaten("+b"));
}

#[test]
fn keep_mods_holds_output_modifiers_until_the_user_modifier_is_released() {
    let cfg = format!(
        "{CMD}\n[keymap]\n\"Cmd-Tab\" = {{ do = \"M-Tab\", keep_mods = true }}\n\
         \"Cmd-S-Tab\" = {{ do = \"M-S-Tab\", keep_mods = true }}"
    );
    let mut t = t(&cfg);
    assert_eq!(t.down("F19"), eaten("+LCtrl"));
    assert_eq!(t.down("Tab"), eaten("-LCtrl +LAlt +Tab"));
    assert_eq!(t.up("Tab"), eaten("-Tab"));
    assert_eq!(t.down("Tab"), eaten("+Tab"));
    assert_eq!(t.up("Tab"), eaten("-Tab"));
    t.down("LShift");
    assert_eq!(t.down("Tab"), eaten("+Tab"));
    assert_eq!(t.up("Tab"), eaten("-Tab"));
    assert_eq!(t.up("LShift"), pass());
    assert_eq!(t.down("F19"), eaten("+LAlt"));
    assert_eq!(t.up("F19"), eaten("+vk:0xE8 -vk:0xE8 -LAlt"));
    // Next time Cmd is Ctrl again.
    assert_eq!(t.down("F19"), eaten("+LCtrl"));
}

#[test]
fn keep_mods_with_a_real_modifier_replaces_it_until_released() {
    let cfg = "[keymap]\n\"M-Tab\" = { do = \"C-Tab\", keep_mods = true }\n\
               \"M-S-Tab\" = { do = \"C-S-Tab\", keep_mods = true }";
    let mut t = t(cfg);
    assert_eq!(t.down("LAlt"), pass());
    assert_eq!(t.down("Tab"), eaten("+LCtrl +vk:0xE8 -vk:0xE8 -LAlt +Tab"));
    assert_eq!(t.up("Tab"), eaten("-Tab"));
    assert_eq!(t.down("Tab"), eaten("+Tab"));
    assert_eq!(t.up("Tab"), eaten("-Tab"));
    assert_eq!(t.down("LShift"), pass());
    assert_eq!(t.down("Tab"), eaten("+Tab"));
    assert_eq!(t.up("Tab"), eaten("-Tab"));
    assert_eq!(t.up("LShift"), pass());
    // Releasing Alt ends the replacement; the OS already saw Alt go up.
    assert_eq!(t.up("LAlt"), eaten("-LCtrl"));
    assert_eq!(t.down("a"), pass());
}
