//! The shipped example configs compile and behave as documented.
mod common;
use common::*;
use grapnel_engine::{Command, Reaction};

fn examples() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples")
}

fn example(name: &str) -> T {
    t_file(&examples().join(name))
}

#[test]
fn examples_combine_through_includes() {
    let dir = std::env::temp_dir().join(format!("grapnel-examples-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let ex = examples().canonicalize().unwrap();
    let text = format!("include = [{:?}, {:?}]", ex.join("emacs.toml"), ex.join("mac-cmd.toml"));
    std::fs::write(dir.join("config.toml"), text).unwrap();
    let mut t = t_file(&dir.join("config.toml"));
    t.app("notepad.exe");
    // mac-cmd.toml: Cmd-S-] switches tabs.
    t.down("F19");
    t.down("LShift");
    assert_eq!(t.down("]"), eaten("-LShift +Tab"));
    t.up("]");
    t.up("LShift");
    t.up("F19");
    // emacs.toml: C-f moves right.
    t.down("LCtrl");
    assert_eq!(t.down("f"), eaten("-LCtrl +Right"));
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
    assert_eq!(t.down("Space").commands, [vec![Command::ModeChanged("mark".into())], keys("+LShift")].concat());
    t.up("Space");
    // Shift is held, so movement only needs Ctrl lifted.
    assert_eq!(t.down("f"), eaten("-LCtrl +Right"));
    assert_eq!(t.down("f"), eaten("+Right"));
    assert_eq!(t.up("f"), eaten("-Right +LCtrl"));
    assert_eq!(t.down("e"), eaten("-LCtrl +End"));
    t.up("e");
    // Cut ends the mark and releases Shift.
    t.down("w");
    t.up("w");
    assert_eq!(t.e.mode_name(), "default");
    assert_eq!(t.down("f"), eaten("-LCtrl +Right"));
    t.up("f");
    t.up("LCtrl");
    // Plain arrows extend too; typing releases Shift first, then types and ends the mark.
    t.down("LCtrl");
    t.tap("Space");
    t.up("LCtrl");
    assert_eq!(t.down("Left"), eaten("+Left"));
    t.up("Left");
    let mut expected = vec![Command::ModeChanged("default".into())];
    expected.extend(keys("-LShift +x"));
    assert_eq!(t.down("x"), Reaction { consume: true, commands: expected });
    assert_eq!(t.up("x"), eaten("-x"));
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

/// Presses C-x (Ctrl held), then `key` with or without Ctrl.
fn after_cx(t: &mut T, key: &str, ctrl: bool) -> grapnel_engine::Reaction {
    t.down("LCtrl");
    assert_eq!(t.down("x"), eaten(""));
    t.up("x");
    if !ctrl {
        t.up("LCtrl");
    }
    let r = t.down(key);
    t.up(key);
    if ctrl {
        t.up("LCtrl");
    }
    r
}

#[test]
fn emacs_cx_commands() {
    let mut t = example("emacs.toml");
    t.app("notepad.exe");
    assert_eq!(after_cx(&mut t, "h", false), eaten("+LCtrl +a"));
    assert_eq!(after_cx(&mut t, "s", true), eaten("+s"));
    assert_eq!(after_cx(&mut t, "w", true), eaten("+LShift +s"));
    assert_eq!(after_cx(&mut t, "k", false), eaten("+LCtrl +w"));
    assert_eq!(after_cx(&mut t, "f", true), eaten("+o"));
    assert_eq!(after_cx(&mut t, "c", true), eaten("-LCtrl +LAlt +F4"));
    assert_eq!(after_cx(&mut t, "u", false), eaten("+LCtrl +z"));
    // C-x C-u is not bound, so like any unknown key after C-x it is dropped (not Ctrl+X = cut).
    let eaten_undefined = |s: &str| grapnel_engine::Reaction { consume: true, commands: vec![undefined(s)] };
    assert_eq!(after_cx(&mut t, "u", true), eaten_undefined("C-x C-u"));
    assert_eq!(after_cx(&mut t, "q", false), eaten_undefined("C-x q"));
}

#[test]
fn emacs_redo_depends_on_the_app() {
    let mut t = example("emacs.toml");
    t.down("LCtrl");
    t.down("LShift");
    for (app, out) in
        [("notepad.exe", "-LShift +y"), ("WINWORD.EXE", "-LShift +y"), ("blender.exe", "+z"), ("Photoshop.exe", "+z")]
    {
        t.app(app);
        assert_eq!(t.down("/"), eaten(out), "{app}");
        t.up("/");
    }
    t.app("emacs.exe");
    assert_eq!(t.down("/"), pass());
}

#[test]
fn emacs_cx_is_not_captured_in_emacs() {
    let mut t = example("emacs.toml");
    t.app("emacs.exe");
    t.down("LCtrl");
    assert_eq!(t.down("x"), pass());
}

#[test]
fn mac_virtual_desktops() {
    let mut t = example("mac-cmd.toml");
    t.down("LCtrl");
    // C-Left / C-Right switch desktops (held, so holding repeats the switch).
    assert_eq!(t.down("Right"), eaten("+LWin +Right"));
    assert_eq!(t.down("Right"), eaten("+Right"));
    assert_eq!(t.up("Right"), eaten("-Right +vk:0xE8 -vk:0xE8 -LWin"));
    assert_eq!(t.down("Left"), eaten("+LWin +Left"));
    t.up("Left");
    // C-Up: task view, C-Down: show desktop.
    assert_eq!(t.down("Up"), eaten("-LCtrl +LWin +Tab -Tab +vk:0xE8 -vk:0xE8 -LWin +LCtrl"));
    t.up("Up");
    assert_eq!(t.down("Down"), eaten("-LCtrl +LWin +d -d +vk:0xE8 -vk:0xE8 -LWin +LCtrl"));
    t.up("Down");
    t.up("LCtrl");
    // Cmd (F19) + arrows still reach apps as Ctrl + arrows (word movement).
    t.down("F19");
    assert_eq!(t.down("Right"), pass());
}

#[test]
fn emacs_meta_x_opens_a_bottom_prompt() {
    let mut t = example("emacs.toml");
    t.app("notepad.exe");
    t.down("LAlt");
    let then = grapnel_config::Then::Named("{arg}".into());
    let prompt = Command::InputBox { prompt: "M-x".into(), then, position: grapnel_config::InputPosition::Bottom };
    assert_eq!(t.down("x").commands, vec![prompt]);
    t.up("x");
    t.app("emacs.exe");
    assert_eq!(t.down("x"), pass());
}

/// vim.toml in a text field of notepad.
fn vim() -> T {
    let mut t = example("vim.toml");
    t.app("notepad.exe");
    t.win.uia_type = "Edit".into();
    t
}

fn mode(r: &grapnel_engine::Reaction) -> Option<&str> {
    r.commands.iter().find_map(|c| if let Command::ModeChanged(m) = c { Some(m.as_str()) } else { None })
}

#[test]
fn vim_enters_normal_only_in_text_fields() {
    let mut t = vim();
    t.win.uia_type = "Button".into();
    assert_eq!(t.down("Esc"), pass());
    t.up("Esc");
    t.win.uia_type = "Edit".into();
    t.app("WindowsTerminal.exe");
    assert_eq!(t.down("Esc"), pass());
    t.up("Esc");
    t.app("notepad.exe");
    assert_eq!(mode(&t.down("Esc")), Some("normal"));
    t.up("Esc");
}

#[test]
fn vim_normal_mode() {
    let mut t = vim();
    t.tap("Esc");
    assert_eq!(t.down("j"), eaten("+Down"));
    assert_eq!(t.up("j"), eaten("-Down"));
    // Unassigned letters do nothing; d then an unknown key is dropped.
    assert!(t.down("q").commands.is_empty());
    t.up("q");
    t.tap("d");
    assert!(t.down("q").consume);
    t.up("q");
    assert_eq!(t.down("x"), eaten("+Delete"));
    t.up("x");
    // i types again; Esc returns.
    assert_eq!(mode(&t.down("i")), Some("input"));
    t.up("i");
    assert_eq!(t.down("j"), pass());
    t.up("j");
    t.tap("Esc");
    assert_eq!(t.e.mode_name(), "normal");
    // Anything else (here F5) goes through and leaves normal mode.
    let r = t.down("F5");
    assert_eq!((r.consume, mode(&r)), (false, Some("input")));
}

#[test]
fn vim_visual_command_and_search() {
    let mut t = vim();
    t.tap("Esc");
    // visual holds Shift; y copies and returns to normal.
    assert_eq!(mode(&t.down("v")), Some("visual"));
    t.up("v");
    assert_eq!(t.down("l"), eaten("+Right"));
    t.up("l");
    assert_eq!(mode(&t.down("y")), Some("normal"));
    t.up("y");
    // : opens a prompt whose text names a vim:* action; :w saves and returns to normal.
    let r = t.down(":");
    t.up(":");
    let then = grapnel_config::Then::Named("vim:{arg}".into());
    assert_eq!(mode(&r), Some("command"));
    assert!(r.commands.iter().any(|c| matches!(c, Command::InputBox { then: t, .. } if *t == then)));
    let r = Reaction { consume: true, commands: t.e.invoke_input(&then, "w", &t.win) };
    assert_eq!(mode(&r), Some("normal"));
    assert!(r.commands.ends_with(&keys("+LCtrl +s -s -LCtrl")), "{:?}", r.commands);
    // A dismissed prompt leaves command mode on the next key, which is dropped.
    t.tap(":");
    let r = t.down("j");
    assert_eq!((r.consume, mode(&r)), (true, Some("normal")));
    t.up("j");
    // / opens the app's search; typing goes there; Enter searches and returns.
    t.tap("/");
    assert_eq!(t.e.mode_name(), "search");
    assert_eq!(t.down("a"), pass());
    t.up("a");
    assert_eq!(mode(&t.down("Enter")), Some("normal"));
}

#[test]
fn vim_counts_and_dot() {
    let mut t = vim();
    t.tap("Esc");
    t.tap("3");
    assert_eq!(t.down("j"), eaten("+Down -Down +Down -Down +Down -Down"));
    t.up("j");
    t.tap("2");
    assert_eq!(t.down("x"), eaten("+Delete -Delete +Delete -Delete"));
    t.up("x");
    assert_eq!(t.down("."), eaten("+Delete -Delete +Delete -Delete"));
    t.up(".");
}
