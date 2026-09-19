//! grapnel-settings: loads, validates and saves grapnel config files; tells grapnel to reload.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use grapnel_config::Problem;
use grapnel_keys::msg;
use grapnel_schema::RawConfig;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::Mutex;

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
#[tauri::command]
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
#[tauri::command]
fn validate(files: String, lang: String) -> Vec<Shown> {
    let problems = to_pairs(&files).and_then(|p| grapnel_config::compile(&p).map(drop)).err();
    shown(problems.unwrap_or_default(), &lang)
}

/// Validates, then writes every changed file (each atomically), so unchanged ones keep their
/// comments. Nothing is written when invalid.
/// Afterwards the files are re-read from disk and checked again, which catches `include` edits
/// that change which files belong to the configuration.
#[tauri::command]
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
#[tauri::command]
fn apply(loaded: tauri::State<Loaded>) -> Result<(), String> {
    let entry = loaded.0.lock().unwrap().first().map(|(p, _)| p.clone()).ok_or("nothing loaded")?;
    let entry = std::path::absolute(&entry).map_err(|e| e.to_string())?;
    grapnel_win::pipe::send(&format!("reload {}", entry.display())).map_err(|e| e.to_string())
}

fn main() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .manage(Loaded::default())
        .invoke_handler(tauri::generate_handler![initial_entry, pick_entry, load, validate, save, apply])
        .run(tauri::generate_context!())
        .expect("error while running grapnel-settings");
}
