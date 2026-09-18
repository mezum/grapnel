use grapnel_config::*;
use grapnel_keys::{Key, Mods, parse_seq};
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

const BASE: &str = r#"
[settings]
initial_mode = "normal"
passthrough = ["game"]
suspend_hotkey = "C-M-p"
[modes.normal]
block_unmapped = true
[modifiers.Mu]
key = "Muhenkan"
[targets.editor]
app = "code.exe"
[targets.game]
app = 'C:\Games\*'
[[rules]]
keys = "Mu-j C-x"
action = "down"
targets = ["editor"]
modes = ["normal"]
press = "tap"
fallback = ""
[[actions.down]]
when = ["editor"]
do = ["Down", { call = "down" }, { mode = "default" }, { input = "?", then = "down" }]
"#;

#[test]
fn compiles_base() {
    let c = ok(BASE);
    assert_eq!(c.modes[c.settings.initial_mode].name, "normal");
    assert!(c.modes[1].block_unmapped);
    assert_eq!(c.settings.passthrough, [1]);
    assert_eq!(c.settings.suspend_hotkey.as_ref().unwrap().mods, Mods::CTRL | Mods::ALT);
    assert_eq!(c.modifiers[0].tap, parse_seq("Muhenkan", &[]).unwrap());
    assert_eq!(c.targets[1].fields[0].0, Field::ExePath);
    let r = &c.rules[0];
    assert_eq!(r.keys.0[0].mods, Mods::user(0));
    assert_eq!((r.targets.as_slice(), r.modes.as_slice(), r.press), (&[0][..], &[1][..], Press::Tap));
    assert_eq!(r.fallback, Some(Default::default()));
    let steps = &c.actions[0].impls[0].steps;
    assert_eq!(steps[1], Step::Call { action: 0, arg: None });
    assert_eq!(steps[2], Step::Mode(0));
}

#[test]
fn defaults() {
    let c = ok("[[rules]]\nkeys = \"a\"\naction = \"x\"\n[[actions.x]]\ndo = [\"b\"]");
    let r = &c.rules[0];
    assert_eq!((r.press, r.on_mismatch, r.timeout_ms, &r.fallback), (Press::Hold, Mismatch::Replay, 0, &None));
    assert_eq!(c.settings.gesture_threshold, 30);
    assert_eq!(c.modes[c.settings.initial_mode].name, DEFAULT_MODE);
}

#[test]
fn reports_unknown_names_with_location() {
    let e = errs(&[("m.toml", "[[rules]]\nkeys = \"Foo\"\naction = \"nope\"\ntargets = [\"t\"]\nmodes = [\"z\"]")]);
    assert!(e.contains("m.toml: rules[0].keys: unknown key 'Foo'"), "{e}");
    assert!(e.contains("rules[0].action: unknown action 'nope'"), "{e}");
    assert!(e.contains("rules[0].targets: unknown target 't'"), "{e}");
    assert!(e.contains("rules[0].modes: unknown mode 'z'"), "{e}");
}

#[test]
fn rejects_bad_keys() {
    let rule = |k: &str| {
        format!("[modifiers.Mu]\nkey = \"Muhenkan\"\n[[rules]]\nkeys = \"{k}\"\naction = \"x\"\n[[actions.x]]\ndo = []")
    };
    assert!(errs(&[("m", &rule("LCtrl"))]).contains("is a modifier"));
    assert!(errs(&[("m", &rule("Muhenkan"))]).contains("user modifier key"));
    assert!(errs(&[("m", &rule(""))]).contains("empty"));
    let e = errs(&[("m", "[[actions.x]]\ndo = [\"Pad.A\", \"Mu-a\", \"RButton:U\"]")]);
    assert_eq!(e.lines().count(), 3, "{e}");
    assert!(errs(&[("m", "[modifiers.C]\nkey = \"a\"")]).contains("name must be"));
    assert!(errs(&[("m", "[modifiers.X]\nkey = \"LShift\"")]).contains("cannot be"));
    assert!(errs(&[("m", "[settings]\nsuspend_hotkey = \"C-LButton\"")]).contains("suspend_hotkey"));
}

#[test]
fn multi_file_merge_and_duplicates() {
    let a = "[targets.t]\n[[actions.x]]\ndo = [\"a\"]\n[[rules]]\nkeys = \"a\"\naction = \"x\"";
    let b = "[[actions.x]]\nwhen = [\"t\"]\ndo = [\"b\"]\n[[rules]]\nkeys = \"b\"\naction = \"x\"";
    let c = compile(&files(&[("a", a), ("b", b)])).unwrap();
    assert_eq!(c.actions[0].impls.len(), 2);
    assert_eq!(c.rules.len(), 2);
    let e = errs(&[("a", a), ("b", "[targets.t]\n[settings]")]);
    assert!(e.contains("b: targets.t: already defined in a"), "{e}");
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
    let c = ok(
        "[modifiers.K]\nkey = \"Kana\"\n[[rules]]\nkeys = \"sc:0x7B Zenkaku\"\naction = \"x\"\n[[actions.x]]\ndo = []",
    );
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
    write(&d.join("apps/bad.toml"), "[[rules]]\nkeys = 1");
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
    s += "[[rules]]\nkeys = \"U28-a\"\naction = \"x\"\n[[actions.x]]\ndo = []";
    assert!(errs(&[("m", &s)]).contains("too many modifiers"));
}

#[test]
fn scancode_modifiers_are_rejected() {
    let e = errs(&[("m", "[[rules]]\nkeys = \"sc:0xE01D\"\naction = \"x\"\n[[actions.x]]\ndo = []")]);
    assert!(e.contains("is a modifier"), "{e}");
    assert!(errs(&[("m", "[modifiers.X]\nkey = \"sc:0x2A\"")]).contains("cannot be"));
}

#[test]
fn compiling_nothing_is_an_error() {
    assert!(compile(&[]).is_err());
}
