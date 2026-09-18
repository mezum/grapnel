//! grapnel-settings front end: form editing of every loaded config file.

mod actions;
mod fields;
mod keymap;
mod sections;

use fields::indices;
use grapnel_schema::RawConfig;
use leptos::prelude::*;
use leptos::task::spawn_local;
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
struct Files {
    files: Vec<FileDoc>,
}

#[derive(Serialize)]
struct Entry {
    entry: String,
}

/// Shared UI state.
#[derive(Clone, Copy)]
pub struct Store {
    pub docs: RwSignal<Vec<FileDoc>>,
    pub cur: RwSignal<usize>,
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

    /// User modifier names across all files, in compile order.
    pub fn user_mods(&self) -> Vec<String> {
        self.docs.with(|d| d.iter().flat_map(|f| f.raw.modifiers.keys().cloned()).collect())
    }
}

const TABS: [&str; 6] = ["全般", "モード", "修飾キー", "ターゲット", "キーマップ", "アクション"];

#[component]
fn App() -> impl IntoView {
    let store = Store { docs: RwSignal::new(vec![]), cur: RwSignal::new(0) };
    provide_context(store);
    let entry = RwSignal::new(String::new());
    let tab = RwSignal::new(0usize);
    let errors = RwSignal::new(Vec::<String>::new());
    let status = RwSignal::new(String::new());

    let load = move || {
        spawn_local(async move {
            match call::<_, Vec<FileDoc>, Vec<String>>("load", &Entry { entry: entry.get_untracked() }).await {
                Ok(docs) => {
                    store.cur.set(0);
                    store.docs.set(docs);
                    status.set("読み込みました".into());
                }
                Err(e) => errors.set(e),
            }
        })
    };
    spawn_local(async move {
        entry.set(call::<_, String, String>("initial_entry", &()).await.unwrap_or_default());
        load();
    });
    // Validate after every edit.
    Effect::new(move |_| {
        let files = store.docs.get();
        if !files.is_empty() {
            spawn_local(
                async move { errors.set(call::<_, Vec<String>, ()>("validate", &Files { files }).await.unwrap()) },
            );
        }
    });
    let save = move |apply: bool| {
        spawn_local(async move {
            let files = Files { files: store.docs.get_untracked() };
            if let Err(e) = call::<_, (), Vec<String>>("save", &files).await {
                errors.set(e);
                return status.set("保存できませんでした。エラーを確認してください".into());
            }
            status.set("保存しました".into());
            if apply {
                match call::<_, (), String>("apply", &()).await {
                    Ok(()) => status.set("保存して grapnel に再読み込みを指示しました".into()),
                    Err(e) => status.set(e),
                }
            }
        })
    };
    let cannot_save = move || !errors.with(Vec::is_empty) || store.docs.with(Vec::is_empty);

    view! {
        <header>
            <input class="entry" prop:value=move || entry.get() on:change=move |ev| entry.set(event_target_value(&ev)) />
            <button on:click=move |_| load()>"読み込み"</button>
            <button on:click=move |_| save(false) disabled=cannot_save>"保存"</button>
            <button on:click=move |_| save(true) disabled=cannot_save>"保存して適用"</button>
            <span class="status">{move || status.get()}</span>
        </header>
        <nav class="files">
            <For each=move || indices(store.docs.with(Vec::len)) key=|i| *i let:i>
                <button class:active=move || store.cur.get() == i on:click=move |_| store.cur.set(i)>
                    {move || store.docs.with(|d| d.get(i).map(|f| f.path.clone()).unwrap_or_default())}
                </button>
            </For>
        </nav>
        <nav class="tabs">
            {TABS.iter().enumerate().map(|(i, name)| view! {
                <button class:active=move || tab.get() == i on:click=move |_| tab.set(i)>{*name}</button>
            }).collect_view()}
        </nav>
        <ul class="errors">
            {move || errors.get().into_iter().map(|e| view! { <li>{e}</li> }).collect_view()}
        </ul>
        <Show when=move || !store.docs.with(Vec::is_empty)>
            <main>
                {move || match tab.get() {
                    0 => sections::settings().into_any(),
                    1 => sections::modes().into_any(),
                    2 => sections::modifiers().into_any(),
                    3 => sections::targets().into_any(),
                    4 => sections::keymap().into_any(),
                    _ => actions::actions().into_any(),
                }}
            </main>
        </Show>
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
