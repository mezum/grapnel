//! Form sections for settings, modes, modifiers, targets and the keymap.

use crate::Store;
use crate::fields::*;
use crate::keymap::node_editor;
use grapnel_schema::*;
use leptos::prelude::*;
use std::collections::BTreeMap;

fn store() -> Store {
    use_context::<Store>().expect("store")
}

type MapOf<V> = fn(&mut RawConfig) -> &mut BTreeMap<String, V>;

/// Name input (renames on change) and a delete button for a map entry.
pub fn name_field<V: 'static>(store: Store, name: String, map: MapOf<V>) -> impl IntoView {
    let (old, del) = (name.clone(), name.clone());
    let rename = move |ev: leptos::ev::Event| {
        let input = event_target::<leptos::web_sys::HtmlInputElement>(&ev);
        let new = input.value();
        let mut ok = false;
        store.edit(|c| {
            let m = map(c);
            if !new.is_empty()
                && !m.contains_key(&new)
                && let Some(v) = m.remove(&old)
            {
                m.insert(new, v);
                ok = true;
            }
        });
        if !ok {
            input.set_value(&old); // empty or taken name: keep the old one
        }
    };
    view! {
        <div class="name">
            <input prop:value=name on:change=rename />
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
            <Show when=move || store.cur.get() == 0 fallback=move || view! {
                <p>"settings はエントリファイルにだけ書けます。"</p>
                <Show when=move || store.read(|c| c.settings.is_some())>
                    <button class="del" on:click=move |_| store.edit(|c| c.settings = None)>"このファイルの settings を削除"</button>
                </Show>
            }>
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
            view! {
                {check("定義外のキーを握りつぶす", &p, |m| m.block_unmapped, |m, x| m.block_unmapped = x)}
                {opt_text("unmapped_to (定義外の入力で移るモード)", &p, |m| m.unmapped_to.clone(), |m, x| m.unmapped_to = x)}
                {opt_text("hold (モード中に押したままにする修飾キー 例: S)", &p, |m| m.hold.clone(), |m, x| m.hold = x)}
                <details class="mode-keymap">
                    <summary>"このモード専用のキーマップ"</summary>
                    {node_editor(p.map(|m| Some(&m.keymap), |m| Some(&mut m.keymap)))}
                </details>
            }
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
            {opt_text("as (例: C, C-S)", &p, |m| m.emulate.clone(), |m, x| m.emulate = x)}
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

pub fn keymap() -> impl IntoView {
    let store = store();
    let p = Place::<RawNode>::new(store, |c| Some(&c.keymap), |c| Some(&mut c.keymap));
    view! {
        <section>
            <p class="hint">"全モード共通のキーマップ。キーは chord (例: C-x)。子の節にすると、続けて押すキー列 (C-x t 0 など) になる。"</p>
            {node_editor(p)}
        </section>
    }
}
