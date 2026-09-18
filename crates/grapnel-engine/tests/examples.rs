//! The shipped example configs compile and behave as documented.
mod common;
use common::*;

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
    assert_eq!(t.tap("F19"), (eaten(""), eaten("")));
    t.down("F19");
    // Plain Cmd combinations become Ctrl.
    assert_eq!(t.down("c"), eaten("+LCtrl +c"));
    assert_eq!(t.up("c"), eaten("-c -LCtrl"));
    // Tab switching and capture keep the physical Shift only where needed.
    t.down("LShift");
    assert_eq!(t.down("]"), eaten("+LCtrl -LShift +Tab"));
    assert_eq!(t.up("]"), eaten("-Tab -LCtrl +LShift"));
    assert_eq!(t.down("["), eaten("+LCtrl +Tab"));
    assert_eq!(t.up("["), eaten("-Tab -LCtrl"));
    assert_eq!(t.down("5"), eaten("+LWin +s -s +vk:0xE8 -vk:0xE8 -LWin"));
    assert_eq!(t.up("5"), eaten(""));
    // Cmd-S-a is Ctrl+Shift+A.
    assert_eq!(t.down("a"), eaten("+LCtrl +a"));
}
