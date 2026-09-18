use super::*;

const SAMPLE: &str = r#"
include = ["apps/*.toml"]

[settings]
initial_mode = "insert"
passthrough = ["games"]
suspend_hotkey = "C-M-S-p"
gesture_threshold = 30

[modes.insert]
[modes.normal]
block_unmapped = true

[modifiers.Mu]
key = "Muhenkan"
tap = "Muhenkan"
tap_timeout_ms = 300

[targets.editor]
app = "code.exe"
[targets.games]
any = ["steam_game", "emulator"]

[[rules]]
keys = "C-x t 0"
action = "close_tab"
targets = ["editor"]
on_mismatch = "discard"
timeout_ms = 1500

[[rules]]
keys = "Mu-j"
action = "down"
press = "tap"

[[actions.close_tab]]
when = ["editor"]
do = ["C-w", { text = "{arg}" }, { sleep = 10 }, { mouse_move = [1, -1] }]

[[actions.search]]
do = [{ input = "検索", then = "open" }, { run = "cmd", args = ["/c"] }, { call = "x" }, { mode = "normal" }, { control = "reload" }]
"#;

fn parse(s: &str) -> Result<RawConfig, toml::de::Error> {
    toml::from_str(s)
}

#[test]
fn parses_spec_sample() {
    let c = parse(SAMPLE).unwrap();
    assert_eq!(c.include, ["apps/*.toml"]);
    assert_eq!(c.settings.as_ref().unwrap().gesture_threshold, Some(30));
    assert!(c.modes["normal"].block_unmapped);
    assert_eq!(c.modifiers["Mu"].tap_timeout_ms, Some(300));
    assert_eq!(c.targets["games"].any, ["steam_game", "emulator"]);
    assert_eq!(c.rules[0].on_mismatch, Some(Mismatch::Discard));
    assert_eq!(c.rules[1].press, Some(Press::Tap));
    let steps = &c.actions["close_tab"][0].steps;
    assert_eq!(steps[0], RawStep::Short("C-w".into()));
    assert_eq!(steps[3], RawStep::MouseMove { mouse_move: [1, -1] });
    let steps = &c.actions["search"][0].steps;
    assert_eq!(steps[0], RawStep::Input { input: "検索".into(), then: "open".into() });
    assert_eq!(steps[2], RawStep::Call { call: "x".into(), arg: None });
    assert_eq!(steps[4], RawStep::Control { control: ControlCmd::Reload });
}

#[test]
fn serialize_round_trips() {
    let c = parse(SAMPLE).unwrap();
    let text = toml::to_string_pretty(&c).unwrap();
    assert_eq!(parse(&text).unwrap(), c);
}

#[test]
fn rejects_unknown_fields() {
    assert!(parse("[[rules]]\nkeys = \"a\"\naction = \"b\"\ntypo = 1").is_err());
    assert!(parse("[[actions.a]]\ndo = [{ keys = \"a\", text = \"b\" }]").is_err());
    assert!(parse("[[actions.a]]\ndo = [{ run = \"a\", argz = [] }]").is_err());
    assert!(parse("[[rules]]\nkeys = \"a\"\naction = \"b\"\npress = \"long\"").is_err());
}
