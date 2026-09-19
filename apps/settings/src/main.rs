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
struct Entry {
    entry: String,
    lang: String,
}

impl Entry {
    fn of(entry: String) -> Entry {
        Entry { entry, lang: rust_i18n::locale().to_string() }
    }
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

/// The theme after `theme`.
fn next_theme(theme: &str) -> &'static str {
    match theme {
        "auto" => "light",
        "light" => "dark",
        _ => "auto",
    }
}

/// 24x24 icon path for `theme`, from Google Material Icons (Apache-2.0, see THIRD_PARTY_NOTICES.md).
fn theme_icon(theme: &str) -> &'static str {
    match theme {
        "light" => {
            "M12 7c-2.76 0-5 2.24-5 5s2.24 5 5 5 5-2.24 5-5-2.24-5-5-5zM2 13h2c.55 0 1-.45 1-1s-.45-1-1-1H2c-.55 0-1 .45-1 1s.45 1 1 1zm18 0h2c.55 0 1-.45 1-1s-.45-1-1-1h-2c-.55 0-1 .45-1 1s.45 1 1 1zM11 2v2c0 .55.45 1 1 1s1-.45 1-1V2c0-.55-.45-1-1-1s-1 .45-1 1zm0 18v2c0 .55.45 1 1 1s1-.45 1-1v-2c0-.55-.45-1-1-1s-1 .45-1 1zM5.99 4.58c-.39-.39-1.03-.39-1.41 0-.39.39-.39 1.03 0 1.41l1.06 1.06c.39.39 1.03.39 1.41 0s.39-1.03 0-1.41L5.99 4.58zm12.37 12.37c-.39-.39-1.03-.39-1.41 0-.39.39-.39 1.03 0 1.41l1.06 1.06c.39.39 1.03.39 1.41 0 .39-.39.39-1.03 0-1.41l-1.06-1.06zm1.06-10.96c.39-.39.39-1.03 0-1.41-.39-.39-1.03-.39-1.41 0l-1.06 1.06c-.39.39-.39 1.03 0 1.41s1.03.39 1.41 0l1.06-1.06zM7.05 18.36c.39-.39.39-1.03 0-1.41-.39-.39-1.03-.39-1.41 0l-1.06 1.06c-.39.39-.39 1.03 0 1.41s1.03.39 1.41 0l1.06-1.06z"
        }
        "dark" => {
            "M12 3c-4.97 0-9 4.03-9 9s4.03 9 9 9 9-4.03 9-9c0-.46-.04-.92-.1-1.36-.98 1.37-2.58 2.26-4.4 2.26-2.98 0-5.4-2.42-5.4-5.4 0-1.81.89-3.42 2.26-4.4-.44-.06-.9-.1-1.36-.1z"
        }
        _ => {
            "M10.85 12.65h2.3L12 9l-1.15 3.65zM20 8.69V4h-4.69L12 .69 8.69 4H4v4.69L.69 12 4 15.31V20h4.69L12 23.31 15.31 20H20v-4.69L23.31 12 20 8.69zM14.3 16l-.7-2h-3.2l-.7 2H7.8L11 7h2l3.2 9h-1.9z"
        }
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
}

#[component]
fn App() -> impl IntoView {
    let store = Store { docs: RwSignal::new(vec![]), cur: RwSignal::new(0), errors: RwSignal::new(vec![]) };
    provide_context(store);
    let entry = RwSignal::new(String::new());
    let tab = RwSignal::new(0usize);
    let errors = store.errors;
    let status = RwSignal::new(String::new());
    let lang = RwSignal::new(initial_language());
    set_language(&lang.get_untracked());
    let theme = RwSignal::new(saved_theme());
    set_theme(&theme.get_untracked());

    let load = move || {
        spawn_local(async move {
            match call::<_, String, Vec<Problem>>("load", &Entry::of(entry.get_untracked())).await {
                Ok(json) => {
                    let docs: Vec<FileDoc> = serde_json::from_str(&json).unwrap();
                    store.cur.set(0);
                    store.docs.set(docs);
                    status.set(t!("ui.status.loaded").into());
                }
                Err(e) => errors.set(e),
            }
        })
    };
    let open = move || {
        spawn_local(async move {
            let picked = call::<_, Option<String>, ()>("pick_entry", &Entry::of(entry.get_untracked())).await;
            if let Ok(Some(path)) = picked {
                entry.set(path);
                load();
            }
        })
    };
    spawn_local(async move {
        entry.set(call::<_, String, String>("initial_entry", &()).await.unwrap_or_default());
        load();
    });
    // Validate after every edit, and again in a newly chosen language.
    Effect::new(move |_| {
        lang.track();
        let files = store.docs.get();
        if !files.is_empty() {
            let files = Files::of(&files);
            spawn_local(async move { errors.set(call::<_, Vec<Problem>, ()>("validate", &files).await.unwrap()) });
        }
    });
    let save = move |apply: bool| {
        spawn_local(async move {
            let files = Files::of(&store.docs.get_untracked());
            if let Err(e) = call::<_, (), SaveError>("save", &files).await {
                errors.set(e.errors);
                let text = if e.saved { t!("ui.status.saved_with_errors") } else { t!("ui.status.save_failed") };
                return status.set(text.into());
            }
            status.set(t!("ui.status.saved").into());
            if apply {
                match call::<_, (), String>("apply", &()).await {
                    Ok(()) => status.set(t!("ui.status.applied").into()),
                    Err(e) => status.set(t!("ui.status.apply_failed", error = e).into()),
                }
            }
        })
    };
    let cannot_save = move || !errors.with(Vec::is_empty) || store.docs.with(Vec::is_empty);

    let change_language = move |ev| {
        let l = event_target_value(&ev);
        set_language(&l);
        status.set(String::new());
        lang.set(l);
    };
    let tabs = || {
        [t!("ui.tab.general"), t!("ui.tab.modes"), t!("ui.tab.modifiers")].into_iter().chain([
            t!("ui.tab.targets"),
            t!("ui.tab.keymap"),
            t!("ui.tab.keyswap"),
            t!("ui.tab.actions"),
        ])
    };

    // Everything is rebuilt when the language changes; the state lives in signals above.
    move || {
        lang.track();
        view! {
        <header>
            <span class="brand">"grapnel"</span>
            <button on:click=move |_| open()>{t!("ui.open")}</button>
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
            <select class="lang" title="Language" prop:value=move || lang.get() on:change=change_language>
                {languages().into_iter().map(|l| {
                    let name = t!("language_name", locale = &l).into_owned();
                    view! { <option value=l.into_owned()>{name}</option> }
                }).collect_view()}
            </select>
            {move || {
                let name = match theme.get().as_str() {
                    "light" => t!("ui.theme.light"),
                    "dark" => t!("ui.theme.dark"),
                    _ => t!("ui.theme.auto"),
                };
                let label = t!("ui.theme.label", name = name).into_owned();
                view! {
                    <button class="icon" title=label.clone() aria-label=label
                        on:click=move |_| theme.update(|t| { *t = next_theme(t).into(); set_theme(t) })>
                        <svg viewBox="0 0 24 24" aria-hidden="true"><path d=theme_icon(&theme.get()) /></svg>
                    </button>
                }
            }}
        </header>
        <div class="content">
        <nav class="tabs">
            {tabs().enumerate().map(|(i, name)| view! {
                <button class:active=move || tab.get() == i on:click=move |_| tab.set(i)>{name}</button>
            }).collect_view()}
            <span class="status">{move || status.get()}</span>
        </nav>
        <ul class="errors">
            {move || errors.get().into_iter().map(|e| view! { <li>{e.to_string()}</li> }).collect_view()}
        </ul>
        <Show when=move || !store.docs.with(Vec::is_empty)>
            <main>
                {move || match tab.get() {
                    0 => sections::settings().into_any(),
                    1 => sections::modes().into_any(),
                    2 => sections::modifiers().into_any(),
                    3 => sections::targets().into_any(),
                    4 => sections::keymap().into_any(),
                    5 => sections::keyswap().into_any(),
                    _ => actions::actions().into_any(),
                }}
            </main>
        </Show>
        </div>
        }
        .into_any()
    }
}

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
