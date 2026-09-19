//! `[keyswap]`: physical keys typed as other chords, and the keymap seeing those chords.
mod common;
use common::*;

const CFG: &str = r#"
[keyswap]
"S-2" = "@"
":" = "'"
"S-;" = ":"
"@" = "["
[modes.normal]
count = true
[keymap]
":" = "z"
"C-[" = "Esc"
[modes.normal.keymap]
"@" = "x"
"#;

#[test]
fn unbound_swapped_keys_type_their_chord() {
    let mut t = t(CFG);
    assert_eq!(t.down(":"), eaten("+LShift +7"));
    assert_eq!(t.down(":"), eaten("+7"), "repeat");
    assert_eq!(t.up(":"), eaten("-7 -LShift"));
    t.down("LShift");
    assert_eq!(t.down("2"), eaten("-LShift +@"));
    assert_eq!(t.up("2"), eaten("-@ +LShift"));
    // Unswapped keys pass as before.
    assert_eq!(t.down("3"), pass());
    t.up("3");
    t.up("LShift");
    // Other modifiers stay: Alt on the AX [ key is M-[.
    t.down("LAlt");
    assert_eq!(t.down("@"), eaten("+["));
    assert_eq!(t.up("@"), eaten("-["));
}

#[test]
fn keys_pressed_while_a_swap_is_held_get_the_real_modifiers() {
    let mut t = t(CFG);
    assert_eq!(t.down(":"), eaten("+LShift +7"));
    assert_eq!(t.down("a"), eaten("-LShift +a"));
    assert_eq!(t.up(":"), eaten("-7"));
    assert_eq!(t.up("a"), eaten("-a"));
    assert_eq!(t.tap("a"), (pass(), pass()));
}

#[test]
fn the_keymap_sees_swapped_chords() {
    let mut t = t(CFG);
    // S-; types `:`, which is bound; plain `:` (the physical key) is not.
    t.down("LShift");
    assert_eq!(t.down(";"), eaten("-LShift +z"));
    assert_eq!(t.up(";"), eaten("-z +LShift"));
    t.up("LShift");
    t.down("LCtrl");
    assert_eq!(t.down("@"), eaten("-LCtrl +Esc"));
    t.up("@");
    t.up("LCtrl");
}

#[test]
fn mode_bindings_and_counts_see_swapped_chords() {
    let mut t = t(&format!(
        "[settings]
initial_mode = \"normal\"
{CFG}"
    ));
    t.tap("3");
    t.down("LShift");
    assert_eq!(t.down("2"), eaten(&"-LShift +x -x +LShift ".repeat(3)));
}
