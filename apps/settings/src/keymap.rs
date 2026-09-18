//! Keymap tree editor: each chord is a key/action name, steps, a leaf with options, or a subtree.

use crate::actions::{action_editor, steps_editor};
use crate::fields::*;
use grapnel_schema::*;
use leptos::prelude::*;

const BINDING_KINDS: &[(&str, &str)] =
    &[("short", "キー / アクション名"), ("steps", "手順"), ("leaf", "設定付き"), ("node", "子の節 (続けて押す)")];
const PRESS: &[(&str, &str)] = &[("", "既定 (hold)"), ("hold", "hold"), ("tap", "tap")];
const MISMATCH: &[(&str, &str)] =
    &[("", "親と同じ"), ("replay", "replay"), ("discard", "discard"), ("fallback", "fallback")];

fn binding_kind(b: &RawBinding) -> String {
    match b {
        RawBinding::Short(_) => "short",
        RawBinding::Steps(_) => "steps",
        RawBinding::Leaf(_) => "leaf",
        RawBinding::Node(_) => "node",
    }
    .into()
}

fn leaf(action: RawAction) -> RawBinding {
    let unknown = IndexMap::new();
    RawBinding::Leaf(RawLeaf { action, press: None, fallback: None, keep_mods: false, targets: vec![], unknown })
}

/// Switches the kind, keeping what can be kept (e.g. a key string becomes the leaf's `do`).
fn set_binding_kind(b: &mut RawBinding, k: String) {
    *b = match (k.as_str(), std::mem::replace(b, RawBinding::Short(String::new()))) {
        ("short", RawBinding::Leaf(RawLeaf { action: RawAction::Short(s), .. })) => RawBinding::Short(s),
        ("short", RawBinding::Short(s)) => RawBinding::Short(s),
        ("steps", RawBinding::Short(s)) => RawBinding::Steps(vec![RawStep::Short(s)]),
        ("steps", RawBinding::Steps(v)) => RawBinding::Steps(v),
        ("leaf", RawBinding::Short(s)) => leaf(RawAction::Short(s)),
        ("leaf", RawBinding::Steps(v)) => leaf(RawAction::Steps(v)),
        ("leaf", l @ RawBinding::Leaf(_)) => l,
        ("node", n @ RawBinding::Node(_)) => n,
        ("node", _) => RawBinding::Node(RawNode::default()),
        ("steps", _) => RawBinding::Steps(vec![]),
        ("leaf", _) => leaf(RawAction::Short(String::new())),
        _ => RawBinding::Short(String::new()),
    }
}

fn leaf_editor(p: Place<RawLeaf>) -> AnyView {
    let action = p.map(|l| Some(&l.action), |l| Some(&mut l.action));
    view! {
        {action_editor(action, true)}
        <div class="options">
            {select("press", &p, PRESS,
                |l| match l.press { None => "", Some(Press::Hold) => "hold", Some(Press::Tap) => "tap" }.into(),
                |l, x| l.press = match x.as_str() { "hold" => Some(Press::Hold), "tap" => Some(Press::Tap), _ => None })}
            {opt_keys("fallback (空欄 = 元の入力)", &p, |l| l.fallback.clone(), |l, x| l.fallback = x)}
            {check("fallback で何も送らない", &p, |l| l.fallback.as_deref() == Some(""), |l, x| l.fallback = x.then(String::new))}
            {check("keep_mods (入力の修飾キーを離すまで出力の修飾キーを保持)", &p, |l| l.keep_mods, |l, x| l.keep_mods = x)}
            {list("targets (入力を捕まえる条件)", &p, |l| l.targets.clone(), |l, x| l.targets = x)}
        </div>
    }
    .into_any()
}

fn options_editor(p: Place<RawNode>) -> AnyView {
    let o = p.map(|n| n.options.as_ref(), |n| Some(n.options.get_or_insert_default()));
    view! {
        <details class="options">
            <summary>"節の設定 (子孫に引き継ぐ)"</summary>
            {select("on_mismatch", &o, MISMATCH,
                |o| match o.on_mismatch {
                    None => "", Some(Mismatch::Replay) => "replay", Some(Mismatch::Discard) => "discard", Some(Mismatch::Fallback) => "fallback",
                }.into(),
                |o, x| o.on_mismatch = match x.as_str() {
                    "replay" => Some(Mismatch::Replay), "discard" => Some(Mismatch::Discard), "fallback" => Some(Mismatch::Fallback), _ => None,
                })}
            {num("timeout_ms", &o, |o| o.timeout_ms, |o, x| o.timeout_ms = x)}
            {opt_keys("fallback", &o, |o| o.fallback.clone(), |o, x| o.fallback = x)}
        </details>
    }
    .into_any()
}

fn binding_editor(p: Place<RawBinding>) -> AnyView {
    let k = {
        let p = p.clone();
        Memo::new(move |_| p.read(binding_kind))
    };
    let fields = {
        let p = p.clone();
        move || match k.get().as_str() {
            "short" => keys_or_action(
                "キー / アクション名",
                &p,
                |b| if let RawBinding::Short(s) = b { s.clone() } else { String::new() },
                |b, x| *b = RawBinding::Short(x),
            )
            .into_any(),
            "steps" => steps_editor(p.map(
                |b| if let RawBinding::Steps(v) = b { Some(v) } else { None },
                |b| if let RawBinding::Steps(v) = b { Some(v) } else { None },
            )),
            "leaf" => leaf_editor(p.map(
                |b| if let RawBinding::Leaf(l) = b { Some(l) } else { None },
                |b| if let RawBinding::Leaf(l) = b { Some(l) } else { None },
            )),
            _ => node_editor(p.map(
                |b| if let RawBinding::Node(n) = b { Some(n) } else { None },
                |b| if let RawBinding::Node(n) = b { Some(n) } else { None },
            )),
        }
    };
    view! { {select("種類", &p, BINDING_KINDS, binding_kind, set_binding_kind)} {fields} }.into_any()
}

/// A keymap node: its options and one row per chord, recursively.
pub fn node_editor(p: Place<RawNode>) -> AnyView {
    let (list, add, opts) = (p.clone(), p.clone(), p.clone());
    let row = move |key: String| {
        let (a, b) = (key.clone(), key.clone());
        let bp = p.map(move |n| n.children.get(&a), move |n| n.children.get_mut(&b));
        let (r, d, del) = (p.clone(), p.clone(), key.clone());
        view! {
            <div class="binding">
                {name_row(
                    key,
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
            {options_editor(opts)}
            <For each=move || list.read(|n| n.children.keys().cloned().collect::<Vec<_>>()) key=|k| k.clone() let:k>
                {row(k)}
            </For>
            <button on:click=add_row>"キーを追加"</button>
        </div>
    }
    .into_any()
}
