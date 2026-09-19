//! Keymap tree editor: each chord runs keys/an action name, steps or target-keyed steps (with
//! optional extended options), or is a subtree.

use crate::actions::{action_kind, by_target_editor, set_action_kind, steps_editor};
use crate::fields::*;
use grapnel_schema::*;
use leptos::prelude::*;
use rust_i18n::t;

/// A binding runs keys/an action name, steps or target-keyed steps, or is a subtree. Extended
/// options are a separate switch (they make the binding a table with `do`).
fn binding_kinds() -> Options {
    vec![
        ("short".into(), t!("ui.keys_or_action")),
        ("steps".into(), t!("ui.steps")),
        ("by_target".into(), t!("ui.action.by_target")),
        ("node".into(), t!("ui.keymap.node")),
    ]
}

fn press() -> Options {
    vec![("".into(), t!("ui.keymap.press_default")), ("hold".into(), "hold".into()), ("tap".into(), "tap".into())]
}

/// `root`: nothing to inherit, so unset means the built-in `replay`.
fn mismatch(root: bool) -> Options {
    let own = ["replay", "discard", "fallback"].map(|m| (m.into(), m.into()));
    let unset = if root { t!("ui.keymap.mismatch_default") } else { t!("ui.keymap.inherit") };
    std::iter::once(("".into(), unset)).chain(own).collect()
}

fn binding_kind(b: &RawBinding) -> String {
    match b {
        RawBinding::Short(_) => "short".into(),
        RawBinding::Steps(_) => "steps".into(),
        RawBinding::Leaf(l) => action_kind(&l.action),
        RawBinding::Node(_) => "node".into(),
    }
}

fn leaf(action: RawAction) -> RawLeaf {
    let unknown = IndexMap::new();
    RawLeaf { action, press: None, fallback: None, keep_mods: false, repeat: false, targets: vec![], unknown }
}

/// Without extended options: the bare action where it can stand alone (target-keyed steps cannot).
fn plain(l: RawLeaf) -> RawBinding {
    match l.action {
        RawAction::Short(s) => RawBinding::Short(s),
        RawAction::Steps(v) => RawBinding::Steps(v),
        a => RawBinding::Leaf(RawLeaf { action: a, ..l }),
    }
}

/// Switches the kind, keeping the action where possible and the extended options if any.
fn set_binding_kind(b: &mut RawBinding, k: String) {
    if k == "node" {
        if !matches!(b, RawBinding::Node(_)) {
            *b = RawBinding::Node(RawNode::default());
        }
        return;
    }
    let extended = matches!(b, RawBinding::Leaf(_));
    let mut l = match std::mem::replace(b, RawBinding::Short(String::new())) {
        RawBinding::Leaf(l) => l,
        RawBinding::Short(s) => leaf(RawAction::Short(s)),
        RawBinding::Steps(v) => leaf(RawAction::Steps(v)),
        RawBinding::Node(_) => leaf(RawAction::Short(String::new())),
    };
    set_action_kind(&mut l.action, k);
    *b = if extended { RawBinding::Leaf(l) } else { plain(l) };
}

/// Turns extended options on (a table with `do`) or off (dropping them).
fn set_extended(b: &mut RawBinding, on: bool) {
    *b = match (on, std::mem::replace(b, RawBinding::Short(String::new()))) {
        (true, RawBinding::Short(s)) => RawBinding::Leaf(leaf(RawAction::Short(s))),
        (true, RawBinding::Steps(v)) => RawBinding::Leaf(leaf(RawAction::Steps(v))),
        (false, RawBinding::Leaf(l)) => plain(l),
        (_, same) => same,
    }
}

/// The extended options, folded, with their on/off `switch` next to the fold toggle. Without them
/// (`p` is then empty) the fields are disabled and show the defaults.
fn leaf_options(
    p: Place<RawLeaf>,
    switch: impl IntoView + 'static,
    off: impl Fn() -> bool + Send + Sync + 'static,
) -> impl IntoView {
    view! {
        <details class="options">
            <summary>{t!("ui.keymap.extended")}{switch}</summary>
            <fieldset disabled=off>
                {select("press", &p, press(),
                    |l| match l.press { None => "", Some(Press::Hold) => "hold", Some(Press::Tap) => "tap" }.into(),
                    |l, x| l.press = match x.as_str() { "hold" => Some(Press::Hold), "tap" => Some(Press::Tap), _ => None })}
                {opt_keys(&t!("ui.keymap.fallback"), &p.at(".fallback"), |l| l.fallback.clone(), |l, x| l.fallback = x, || t!("ui.hint.original_input").into_owned())}
                {check(&t!("ui.keymap.fallback_none"), &p.at(".fallback"), |l| l.fallback.as_deref() == Some(""), |l, x| l.fallback = x.then(String::new))}
                {check(&t!("ui.keymap.keep_mods"), &p.at(".keep_mods"), |l| l.keep_mods, |l, x| l.keep_mods = x)}
                {check(&t!("ui.keymap.repeat"), &p.at(".repeat"), |l| l.repeat, |l, x| l.repeat = x)}
                {list(&t!("ui.keymap.targets"), p.map(".targets", |l| Some(&l.targets), |l| Some(&mut l.targets)))}
            </fieldset>
        </details>
    }
}

/// `root`: the top of a keymap, which inherits nothing.
fn options_editor(p: Place<RawNode>, root: bool) -> AnyView {
    let hint = move |top: &'static str| move || if root { t!(top) } else { t!("ui.hint.inherit") }.into_owned();
    let o = p.map(".options", |n| n.options.as_ref(), |n| Some(n.options.get_or_insert_default()));
    view! {
        <details class="options">
            <summary>{t!("ui.keymap.options")}</summary>
            <fieldset>
            {select("on_mismatch", &o, mismatch(root),
                |o| match o.on_mismatch {
                    None => "", Some(Mismatch::Replay) => "replay", Some(Mismatch::Discard) => "discard", Some(Mismatch::Fallback) => "fallback",
                }.into(),
                |o, x| o.on_mismatch = match x.as_str() {
                    "replay" => Some(Mismatch::Replay), "discard" => Some(Mismatch::Discard), "fallback" => Some(Mismatch::Fallback), _ => None,
                })}
            {num("timeout_ms", &o.at(".timeout_ms"), |o| o.timeout_ms, |o, x| o.timeout_ms = x, hint("ui.hint.unlimited"))}
            {opt_keys("fallback", &o.at(".fallback"), |o| o.fallback.clone(), |o, x| o.fallback = x, hint("ui.hint.replay"))}
            </fieldset>
        </details>
    }
    .into_any()
}

fn binding_editor(p: Place<RawBinding>) -> AnyView {
    let k = {
        let p = p.clone();
        Memo::new(move |_| p.read(binding_kind))
    };
    let is_leaf = {
        let p = p.clone();
        Memo::new(move |_| p.read(|b| matches!(b, RawBinding::Leaf(_))))
    };
    // The value edits the binding's own string or steps, or its `do` when it has extended options.
    let fields = {
        let p = p.clone();
        move || match k.get().as_str() {
            "short" => keys_or_action(
                &t!("ui.keys_or_action"),
                &p,
                |b| match b {
                    RawBinding::Short(s) | RawBinding::Leaf(RawLeaf { action: RawAction::Short(s), .. }) => s.clone(),
                    _ => String::new(),
                },
                |b, x| match b {
                    RawBinding::Leaf(l) => l.action = RawAction::Short(x),
                    _ => *b = RawBinding::Short(x),
                },
            )
            .into_any(),
            "steps" => steps_editor(p.map(
                "",
                |b| match b {
                    RawBinding::Steps(v) | RawBinding::Leaf(RawLeaf { action: RawAction::Steps(v), .. }) => Some(v),
                    _ => None,
                },
                |b| match b {
                    RawBinding::Steps(v) | RawBinding::Leaf(RawLeaf { action: RawAction::Steps(v), .. }) => Some(v),
                    _ => None,
                },
            )),
            "by_target" => by_target_editor(p.map(
                "",
                |b| match b {
                    RawBinding::Leaf(RawLeaf { action: RawAction::ByTarget(m), .. }) => Some(m),
                    _ => None,
                },
                |b| match b {
                    RawBinding::Leaf(RawLeaf { action: RawAction::ByTarget(m), .. }) => Some(m),
                    _ => None,
                },
            )),
            _ => node_editor(
                p.map(
                    "",
                    |b| if let RawBinding::Node(n) = b { Some(n) } else { None },
                    |b| if let RawBinding::Node(n) = b { Some(n) } else { None },
                ),
                false,
            ),
        }
    };
    let leaf_place = p.map(
        "",
        |b| if let RawBinding::Leaf(l) = b { Some(l) } else { None },
        |b| if let RawBinding::Leaf(l) = b { Some(l) } else { None },
    );
    let ext = p.clone();
    let options = move || {
        let ext = ext.clone();
        // Target-keyed steps exist only as extended options, so the switch stays on for them.
        let switch = view! {
            <input type="checkbox" class="extended" title=t!("ui.keymap.extended") aria-label=t!("ui.keymap.extended")
                prop:checked=move || is_leaf.get() disabled=move || k.get() == "by_target"
                on:change=move |ev| ext.edit(|b| set_extended(b, event_target_checked(&ev))) />
        };
        (k.get() != "node").then(|| leaf_options(leaf_place.clone(), switch, move || !is_leaf.get()))
    };
    view! { {select(&t!("ui.kind"), &p, binding_kinds(), binding_kind, set_binding_kind)} {fields} {options} }
        .into_any()
}

/// A keymap node: its options and one row per chord, recursively.
pub fn node_editor(p: Place<RawNode>, root: bool) -> AnyView {
    let (list, add, opts) = (p.clone(), p.clone(), p.clone());
    let row = move |key: String| {
        let (a, b) = (key.clone(), key.clone());
        let bp = p.map(&format!(".\"{key}\""), move |n| n.children.get(&a), move |n| n.children.get_mut(&b));
        let (r, d, del, bad) = (p.clone(), p.clone(), key.clone(), bp.clone());
        view! {
            <div class="binding">
                {name_row(
                    None,
                    key,
                    move || bad.key_problem(),
                    move |old, new| { let mut ok = false; r.edit(|n| ok = rename_key(&mut n.children, old, new)); ok },
                    move || d.edit(|n| drop(n.children.shift_remove(&del))),
                )}
                {binding_editor(bp)}
            </div>
        }
    };
    let add_row = move |_| {
        // An empty key is reported by validation until the user types the chord.
        add.edit(|n| {
            n.children.entry(String::new()).or_insert(RawBinding::Short(String::new()));
        })
    };
    view! {
        <div class="node">
            {options_editor(opts, root)}
            <For each=move || list.read(|n| n.children.keys().cloned().collect::<Vec<_>>()) key=|k| k.clone() let:k>
                {row(k)}
            </For>
            <button class="add" on:click=add_row>{t!("ui.keymap.add_key")}</button>
        </div>
    }
    .into_any()
}
