//! grapnel-settings: loads, validates and saves grapnel config files; tells grapnel to reload.
//! Commands that touch the disk or a pipe are `#[tauri::command(async)]`: without it Tauri runs
//! them on the main thread, where they freeze the window.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use grapnel_config::Problem;
use grapnel_keys::msg;
use grapnel_schema::RawConfig;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::Mutex;
use tauri::Emitter;
use tauri::menu::{Menu, MenuItem, PredefinedMenuItem, Submenu};

rust_i18n::i18n!("../../../locales", fallback = "en");

/// Files the backend loaded, as last read or written. `save` refuses to write anywhere else and
/// skips files whose content did not change.
#[derive(Default)]
struct Loaded(Mutex<Vec<(PathBuf, RawConfig)>>);

/// One config file as edited in the UI.
#[derive(Serialize, Deserialize)]
struct FileDoc {
    path: String,
    raw: RawConfig,
}

/// A problem as the front end shows it: `text` is the message alone, in the UI language.
#[derive(Serialize)]
struct Shown {
    file: String,
    at: String,
    on_key: bool,
    text: String,
}

fn shown(problems: Vec<Problem>, lang: &str) -> Vec<Shown> {
    let file = |p: &Problem| p.file.as_ref().map(|f| f.display().to_string()).unwrap_or_default();
    let show = |p: Problem| Shown { file: file(&p), at: p.at.clone(), on_key: p.on_key, text: p.msg.text(lang) };
    problems.into_iter().map(show).collect()
}

fn problem(file: Option<PathBuf>, msg: grapnel_keys::Msg) -> Vec<Problem> {
    vec![Problem { file, at: String::new(), on_key: false, msg }]
}

/// Files travel as JSON text so key order survives the JavaScript side.
fn to_pairs(files: &str) -> Result<Vec<(PathBuf, RawConfig)>, Vec<Problem>> {
    let files: Vec<FileDoc> =
        serde_json::from_str(files).map_err(|e| problem(None, msg!("config.internal", error = e)))?;
    Ok(files.into_iter().map(|f| (PathBuf::from(f.path), f.raw)).collect())
}

/// Entry file from `--config`, else the default location.
#[tauri::command]
fn initial_entry() -> String {
    let args: Vec<String> = std::env::args().collect();
    let path = match args.iter().position(|a| a == "--config") {
        Some(i) if i + 1 < args.len() => PathBuf::from(&args[i + 1]),
        _ => grapnel_config::default_entry(),
    };
    path.display().to_string()
}

/// Asks for an entry file with the Windows open dialog, starting next to `entry`.
#[tauri::command]
async fn pick_entry(window: tauri::Window, entry: String) -> Option<String> {
    use tauri_plugin_dialog::DialogExt;
    let mut dialog = window.dialog().file().set_parent(&window).add_filter("TOML", &["toml"]);
    if let Some(dir) = std::path::Path::new(&entry).parent().filter(|d| d.is_dir()) {
        dialog = dialog.set_directory(dir);
    }
    Some(dialog.blocking_pick_file()?.into_path().ok()?.display().to_string())
}

/// Reads the entry file and everything it includes. A missing entry yields one empty file.
#[tauri::command(async)]
fn load(entry: String, lang: String, loaded: tauri::State<Loaded>) -> Result<String, Vec<Shown>> {
    let path = PathBuf::from(&entry);
    let files = match path.exists() {
        true => grapnel_config::load(&path).map_err(|e| shown(e, &lang))?,
        false => vec![(path, RawConfig::default())],
    };
    *loaded.0.lock().unwrap() = files.clone();
    let docs: Vec<FileDoc> = files.into_iter().map(|(p, raw)| FileDoc { path: p.display().to_string(), raw }).collect();
    serde_json::to_string(&docs).map_err(|e| shown(problem(None, msg!("config.internal", error = e)), &lang))
}

/// Returns every validation error (empty when valid).
#[tauri::command(async)]
fn validate(files: String, lang: String) -> Vec<Shown> {
    let problems = to_pairs(&files).and_then(|p| grapnel_config::compile(&p).map(drop)).err();
    shown(problems.unwrap_or_default(), &lang)
}

/// Validates, then writes every changed file (each atomically), so unchanged ones keep their
/// comments. Nothing is written when invalid.
/// Afterwards the files are re-read from disk and checked again, which catches `include` edits
/// that change which files belong to the configuration.
#[tauri::command(async)]
fn save(files: String, lang: String, loaded: tauri::State<Loaded>) -> Result<(), SaveError> {
    let fail = |saved, errors| SaveError { saved, errors: shown(errors, &lang) };
    let pairs = to_pairs(&files).map_err(|e| fail(false, e))?;
    let mut known = loaded.0.lock().unwrap();
    if let Some((p, _)) = pairs.iter().find(|(p, _)| !known.iter().any(|(k, _)| k == p)) {
        return Err(fail(false, problem(Some(p.clone()), msg!("config.not_loaded"))));
    }
    grapnel_config::compile(&pairs).map_err(|e| fail(false, e))?;
    // ponytail: files are replaced one by one; a failure midway leaves earlier files saved.
    let mut wrote = false;
    for (path, raw) in &pairs {
        let before = known.iter_mut().find(|(k, _)| k == path).map(|(_, r)| r).unwrap();
        if before == raw && path.exists() {
            continue;
        }
        let error = |e: std::io::Error| problem(Some(path.clone()), msg!("config.write", error = e));
        grapnel_config::save(path, raw).map_err(|e| fail(wrote, error(e)))?;
        *before = raw.clone();
        wrote = true;
    }
    drop(known);
    let reread = grapnel_config::load(&pairs[0].0).and_then(|f| grapnel_config::compile(&f));
    reread.map(drop).map_err(|e| fail(true, e))
}

#[derive(Serialize)]
struct SaveError {
    /// Some or all files were written (a later write or reading them back failed).
    saved: bool,
    errors: Vec<Shown>,
}

/// Asks the running grapnel to reload from the entry file edited here, which it keeps using.
#[tauri::command(async)]
fn apply(loaded: tauri::State<Loaded>) -> Result<(), String> {
    let entry = loaded.0.lock().unwrap().first().map(|(p, _)| p.clone()).ok_or("nothing loaded")?;
    let entry = std::path::absolute(&entry).map_err(|e| e.to_string())?;
    grapnel_win::pipe::send(&format!("reload {}", entry.display())).map_err(|e| e.to_string())
}

/// Whether the running grapnel's control pipe exists (listing pipes does not connect to it).
#[tauri::command(async)]
fn grapnel_running() -> bool {
    let name = grapnel_win::pipe::name();
    let name = name.rsplit('\\').next().unwrap_or_default().to_string();
    let pipes = std::fs::read_dir(r"\\.\pipe\").into_iter().flatten().flatten();
    pipes.map(|e| e.file_name()).any(|n| n.to_string_lossy().eq_ignore_ascii_case(&name))
}

/// The `--bg` of `styles.css` for the Windows theme. Windows paints the window layer, not the page,
/// so it is what shows around the web view and while it is torn down on closing.
fn background(dark: bool) -> tauri::window::Color {
    match dark {
        true => tauri::window::Color(0x17, 0x18, 0x1b, 0xff),
        false => tauri::window::Color(0xf4, 0xf5, 0xf7, 0xff),
    }
}

/// Builds the menu bar in `lang`; the front end calls it again when the language changes. Items
/// other than Help are handled by the front end, which gets their ids as `menu` events. Shortcuts
/// are only shown here (after a tab); the page handles the keys, so they work in the web view.
#[tauri::command]
fn set_menu(app: tauri::AppHandle, lang: String) -> tauri::Result<()> {
    let t = |key: &str| rust_i18n::t!(key, locale = &lang).into_owned();
    let item = |id: &str, key: &str, keys: &str| {
        let label = if keys.is_empty() { t(key) } else { format!("{}\t{keys}", t(key)) };
        MenuItem::with_id(&app, id, label, true, None::<&str>)
    };
    let sep = || PredefinedMenuItem::separator(&app);
    let file = Submenu::with_items(
        &app,
        t("ui.menu.file"),
        true,
        &[
            &item("open", "ui.menu.open", "Ctrl+O")?,
            &item("reload", "ui.menu.reload", "")?,
            &sep()?,
            &item("save", "ui.menu.save", "Ctrl+S")?,
            &item("save_apply", "ui.menu.save_apply", "Ctrl+Shift+S")?,
            &sep()?,
            &item("exit", "ui.menu.exit", "")?,
        ],
    )?;
    let theme = Submenu::with_items(
        &app,
        t("ui.menu.theme"),
        true,
        &[
            &item("theme:auto", "ui.theme.auto", "")?,
            &item("theme:light", "ui.theme.light", "")?,
            &item("theme:dark", "ui.theme.dark", "")?,
        ],
    )?;
    let language = Submenu::new(&app, t("ui.menu.language"), true)?;
    for l in rust_i18n::available_locales!() {
        let name = rust_i18n::t!("language_name", locale = l).into_owned();
        language.append(&MenuItem::with_id(&app, format!("lang:{l}"), name, true, None::<&str>)?)?;
    }
    let view = Submenu::with_items(&app, t("ui.menu.view"), true, &[&theme, &language])?;
    let help = Submenu::with_items(
        &app,
        t("ui.menu.help"),
        true,
        &[&item("docs", "ui.menu.docs", "")?, &item("about", "ui.menu.about", "")?],
    )?;
    app.set_menu(Menu::with_items(&app, &[&file, &view, &help])?)?;
    *ABOUT.lock().unwrap() = t("ui.menu.about_text").replace("%{version}", env!("CARGO_PKG_VERSION"));
    Ok(())
}

/// The About text in the menu's language.
static ABOUT: Mutex<String> = Mutex::new(String::new());

fn on_menu(app: &tauri::AppHandle, id: &str) {
    use tauri_plugin_dialog::DialogExt;
    match id {
        "docs" => {
            let url = "https://github.com/mezum/grapnel/blob/main/docs/spec.md";
            if let Err(e) = std::process::Command::new("explorer").arg(url).spawn() {
                eprintln!("cannot open {url}: {e}");
            }
        }
        "about" => app.dialog().message(ABOUT.lock().unwrap().clone()).title("grapnel-settings").show(|_| {}),
        "exit" => app.exit(0),
        id => drop(app.emit("menu", id)),
    }
}

fn main() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .manage(Loaded::default())
        .on_menu_event(|app, ev| on_menu(app, ev.id().as_ref()))
        // The window is built here rather than in `tauri.conf.json` so that its background colour
        // is right from the first frame: setting it afterwards only lands once the web view
        // attaches, a few hundred milliseconds of white.
        .setup(|app| {
            let window = tauri::WebviewWindowBuilder::new(app, "main", tauri::WebviewUrl::default())
                .title("grapnel")
                .inner_size(1100.0, 760.0)
                .disable_drag_drop_handler()
                .background_color(background(grapnel_win::dark_mode()))
                .build()?;
            let themed = window.clone();
            window.on_window_event(move |ev| {
                if let tauri::WindowEvent::ThemeChanged(theme) = ev {
                    let _ = themed.set_background_color(Some(background(*theme == tauri::Theme::Dark)));
                }
            });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            initial_entry,
            pick_entry,
            load,
            validate,
            save,
            apply,
            set_menu,
            grapnel_running
        ])
        .run(tauri::generate_context!())
        .expect("error while running grapnel-settings");
}
