//! grapnel-settings front end: form editing of every loaded config file.

rust_i18n::i18n!("../../locales", fallback = "en");

mod actions;
mod fields;
mod keymap;
mod sections;

use fields::indices;
use grapnel_schema::RawConfig;
use leptos::prelude::*;
use leptos::task::spawn_local;
use rust_i18n::t;
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use wasm_bindgen::prelude::*;

#[wasm_bindgen]
extern "C" {
    #[wasm_bindgen(js_namespace = ["window", "__TAURI__", "core"], catch)]
    async fn invoke(cmd: &str, args: JsValue) -> Result<JsValue, JsValue>;
    #[wasm_bindgen(js_namespace = ["window", "__TAURI__", "event"])]
    fn listen(event: &str, handler: &Closure<dyn FnMut(JsValue)>) -> js_sys::Promise;
}

/// Calls `f` with the payload of every `menu` event (the id of the chosen menu item).
fn on_menu(f: impl Fn(String) + 'static) {
    let handler = Closure::<dyn FnMut(JsValue)>::new(move |ev| f(prop(&ev, "payload").as_string().unwrap_or_default()));
    let _ = listen("menu", &handler);
    handler.forget(); // listens for the page's lifetime
}

/// Calls a Tauri command. Arguments go through JSON text so every map key (even `__proto__`)
/// becomes an own property.
async fn call<A: Serialize, R: DeserializeOwned, E: DeserializeOwned>(cmd: &str, args: &A) -> Result<R, E> {
    let args = js_sys::JSON::parse(&serde_json::to_string(args).unwrap()).unwrap();
    match invoke(cmd, args).await {
        Ok(v) => Ok(serde_wasm_bindgen::from_value(v).unwrap()),
        Err(e) => Err(serde_wasm_bindgen::from_value(e).unwrap()),
    }
}

#[derive(Serialize, Deserialize, Clone, PartialEq)]
pub struct FileDoc {
    path: String,
    raw: RawConfig,
}

#[derive(Serialize)]
/// Files travel as JSON text: a JS object would reorder integer-like keys. Problems come back
/// worded in `lang`.
struct Files {
    files: String,
    lang: String,
}

impl Files {
    fn of(docs: &[FileDoc]) -> Files {
        Files { files: serde_json::to_string(docs).unwrap(), lang: rust_i18n::locale().to_string() }
    }
}

#[derive(Serialize)]
struct Lang {
    lang: String,
}

#[derive(Serialize)]
struct Entry {
    entry: String,
    lang: String,
}

impl Entry {
    fn of(entry: String) -> Entry {
        Entry { entry, lang: rust_i18n::locale().to_string() }
    }
}

/// Asks for a config file with the Windows open dialog, starting next to `near`.
async fn pick_file(near: String) -> Option<String> {
    call::<_, Option<String>, ()>("pick_entry", &Entry::of(near)).await.ok().flatten()
}

/// An include path written both ways, as `grapnel_config::include_forms` gives it.
#[derive(Deserialize, Clone, PartialEq)]
pub struct IncludeForms {
    /// The path as given is absolute.
    pub absolute: bool,
    pub abs: String,
    /// `None` on another drive or share, which a relative path cannot reach.
    pub rel: Option<String>,
}

#[derive(Serialize)]
struct IncludeOf {
    file: String,
    path: String,
}

/// `path`, included from `file`, written both ways; `None` if the backend could not be asked.
async fn include_forms(file: String, path: String) -> Option<IncludeForms> {
    call::<_, IncludeForms, ()>("include_forms", &IncludeOf { file, path }).await.ok()
}

/// A problem found by the backend, already in the UI language.
#[derive(Deserialize, Clone, PartialEq)]
pub struct Problem {
    pub file: String,
    /// Location inside the file, as `grapnel_config` names it (`settings.passthrough[1]`).
    pub at: String,
    /// About the name at `at` (a map key) rather than its value.
    pub on_key: bool,
    pub text: String,
}

impl std::fmt::Display for Problem {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        let parts = [&self.file, &self.at, &self.text];
        let parts: Vec<&str> = parts.iter().map(|s| s.as_str()).filter(|s| !s.is_empty()).collect();
        f.write_str(&parts.join(": "))
    }
}

#[derive(Deserialize)]
struct SaveError {
    /// Some or all files were written (a later write or reading them back failed).
    saved: bool,
    errors: Vec<Problem>,
}

/// Bundled languages (`"en"`, `"ja"`, ...).
pub fn languages() -> Vec<std::borrow::Cow<'static, str>> {
    let mut all = rust_i18n::available_locales!();
    all.sort();
    all
}

/// `o[k]`, or `undefined`.
fn prop(o: &JsValue, k: &str) -> JsValue {
    js_sys::Reflect::get(o, &k.into()).unwrap_or_default()
}

/// The bundled language closest to the browser's (which follows Windows), else English.
fn initial_language() -> String {
    let wanted = prop(&prop(&js_sys::global(), "navigator"), "language").as_string().unwrap_or_default();
    let fits = |l: &str| wanted == l || wanted.starts_with(&format!("{l}-"));
    languages().into_iter().find(|l| fits(l)).map_or("en".into(), |l| l.into_owned())
}

fn set_language(lang: &str) {
    rust_i18n::set_locale(lang);
    let _ =
        js_sys::Reflect::set(&prop(&js_sys::global(), "document"), &"title".into(), &t!("ui.title").as_ref().into());
}

const THEME_KEY: &str = "grapnelTheme";

/// `"auto"` (follow Windows), `"light"` or `"dark"`, as last chosen on this PC.
fn saved_theme() -> String {
    prop(&prop(&js_sys::global(), "localStorage"), THEME_KEY).as_string().unwrap_or_else(|| "auto".into())
}

/// Sets `<html data-theme>` (none for auto) and remembers the choice. Storage may be unavailable.
fn set_theme(theme: &str) {
    let root = prop(&prop(&js_sys::global(), "document"), "documentElement");
    let dataset = prop(&root, "dataset");
    let _ = match theme {
        "auto" => js_sys::Reflect::delete_property(&dataset.into(), &"theme".into()),
        _ => js_sys::Reflect::set(&dataset, &"theme".into(), &theme.into()),
    };
    let _ = js_sys::Reflect::set(&prop(&js_sys::global(), "localStorage"), &THEME_KEY.into(), &theme.into());
}

/// The files as JSON, which keeps map order (unlike `RawConfig`'s equality).
fn snapshot(docs: &[FileDoc]) -> String {
    serde_json::to_string(docs).unwrap()
}

/// Blurs the focused element, so a field being edited commits its text (`change`) first.
fn blur_focused() {
    let el = prop(&prop(&js_sys::global(), "document"), "activeElement");
    if let Ok(blur) = prop(&el, "blur").dyn_into::<js_sys::Function>() {
        let _ = blur.call0(&el);
    }
}

/// Shared UI state.
#[derive(Clone, Copy)]
pub struct Store {
    pub docs: RwSignal<Vec<FileDoc>>,
    pub cur: RwSignal<usize>,
    pub errors: RwSignal<Vec<Problem>>,
}

impl Store {
    /// Reads the current file (tracked).
    pub fn read<T>(&self, f: impl FnOnce(&RawConfig) -> T) -> T {
        let i = self.cur.get();
        self.docs.with(|d| f(&d[i.min(d.len() - 1)].raw))
    }

    pub fn edit(&self, f: impl FnOnce(&mut RawConfig)) {
        let i = self.cur.get_untracked();
        self.docs.update(|d| f(&mut d[i].raw));
    }

    /// The backend's message about location `at` of the current file, or about its name when
    /// `on_key` (tracked).
    pub fn problem(&self, at: &str, on_key: bool) -> Option<String> {
        let file = self.docs.with(|d| d.get(self.cur.get()).map(|f| f.path.clone()))?;
        let found = |e: &&Problem| e.file == file && e.at == at && e.on_key == on_key;
        self.errors.with(|es| es.iter().find(found).map(|e| e.text.clone()))
    }

    /// User modifier names across all files, in compile order.
    pub fn user_mods(&self) -> Vec<String> {
        self.docs.with(|d| d.iter().flat_map(|f| f.raw.modifiers.keys().cloned()).collect())
    }
    /// The entry file's keyboard layout; an unknown name falls back to JIS like an unset one.
    pub fn layout(&self) -> grapnel_keys::Layout {
        let name = self.docs.with(|d| d.first().and_then(|f| f.raw.settings.as_ref()?.layout.clone()));
        name.and_then(|n| grapnel_keys::Layout::from_name(&n)).unwrap_or_default()
    }
}

#[component]
fn App() -> impl IntoView {
    let store = Store { docs: RwSignal::new(vec![]), cur: RwSignal::new(0), errors: RwSignal::new(vec![]) };
    provide_context(store);
    let entry = RwSignal::new(String::new());
    let tab = RwSignal::new(0usize);
    let errors = store.errors;
    let status = RwSignal::new(String::new());
    // The files as last loaded or saved, serialized, to tell unsaved changes (reordering included).
    let saved = RwSignal::new(String::new());
    let running = RwSignal::new(true);
    let lang = RwSignal::new(initial_language());
    set_language(&lang.get_untracked());
    let theme = RwSignal::new(saved_theme());
    set_theme(&theme.get_untracked());

    // One load or save at a time. The backend runs them off its main thread, so a second one
    // started meanwhile could finish first and load the old file or write the older edits.
    let busy = RwSignal::new(false);
    let load = move || {
        if busy.get_untracked() {
            return;
        }
        busy.set(true);
        spawn_local(async move {
            match call::<_, String, Vec<Problem>>("load", &Entry::of(entry.get_untracked())).await {
                Ok(json) => {
                    let docs: Vec<FileDoc> = serde_json::from_str(&json).unwrap();
                    store.cur.set(0);
                    saved.set(snapshot(&docs));
                    store.docs.set(docs);
                    status.set(t!("ui.status.loaded").into());
                }
                Err(e) => errors.set(e),
            }
            busy.set(false);
        })
    };
    let open = move || {
        spawn_local(async move {
            if let Some(path) = pick_file(entry.get_untracked()).await {
                entry.set(path);
                load();
            }
        })
    };
    spawn_local(async move {
        entry.set(call::<_, String, String>("initial_entry", &()).await.unwrap_or_default());
        load();
    });
    // Validate after every edit, and again in a newly chosen language. The backend validates off
    // its main thread, so two answers can overtake each other; only the newest one is shown.
    let latest = std::rc::Rc::new(std::cell::Cell::new(0u32));
    Effect::new(move |_| {
        lang.track();
        let files = store.docs.get();
        if !files.is_empty() {
            let files = Files::of(&files);
            let (latest, id) = (latest.clone(), latest.get() + 1);
            latest.set(id);
            spawn_local(async move {
                let found = call::<_, Vec<Problem>, ()>("validate", &files).await.unwrap();
                if latest.get() == id {
                    errors.set(found);
                }
            });
        }
    });
    let save = move |apply: bool| {
        if busy.get_untracked() {
            return;
        }
        busy.set(true);
        spawn_local(async move {
            let docs = store.docs.get_untracked();
            match call::<_, (), SaveError>("save", &Files::of(&docs)).await {
                Err(e) => {
                    errors.set(e.errors);
                    let text = if e.saved { t!("ui.status.saved_with_errors") } else { t!("ui.status.save_failed") };
                    status.set(text.into());
                }
                Ok(()) => {
                    saved.set(snapshot(&docs));
                    status.set(t!("ui.status.saved").into());
                    if apply {
                        match call::<_, (), String>("apply", &()).await {
                            Ok(()) => status.set(t!("ui.status.applied").into()),
                            Err(e) => status.set(t!("ui.status.apply_failed", error = e).into()),
                        }
                    }
                }
            }
            busy.set(false);
        })
    };
    let cannot_save = move || busy.get() || !errors.with(Vec::is_empty) || store.docs.with(Vec::is_empty);

    let change_language = move |l: String| {
        set_language(&l);
        status.set(String::new());
        let menu = Lang { lang: l.clone() };
        spawn_local(async move {
            if let Err(e) = call::<_, (), String>("set_menu", &menu).await {
                web_sys_log(&e);
            }
        });
        lang.set(l);
    };
    change_language(lang.get_untracked());
    // Menu items and their shortcuts. A field commits its text on `change`, which blurring fires,
    // so that happens first; saving an invalid state is refused by the backend with its errors.
    let command = move |id: &str| {
        blur_focused();
        match id {
            "open" => open(),
            "reload" => load(),
            "save" | "save_apply" if !store.docs.with(Vec::is_empty) => save(id == "save_apply"),
            id => {
                if let Some(t) = id.strip_prefix("theme:") {
                    theme.set(t.into());
                    set_theme(t);
                } else if let Some(l) = id.strip_prefix("lang:") {
                    change_language(l.into());
                }
            }
        }
    };
    on_menu(move |id| command(&id));
    // The menu only shows these shortcuts; the keys reach the web view, so they are handled here.
    let _ = window_event_listener(leptos::ev::keydown, move |ev| {
        let id = match (ev.ctrl_key(), ev.shift_key(), ev.key().to_ascii_lowercase().as_str()) {
            (true, false, "o") => "open",
            (true, false, "s") => "save",
            (true, true, "s") => "save_apply",
            _ => return,
        };
        ev.prevent_default();
        command(id);
    });
    let poll = move || {
        spawn_local(async move { running.set(call::<_, bool, ()>("grapnel_running", &()).await.unwrap_or(false)) })
    };
    poll();
    let _ = set_interval_with_handle(poll, std::time::Duration::from_secs(3));
    let dirty = Memo::new(move |_| store.docs.with(|d| snapshot(d)) != saved.get());
    let tabs = || {
        TABS.iter().map(|k| {
            let key = format!("ui.tab.{k}");
            t!(&key).into_owned()
        })
    };

    // Everything is rebuilt when the language changes; the state lives in signals above.
    move || {
        lang.track();
        view! {
        <header>
            <select class="files" title=t!("ui.file") aria-label=t!("ui.file")
                prop:value=move || store.cur.get().to_string()
                on:change=move |ev| store.cur.set(event_target_value(&ev).parse().unwrap_or(0))>
                <For each=move || indices(store.docs.with(Vec::len)) key=|i| *i let:i>
                    <option value=i.to_string()>
                        {move || store.docs.with(|d| d.get(i).map(|f| f.path.clone()).unwrap_or_default())}
                    </option>
                </For>
            </select>
            <button on:click=move |_| save(false) disabled=cannot_save>{t!("ui.save")}</button>
            <button class="primary" on:click=move |_| save(true) disabled=cannot_save>{t!("ui.save_apply")}</button>
        </header>
        <div class="content">
        <nav class="tabs">
            {tabs().enumerate().map(|(i, name)| view! {
                <button class:active=move || tab.get() == i on:click=move |_| tab.set(i)>{name}</button>
            }).collect_view()}
        </nav>
        <ul class="errors">
            {move || errors.get().into_iter().map(|e| view! { <li>{e.to_string()}</li> }).collect_view()}
        </ul>
        <Show when=move || !store.docs.with(Vec::is_empty)>
            <main>
                <p class="hint">{move || { let key = format!("ui.tab_hint.{}", TABS[tab.get()]); t!(&key).into_owned() }}</p>
                {move || match tab.get() {
                    0 => sections::imports().into_any(),
                    1 => sections::keymap().into_any(),
                    2 => sections::modes().into_any(),
                    3 => sections::keyswap().into_any(),
                    4 => sections::modifiers().into_any(),
                    5 => sections::targets().into_any(),
                    6 => actions::actions().into_any(),
                    _ => sections::others().into_any(),
                }}
            </main>
        </Show>
        </div>
        <footer class="statusbar">
            <span class="path">{move || entry.get()}</span>
            <Show when=move || dirty.get()><span>{t!("ui.bar.unsaved")}</span></Show>
            <Show when=move || !errors.with(Vec::is_empty)>
                <span class="bad">{move || t!("ui.bar.errors", count = errors.with(Vec::len)).into_owned()}</span>
            </Show>
            <span class="message">{move || status.get()}</span>
            <span class:bad=move || !running.get()>
                {move || if running.get() { t!("ui.bar.running") } else { t!("ui.bar.not_running") }}
            </span>
        </footer>
        }
        .into_any()
    }
}

/// Tab keys in display order (`ui.tab.*`, `ui.tab_hint.*`); the index picks the section below.
const TABS: [&str; 8] = ["imports", "keymap", "modes", "keyswap", "modifiers", "targets", "actions", "others"];

fn main() {
    console_error_panic_hook_set();
    leptos::mount::mount_to_body(App);
}

fn console_error_panic_hook_set() {
    std::panic::set_hook(Box::new(|info| web_sys_log(&info.to_string())));
}

#[wasm_bindgen]
extern "C" {
    #[wasm_bindgen(js_namespace = console, js_name = error)]
    fn web_sys_log(s: &str);
}
