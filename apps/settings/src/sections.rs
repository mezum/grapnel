//! Form sections: imports, keymaps, keyswap, modifiers, targets and other settings.

use crate::Store;
use crate::fields::*;
use crate::keymap::node_editor;
use grapnel_schema::*;
use leptos::prelude::*;
use leptos::task::spawn_local;
use rust_i18n::t;

fn store() -> Store {
    use_context::<Store>().expect("store")
}

type MapOf<V> = fn(&mut RawConfig) -> &mut IndexMap<String, V>;

/// Name input (renames on change) and a delete button for a map entry.
pub fn name_field<V: 'static>(store: Store, section: &str, name: String, map: MapOf<V>) -> impl IntoView {
    let (old, del) = (name.clone(), name.clone());
    let (at, why) = (format!("{section}.{name}"), format!("{section}.{name}"));
    let label = match section {
        "modes" => t!("ui.name.mode"),
        "modifiers" => t!("ui.name.modifier"),
        _ => t!("ui.name.target"),
    };
    let rename = move |ev: leptos::ev::Event| {
        let input = event_target::<leptos::web_sys::HtmlInputElement>(&ev);
        let new = input.value();
        let mut ok = false;
        store.edit(|c| ok = rename_key(map(c), &old, &new));
        if !ok {
            input.set_value(&old); // empty or taken name: keep the old one
        }
    };
    view! {
        <div class="field">
            <span>{label.clone()}</span>
            <div class="name">
                <input prop:value=name on:change=rename aria-label=label
                    class:invalid=move || store.problem(&at, true).is_some() title=move || store.problem(&why, true) />
                <button class="del" on:click=move |_| store.edit(|c| drop(map(c).shift_remove(&del))) title=t!("ui.delete") aria-label=t!("ui.delete")>{trash()}</button>
            </div>
        </div>
        <div class="break"></div>
    }
}

/// Lists a map section with one reorderable row per entry and an add button.
fn map_section<V: Default + 'static, R: IntoView + 'static>(
    section: &'static str,
    base: &'static str,
    get: fn(&RawConfig) -> &IndexMap<String, V>,
    map: MapOf<V>,
    row: fn(Store, String) -> R,
) -> impl IntoView {
    let store = store();
    let p = Place::new(store, section, move |c| Some(get(c)), move |c| Some(map(c)));
    let drag = RwSignal::new(None);
    let add = move |_| {
        store.edit(|c| {
            let m = map(c);
            let n = fresh_name(base, |n| m.contains_key(n));
            m.insert(n, V::default());
        })
    };
    view! {
        <section>
            <For each=move || store.read(|c| get(c).keys().cloned().collect::<Vec<_>>()) key=|n| n.clone() let:name>
                {
                    let key = name.clone();
                    let body = view! { {name_field(store, section, name.clone(), map)}{row(store, name)} };
                    sortable(&p, drag, move |m| m.get_index_of(&key), IndexMap::len, IndexMap::move_index, "row", body)
                }
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

/// Keyboard layouts; JIS is the default, so choosing it removes the setting.
fn layout_options() -> Options {
    grapnel_keys::Layout::ALL.iter().map(|l| (l.name().into(), t!(format!("ui.layout.{}", l.name())))).collect()
}

/// The file being edited, whose folder its includes are read from.
fn cur_path(store: Store) -> String {
    store.docs.with(|d| d.get(store.cur.get()).map(|f| f.path.clone()).unwrap_or_default())
}

pub fn imports() -> impl IntoView {
    let store = store();
    let p = Place::<Vec<String>>::new(store, "include", |c| Some(&c.include), |c| Some(&mut c.include));
    let (items, add) = (p.clone(), p.clone());
    let drag = RwSignal::new(None);
    let row = move |j: usize| {
        let (bad, why) = (p.at(&format!("[{j}]")), p.at(&format!("[{j}]")));
        let text = {
            let get = p.clone();
            move || get.read(|v| v.get(j).cloned().unwrap_or_default())
        };
        let put = {
            let set = p.clone();
            move |s: String| {
                set.edit(|v| {
                    if let Some(x) = v.get_mut(j) {
                        *x = s
                    }
                })
            }
        };
        // The path written both ways, which the form choice shows and switches between.
        let forms = LocalResource::new({
            let text = text.clone();
            move || crate::include_forms(cur_path(store), text())
        });
        let now = move || forms.get().flatten();
        // The dialog starts next to the path being replaced, or next to the file being edited; the
        // chosen file is written in the row's current form. Both come from the text as it is now,
        // which `forms` may not have caught up with.
        let pick = {
            let (text, put) = (text.clone(), put.clone());
            move |_| {
                let (file, cur, put) = (cur_path(store), text(), put.clone());
                spawn_local(async move {
                    let f = crate::include_forms(file.clone(), cur).await;
                    let rel = f.as_ref().is_none_or(|f| !f.absolute);
                    let start = f.map(|f| f.abs).filter(|a| !a.is_empty()).unwrap_or_else(|| file.clone());
                    let Some(path) = crate::pick_file(start).await else { return };
                    let f = if rel { crate::include_forms(file, path.clone()).await } else { None };
                    put(f.and_then(|f| f.rel).unwrap_or(path));
                })
            }
        };
        // Choosing a form converts the path there and then.
        let kind = {
            let (text, put) = (text.clone(), put.clone());
            move |ev: leptos::ev::Event| {
                let (abs, file, cur, put) = (event_target_value(&ev) == "abs", cur_path(store), text(), put.clone());
                spawn_local(async move {
                    if let Some(f) = crate::include_forms(file, cur).await {
                        put(if abs { f.abs } else { f.rel.unwrap_or(f.abs) });
                    }
                })
            }
        };
        let typed = {
            let put = put.clone();
            move |ev| put(event_target_value(&ev))
        };
        // A path on another drive or share has no relative form, so that choice is closed off.
        let no_rel = move || now().is_some_and(|f| f.rel.is_none());
        let body = view! {
            <label class="field path">
                <span>{t!("ui.general.path")}</span>
                <input prop:value=text on:change=typed
                    class:invalid=move || bad.problem().is_some() title=move || why.problem() />
            </label>
            <button on:click=pick>{t!("ui.general.browse")}</button>
            <label class="field">
                <span>{t!("ui.general.path_kind")}</span>
                <select on:change=kind title=move || no_rel().then(|| t!("ui.general.no_relative").into_owned())
                    prop:value=move || if now().is_some_and(|f| f.absolute) { "abs" } else { "rel" }.to_string()>
                    <option value="abs">{t!("ui.general.absolute")}</option>
                    <option value="rel" disabled=no_rel>{t!("ui.general.relative")}</option>
                </select>
            </label>
            {del_button(&p, j)}
        };
        sortable(&p, drag, move |_| Some(j), Vec::len, vec_move, "row include", body)
    };
    view! {
        <section>
            <For each=move || indices(items.read(|v| v.len())) key=|j| *j let:j>{row(j)}</For>
            <button class="add" on:click=move |_| add.edit(|v| v.push(String::new()))>{t!("ui.add")}</button>
        </section>
    }
}

/// `[settings]`: only in the entry file.
pub fn others() -> impl IntoView {
    let store = store();
    let s = Place::<RawSettings>::new(
        store,
        "settings",
        |c| c.settings.as_ref(),
        |c| Some(c.settings.get_or_insert_default()),
    );
    view! {
        <section>
            <Show when=move || store.cur.get() == 0 fallback=move || view! {
                <p>{t!("ui.general.entry_only")}</p>
                <Show when=move || store.read(|c| c.settings.is_some())>
                    <button class="del" on:click=move |_| store.edit(|c| c.settings = None)>{t!("ui.general.delete_settings")}</button>
                </Show>
            }>
                {opt_text("initial_mode", &s.at(".initial_mode"), |s| s.initial_mode.clone(), |s, x| s.initial_mode = x, || "default".into())}
                {list(&t!("ui.general.passthrough"), s.map(".passthrough", |s| Some(&s.passthrough), |s| Some(&mut s.passthrough)))}
                {opt_keys("suspend_hotkey", &s.at(".suspend_hotkey"), |s| s.suspend_hotkey.clone(), |s, x| s.suspend_hotkey = x, || t!("ui.hint.none").into_owned())}
                {num("gesture_threshold (px)", &s.at(".gesture_threshold"), |s| s.gesture_threshold, |s, x| s.gesture_threshold = x, || "30".into())}
                {select(&t!("ui.general.language"), &s, language_options(),
                    |s| s.language.clone().unwrap_or_default(),
                    |s, x| s.language = (!x.is_empty()).then_some(x))}
                {select(&t!("ui.general.layout"), &s, layout_options(),
                    |s| s.layout.clone().unwrap_or_else(|| "jis".into()),
                    |s, x| s.layout = (x != "jis").then_some(x))}
            </Show>
        </section>
    }
}

pub fn modes() -> impl IntoView {
    map_section(
        "modes",
        "mode",
        |c| &c.modes,
        |c| &mut c.modes,
        |store, name| {
            let (a, b) = (name.clone(), name);
            let at = format!("modes.{a}");
            let p = Place::<RawMode>::new(store, &at, move |c| c.modes.get(&a), move |c| c.modes.get_mut(&b));
            view! {
                <div class="stack">
                    {opt_text(&t!("ui.mode.hold"), &p.at(".hold"), |m| m.hold.clone(), |m, x| m.hold = x, || t!("ui.hint.none").into_owned())}
                    {opt_text(&t!("ui.mode.unmapped_to"), &p.at(".unmapped_to"), |m| m.unmapped_to.clone(), |m, x| m.unmapped_to = x, || t!("ui.hint.none").into_owned())}
                    {check(&t!("ui.mode.block_unmapped"), &p.at(".block_unmapped"), |m| m.block_unmapped, |m, x| m.block_unmapped = x)}
                    {check(&t!("ui.mode.count"), &p.at(".count"), |m| m.count, |m, x| m.count = x)}
                </div>
                <details class="mode-keymap">
                    <summary>{t!("ui.mode.keymap")}</summary>
                    {node_editor(p.map(".keymap", |m| Some(&m.keymap), |m| Some(&mut m.keymap)), true, false.into())}
                </details>
            }
        },
    )
}

pub fn modifiers() -> impl IntoView {
    map_section(
        "modifiers",
        "Mod",
        |c| &c.modifiers,
        |c| &mut c.modifiers,
        |store, name| {
            let (a, b) = (name.clone(), name);
            let at = format!("modifiers.{a}");
            let p =
                Place::<RawModifier>::new(store, &at, move |c| c.modifiers.get(&a), move |c| c.modifiers.get_mut(&b));
            let (no_tap, tap) = (p.clone(), p.clone());
            view! {
                <div class="stack">
                    {keys(&t!("ui.modifier.key"), &p.at(".key"), |m| m.key.clone(), |m, x| m.key = x, false)}
                    {check(&t!("ui.modifier.tap_none"), &p.at(".tap"), |m| m.tap.as_deref() == Some(""), |m, x| m.tap = x.then(String::new))}
                    // Sending nothing leaves no keys to type.
                    {move || no_tap.read(|m| m.tap.as_deref() != Some("")).then(|| {
                        opt_keys(&t!("ui.modifier.tap"), &tap.at(".tap"), |m| m.tap.clone(), |m, x| m.tap = x, { let k = tap.clone(); move || k.read(|m| m.key.clone()) })
                    })}
                    {num(&t!("ui.modifier.tap_timeout_ms"), &p.at(".tap_timeout_ms"), |m| m.tap_timeout_ms, |m, x| m.tap_timeout_ms = x, || t!("ui.hint.unlimited").into_owned())}
                    {opt_text(&t!("ui.modifier.as"), &p.at(".as"), |m| m.emulate.clone(), |m, x| m.emulate = x, || t!("ui.hint.none").into_owned())}
                </div>
            }
        },
    )
}

/// Target conditions, in the order they are shown; the first ones match strings.
const TARGET_FIELDS: [&str; 10] =
    ["app", "title", "class", "control", "uia_id", "uia_name", "uia_type", "not", "any", "all"];
const STRING_FIELDS: usize = 7;

/// The items of condition `f`.
fn target_list(p: &Place<RawTarget>, f: &str) -> Place<Vec<String>> {
    match f {
        "app" => p.map(".app", |t| Some(&t.app), |t| Some(&mut t.app)),
        "title" => p.map(".title", |t| Some(&t.title), |t| Some(&mut t.title)),
        "class" => p.map(".class", |t| Some(&t.class), |t| Some(&mut t.class)),
        "control" => p.map(".control", |t| Some(&t.control), |t| Some(&mut t.control)),
        "uia_id" => p.map(".uia_id", |t| Some(&t.uia_id), |t| Some(&mut t.uia_id)),
        "uia_name" => p.map(".uia_name", |t| Some(&t.uia_name), |t| Some(&mut t.uia_name)),
        "uia_type" => p.map(".uia_type", |t| Some(&t.uia_type), |t| Some(&mut t.uia_type)),
        "not" => p.map(".not", |t| Some(&t.not), |t| Some(&mut t.not)),
        "any" => p.map(".any", |t| Some(&t.any), |t| Some(&mut t.any)),
        _ => p.map(".all", |t| Some(&t.all), |t| Some(&mut t.all)),
    }
}

/// How a target string is matched: as written, as a glob or as a regular expression.
const MATCH_KINDS: [(&str, &str); 3] = [("", "raw"), ("glob:", "glob"), ("re:", "re")];

/// The match prefix of `s` and the rest.
fn split_kind(s: &str) -> (&'static str, &str) {
    MATCH_KINDS[1..].iter().find_map(|(k, _)| s.strip_prefix(k).map(|r| (*k, r))).unwrap_or(("", s))
}

/// Item `j` of a target string list: how it is matched (its prefix) in a drop-down, then the
/// text after it.
fn pattern(label: &str, p: &Place<Vec<String>>, j: usize) -> AnyView {
    let get = move |v: &Vec<String>| v.get(j).cloned().unwrap_or_default();
    // The kind last chosen. Raw shows the whole value, prefix and all, so it sticks even when the
    // value has a prefix; otherwise the value's prefix decides, and this only while it is empty.
    let picked = RwSignal::new(untrack(|| split_kind(&p.read(get)).0));
    let kind = {
        let p = p.clone();
        move || match (picked.get(), p.read(get)) {
            ("", _) => "",
            (k, s) if s.is_empty() => k,
            (_, s) => split_kind(&s).0,
        }
    };
    let text = {
        let (p, kind) = (p.clone(), kind.clone());
        move || {
            let s = p.read(get);
            if kind().is_empty() { s } else { split_kind(&s).1.to_owned() }
        }
    };
    let (pk, pt) = (p.clone(), p.clone());
    let on_kind = move |ev| {
        let v = event_target_value(&ev);
        let k = MATCH_KINDS.iter().map(|(k, _)| *k).find(|k| *k == v).unwrap_or("");
        picked.set(k);
        // Raw leaves the value as it is, now shown whole.
        if !k.is_empty() {
            pk.edit(|v| {
                if let Some(x) = v.get_mut(j) {
                    *x = format!("{k}{}", split_kind(x).1)
                }
            })
        }
    };
    let on_text = {
        let kind = kind.clone();
        move |ev| {
            let s = format!("{}{}", kind(), event_target_value(&ev));
            pt.edit(|v| {
                if let Some(x) = v.get_mut(j) {
                    *x = s
                }
            })
        }
    };
    let (bad, why) = (p.at(&format!("[{j}]")), p.at(&format!("[{j}]")));
    view! {
        <select prop:value=move || kind().to_string() on:change=on_kind aria-label=t!("ui.kind")>
            {MATCH_KINDS.map(|(k, n)| view! { <option value=k>{t!(format!("ui.target.kind_{n}"))}</option> })}
        </select>
        <input prop:value=text on:change=on_text aria-label=label.to_owned()
            class:invalid=move || bad.problem().is_some() title=move || why.problem() />
    }
    .into_any()
}

/// Shows the conditions that are set, each a list; the others wait in a drop-down until added.
pub fn targets() -> impl IntoView {
    map_section(
        "targets",
        "target",
        |c| &c.targets,
        |c| &mut c.targets,
        |store, name| {
            let (a, b) = (name.clone(), name);
            let at = format!("targets.{a}");
            let p = Place::<RawTarget>::new(store, &at, move |c| c.targets.get(&a), move |c| c.targets.get_mut(&b));
            let lists = TARGET_FIELDS.map(|f| target_list(&p, f));
            let shown = {
                let lists = lists.clone();
                move |i: usize| lists[i].read(|v| !v.is_empty())
            };
            let fields = TARGET_FIELDS.iter().zip(lists.clone()).enumerate().map(|(i, (f, l))| {
                let shown = shown.clone();
                let item: ItemView = if i < STRING_FIELDS { pattern } else { plain_item };
                view! { <Show when=move || shown(i)>{list_of(f, l.clone(), item)}</Show> }
            });
            let fields = fields.collect_view();
            let options = {
                let shown = shown.clone();
                move || {
                    TARGET_FIELDS
                        .iter()
                        .enumerate()
                        .filter(|(i, _)| !shown(*i))
                        .map(|(_, f)| view! { <option value=*f>{t!(format!("ui.target.{f}"))}</option> })
                        .collect_view()
                }
            };
            // Adding a condition gives it one empty item to fill in.
            let add = move |ev: leptos::ev::Event| {
                let select = event_target::<leptos::web_sys::HtmlSelectElement>(&ev);
                if let Some(i) = TARGET_FIELDS.iter().position(|f| *f == select.value()) {
                    lists[i].edit(|v| v.push(String::new()));
                }
                select.set_value("");
            };
            view! {
                <div class="stack target">
                    {fields}
                    <Show when=move || (0..TARGET_FIELDS.len()).any(|i| !shown(i))>
                        <select on:change=add.clone() aria-label=t!("ui.target.add")>
                            <option value="" selected>{t!("ui.target.add")}</option>
                            {options.clone()}
                        </select>
                    </Show>
                </div>
            }
        },
    )
}

pub fn keymap() -> impl IntoView {
    let store = store();
    let p = Place::<RawNode>::new(store, "keymap", |c| Some(&c.keymap), |c| Some(&mut c.keymap));
    view! {
        <section>
            {node_editor(p, true, false.into())}
        </section>
    }
}

pub fn keyswap() -> impl IntoView {
    let p = Place::<IndexMap<String, String>>::new(store(), "keyswap", |c| Some(&c.keyswap), |c| Some(&mut c.keyswap));
    let (list, add) = (p.clone(), p.clone());
    let drag = RwSignal::new(None);
    let row = move |key: String| {
        let (a, b) = (key.clone(), key.clone());
        let vp = p.map(&format!(".\"{key}\""), move |m| m.get(&a), move |m| m.get_mut(&b));
        let (r, d, del, old, pos) = (p.clone(), p.clone(), key.clone(), key.clone(), key.clone());
        let (bad, why) = (vp.clone(), vp.clone());
        // Renames on change; an empty or taken key puts the old one back.
        let rename = move |ev: leptos::ev::Event| {
            let input = event_target::<leptos::web_sys::HtmlInputElement>(&ev);
            let mut ok = false;
            r.edit(|m| ok = rename_key(m, &old, &input.value()));
            if !ok {
                input.set_value(&old);
            }
        };
        let body = view! {
            <label class="field">
                <span>{t!("ui.keyswap.from")}</span>
                <input prop:value=key on:change=rename
                    class:invalid=move || bad.key_problem().is_some() title=move || why.key_problem() />
            </label>
            {keys(&t!("ui.keyswap.to"), &vp, |v| v.clone(), |v, x| *v = x, false)}
            <button class="del" on:click=move |_| d.edit(|m| drop(m.shift_remove(&del)))
                title=t!("ui.delete") aria-label=t!("ui.delete")>{trash()}</button>
        };
        sortable(&p, drag, move |m| m.get_index_of(&pos), IndexMap::len, IndexMap::move_index, "row swap", body)
    };
    // An empty key is reported by validation until the user types it.
    let add_row = move |_| {
        add.edit(|m| {
            m.entry(String::new()).or_default();
        })
    };
    view! {
        <section>
            <For each=move || list.read(|m| m.keys().cloned().collect::<Vec<_>>()) key=|k| k.clone() let:k>{row(k)}</For>
            <button class="add" on:click=add_row>{t!("ui.keyswap.add")}</button>
        </section>
    }
}
