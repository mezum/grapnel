use super::*;

const SAMPLE: &str = r#"
include = ["apps/*.toml"]

[settings]
passthrough = ["games"]
suspend_hotkey = "C-M-S-p"
gesture_threshold = 30

[modifiers.Mu]
key = "Muhenkan"
tap_timeout_ms = 300
as = "C"

[targets.editor]
app = "code.exe"
[targets.games]
any = ["steam_game", "emulator"]

[keymap]
"C-f" = "Right"
"C-/" = "undo"
"C-M-s" = [{ input = "検索", then = "search" }]
"M-Tab" = { do = "C-Tab", keep_mods = true, press = "tap" }

[keymap."C-x"]
"C-s" = "C-s"
t = { Enter = "C-t", "0" = "close_tab" }

[keymap."C-x".options]
on_mismatch = "discard"
timeout_ms = 1500

[modes.mark]
unmapped_to = "default"
hold = "S"
[modes.mark.keymap]
"C-g" = [{ mode = "default" }]

[actions]
undo = "C-z"
close_tab = { editor = "C-w", "*" = ["C-F4", { sleep = 10 }] }
search = [{ run = "cmd", args = ["/c"] }, { call = "x" }, { control = "reload" }, { mouse_move = [1, -1] }]
"#;

fn parse(s: &str) -> Result<RawConfig, toml::de::Error> {
    toml::from_str(s)
}

#[test]
fn parses_spec_sample() {
    let c = parse(SAMPLE).unwrap();
    assert_eq!(c.include, ["apps/*.toml"]);
    assert_eq!(c.modifiers["Mu"].emulate.as_deref(), Some("C"));
    let k = &c.keymap.children;
    assert_eq!(k["C-f"], RawBinding::Short("Right".into()));
    assert!(matches!(&k["C-M-s"], RawBinding::Steps(s) if s.len() == 1));
    let RawBinding::Leaf(leaf) = &k["M-Tab"] else { panic!("{:?}", k["M-Tab"]) };
    assert!(leaf.keep_mods);
    assert_eq!(leaf.press, Some(Press::Tap));
    let RawBinding::Node(cx) = &k["C-x"] else { panic!() };
    assert_eq!(cx.options.as_ref().unwrap().on_mismatch, Some(Mismatch::Discard));
    let RawBinding::Node(t) = &cx.children["t"] else { panic!() };
    assert_eq!(t.children["0"], RawBinding::Short("close_tab".into()));
    assert_eq!(c.modes["mark"].keymap.children.len(), 1);
    let RawAction::ByTarget(by) = &c.actions["close_tab"] else { panic!() };
    assert_eq!(by.keys().collect::<Vec<_>>(), ["editor", "*"]);
    assert_eq!(c.actions["undo"], RawAction::Short("C-z".into()));
}

#[test]
fn serialize_round_trips() {
    let c = parse(SAMPLE).unwrap();
    let text = toml::to_string_pretty(&c).unwrap();
    assert_eq!(parse(&text).unwrap(), c, "{text}");
}

#[test]
fn rejects_malformed_input() {
    assert!(parse("typo = 1").is_err());
    assert!(parse("[keymap]\na = [{ keys = \"a\", text = \"b\" }]").is_err());
    assert!(parse("[keymap]\na = [{ run = \"a\", argz = [] }]").is_err());
    assert!(parse("[keymap]\na = 1").is_err());
    // A misspelled leaf option stays a leaf; the compiler reports it.
    let c = parse("[keymap]\na = { do = \"b\", pres = \"tap\" }").unwrap();
    assert!(matches!(&c.keymap.children["a"], RawBinding::Leaf(l) if l.unknown.contains_key("pres")));
    assert!(parse("[keymap.a.options]\ntypo = 1").is_err());
    assert!(parse("[actions]\na = 1").is_err());
}

#[test]
fn unknown_leaf_options_survive_serialization() {
    let c = parse("[keymap]\na = { do = \"b\", pres = \"tap\" }").unwrap();
    let back = parse(&toml::to_string(&c).unwrap()).unwrap();
    assert_eq!(back, c);
    let RawBinding::Leaf(l) = &back.keymap.children["a"] else { panic!() };
    assert!(l.unknown.contains_key("pres"));
}

#[test]
fn input_step_then_is_optional_and_has_a_position() {
    let c = parse("[actions]\nm = [{ input = \"M-x\", position = \"bottom\" }]\nn = [{ input = \"?\", then = \"m\" }]")
        .unwrap();
    let RawAction::Steps(m) = &c.actions["m"] else { panic!() };
    assert_eq!(m[0], RawStep::Input { input: "M-x".into(), then: None, position: Some(InputPosition::Bottom) });
    let RawAction::Steps(n) = &c.actions["n"] else { panic!() };
    assert_eq!(n[0], RawStep::Input { input: "?".into(), then: Some("m".into()), position: None });
    assert!(parse("[actions]\nm = [{ input = \"x\", position = \"left\" }]").is_err());
}
