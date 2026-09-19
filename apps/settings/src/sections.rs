//! Form sections for settings, modes, modifiers, targets and the keymap.

use crate::Store;
use crate::fields::*;
use crate::keymap::node_editor;
use grapnel_schema::*;
use leptos::prelude::*;
use rust_i18n::t;
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
            <button class="del" on:click=move |_| store.edit(|c| drop(map(c).remove(&del))) title=t!("ui.delete") aria-label=t!("ui.delete")>"✕"</button>
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
            <button class="add" on:click=add>{t!("ui.add")}</button>
        </section>
    }
}

/// "Follow Windows" plus every bundled language, each named in itself.
fn language_options() -> Options {
    let own = crate::languages().into_iter().map(|l| {
        let name = t!("language_name", locale = &l).into_owned();
        (l, name.into())
    });
    std::iter::once(("".into(), t!("ui.language_auto"))).chain(own).collect()
}

pub fn settings() -> impl IntoView {
    let store = store();
    let top = Place::<RawConfig>::new(store, |c| Some(c), |c| Some(c));
    let s = Place::<RawSettings>::new(store, |c| c.settings.as_ref(), |c| Some(c.settings.get_or_insert_default()));
    view! {
        <section>
            {list(&t!("ui.general.include"), top.map(|c| Some(&c.include), |c| Some(&mut c.include)))}
            <Show when=move || store.cur.get() == 0 fallback=move || view! {
                <p>{t!("ui.general.entry_only")}</p>
                <Show when=move || store.read(|c| c.settings.is_some())>
                    <button class="del" on:click=move |_| store.edit(|c| c.settings = None)>{t!("ui.general.delete_settings")}</button>
                </Show>
            }>
                {opt_text("initial_mode", &s, |s| s.initial_mode.clone(), |s, x| s.initial_mode = x)}
                {list(&t!("ui.general.passthrough"), s.map(|s| Some(&s.passthrough), |s| Some(&mut s.passthrough)))}
                {opt_keys("suspend_hotkey", &s, |s| s.suspend_hotkey.clone(), |s, x| s.suspend_hotkey = x)}
                {num("gesture_threshold (px)", &s, |s| s.gesture_threshold, |s, x| s.gesture_threshold = x)}
                {select(&t!("ui.general.language"), &s, language_options(),
                    |s| s.language.clone().unwrap_or_default(),
                    |s, x| s.language = (!x.is_empty()).then_some(x))}
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
                {check(&t!("ui.mode.block_unmapped"), &p, |m| m.block_unmapped, |m, x| m.block_unmapped = x)}
                {opt_text(&t!("ui.mode.unmapped_to"), &p, |m| m.unmapped_to.clone(), |m, x| m.unmapped_to = x)}
                {opt_text(&t!("ui.mode.hold"), &p, |m| m.hold.clone(), |m, x| m.hold = x)}
                <details class="mode-keymap">
                    <summary>{t!("ui.mode.keymap")}</summary>
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
                {opt_keys(&t!("ui.modifier.tap"), &p, |m| m.tap.clone(), |m, x| m.tap = x)}
                {check(&t!("ui.modifier.tap_none"), &p, |m| m.tap.as_deref() == Some(""), |m, x| m.tap = x.then(String::new))}
                {num("tap_timeout_ms", &p, |m| m.tap_timeout_ms, |m, x| m.tap_timeout_ms = x)}
                {opt_text(&t!("ui.modifier.as"), &p, |m| m.emulate.clone(), |m, x| m.emulate = x)}
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
                {list("any", p.map(|t| Some(&t.any), |t| Some(&mut t.any)))}
                {list("all", p.map(|t| Some(&t.all), |t| Some(&mut t.all)))}
            }
        },
    )
}

pub fn keymap() -> impl IntoView {
    let store = store();
    let p = Place::<RawNode>::new(store, |c| Some(&c.keymap), |c| Some(&mut c.keymap));
    view! {
        <section>
            <p class="hint">{t!("ui.keymap.hint")}</p>
            {node_editor(p)}
        </section>
    }
}
