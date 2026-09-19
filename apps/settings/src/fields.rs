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
            <button class="del" on:click=move |_| delete() title=t!("ui.delete") aria-label=t!("ui.delete")>"✕"</button>
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

/// One input per item (so items may hold commas), each with move/delete buttons, and an add button.
pub fn list(label: &str, p: Place<Vec<String>>) -> impl IntoView + use<> {
    let label = label.to_owned();
    let (items, add) = (p.clone(), p.clone());
    let row = {
        let label = label.clone();
        move |j: usize| {
            let (get, set) = (p.clone(), p.clone());
            view! {
                <div class="item">
                    <input aria-label=label.clone()
                        prop:value=move || get.read(|v| v.get(j).cloned().unwrap_or_default())
                        on:change=move |ev| set.edit(|v| if let Some(x) = v.get_mut(j) { *x = event_target_value(&ev) }) />
                    {item_buttons(&p, j)}
                </div>
            }
        }
    };
    view! {
        <div class="field">
            <span>{label}</span>
            <For each=move || indices(items.read(|v| v.len())) key=|j| *j let:j>{row(j)}</For>
            <button class="add" on:click=move |_| add.edit(|v| v.push(String::new()))>{t!("ui.add")}</button>
        </div>
    }
}

/// Up/down buttons that swap the entry at `pos` with its neighbour.
pub fn move_buttons<V: 'static, P: Fn(&V) -> Option<usize> + Clone + Send + Sync + 'static>(
    p: &Place<V>,
    pos: P,
    len: fn(&V) -> usize,
    swap: fn(&mut V, usize, usize),
) -> impl IntoView + use<V, P> {
    let (a, b, c, d) = (p.clone(), p.clone(), p.clone(), p.clone());
    let (pa, pb, pc, pd) = (pos.clone(), pos.clone(), pos.clone(), pos);
    view! {
        <button class="move" title=t!("ui.move_up") aria-label=t!("ui.move_up")
            disabled=move || a.read(|v| pa(v).is_none_or(|i| i == 0))
            on:click=move |_| b.edit(|v| if let Some(i) = pb(v).filter(|&i| i > 0) { swap(v, i - 1, i) })>"↑"</button>
        <button class="move" title=t!("ui.move_down") aria-label=t!("ui.move_down")
            disabled=move || c.read(|v| pc(v).is_none_or(|i| i + 1 >= len(v)))
            on:click=move |_| d.edit(|v| if let Some(i) = pd(v).filter(|&i| i + 1 < len(v)) { swap(v, i, i + 1) })>"↓"</button>
    }
}

/// Move and delete buttons for item `j` of a list.
pub fn item_buttons<T: 'static>(p: &Place<Vec<T>>, j: usize) -> impl IntoView + use<T> {
    let del = p.clone();
    view! {
        {move_buttons(p, move |_| Some(j), Vec::len, |v, a, b| v.swap(a, b))}
        <button class="del" title=t!("ui.delete") aria-label=t!("ui.delete")
            on:click=move |_| del.edit(|v| if j < v.len() { drop(v.remove(j)) })>"✕"</button>
    }
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
