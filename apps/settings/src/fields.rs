//! A `Place` points at one value inside the current file; field helpers bind inputs to it.

use crate::Store;
use grapnel_schema::{IndexMap, RawConfig};
use leptos::prelude::*;
use rust_i18n::t;
use std::borrow::Cow;
use std::sync::Arc;

/// Drop-down entries: (value, label).
pub type Options = Vec<(Cow<'static, str>, Cow<'static, str>)>;

type Get<V> = dyn for<'a> Fn(&'a RawConfig) -> Option<&'a V> + Send + Sync;
type GetMut<V> = dyn for<'a> Fn(&'a mut RawConfig) -> Option<&'a mut V> + Send + Sync;

pub struct Place<V: 'static> {
    store: Store,
    get: Arc<Get<V>>,
    get_mut: Arc<GetMut<V>>,
}

impl<V> Clone for Place<V> {
    fn clone(&self) -> Self {
        Place { store: self.store, get: self.get.clone(), get_mut: self.get_mut.clone() }
    }
}

impl<V: 'static> Place<V> {
    pub fn new(
        store: Store,
        get: impl for<'a> Fn(&'a RawConfig) -> Option<&'a V> + Send + Sync + 'static,
        get_mut: impl for<'a> Fn(&'a mut RawConfig) -> Option<&'a mut V> + Send + Sync + 'static,
    ) -> Self {
        Place { store, get: Arc::new(get), get_mut: Arc::new(get_mut) }
    }

    /// Reads a projection of the value (tracked); `T::default()` when the value is gone.
    pub fn read<T: Default>(&self, f: impl FnOnce(&V) -> T) -> T {
        self.store.read(|c| (self.get)(c).map(f).unwrap_or_default())
    }

    pub fn edit(&self, f: impl FnOnce(&mut V)) {
        let get_mut = self.get_mut.clone();
        self.store.edit(move |c| {
            if let Some(v) = get_mut(c) {
                f(v)
            }
        });
    }

    /// A place for a value inside this one.
    pub fn map<W: 'static>(
        &self,
        f: impl for<'a> Fn(&'a V) -> Option<&'a W> + Send + Sync + 'static,
        g: impl for<'a> Fn(&'a mut V) -> Option<&'a mut W> + Send + Sync + 'static,
    ) -> Place<W> {
        let (get, get_mut) = (self.get.clone(), self.get_mut.clone());
        Place::new(self.store, move |c| get(c).and_then(&f), move |c| get_mut(c).and_then(&g))
    }
}

/// Renames a key in place (order kept). Fails for an empty or taken name.
pub fn rename_key<V>(m: &mut IndexMap<String, V>, old: &str, new: &str) -> bool {
    if new.is_empty() || m.contains_key(new) {
        return false;
    }
    let Some(i) = m.get_index_of(old) else { return false };
    let (_, v) = m.shift_remove_index(i).unwrap();
    let (j, _) = m.insert_full(new.to_string(), v);
    m.move_index(j, i);
    true
}

/// A name input that commits on change and restores the old name when `rename` refuses, plus a
/// delete button.
pub fn name_row(
    name: String,
    rename: impl Fn(&str, &str) -> bool + 'static,
    delete: impl Fn() + 'static,
) -> impl IntoView {
    let old = name.clone();
    let on_change = move |ev: leptos::ev::Event| {
        let input = event_target::<leptos::web_sys::HtmlInputElement>(&ev);
        if !rename(&old, &input.value()) {
            input.set_value(&old);
        }
    };
    view! {
        <div class="name">
            <input prop:value=name on:change=on_change />
            <button class="del" on:click=move |_| delete()>{t!("ui.delete")}</button>
        </div>
    }
}

/// Binds a string-ish input. `to`/`from` convert between the field and the input text.
fn input<V: 'static, T: Default + 'static>(
    label: &str,
    p: &Place<V>,
    get: fn(&V) -> T,
    set: fn(&mut V, T),
    to: fn(T) -> String,
    from: fn(String) -> T,
    check: Option<Check>,
) -> impl IntoView + use<V, T> {
    let label = label.to_owned();
    let (pg, ps) = (p.clone(), p.clone());
    let value = move || to(pg.read(get));
    let error = {
        let value = value.clone();
        move || check.as_ref().and_then(|c| c(&value()))
    };
    view! {
        <label class="field">
            <span>{label}</span>
            <input prop:value=value on:change=move |ev| ps.edit(|v| set(v, from(event_target_value(&ev)))) />
            <small class="error">{error}</small>
        </label>
    }
}

pub fn text<V>(label: &str, p: &Place<V>, get: fn(&V) -> String, set: fn(&mut V, String)) -> impl IntoView + use<V> {
    input(label, p, get, set, |s| s, |s| s, None)
}

/// Empty input means "not set".
pub fn opt_text<V>(
    label: &str,
    p: &Place<V>,
    get: fn(&V) -> Option<String>,
    set: fn(&mut V, Option<String>),
) -> impl IntoView + use<V> {
    input(label, p, get, set, Option::unwrap_or_default, |s| (!s.is_empty()).then_some(s), None)
}

/// Comma-separated list.
pub fn list<V>(
    label: &str,
    p: &Place<V>,
    get: fn(&V) -> Vec<String>,
    set: fn(&mut V, Vec<String>),
) -> impl IntoView + use<V> {
    let split = |s: String| s.split(',').map(|x| x.trim().to_string()).filter(|x| !x.is_empty()).collect();
    input(label, p, get, set, |v| v.join(", "), split, None)
}

pub fn num<V>(
    label: &str,
    p: &Place<V>,
    get: fn(&V) -> Option<u32>,
    set: fn(&mut V, Option<u32>),
) -> impl IntoView + use<V> {
    input(label, p, get, set, |n| n.map(|n| n.to_string()).unwrap_or_default(), |s| s.trim().parse().ok(), None)
}

type Check = Arc<dyn Fn(&str) -> Option<String> + Send + Sync>;

/// Keys to send, or the name of an action defined in any loaded file.
pub fn keys_or_action<V>(
    label: &str,
    p: &Place<V>,
    get: fn(&V) -> String,
    set: fn(&mut V, String),
) -> impl IntoView + use<V> {
    let store = p.store;
    let check: Check = Arc::new(move |s: &str| {
        let named = store.docs.with(|d| d.iter().any(|f| f.raw.actions.contains_key(s)));
        (!named && grapnel_keys::parse_seq(s, &[]).is_err())
            .then(|| t!("ui.unknown_action_or_key", name = s).into_owned())
    });
    input(label, p, get, set, |s| s, |s| s, Some(check))
}

/// Live key-sequence syntax check. `user_mods` allows user modifiers (input keys only).
fn key_check(store: Store, user_mods: bool) -> Option<Check> {
    Some(Arc::new(move |s: &str| {
        let names = if user_mods { store.user_mods() } else { vec![] };
        let names: Vec<&str> = names.iter().map(String::as_str).collect();
        grapnel_keys::parse_seq(s, &names).err()
    }))
}

pub fn keys<V>(
    label: &str,
    p: &Place<V>,
    get: fn(&V) -> String,
    set: fn(&mut V, String),
    user_mods: bool,
) -> impl IntoView + use<V> {
    input(label, p, get, set, |s| s, |s| s, key_check(p.store, user_mods))
}

/// Optional key sequence; empty input means "not set".
pub fn opt_keys<V>(
    label: &str,
    p: &Place<V>,
    get: fn(&V) -> Option<String>,
    set: fn(&mut V, Option<String>),
) -> impl IntoView + use<V> {
    let from = |s: String| (!s.is_empty()).then_some(s);
    input(label, p, get, set, Option::unwrap_or_default, from, key_check(p.store, false))
}

pub fn check<V>(label: &str, p: &Place<V>, get: fn(&V) -> bool, set: fn(&mut V, bool)) -> impl IntoView + use<V> {
    let label = label.to_owned();
    let (pg, ps) = (p.clone(), p.clone());
    view! {
        <label class="field check">
            <input type="checkbox" prop:checked=move || pg.read(get) on:change=move |ev| ps.edit(|v| set(v, event_target_checked(&ev))) />
            <span>{label}</span>
        </label>
    }
}

/// Drop-down over `options` (value, label). The field is converted with `get`/`set` as strings.
pub fn select<V>(
    label: &str,
    p: &Place<V>,
    options: Options,
    get: fn(&V) -> String,
    set: fn(&mut V, String),
) -> impl IntoView + use<V> {
    let label = label.to_owned();
    let (pg, ps) = (p.clone(), p.clone());
    view! {
        <label class="field">
            <span>{label}</span>
            <select prop:value=move || pg.read(get) on:change=move |ev| ps.edit(|v| set(v, event_target_value(&ev)))>
                {options.into_iter().map(|(v, l)| view! { <option value=v.into_owned()>{l.into_owned()}</option> }).collect_view()}
            </select>
        </label>
    }
}

/// Picks a name not yet in `taken`: `base`, `base2`, `base3`, ...
pub fn fresh_name(base: &str, taken: impl Fn(&str) -> bool) -> String {
    (1..).map(|i| if i == 1 { base.to_string() } else { format!("{base}{i}") }).find(|n| !taken(n)).unwrap()
}

/// `0..n` as a Vec, for index-keyed `<For>` lists.
pub fn indices(n: usize) -> Vec<usize> {
    (0..n).collect()
}
