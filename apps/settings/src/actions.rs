//! Action section: implementations (`when`) and their steps.

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

fn impl_view(store: Store, name: String, i: usize) -> impl IntoView {
    let (a, b) = (name.clone(), name.clone());
    let p = Place::<RawActionImpl>::new(
        store,
        move |c| c.actions.get(&a)?.get(i),
        move |c| c.actions.get_mut(&b)?.get_mut(i),
    );
    let (ps, pd) = (p.clone(), p.clone());
    let (del, steps) = (name.clone(), p.clone());
    let step = move |j: usize| {
        let (a, b) = (name.clone(), name.clone());
        let sp = Place::<RawStep>::new(
            store,
            move |c| c.actions.get(&a)?.get(i)?.steps.get(j),
            move |c| c.actions.get_mut(&b)?.get_mut(i)?.steps.get_mut(j),
        );
        let pd = pd.clone();
        view! {
            <div class="step">
                {select("種類", &sp, KINDS, kind, set_kind)}
                {step_fields(sp.clone())}
                <button class="del" on:click=move |_| pd.edit(|m| drop(m.steps.remove(j)))>"×"</button>
            </div>
        }
    };
    view! {
        <div class="impl">
            <button class="del" on:click=move |_| store.edit(|c| drop(c.actions.get_mut(&del).map(|v| v.remove(i))))>
                "実装を削除"
            </button>
            {list("when (ターゲット)", &p, |m| m.when.clone(), |m, x| m.when = x)}
            <For each=move || indices(steps.read(|m| m.steps.len())) key=|j| *j let:j>
                {step(j)}
            </For>
            <button on:click=move |_| ps.edit(|m| m.steps.push(RawStep::Short(String::new())))>"手順を追加"</button>
        </div>
    }
}

fn action_names(c: &RawConfig) -> Vec<String> {
    c.actions.keys().cloned().collect()
}

pub fn actions() -> impl IntoView {
    let store = use_context::<Store>().expect("store");
    let add = move |_| {
        store.edit(|c| {
            let n = fresh_name("action", |n| c.actions.contains_key(n));
            c.actions.insert(n, vec![RawActionImpl::default()]);
        })
    };
    view! {
        <section>
            <For each=move || store.read(action_names) key=|n| n.clone() let:name>
                <div class="row">
                    {
                        let (n1, n2, n3) = (name.clone(), name.clone(), name.clone());
                        view! {
                    <ActionName name=n1 />
                    <For
                        each=move || indices(store.read(|c| c.actions.get(&n2).map_or(0, Vec::len)))
                        key=|i| *i
                        let:i
                    >
                        {impl_view(store, name.clone(), i)}
                    </For>
                    <button on:click=move |_| store.edit(|c| c.actions.entry(n3.clone()).or_default().push(RawActionImpl::default()))>
                        "実装を追加"
                    </button>
                        }
                    }
                </div>
            </For>
            <button on:click=add>"アクションを追加"</button>
        </section>
    }
}

/// Rename/delete for an action (all implementations move together).
#[component]
fn ActionName(name: String) -> impl IntoView {
    let store = use_context::<Store>().expect("store");
    let (old, del) = (name.clone(), name.clone());
    let rename = move |new: String| {
        store.edit(|c| {
            if !new.is_empty()
                && !c.actions.contains_key(&new)
                && let Some(v) = c.actions.remove(&old)
            {
                c.actions.insert(new, v);
            }
        })
    };
    view! {
        <div class="name">
            <input prop:value=name on:change=move |ev| rename(event_target_value(&ev)) />
            <button class="del" on:click=move |_| store.edit(|c| drop(c.actions.remove(&del)))>"削除"</button>
        </div>
    }
}
