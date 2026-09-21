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

/// Whether `p` is a Windows absolute path (`C:\...` or `\\server\share\...`).
fn is_abs(p: &str) -> bool {
    let b = p.as_bytes();
    matches!(b, [b'\\' | b'/', b'\\' | b'/', ..]) || matches!(b, [c, b':', b'\\' | b'/', ..] if c.is_ascii_alphabetic())
}

/// The parts of a path, without the separators and `.`; `\\server\share` starts with `server`.
fn parts(p: &str) -> Vec<&str> {
    p.split(['\\', '/']).filter(|s| !s.is_empty() && *s != ".").collect()
}

/// How many leading parts name a path's root, which `..` cannot leave: `C:`, or `server` and
/// `share` of a UNC path.
fn root_len(p: &str) -> usize {
    if matches!(p.as_bytes(), [b'\\' | b'/', b'\\' | b'/', ..]) { 2 } else { 1 }
}

/// `path` made absolute, reading a relative one from `dir` as `include` does.
fn to_abs(dir: &str, path: &str) -> String {
    if is_abs(path) || dir.is_empty() { path.to_string() } else { resolve(dir, path) }
}

/// Absolute `dir` joined with relative `path`, `.` and `..` resolved.
fn resolve(dir: &str, path: &str) -> String {
    let root = root_len(dir);
    let unc = root == 2;
    let mut out = Vec::new();
    for s in parts(dir).into_iter().chain(parts(path)) {
        match s == ".." {
            true if out.len() > root => drop(out.pop()),
            true => {}
            false => out.push(s),
        }
    }
    let joined = out.join("\\");
    if unc { format!("\\\\{joined}") } else { joined }
}

/// `path` written relative to `dir`; left as it is when it is on another drive or share.
fn to_rel(dir: &str, path: &str) -> String {
    if !is_abs(path) || !can_rel(dir, path) {
        return path.to_string();
    }
    let (d, p) = (parts(dir), parts(path));
    let common = d.iter().zip(&p).take_while(|(a, b)| a.eq_ignore_ascii_case(b)).count();
    let mut out = vec![".."; d.len() - common];
    out.extend_from_slice(&p[common..]);
    if out.is_empty() { ".".to_string() } else { out.join("\\") }
}

/// Whether a relative form of `path` can reach it from `dir`, which needs the same root.
fn can_rel(dir: &str, path: &str) -> bool {
    if !is_abs(path) {
        return true;
    }
    let (n, (d, p)) = (root_len(path), (parts(dir), parts(path)));
    root_len(dir) == n
        && d.len() >= n
        && p.len() >= n
        && d[..n].iter().zip(&p[..n]).all(|(a, b)| a.eq_ignore_ascii_case(b))
}

/// The file being edited and the folder it is in.
fn cur_file(store: Store) -> (String, String) {
    let path = store.docs.with(|d| d.get(store.cur.get()).map(|f| f.path.clone()).unwrap_or_default());
    let dir = path.rfind(['\\', '/']).map_or(String::new(), |i| path[..i].to_string());
    // An included file keeps the `..` it was reached through, which `to_rel` must not count.
    let dir = if is_abs(&path) { resolve(&dir, "") } else { dir };
    (path, dir)
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
        // The dialog starts next to the path being replaced, or next to the file being edited.
        let pick = {
            let (text, put) = (text.clone(), put.clone());
            move |_| {
                let (file, dir) = cur_file(store);
                let (cur, put) = (text(), put.clone());
                let start = if cur.is_empty() { file } else { to_abs(&dir, &cur) };
                let rel = !is_abs(&cur);
                spawn_local(async move {
                    if let Some(path) = crate::pick_file(start).await {
                        put(if rel { to_rel(&dir, &path) } else { path });
                    }
                })
            }
        };
        // Choosing a form converts the path there and then.
        let kind = {
            let (text, put) = (text.clone(), put.clone());
            move |ev: leptos::ev::Event| {
                let (_, dir) = cur_file(store);
                let cur = text();
                put(if event_target_value(&ev) == "abs" { to_abs(&dir, &cur) } else { to_rel(&dir, &cur) })
            }
        };
        let typed = {
            let put = put.clone();
            move |ev| put(event_target_value(&ev))
        };
        // A path on another drive has no relative form, so that choice is closed off.
        let rel_ok = {
            let text = text.clone();
            move || can_rel(&cur_file(store).1, &text())
        };
        let (shown, why_abs) = (text.clone(), rel_ok.clone());
        let body = view! {
            <label class="field path">
                <span>{t!("ui.general.path")}</span>
                <input prop:value=text on:change=typed
                    class:invalid=move || bad.problem().is_some() title=move || why.problem() />
            </label>
            <button on:click=pick>{t!("ui.general.browse")}</button>
            <label class="field">
                <span>{t!("ui.general.path_kind")}</span>
                <select on:change=kind title=move || (!why_abs()).then(|| t!("ui.general.no_relative").into_owned())
                    prop:value=move || if is_abs(&shown()) { "abs".to_string() } else { "rel".to_string() }>
                    <option value="abs">{t!("ui.general.absolute")}</option>
                    <option value="rel" disabled=move || !rel_ok()>{t!("ui.general.relative")}</option>
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
            view! {
                {opt_text("app", &p.at(".app"), |t| t.app.clone(), |t, x| t.app = x, String::new)}
                {opt_text("title", &p.at(".title"), |t| t.title.clone(), |t, x| t.title = x, String::new)}
                {opt_text("class", &p.at(".class"), |t| t.class.clone(), |t, x| t.class = x, String::new)}
                {opt_text("control", &p.at(".control"), |t| t.control.clone(), |t, x| t.control = x, String::new)}
                {opt_text("uia_id", &p.at(".uia_id"), |t| t.uia_id.clone(), |t, x| t.uia_id = x, String::new)}
                {opt_text("uia_name", &p.at(".uia_name"), |t| t.uia_name.clone(), |t, x| t.uia_name = x, String::new)}
                {opt_text("uia_type", &p.at(".uia_type"), |t| t.uia_type.clone(), |t, x| t.uia_type = x, String::new)}
                {opt_text("not", &p.at(".not"), |t| t.not.clone(), |t, x| t.not = x, String::new)}
                {list("any", p.map(".any", |t| Some(&t.any), |t| Some(&mut t.any)))}
                {list("all", p.map(".all", |t| Some(&t.all), |t| Some(&mut t.all)))}
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
