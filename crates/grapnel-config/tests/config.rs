use grapnel_config::*;
use grapnel_keys::{Key, KeySeq, Mods, parse_seq};
use std::path::{Path, PathBuf};

fn files(list: &[(&str, &str)]) -> Vec<(PathBuf, RawConfig)> {
    list.iter().map(|(p, s)| (PathBuf::from(p), toml::from_str(s).unwrap())).collect()
}

fn ok(s: &str) -> Config {
    compile(&files(&[("main.toml", s)])).unwrap()
}

fn errs(list: &[(&str, &str)]) -> String {
    compile(&files(list)).unwrap_err().join("\n")
}

fn seq(s: &str, user: &[&str]) -> KeySeq {
    parse_seq(s, user).unwrap()
}

const BASE: &str = r#"
[settings]
initial_mode = "normal"
passthrough = ["game"]
suspend_hotkey = "C-M-p"
[modes.normal]
block_unmapped = true
[modes.normal.keymap]
"C-g" = [{ mode = "default" }]
[modifiers.Mu]
key = "Muhenkan"
[targets.editor]
app = "code.exe"
[targets.game]
app = 'C:\Games\*'
[keymap]
"Mu-j" = { do = "down", press = "tap", fallback = "", targets = ["editor"] }
[actions]
down = { editor = ["Down", { call = "down" }, { mode = "default" }, { input = "?", then = "down" }], "*" = "Up" }
"#;

#[test]
fn compiles_base() {
    let c = ok(BASE);
    assert_eq!(c.modes[c.settings.initial_mode].name, "normal");
    assert!(c.modes[1].block_unmapped);
    assert_eq!(c.settings.passthrough, [1]);
    assert_eq!(c.settings.suspend_hotkey.as_ref().unwrap().mods, Mods::CTRL | Mods::ALT);
    assert_eq!(c.modifiers[0].tap, seq("Muhenkan", &[]));
    assert_eq!(c.targets[1].fields[0].0, Field::ExePath);
    // The mode keymap comes first.
    assert_eq!(c.rules[0].modes, [1]);
    let r = &c.rules[1];
    assert_eq!(r.keys.0[0].mods, Mods::user(0));
    assert_eq!((r.targets.as_slice(), r.modes.as_slice(), r.press), (&[0][..], &[][..], Press::Tap));
    assert_eq!(r.fallback, Some(Default::default()));
    let down = &c.actions[c.action_id("down").unwrap()];
    assert_eq!(down.impls.len(), 2);
    assert_eq!(down.impls[0].when, [0]);
    assert_eq!(down.impls[0].steps[1], Step::Call { action: 0, arg: None });
    assert_eq!(down.impls[1].when, [] as [usize; 0]);
}

#[test]
fn defaults() {
    let c = ok("[keymap]\na = \"b\"");
    let r = &c.rules[0];
    assert_eq!((r.press, &r.fallback, r.keep_mods), (Press::Hold, &None, false));
    assert_eq!(c.policy(&r.keys.0, 0), Policy::default());
    assert_eq!(c.settings.gesture_threshold, 30);
    assert_eq!(c.modes[c.settings.initial_mode].name, DEFAULT_MODE);
}

#[test]
fn strings_are_action_names_or_keys() {
    let c = ok("[keymap]\na = \"undo\"\nb = \"C-z\"\n[actions]\nundo = \"C-z\"");
    assert_eq!(c.rules[0].action, c.action_id("undo").unwrap());
    let anon = &c.actions[c.rules[1].action];
    assert_eq!(anon.impls[0].steps, [Step::Keys(seq("C-z", &[]))]);
    assert!(errs(&[("m", "[keymap]\na = \"nope\"")]).contains("m: keymap.\"a\": unknown action or key 'nope'"));
}

#[test]
fn tree_flattens_to_sequences_with_inherited_options() {
    let c = ok(r#"
[keymap.options]
timeout_ms = 500
[keymap."C-x"]
"C-s" = "save"
"t Enter" = "new"
[keymap."C-x".t]
"0" = "close"
[keymap."C-x".t.options]
on_mismatch = "fallback"
fallback = "Esc"
[actions]
save = "C-s"
new = "C-t"
close = "C-w"
"#);
    let keys: Vec<_> = c.rules.iter().map(|r| r.keys.clone()).collect();
    assert_eq!(keys, [seq("C-x C-s", &[]), seq("C-x t Enter", &[]), seq("C-x t 0", &[])]);
    let root = c.policy(&seq("C-x", &[]).0, 0);
    assert_eq!((root.on_mismatch, root.timeout_ms), (Mismatch::Replay, 500));
    let t = c.policy(&seq("C-x t", &[]).0, 0);
    assert_eq!((t.on_mismatch, t.timeout_ms, t.fallback), (Mismatch::Fallback, 500, Some(seq("Esc", &[]))));
}

#[test]
fn reports_unknown_names_with_location() {
    let e = errs(&[("m.toml", "[keymap]\nFoo = \"a\"\nb = { do = \"a\", targets = [\"t\"] }\nc = [{ mode = \"z\" }]")]);
    assert!(e.contains("m.toml: keymap.\"Foo\": unknown key 'Foo'"), "{e}");
    assert!(e.contains("keymap.\"b\".targets: unknown target 't'"), "{e}");
    assert!(e.contains("keymap.\"c\"[0]: unknown mode 'z'"), "{e}");
}

#[test]
fn rejects_bad_keys_and_options() {
    let bind = |k: &str| format!("[modifiers.Mu]\nkey = \"Muhenkan\"\n[keymap]\n\"{k}\" = \"a\"");
    assert!(errs(&[("m", &bind("LCtrl"))]).contains("is a modifier"));
    assert!(errs(&[("m", &bind("Muhenkan"))]).contains("user modifier key"));
    assert!(errs(&[("m", &bind(""))]).contains("empty"));
    let e = errs(&[("m", "[actions]\nx = [\"Pad.A\", \"Mu-a\", \"RButton:U\"]")]);
    assert_eq!(e.lines().count(), 3, "{e}");
    assert!(errs(&[("m", "[keymap]\na = { do = \"b\", pres = \"tap\" }")]).contains("unknown option 'pres'"));
    assert!(errs(&[("m", "[modifiers.C]\nkey = \"a\"")]).contains("name must be"));
    assert!(errs(&[("m", "[modifiers.X]\nkey = \"LShift\"")]).contains("cannot be"));
    assert!(errs(&[("m", "[settings]\nsuspend_hotkey = \"C-LButton\"")]).contains("suspend_hotkey"));
}

#[test]
fn duplicates_and_unreachable_bindings() {
    let e = errs(&[("a", "[keymap]\n\"C-x\" = \"b\""), ("b", "[keymap]\n\"C-x\" = \"c\"\n\"C-x t\" = \"d\"")]);
    assert!(e.contains("b: keymap.\"C-x\": already bound at a: keymap.\"C-x\""), "{e}");
    assert!(e.contains("b: keymap.\"C-x t\": unreachable"), "{e}");
    // A mode keymap may override the global one.
    ok("[keymap]\na = \"b\"\n[modes.m.keymap]\na = \"c\"");
}

#[test]
fn multi_file_merge_and_duplicate_definitions() {
    let a = "[targets.t]\n[actions]\nx = \"a\"\n[keymap]\na = \"x\"";
    let b = "[keymap]\nb = \"x\"";
    let c = compile(&files(&[("a", a), ("b", b)])).unwrap();
    assert_eq!(c.rules.len(), 2);
    let e = errs(&[("a", a), ("b", "[targets.t]\n[actions]\nx = \"b\"\n[settings]")]);
    assert!(e.contains("b: targets.t: already defined in a"), "{e}");
    assert!(e.contains("b: actions.x: already defined in a"), "{e}");
    assert!(e.contains("b: settings: only allowed in the entry file"), "{e}");
}

#[test]
fn circular_targets() {
    let e = errs(&[("m", "[targets.a]\nnot = \"b\"\n[targets.b]\nany = [\"a\"]\n[targets.c]\nall = [\"a\"]")]);
    assert!(e.contains("targets.a: circular") && e.contains("targets.b: circular"), "{e}");
    assert!(!e.contains("targets.c"), "{e}");
}

#[test]
fn scancodes_are_collected() {
    let c = ok("[modifiers.K]\nkey = \"Kana\"\n[keymap]\n\"sc:0x7B Zenkaku\" = \"a\"");
    assert_eq!(c.scancodes(), [0x29, 0x70, 0x7B]);
    assert_eq!(c.modifiers[0].key, Key::Sc(0x70));
}

fn temp_dir(name: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("grapnel-test-{}-{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(d.join("apps")).unwrap();
    d
}

fn write(p: &Path, s: &str) {
    std::fs::write(p, s).unwrap();
}

#[test]
fn load_expands_includes_in_order_once() {
    let d = temp_dir("load");
    write(&d.join("config.toml"), "include = [\"apps/*.toml\", \"apps/b.toml\"]\n[targets.main]");
    write(&d.join("apps/b.toml"), "[targets.b]");
    write(&d.join("apps/a.toml"), "include = [\"../config.toml\"]\n[targets.a]");
    write(&d.join("apps/skip.txt"), "garbage");
    let loaded = load(&d.join("config.toml")).unwrap();
    let names: Vec<_> = loaded.iter().map(|(p, _)| p.file_name().unwrap().to_str().unwrap()).collect();
    assert_eq!(names, ["config.toml", "a.toml", "b.toml"]);
}

#[test]
fn load_reports_missing_and_invalid() {
    let d = temp_dir("bad");
    write(&d.join("config.toml"), "include = [\"missing.toml\", \"apps/bad.toml\"]");
    write(&d.join("apps/bad.toml"), "[keymap]\na = 1");
    let e = load(&d.join("config.toml")).unwrap_err().join("\n");
    assert!(e.contains("include 'missing.toml': file not found"), "{e}");
    assert!(e.contains("bad.toml"), "{e}");
}

#[test]
fn save_round_trips() {
    let d = temp_dir("save");
    let raw: RawConfig = toml::from_str(BASE).unwrap();
    save(&d.join("new/config.toml"), &raw).unwrap();
    assert_eq!(load(&d.join("new/config.toml")).unwrap()[0].1, raw);
}

#[test]
fn too_many_modifiers_is_an_error_not_a_panic() {
    let mut s: String = (0..29).map(|i| format!("[modifiers.U{i:02}]\nkey = \"F{}\"\n", i % 24 + 1)).collect();
    s += "[keymap]\n\"U28-a\" = \"b\"";
    assert!(errs(&[("m", &s)]).contains("too many modifiers"));
}

#[test]
fn scancode_modifiers_are_rejected() {
    assert!(errs(&[("m", "[keymap]\n\"sc:0xE01D\" = \"a\"")]).contains("is a modifier"));
    assert!(errs(&[("m", "[modifiers.X]\nkey = \"sc:0x2A\"")]).contains("cannot be"));
}

#[test]
fn compiling_nothing_is_an_error() {
    assert!(compile(&[]).is_err());
}

#[test]
fn regex_app_uses_path_only_with_literal_backslash() {
    let c = ok(r#"
[targets.name]
app = 're:^emacs\.exe$'
[targets.path]
app = 're:\\Emacs\\bin\\'
"#);
    assert_eq!(c.targets[0].fields[0].0, Field::ExeName);
    assert_eq!(c.targets[1].fields[0].0, Field::ExePath);
}

#[test]
fn modifier_as_emulates_real_modifiers() {
    let c = ok("[modifiers.Cmd]\nkey = \"F19\"\nas = \"C-S\"");
    assert_eq!(c.modifiers[0].emulate, Mods::CTRL | Mods::SHIFT);
    assert_eq!(ok("[modifiers.Cmd]\nkey = \"F19\"").modifiers[0].emulate, Mods::NONE);
    assert!(errs(&[("m", "[modifiers.Cmd]\nkey = \"F19\"\nas = \"C-X\"")]).contains("modifiers.Cmd.as"));
}

#[test]
fn keep_mods_needs_a_modifier() {
    let bind =
        |k: &str| format!("[modifiers.Cmd]\nkey = \"F19\"\n[keymap]\n\"{k}\" = {{ do = \"a\", keep_mods = true }}");
    assert!(ok(&bind("Cmd-Tab")).rules[0].keep_mods);
    assert!(ok(&bind("M-Tab")).rules[0].keep_mods);
    assert!(errs(&[("m", &bind("Tab"))]).contains("keymap.\"Tab\".keep_mods"));
}

#[test]
fn modes_unmapped_to_and_hold() {
    let c = ok("[modes.mark]\nunmapped_to = \"default\"\nhold = \"S\"");
    assert_eq!((c.modes[1].unmapped_to, c.modes[1].hold), (Some(0), Mods::SHIFT));
    assert_eq!(c.modes[0].unmapped_to, None);
    assert!(
        errs(&[("m", "[modes.mark]\nunmapped_to = \"nope\"")]).contains("modes.mark.unmapped_to: unknown mode 'nope'")
    );
    assert!(errs(&[("m", "[modes.mark]\nhold = \"Q\"")]).contains("modes.mark.hold"));
}

#[test]
fn save_keeps_target_order_with_step_lists() {
    let d = temp_dir("order");
    let raw: RawConfig = toml::from_str(
        "[targets.editor]\n[actions]\ntest = { editor = [{ text = \"E\" }], \"*\" = \"Esc\" }\n[keymap]\na = \"test\"",
    )
    .unwrap();
    save(&d.join("c.toml"), &raw).unwrap();
    let back = &load(&d.join("c.toml")).unwrap()[0].1;
    assert_eq!(back, &raw, "{}", std::fs::read_to_string(d.join("c.toml")).unwrap());
    let c = compile(&[(d.join("c.toml"), back.clone())]).unwrap();
    assert_eq!(c.actions[c.action_id("test").unwrap()].impls[0].when, [0]);
}

#[test]
fn options_inherit_across_files_and_shorthand_paths() {
    let main = "include = []\n[keymap.options]\non_mismatch = \"discard\"\n[keymap.\"a b\".options]\ntimeout_ms = 50";
    let extra = "[keymap.a]\nx = \"y\"\n[keymap.a.options]\ntimeout_ms = 100\n[keymap.\"a b\"]\nc = \"d\"";
    let c = compile(&files(&[("main", main), ("extra", extra)])).unwrap();
    let a = c.policy(&seq("a", &[]).0, 0);
    assert_eq!((a.on_mismatch, a.timeout_ms), (Mismatch::Discard, 100));
    let ab = c.policy(&seq("a b", &[]).0, 0);
    assert_eq!((ab.on_mismatch, ab.timeout_ms), (Mismatch::Discard, 50));
    // Mode nodes inherit global defaults too.
    let m = compile(&files(&[("m", "[keymap.options]\ntimeout_ms = 7\n[modes.v.keymap.g]\nx = \"y\"\n[modes.v.keymap.g.options]\non_mismatch = \"discard\"")])).unwrap();
    let g = m.policy(&seq("g", &[]).0, 1);
    assert_eq!((g.on_mismatch, g.timeout_ms), (Mismatch::Discard, 7));
}

#[test]
fn reachability_respects_targets_and_modes() {
    // A short binding limited by targets does not hide longer ones.
    ok("[targets.editor]\napp = \"code.exe\"\n[keymap]\na = { do = \"x\", targets = [\"editor\"] }\n\"a b\" = \"y\"");
    // A global binding hides a longer mode binding.
    let e = errs(&[("m", "[keymap]\na = \"x\"\n[modes.edit.keymap]\n\"a b\" = \"y\"")]);
    assert!(e.contains("m: modes.edit.keymap.\"a b\": unreachable"), "{e}");
    // A mode binding does not hide a longer global one (it still works in other modes).
    ok("[keymap]\n\"a b\" = \"y\"\n[modes.edit.keymap]\na = \"x\"");
}

#[test]
fn step_strings_call_actions_when_named() {
    let c = ok("[keymap]\na = [\"undo\", \"C-s\", { keys = \"C-z\" }]\n[actions]\nundo = \"C-z\"\nundo2 = \"undo\"");
    let undo = c.action_id("undo").unwrap();
    let steps = &c.actions[c.rules[0].action].impls[0].steps;
    assert_eq!(steps[0], Step::Call { action: undo, arg: None });
    assert_eq!(steps[1], Step::Keys(seq("C-s", &[])));
    // `{ keys = ... }` always means keys.
    assert!(
        errs(&[("m", "[keymap]\na = [{ keys = \"undo\" }]\n[actions]\nundo = \"C-z\"")]).contains("unknown key 'undo'")
    );
    // An action's own string may name another action.
    assert_eq!(c.actions[c.action_id("undo2").unwrap()].impls[0].steps, [Step::Call { action: undo, arg: None }]);
}
