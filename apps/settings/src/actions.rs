//! Action and step editors, and the actions section.

use crate::Store;
use crate::fields::*;
use grapnel_schema::*;
use leptos::prelude::*;

const KINDS: &[(&str, &str)] = &[
    ("keys", "キー入力"),
    ("text", "文字入力"),
    ("mouse_move", "マウス相対移動"),
    ("mouse_move_to", "マウス絶対移動"),
    ("sleep", "待機"),
    ("run", "プログラム起動"),
    ("call", "アクション呼び出し"),
    ("mode", "モード切替"),
    ("control", "常駐側への指示"),
    ("input", "入力欄"),
];
const CONTROLS: &[(&str, &str)] = &[("suspend", "一時停止の切替"), ("reload", "再読み込み"), ("exit", "終了")];

fn kind(s: &RawStep) -> String {
    match s {
        RawStep::Short(_) | RawStep::Keys { .. } => "keys",
        RawStep::Text { .. } => "text",
        RawStep::MouseMove { .. } => "mouse_move",
        RawStep::MouseMoveTo { .. } => "mouse_move_to",
        RawStep::Sleep { .. } => "sleep",
        RawStep::Run { .. } => "run",
        RawStep::Call { .. } => "call",
        RawStep::Mode { .. } => "mode",
        RawStep::Control { .. } => "control",
        RawStep::Input { .. } => "input",
    }
    .into()
}

fn set_kind(s: &mut RawStep, k: String) {
    *s = match k.as_str() {
        "text" => RawStep::Text { text: String::new() },
        "mouse_move" => RawStep::MouseMove { mouse_move: [0, 0] },
        "mouse_move_to" => RawStep::MouseMoveTo { mouse_move_to: [0, 0] },
        "sleep" => RawStep::Sleep { sleep: 100 },
        "run" => RawStep::Run { run: String::new(), args: vec![] },
        "call" => RawStep::Call { call: String::new(), arg: None },
        "mode" => RawStep::Mode { mode: String::new() },
        "control" => RawStep::Control { control: ControlCmd::Suspend },
        "input" => RawStep::Input { input: String::new(), then: String::new() },
        _ => RawStep::Short(String::new()),
    }
}

fn pair(v: [i32; 2]) -> String {
    format!("{}, {}", v[0], v[1])
}

fn parse_pair(s: &str) -> Option<[i32; 2]> {
    let (a, b) = s.split_once(',')?;
    Some([a.trim().parse().ok()?, b.trim().parse().ok()?])
}

/// Fields for the step's current kind. Only rebuilt when the kind changes.
fn step_fields(p: Place<RawStep>) -> impl IntoView {
    let k = {
        let p = p.clone();
        Memo::new(move |_| p.read(kind))
    };
    move || {
        let p = &p;
        match k.get().as_str() {
            "keys" => keys(
                "keys",
                p,
                |s| match s {
                    RawStep::Short(k) | RawStep::Keys { keys: k } => k.clone(),
                    _ => String::new(),
                },
                |s, x| *s = RawStep::Short(x),
                false,
            )
            .into_any(),
            "text" => text(
                "text ({arg} で引数)",
                p,
                |s| if let RawStep::Text { text } = s { text.clone() } else { String::new() },
                |s, x| *s = RawStep::Text { text: x },
            )
            .into_any(),
            "mouse_move" => text(
                "dx, dy",
                p,
                |s| if let RawStep::MouseMove { mouse_move } = s { pair(*mouse_move) } else { String::new() },
                |s, x| {
                    if let (RawStep::MouseMove { mouse_move }, Some(v)) = (s, parse_pair(&x)) {
                        *mouse_move = v
                    }
                },
            )
            .into_any(),
            "mouse_move_to" => text(
                "x, y",
                p,
                |s| if let RawStep::MouseMoveTo { mouse_move_to } = s { pair(*mouse_move_to) } else { String::new() },
                |s, x| {
                    if let (RawStep::MouseMoveTo { mouse_move_to }, Some(v)) = (s, parse_pair(&x)) {
                        *mouse_move_to = v
                    }
                },
            )
            .into_any(),
            "sleep" => num(
                "ms",
                p,
                |s| if let RawStep::Sleep { sleep } = s { Some(*sleep) } else { None },
                |s, x| *s = RawStep::Sleep { sleep: x.unwrap_or(0) },
            )
            .into_any(),
            "run" => view! {
                {text("run", p, |s| if let RawStep::Run { run, .. } = s { run.clone() } else { String::new() },
                    |s, x| if let RawStep::Run { run, .. } = s { *run = x })}
                {list("args", p, |s| if let RawStep::Run { args, .. } = s { args.clone() } else { vec![] },
                    |s, x| if let RawStep::Run { args, .. } = s { *args = x })}
            }
            .into_any(),
            "call" => view! {
                {text("call", p, |s| if let RawStep::Call { call, .. } = s { call.clone() } else { String::new() },
                    |s, x| if let RawStep::Call { call, .. } = s { *call = x })}
                {opt_text("arg", p, |s| if let RawStep::Call { arg, .. } = s { arg.clone() } else { None },
                    |s, x| if let RawStep::Call { arg, .. } = s { *arg = x })}
            }
            .into_any(),
            "mode" => text(
                "mode",
                p,
                |s| if let RawStep::Mode { mode } = s { mode.clone() } else { String::new() },
                |s, x| *s = RawStep::Mode { mode: x },
            )
            .into_any(),
            "control" => select(
                "control",
                p,
                CONTROLS,
                |s| {
                    match s {
                        RawStep::Control { control: ControlCmd::Reload } => "reload",
                        RawStep::Control { control: ControlCmd::Exit } => "exit",
                        _ => "suspend",
                    }
                    .into()
                },
                |s, x| {
                    let control = match x.as_str() {
                        "reload" => ControlCmd::Reload,
                        "exit" => ControlCmd::Exit,
                        _ => ControlCmd::Suspend,
                    };
                    *s = RawStep::Control { control }
                },
            )
            .into_any(),
            _ => view! {
                {text("prompt", p, |s| if let RawStep::Input { input, .. } = s { input.clone() } else { String::new() },
                    |s, x| if let RawStep::Input { input, .. } = s { *input = x })}
                {text("then", p, |s| if let RawStep::Input { then, .. } = s { then.clone() } else { String::new() },
                    |s, x| if let RawStep::Input { then, .. } = s { *then = x })}
            }
            .into_any(),
        }
    }
}

/// An ordered list of steps with add/remove.
pub fn steps_editor(p: Place<Vec<RawStep>>) -> AnyView {
    let (list, add) = (p.clone(), p.clone());
    let step = move |j: usize| {
        let sp = p.map(move |v| v.get(j), move |v| v.get_mut(j));
        let del = p.clone();
        view! {
            <div class="step">
                {select("種類", &sp, KINDS, kind, set_kind)}
                {step_fields(sp.clone())}
                <button class="del" on:click=move |_| del.edit(|v| drop(v.remove(j)))>"×"</button>
            </div>
        }
    };
    view! {
        <div class="steps">
            <For each=move || indices(list.read(|v| v.len())) key=|j| *j let:j>
                {step(j)}
            </For>
            <button on:click=move |_| add.edit(|v| v.push(RawStep::Short(String::new())))>"手順を追加"</button>
        </div>
    }
    .into_any()
}

const STEPS_KINDS: &[(&str, &str)] = &[("keys", "キー"), ("steps", "手順")];

/// Keys (a string) or a list of steps.
fn raw_steps_editor(p: Place<RawSteps>) -> AnyView {
    let k = {
        let p = p.clone();
        Memo::new(move |_| p.read(|s| matches!(s, RawSteps::Steps(_))))
    };
    let set_kind = |s: &mut RawSteps, k: String| {
        *s = match (k.as_str(), std::mem::replace(s, RawSteps::Short(String::new()))) {
            ("steps", RawSteps::Short(keys)) => RawSteps::Steps(vec![RawStep::Short(keys)]),
            ("keys", RawSteps::Steps(_)) => RawSteps::Short(String::new()),
            (_, same) => same,
        }
    };
    let kind = |s: &RawSteps| if matches!(s, RawSteps::Steps(_)) { "steps" } else { "keys" }.to_string();
    let fields = {
        let p = p.clone();
        move || match k.get() {
            false => keys(
                "キー",
                &p,
                |s| if let RawSteps::Short(k) = s { k.clone() } else { String::new() },
                |s, x| *s = RawSteps::Short(x),
                false,
            )
            .into_any(),
            true => steps_editor(p.map(
                |s| if let RawSteps::Steps(v) = s { Some(v) } else { None },
                |s| if let RawSteps::Steps(v) = s { Some(v) } else { None },
            )),
        }
    };
    view! { {select("種類", &p, STEPS_KINDS, kind, set_kind)} {fields} }.into_any()
}

const ACTION_KINDS: &[(&str, &str)] =
    &[("short", "キー / アクション名"), ("steps", "手順"), ("by_target", "ターゲットごと")];

fn action_kind(a: &RawAction) -> String {
    match a {
        RawAction::Short(_) => "short",
        RawAction::Steps(_) => "steps",
        RawAction::ByTarget(_) => "by_target",
    }
    .into()
}

fn set_action_kind(a: &mut RawAction, k: String) {
    *a = match (k.as_str(), std::mem::replace(a, RawAction::Short(String::new()))) {
        ("steps", RawAction::Short(s)) => RawAction::Steps(vec![RawStep::Short(s)]),
        ("steps", RawAction::Steps(v)) => RawAction::Steps(v),
        ("by_target", RawAction::Short(s)) => RawAction::ByTarget(IndexMap::from([("*".into(), RawSteps::Short(s))])),
        ("by_target", RawAction::Steps(v)) => RawAction::ByTarget(IndexMap::from([("*".into(), RawSteps::Steps(v))])),
        ("by_target", by @ RawAction::ByTarget(_)) => by,
        ("short", RawAction::Short(s)) => RawAction::Short(s),
        _ => RawAction::Short(String::new()),
    }
}

/// An action: keys (or, in a keymap, an action name), steps, or target → steps in order.
pub fn action_editor(p: Place<RawAction>, in_keymap: bool) -> AnyView {
    let k = {
        let p = p.clone();
        Memo::new(move |_| p.read(action_kind))
    };
    let fields = {
        let p = p.clone();
        move || match k.get().as_str() {
            "short" => {
                let get = |a: &RawAction| if let RawAction::Short(s) = a { s.clone() } else { String::new() };
                let set = |a: &mut RawAction, x| *a = RawAction::Short(x);
                if in_keymap {
                    keys_or_action("キー / アクション名", &p, get, set).into_any()
                } else {
                    keys("キー", &p, get, set, false).into_any()
                }
            }
            "steps" => steps_editor(p.map(
                |a| if let RawAction::Steps(v) = a { Some(v) } else { None },
                |a| if let RawAction::Steps(v) = a { Some(v) } else { None },
            )),
            _ => by_target_editor(p.map(
                |a| if let RawAction::ByTarget(m) = a { Some(m) } else { None },
                |a| if let RawAction::ByTarget(m) = a { Some(m) } else { None },
            )),
        }
    };
    view! { {select("種類", &p, ACTION_KINDS, action_kind, set_action_kind)} {fields} }.into_any()
}

/// Target name (`*` = always) → keys or steps, evaluated top to bottom.
fn by_target_editor(p: Place<IndexMap<String, RawSteps>>) -> AnyView {
    let (list, add) = (p.clone(), p.clone());
    let row = move |t: String| {
        let (a, b) = (t.clone(), t.clone());
        let sp = p.map(move |m| m.get(&a), move |m| m.get_mut(&b));
        let (r, d, del) = (p.clone(), p.clone(), t.clone());
        view! {
            <div class="impl">
                {name_row(
                    t,
                    move |old, new| { let mut ok = false; r.edit(|m| ok = rename_key(m, old, new)); ok },
                    move || d.edit(|m| drop(m.shift_remove(&del))),
                )}
                {raw_steps_editor(sp)}
            </div>
        }
    };
    let add_row = move |_| {
        add.edit(|m| {
            let n = fresh_name("target", |n| m.contains_key(n));
            m.insert(n, RawSteps::Short(String::new()));
        })
    };
    view! {
        <p class="hint">"上から順に評価し、最初に合ったターゲットの出力を使う (* は常に合う)。"</p>
        <For each=move || list.read(|m| m.keys().cloned().collect::<Vec<_>>()) key=|t| t.clone() let:t>
            {row(t)}
        </For>
        <button on:click=add_row>"ターゲットを追加"</button>
    }
    .into_any()
}

pub fn actions() -> impl IntoView {
    let store = use_context::<Store>().expect("store");
    let all = Place::<IndexMap<String, RawAction>>::new(store, |c| Some(&c.actions), |c| Some(&mut c.actions));
    let (list, add) = (all.clone(), all.clone());
    let row = move |name: String| {
        let (a, b) = (name.clone(), name.clone());
        let ap = all.map(move |m| m.get(&a), move |m| m.get_mut(&b));
        let (r, d, del) = (all.clone(), all.clone(), name.clone());
        view! {
            <div class="row">
                {name_row(
                    name,
                    move |old, new| { let mut ok = false; r.edit(|m| ok = rename_key(m, old, new)); ok },
                    move || d.edit(|m| drop(m.shift_remove(&del))),
                )}
                {action_editor(ap, false)}
            </div>
        }
    };
    let add_action = move |_| {
        add.edit(|m| {
            let n = fresh_name("action", |n| m.contains_key(n));
            m.insert(n, RawAction::Short(String::new()));
        })
    };
    view! {
        <section>
            <For each=move || list.read(|m| m.keys().cloned().collect::<Vec<_>>()) key=|n| n.clone() let:name>
                {row(name)}
            </For>
            <button on:click=add_action>"アクションを追加"</button>
        </section>
    }
}
