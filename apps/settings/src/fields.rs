//! A `Place` points at one value inside the current file; field helpers bind inputs to it.

use crate::Store;
use grapnel_schema::{IndexMap, RawConfig};
use leptos::prelude::*;
use rust_i18n::t;
use std::borrow::Cow;
use std::sync::Arc;

/// Drop-down entries: (value, label).
pub type Options = Vec<(Cow<'static, str>, Cow<'static, str>)>;

type Getter<V> = dyn for<'a> Fn(&'a RawConfig) -> Option<&'a V> + Send + Sync;
type GetMut<V> = dyn for<'a> Fn(&'a mut RawConfig) -> Option<&'a mut V> + Send + Sync;

pub struct Place<V: 'static> {
    store: Store,
    get: Arc<Getter<V>>,
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

/// One input per item (so items may hold commas), each draggable and deletable, and an add button.
pub fn list(label: &str, p: Place<Vec<String>>) -> impl IntoView + use<> {
    let label = label.to_owned();
    let (items, add) = (p.clone(), p.clone());
    let drag = RwSignal::new(None);
    let row = {
        let label = label.clone();
        move |j: usize| {
            let (get, set) = (p.clone(), p.clone());
            let body = view! {
                <input aria-label=label.clone()
                    prop:value=move || get.read(|v| v.get(j).cloned().unwrap_or_default())
                    on:change=move |ev| set.edit(|v| if let Some(x) = v.get_mut(j) { *x = event_target_value(&ev) }) />
                {del_button(&p, j)}
            };
            sortable(&p, drag, move |_| Some(j), Vec::len, vec_move, "item", body)
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

/// Moves item `from` so that it ends up at index `to`.
pub fn vec_move<T>(v: &mut Vec<T>, from: usize, to: usize) {
    let x = v.remove(from);
    v.insert(to, x);
}

/// Delete button for item `j` of a list.
pub fn del_button<T: 'static>(p: &Place<Vec<T>>, j: usize) -> impl IntoView + use<T> {
    let del = p.clone();
    view! {
        <button class="del" title=t!("ui.delete") aria-label=t!("ui.delete")
            on:click=move |_| del.edit(|v| if j < v.len() { drop(v.remove(j)) })>"✕"</button>
    }
}

/// A row being dragged, and the index it would be inserted before.
#[derive(Clone, Copy, PartialEq)]
pub struct Drag {
    from: usize,
    to: Option<usize>,
}

/// Material Icons `drag_indicator` (Apache-2.0, see THIRD_PARTY_NOTICES.md).
const GRIP: &str = "M11 18c0 1.1-.9 2-2 2s-2-.9-2-2 .9-2 2-2 2 .9 2 2zm-2-8c-1.1 0-2 .9-2 2s.9 2 2 2 2-.9 2-2-.9-2-2-2zm0-6c-1.1 0-2 .9-2 2s.9 2 2 2 2-.9 2-2-.9-2-2-2zm6 4c1.1 0 2-.9 2-2s-.9-2-2-2-2 .9-2 2 .9 2 2 2zm0 2c-1.1 0-2 .9-2 2s.9 2 2 2 2-.9 2-2-.9-2-2-2zm0 6c-1.1 0-2 .9-2 2s.9 2 2 2 2-.9 2-2-.9-2-2-2z";

/// A row of a reorderable list: a drag handle, then `body`. Dropping a row of the same list (one
/// `drag` per list) on the upper or lower half of this row moves it before or after this row.
/// ↑/↓ on the focused handle move it by one. `pos` finds this row's index in the list.
pub fn sortable<V, P, B>(
    p: &Place<V>,
    drag: RwSignal<Option<Drag>>,
    pos: P,
    len: fn(&V) -> usize,
    mv: fn(&mut V, usize, usize),
    class: &'static str,
    body: B,
) -> impl IntoView + use<V, P, B>
where
    V: 'static,
    P: Fn(&V) -> Option<usize> + Clone + Send + Sync + 'static,
    B: IntoView,
{
    use leptos::wasm_bindgen::JsCast;
    use leptos::web_sys::{DragEvent, Element, KeyboardEvent};
    let at = {
        let (p, pos) = (p.clone(), pos.clone());
        move || p.read(|v| pos(v))
    };
    let count = {
        let p = p.clone();
        move || p.read(len)
    };
    let start = {
        let at = at.clone();
        move |ev: DragEvent| {
            let Some(i) = at() else { return };
            let row = ev.target().and_then(|t| t.dyn_into::<Element>().ok()).and_then(|e| e.parent_element());
            if let (Some(dt), Some(row)) = (ev.data_transfer(), row) {
                dt.set_effect_allowed("move");
                let r = row.get_bounding_client_rect();
                dt.set_drag_image(&row, ev.client_x() - r.left() as i32, ev.client_y() - r.top() as i32);
            }
            drag.set(Some(Drag { from: i, to: None }));
        }
    };
    let over = {
        let at = at.clone();
        move |ev: DragEvent| {
            let (Some(d), Some(i)) = (drag.get_untracked(), at()) else { return };
            ev.prevent_default(); // accept the drop
            ev.stop_propagation(); // an enclosing list's row is not the target
            let r = ev.current_target().unwrap().unchecked_into::<Element>().get_bounding_client_rect();
            let to = if f64::from(ev.client_y()) < r.top() + r.height() / 2.0 { i } else { i + 1 };
            if d.to != Some(to) {
                drag.set(Some(Drag { to: Some(to), ..d }));
            }
        }
    };
    let dropped = {
        let p = p.clone();
        move |ev: DragEvent| {
            let Some(Drag { from, to: Some(to) }) = drag.get_untracked() else { return };
            ev.prevent_default();
            ev.stop_propagation();
            drag.set(None);
            let to = if to > from { to - 1 } else { to };
            p.edit(|v| {
                if from != to && from.max(to) < len(v) {
                    mv(v, from, to)
                }
            });
        }
    };
    let key = {
        let (p, at) = (p.clone(), at.clone());
        move |ev: KeyboardEvent| {
            let Some(i) = at() else { return };
            let to = match ev.key().as_str() {
                "ArrowUp" if i > 0 => i - 1,
                "ArrowDown" => i + 1,
                _ => return,
            };
            ev.prevent_default();
            p.edit(|v| {
                if to < len(v) {
                    mv(v, i, to)
                }
            });
            // Keep the focus on the moved row's handle once the list is redrawn.
            let list = ev
                .current_target()
                .unwrap()
                .unchecked_into::<Element>()
                .parent_element()
                .and_then(|r| r.parent_element());
            request_animation_frame(move || {
                let grips = list.and_then(|l| l.query_selector_all(":scope > .sortable > .grip").ok());
                if let Some(g) = grips.and_then(|g| g.item(to as u32)) {
                    let _ = g.unchecked_into::<leptos::web_sys::HtmlElement>().focus();
                }
            });
        }
    };
    // The insertion line shows only where the drop would change the order.
    let line = {
        let at = at.clone();
        move |after: bool| {
            let (Some(d), Some(i)) = (drag.get(), at()) else { return false };
            let to = if after { i + 1 } else { i };
            d.to == Some(to) && to != d.from && to != d.from + 1 && (!after || to == count())
        }
    };
    let before = line.clone();
    let dragging = {
        let at = at.clone();
        move || drag.get().is_some_and(|d| Some(d.from) == at())
    };
    view! {
        <div class=format!("sortable {class}") class:dragging=dragging
            class:drop-before=move || before(false) class:drop-after=move || line(true)
            on:dragover=over on:drop=dropped>
            <span class="grip" draggable="true" tabindex="0" role="button" title=t!("ui.drag") aria-label=t!("ui.drag")
                on:dragstart=start on:dragend=move |_| drag.set(None) on:keydown=key>
                <svg viewBox="0 0 24 24" aria-hidden="true"><path d=GRIP /></svg>
            </span>
            {body}
        </div>
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
            .then(|| t!("config.unknown_action_or_key", name = s).into_owned())
    });
    input(label, p, get, set, |s| s, |s| s, Some(check))
}

/// Live key-sequence syntax check. `user_mods` allows user modifiers (input keys only).
fn key_check(store: Store, user_mods: bool) -> Option<Check> {
    Some(Arc::new(move |s: &str| {
        let names = if user_mods { store.user_mods() } else { vec![] };
        let names: Vec<&str> = names.iter().map(String::as_str).collect();
        grapnel_keys::parse_seq(s, &names).err().map(|e| e.text(&rust_i18n::locale()))
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
