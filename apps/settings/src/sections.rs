//! Form sections for settings, modes, modifiers, targets and rules.

use crate::Store;
use crate::fields::*;
use grapnel_schema::*;
use leptos::prelude::*;
use std::collections::BTreeMap;

fn store() -> Store {
    use_context::<Store>().expect("store")
}

type MapOf<V> = fn(&mut RawConfig) -> &mut BTreeMap<String, V>;

/// Name input (renames on change) and a delete button for a map entry.
fn name_field<V: 'static>(store: Store, name: String, map: MapOf<V>) -> impl IntoView {
    let (old, del) = (name.clone(), name.clone());
    let rename = move |new: String| {
        store.edit(|c| {
            let m = map(c);
            if !new.is_empty()
                && !m.contains_key(&new)
                && let Some(v) = m.remove(&old)
            {
                m.insert(new, v);
            }
        })
    };
    view! {
        <div class="name">
            <input prop:value=name on:change=move |ev| rename(event_target_value(&ev)) />
            <button class="del" on:click=move |_| store.edit(|c| drop(map(c).remove(&del)))>"削除"</button>
        </div>
    }
}

/// Lists a map section with one row per entry and an add button.
fn map_section<V: Default + 'static, R: IntoView + 'static>(
    base: &'static str,
    names: fn(&RawConfig) -> Vec<String>,
    map: MapOf<V>,
    row: fn(Store, String) -> R,
) -> impl IntoView {
    let store = store();
    let add = move |_| {
        store.edit(|c| {
            let m = map(c);
            let n = fresh_name(base, |n| m.contains_key(n));
            m.insert(n, V::default());
        })
    };
    view! {
        <section>
            <For each=move || store.read(names) key=|n| n.clone() let:name>
                <div class="row">{name_field(store, name.clone(), map)}{row(store, name)}</div>
            </For>
            <button on:click=add>"追加"</button>
        </section>
    }
}

pub fn settings() -> impl IntoView {
    let store = store();
    let top = Place::<RawConfig>::new(store, |c| Some(c), |c| Some(c));
    let s = Place::<RawSettings>::new(store, |c| c.settings.as_ref(), |c| Some(c.settings.get_or_insert_default()));
    view! {
        <section>
            {list("include (glob 可)", &top, |c| c.include.clone(), |c, x| c.include = x)}
            <Show when=move || store.cur.get() == 0 fallback=|| view! { <p>"settings はエントリファイルにだけ書けます。"</p> }>
                {opt_text("initial_mode", &s, |s| s.initial_mode.clone(), |s, x| s.initial_mode = x)}
                {list("passthrough (ターゲット)", &s, |s| s.passthrough.clone(), |s, x| s.passthrough = x)}
                {opt_keys("suspend_hotkey", &s, |s| s.suspend_hotkey.clone(), |s, x| s.suspend_hotkey = x)}
                {num("gesture_threshold (px)", &s, |s| s.gesture_threshold, |s, x| s.gesture_threshold = x)}
            </Show>
        </section>
    }
}

pub fn modes() -> impl IntoView {
    map_section(
        "mode",
        |c| c.modes.keys().cloned().collect(),
        |c| &mut c.modes,
        |store, name| {
            let (a, b) = (name.clone(), name);
            let p = Place::<RawMode>::new(store, move |c| c.modes.get(&a), move |c| c.modes.get_mut(&b));
            check("定義外のキーを握りつぶす", &p, |m| m.block_unmapped, |m, x| m.block_unmapped = x)
        },
    )
}

pub fn modifiers() -> impl IntoView {
    map_section(
        "Mod",
        |c| c.modifiers.keys().cloned().collect(),
        |c| &mut c.modifiers,
        |store, name| {
            let (a, b) = (name.clone(), name);
            let p = Place::<RawModifier>::new(store, move |c| c.modifiers.get(&a), move |c| c.modifiers.get_mut(&b));
            view! {
                {keys("key", &p, |m| m.key.clone(), |m, x| m.key = x, false)}
                {opt_keys("tap (空欄 = key と同じ)", &p, |m| m.tap.clone(), |m, x| m.tap = x)}
                {check("tap で何も送らない", &p, |m| m.tap.as_deref() == Some(""), |m, x| m.tap = x.then(String::new))}
                {num("tap_timeout_ms", &p, |m| m.tap_timeout_ms, |m, x| m.tap_timeout_ms = x)}
            }
        },
    )
}

pub fn targets() -> impl IntoView {
    map_section(
        "target",
        |c| c.targets.keys().cloned().collect(),
        |c| &mut c.targets,
        |store, name| {
            let (a, b) = (name.clone(), name);
            let p = Place::<RawTarget>::new(store, move |c| c.targets.get(&a), move |c| c.targets.get_mut(&b));
            view! {
                {opt_text("app", &p, |t| t.app.clone(), |t, x| t.app = x)}
                {opt_text("title", &p, |t| t.title.clone(), |t, x| t.title = x)}
                {opt_text("class", &p, |t| t.class.clone(), |t, x| t.class = x)}
                {opt_text("control", &p, |t| t.control.clone(), |t, x| t.control = x)}
                {opt_text("uia_id", &p, |t| t.uia_id.clone(), |t, x| t.uia_id = x)}
                {opt_text("uia_name", &p, |t| t.uia_name.clone(), |t, x| t.uia_name = x)}
                {opt_text("uia_type", &p, |t| t.uia_type.clone(), |t, x| t.uia_type = x)}
                {opt_text("not", &p, |t| t.not.clone(), |t, x| t.not = x)}
                {list("any", &p, |t| t.any.clone(), |t, x| t.any = x)}
                {list("all", &p, |t| t.all.clone(), |t, x| t.all = x)}
            }
        },
    )
}

const PRESS: &[(&str, &str)] = &[("", "既定 (hold)"), ("hold", "hold"), ("tap", "tap")];
const MISMATCH: &[(&str, &str)] =
    &[("", "既定 (replay)"), ("replay", "replay"), ("discard", "discard"), ("fallback", "fallback")];

fn rule_row(store: Store, i: usize) -> impl IntoView {
    let p = Place::<RawRule>::new(store, move |c| c.rules.get(i), move |c| c.rules.get_mut(i));
    view! {
        <div class="row">
            <button class="del" on:click=move |_| store.edit(|c| drop(c.rules.remove(i)))>"削除"</button>
            {keys("keys", &p, |r| r.keys.clone(), |r, x| r.keys = x, true)}
            {text("action", &p, |r| r.action.clone(), |r, x| r.action = x)}
            {list("targets", &p, |r| r.targets.clone(), |r, x| r.targets = x)}
            {list("modes", &p, |r| r.modes.clone(), |r, x| r.modes = x)}
            {select("press", &p, PRESS,
                |r| match r.press { None => "", Some(Press::Hold) => "hold", Some(Press::Tap) => "tap" }.into(),
                |r, x| r.press = match x.as_str() { "hold" => Some(Press::Hold), "tap" => Some(Press::Tap), _ => None })}
            {opt_keys("fallback (空欄 = 元の入力)", &p, |r| r.fallback.clone(), |r, x| r.fallback = x)}
            {check("fallback で何も送らない", &p, |r| r.fallback.as_deref() == Some(""), |r, x| r.fallback = x.then(String::new))}
            {select("on_mismatch", &p, MISMATCH,
                |r| match r.on_mismatch {
                    None => "", Some(Mismatch::Replay) => "replay", Some(Mismatch::Discard) => "discard", Some(Mismatch::Fallback) => "fallback",
                }.into(),
                |r, x| r.on_mismatch = match x.as_str() {
                    "replay" => Some(Mismatch::Replay), "discard" => Some(Mismatch::Discard), "fallback" => Some(Mismatch::Fallback), _ => None,
                })}
            {num("timeout_ms", &p, |r| r.timeout_ms, |r, x| r.timeout_ms = x)}
        </div>
    }
}

pub fn rules() -> impl IntoView {
    let store = store();
    view! {
        <section>
            <For each=move || indices(store.read(|c| c.rules.len())) key=|i| *i let:i>
                {rule_row(store, i)}
            </For>
            <button on:click=move |_| store.edit(|c| c.rules.push(RawRule::default()))>"追加"</button>
        </section>
    }
}
