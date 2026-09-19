//! Action and step editors, and the actions section.

use crate::Store;
use crate::fields::*;
use grapnel_schema::*;
use leptos::prelude::*;
use rust_i18n::t;

fn kinds() -> Options {
    vec![
        ("keys".into(), t!("ui.keys_or_action")),
        ("text".into(), t!("ui.step.text")),
        ("mouse_move".into(), t!("ui.step.mouse_move")),
        ("mouse_move_to".into(), t!("ui.step.mouse_move_to")),
        ("sleep".into(), t!("ui.step.sleep")),
        ("run".into(), t!("ui.step.run")),
        ("call".into(), t!("ui.step.call")),
        ("mode".into(), t!("ui.step.mode")),
        ("control".into(), t!("ui.step.control")),
        ("input".into(), t!("ui.step.input")),
    ]
}

fn positions() -> Options {
    vec![("".into(), t!("ui.step.center")), ("bottom".into(), t!("ui.step.bottom"))]
}

fn controls() -> Options {
    vec![
        ("suspend".into(), t!("ui.step.suspend")),
        ("reload".into(), t!("ui.step.reload")),
        ("exit".into(), t!("ui.step.exit")),
        ("repeat".into(), t!("ui.step.repeat")),
    ]
}

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
        "input" => RawStep::Input { input: String::new(), then: None, position: None },
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
            "keys" => keys_or_action(
                &t!("ui.keys_or_action"),
                p,
                |s| match s {
                    RawStep::Short(k) | RawStep::Keys { keys: k } => k.clone(),
                    _ => String::new(),
                },
                |s, x| *s = RawStep::Short(x),
            )
            .into_any(),
            "text" => text(
                &t!("ui.step.text_label"),
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
                String::new,
            )
            .into_any(),
            "run" => view! {
                {text("run", &p.at(".run"), |s| if let RawStep::Run { run, .. } = s { run.clone() } else { String::new() },
                    |s, x| if let RawStep::Run { run, .. } = s { *run = x })}
                <div class="break later"></div>
                <div class="later">{list("args", p.map(".args",
                    |s| if let RawStep::Run { args, .. } = s { Some(args) } else { None },
                    |s| if let RawStep::Run { args, .. } = s { Some(args) } else { None },
                ))}</div>
            }
            .into_any(),
            "call" => view! {
                {text("call", p, |s| if let RawStep::Call { call, .. } = s { call.clone() } else { String::new() },
                    |s, x| if let RawStep::Call { call, .. } = s { *call = x })}
                {opt_text("arg", &p.at(".arg"), |s| if let RawStep::Call { arg, .. } = s { arg.clone() } else { None },
                    |s, x| if let RawStep::Call { arg, .. } = s { *arg = x }, || t!("ui.hint.none").into_owned())}
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
                controls(),
                |s| {
                    match s {
                        RawStep::Control { control: ControlCmd::Reload } => "reload",
                        RawStep::Control { control: ControlCmd::Exit } => "exit",
                        RawStep::Control { control: ControlCmd::Repeat } => "repeat",
                        _ => "suspend",
                    }
                    .into()
                },
                |s, x| {
                    let control = match x.as_str() {
                        "reload" => ControlCmd::Reload,
                        "exit" => ControlCmd::Exit,
                        "repeat" => ControlCmd::Repeat,
                        _ => ControlCmd::Suspend,
                    };
                    *s = RawStep::Control { control }
                },
            )
            .into_any(),
            _ => view! {
                {text("prompt", &p.at(".input"), |s| if let RawStep::Input { input, .. } = s { input.clone() } else { String::new() },
                    |s, x| if let RawStep::Input { input, .. } = s { *input = x })}
                {select(&t!("ui.step.position"), p, positions(),
                    |s| match s { RawStep::Input { position: Some(InputPosition::Bottom), .. } => "bottom", _ => "" }.into(),
                    |s, x| if let RawStep::Input { position, .. } = s {
                        *position = (x == "bottom").then_some(InputPosition::Bottom)
                    })}
                <div class="break later"></div>
                <div class="later">{opt_text(&t!("ui.step.then"), p,
                    |s| if let RawStep::Input { then, .. } = s { then.clone() } else { None },
                    |s, x| if let RawStep::Input { then, .. } = s { *then = x }, || t!("ui.hint.typed_action").into_owned())}</div>
            }
            .into_any(),
        }
    }
}

/// An ordered list of steps with add/drag/remove.
pub fn steps_editor(p: Place<Vec<RawStep>>) -> AnyView {
    let (list, add) = (p.clone(), p.clone());
    let drag = RwSignal::new(None);
    let step = move |j: usize| {
        let sp = p.map(&format!("[{j}]"), move |v| v.get(j), move |v| v.get_mut(j));
        let body = view! {
            {select(&t!("ui.kind"), &sp, kinds(), kind, set_kind)}
            {step_fields(sp.clone())}
            {del_button(&p, j)}
        };
        sortable(&p, drag, move |_| Some(j), Vec::len, vec_move, "step", body)
    };
    view! {
        <div class="steps">
            <For each=move || indices(list.read(|v| v.len())) key=|j| *j let:j>
                {step(j)}
            </For>
            <button class="add" on:click=move |_| add.edit(|v| v.push(RawStep::Short(String::new())))>{t!("ui.add_step")}</button>
        </div>
    }
    .into_any()
}

fn steps_kinds() -> Options {
    vec![("keys".into(), t!("ui.keys_or_action")), ("steps".into(), t!("ui.steps"))]
}

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
            false => keys_or_action(
                &t!("ui.keys_or_action"),
                &p,
                |s| if let RawSteps::Short(k) = s { k.clone() } else { String::new() },
                |s, x| *s = RawSteps::Short(x),
            )
            .into_any(),
            true => steps_editor(p.map(
                "",
                |s| if let RawSteps::Steps(v) = s { Some(v) } else { None },
                |s| if let RawSteps::Steps(v) = s { Some(v) } else { None },
            )),
        }
    };
    view! { {select(&t!("ui.kind"), &p, steps_kinds(), kind, set_kind)} {fields} }.into_any()
}

fn action_kinds() -> Options {
    vec![
        ("short".into(), t!("ui.keys_or_action")),
        ("steps".into(), t!("ui.steps")),
        ("by_target".into(), t!("ui.action.by_target")),
    ]
}

pub fn action_kind(a: &RawAction) -> String {
    match a {
        RawAction::Short(_) => "short",
        RawAction::Steps(_) => "steps",
        RawAction::ByTarget(_) => "by_target",
    }
    .into()
}

pub fn set_action_kind(a: &mut RawAction, k: String) {
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

/// An action: keys or an action name, steps, or target → steps in order.
pub fn action_editor(p: Place<RawAction>) -> AnyView {
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
                keys_or_action(&t!("ui.keys_or_action"), &p, get, set).into_any()
            }
            "steps" => steps_editor(p.map(
                "",
                |a| if let RawAction::Steps(v) = a { Some(v) } else { None },
                |a| if let RawAction::Steps(v) = a { Some(v) } else { None },
            )),
            _ => by_target_editor(p.map(
                "",
                |a| if let RawAction::ByTarget(m) = a { Some(m) } else { None },
                |a| if let RawAction::ByTarget(m) = a { Some(m) } else { None },
            )),
        }
    };
    view! { {select(&t!("ui.kind"), &p, action_kinds(), action_kind, set_action_kind)} {fields} }.into_any()
}

/// Target name (`*` = always) → keys or steps, evaluated top to bottom.
pub fn by_target_editor(p: Place<IndexMap<String, RawSteps>>) -> AnyView {
    let (list, add) = (p.clone(), p.clone());
    let drag = RwSignal::new(None);
    let row = move |t: String| {
        let (a, b) = (t.clone(), t.clone());
        let sp = p.map(&format!(".\"{t}\""), move |m| m.get(&a), move |m| m.get_mut(&b));
        let (r, d, del, key, bad) = (p.clone(), p.clone(), t.clone(), t.clone(), sp.clone());
        let body = view! {
            {name_row(
                None,
                t,
                move || bad.key_problem(),
                move |old, new| { let mut ok = false; r.edit(|m| ok = rename_key(m, old, new)); ok },
                move || d.edit(|m| drop(m.shift_remove(&del))),
            )}
            {raw_steps_editor(sp)}
        };
        sortable(&p, drag, move |m| m.get_index_of(&key), IndexMap::len, IndexMap::move_index, "impl", body)
    };
    let add_row = move |_| {
        add.edit(|m| {
            let n = fresh_name("target", |n| m.contains_key(n));
            m.insert(n, RawSteps::Short(String::new()));
        })
    };
    view! {
        <p class="hint">{t!("ui.action.by_target_hint")}</p>
        <For each=move || list.read(|m| m.keys().cloned().collect::<Vec<_>>()) key=|t| t.clone() let:t>
            {row(t)}
        </For>
        <button class="add" on:click=add_row>{t!("ui.action.add_target")}</button>
    }
    .into_any()
}

pub fn actions() -> impl IntoView {
    let store = use_context::<Store>().expect("store");
    let all =
        Place::<IndexMap<String, RawAction>>::new(store, "actions", |c| Some(&c.actions), |c| Some(&mut c.actions));
    let (list, add) = (all.clone(), all.clone());
    let drag = RwSignal::new(None);
    let row = move |name: String| {
        let (a, b) = (name.clone(), name.clone());
        let ap = all.map(&format!(".{name}"), move |m| m.get(&a), move |m| m.get_mut(&b));
        let (r, d, del, bad, key) = (all.clone(), all.clone(), name.clone(), ap.clone(), name.clone());
        let body = view! {
            {name_row(
                Some(t!("ui.name.action").into_owned()),
                name,
                move || bad.key_problem(),
                move |old, new| { let mut ok = false; r.edit(|m| ok = rename_key(m, old, new)); ok },
                move || d.edit(|m| drop(m.shift_remove(&del))),
            )}
            {action_editor(ap)}
        };
        sortable(&all, drag, move |m| m.get_index_of(&key), IndexMap::len, IndexMap::move_index, "row", body)
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
            <button class="add" on:click=add_action>{t!("ui.action.add")}</button>
        </section>
    }
}
