//! The shipped example configs compile and behave as documented.
mod common;
use common::*;
use grapnel_engine::{Command, Reaction};

fn example(name: &str) -> T {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples").join(name);
    t(&std::fs::read_to_string(path).unwrap())
}

#[test]
fn emacs_bindings() {
    let mut t = example("emacs.toml");
    t.app("notepad.exe");
    t.down("LCtrl");
    for (key, out) in [("f", "Right"), ("b", "Left"), ("n", "Down"), ("p", "Up")] {
        assert_eq!(t.down(key), eaten(&format!("-LCtrl +{out}")), "C-{key}");
        assert_eq!(t.down(key), eaten(&format!("+{out}")), "C-{key} repeat");
        assert_eq!(t.up(key), eaten(&format!("-{out} +LCtrl")), "C-{key} up");
    }
    for (key, out) in [("a", "Home"), ("e", "End"), ("h", "Backspace"), ("d", "Delete")] {
        assert_eq!(t.down(key), eaten(&format!("-LCtrl +{out}")), "C-{key}");
        assert_eq!(t.up(key), eaten(&format!("-{out} +LCtrl")), "C-{key} up");
    }
    // Other Ctrl shortcuts and plain letters are untouched.
    assert_eq!(t.down("c"), pass());
    t.up("c");
    t.up("LCtrl");
    assert_eq!(t.down("f"), pass());
}

#[test]
fn emacs_kill_and_yank() {
    let mut t = example("emacs.toml");
    t.app("notepad.exe");
    t.down("LCtrl");
    assert_eq!(t.down("k"), eaten("-LCtrl +LShift +End -End +LCtrl -LShift +x"));
    assert_eq!(t.up("k"), eaten("-x"));
    for (key, out) in [("w", "x"), ("y", "v"), ("/", "z")] {
        assert_eq!(t.down(key), eaten(&format!("+{out}")), "C-{key}");
        assert_eq!(t.up(key), eaten(&format!("-{out}")), "C-{key} up");
    }
    t.up("LCtrl");
    t.down("LAlt");
    assert_eq!(t.down("w"), eaten("+LCtrl +vk:0xE8 -vk:0xE8 -LAlt +c"));
    assert_eq!(t.up("w"), eaten("-c -LCtrl +LAlt +vk:0xE8 -vk:0xE8"));
}

#[test]
fn emacs_mark() {
    let mut t = example("emacs.toml");
    t.app("notepad.exe");
    t.down("LCtrl");
    assert_eq!(t.down("Space").commands, vec![Command::ModeChanged("mark".into())]);
    t.up("Space");
    // Movement extends the selection.
    assert_eq!(t.down("f"), eaten("-LCtrl +LShift +Right"));
    assert_eq!(t.down("f"), eaten("+Right"));
    assert_eq!(t.up("f"), eaten("-Right -LShift +LCtrl"));
    assert_eq!(t.down("e"), eaten("-LCtrl +LShift +End"));
    t.up("e");
    // Cut ends the mark.
    let r = t.down("w");
    assert_eq!(r.commands.last(), Some(&Command::ModeChanged("default".into())));
    t.up("w");
    assert_eq!(t.down("f"), eaten("-LCtrl +Right"));
    t.up("f");
    t.up("LCtrl");
    // Plain arrows extend too, and typing passes through and ends the mark.
    t.down("LCtrl");
    t.tap("Space");
    t.up("LCtrl");
    assert_eq!(t.down("Left"), eaten("+LShift +Left"));
    t.up("Left");
    assert_eq!(t.down("x"), Reaction { consume: false, commands: vec![Command::ModeChanged("default".into())] });
    t.up("x");
    assert_eq!(t.down("Left"), pass());
}

#[test]
fn emacs_bindings_skip_emacs_and_terminals() {
    let mut t = example("emacs.toml");
    for exe in ["emacs.exe", "WindowsTerminal.exe"] {
        t.app(exe);
        t.down("LCtrl");
        assert_eq!(t.down("a"), pass(), "{exe}");
        t.up("a");
        t.up("LCtrl");
    }
}

#[test]
fn mac_cmd_modifier() {
    let mut t = example("mac-cmd.toml");
    // Holding Cmd holds Ctrl, so plain Cmd combinations pass through as Ctrl ones.
    assert_eq!(t.down("F19"), eaten("+LCtrl"));
    assert_eq!(t.down("c"), pass());
    t.up("c");
    // Rules lift only what their output does not want.
    t.down("LShift");
    assert_eq!(t.down("]"), eaten("-LShift +Tab"));
    assert_eq!(t.down("]"), eaten("+Tab"));
    assert_eq!(t.up("]"), eaten("-Tab +LShift"));
    assert_eq!(t.down("["), eaten("+Tab"));
    assert_eq!(t.up("["), eaten("-Tab"));
    assert_eq!(t.down("5"), eaten("-LCtrl +LWin +s -s +vk:0xE8 -vk:0xE8 -LWin +LCtrl"));
    assert_eq!(t.up("5"), eaten(""));
    t.up("LShift");
    assert_eq!(t.up("F19"), eaten("-LCtrl"));
    // App switching keeps Alt down until Cmd is released.
    t.down("F19");
    assert_eq!(t.down("Tab"), eaten("-LCtrl +LAlt +Tab"));
    t.up("Tab");
    assert_eq!(t.down("Tab"), eaten("+Tab"));
    t.up("Tab");
    assert_eq!(t.up("F19"), eaten("+vk:0xE8 -vk:0xE8 -LAlt"));
    // Physical Alt-Tab switches tabs, keeping Ctrl down until Alt is released.
    t.down("LAlt");
    assert_eq!(t.down("Tab"), eaten("+LCtrl +vk:0xE8 -vk:0xE8 -LAlt +Tab"));
    t.up("Tab");
    t.down("LShift");
    assert_eq!(t.down("Tab"), eaten("+Tab"));
    t.up("Tab");
    t.up("LShift");
    assert_eq!(t.up("LAlt"), eaten("-LCtrl"));
}
