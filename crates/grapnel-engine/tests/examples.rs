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
