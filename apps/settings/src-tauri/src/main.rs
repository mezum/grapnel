//! grapnel-settings: loads, validates and saves grapnel config files; tells grapnel to reload.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use grapnel_schema::RawConfig;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::Mutex;

/// Paths the backend loaded; `save` refuses to write anywhere else.
#[derive(Default)]
struct Loaded(Mutex<Vec<PathBuf>>);

/// One config file as edited in the UI.
#[derive(Serialize, Deserialize)]
struct FileDoc {
    path: String,
    raw: RawConfig,
}

/// Files travel as JSON text so key order survives the JavaScript side.
fn to_pairs(files: &str) -> Result<Vec<(PathBuf, RawConfig)>, Vec<String>> {
    let files: Vec<FileDoc> = serde_json::from_str(files).map_err(|e| vec![e.to_string()])?;
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
fn load(entry: String, loaded: tauri::State<Loaded>) -> Result<String, Vec<String>> {
    let path = PathBuf::from(&entry);
    let files = if path.exists() { grapnel_config::load(&path)? } else { vec![(path, RawConfig::default())] };
    *loaded.0.lock().unwrap() = files.iter().map(|(p, _)| p.clone()).collect();
    let docs: Vec<FileDoc> = files.into_iter().map(|(p, raw)| FileDoc { path: p.display().to_string(), raw }).collect();
    serde_json::to_string(&docs).map_err(|e| vec![e.to_string()])
}

/// Returns every validation error (empty when valid).
#[tauri::command]
fn validate(files: String) -> Vec<String> {
    to_pairs(&files).and_then(|p| grapnel_config::compile(&p).map(drop)).err().unwrap_or_default()
}

/// Validates, then writes every file (each atomically). Nothing is written when invalid.
/// Afterwards the files are re-read from disk and checked again, which catches `include` edits
/// that change which files belong to the configuration.
#[tauri::command]
fn save(files: String, loaded: tauri::State<Loaded>) -> Result<(), SaveError> {
    let unsaved = |errors| SaveError { saved: false, errors };
    let pairs = to_pairs(&files).map_err(unsaved)?;
    let known = loaded.0.lock().unwrap().clone();
    if let Some((p, _)) = pairs.iter().find(|(p, _)| !known.contains(p)) {
        return Err(unsaved(vec![format!("{}: not loaded, so it cannot be saved", p.display())]));
    }
    grapnel_config::compile(&pairs).map_err(unsaved)?;
    // ponytail: files are replaced one by one; a failure midway leaves earlier files saved.
    for (i, (path, raw)) in pairs.iter().enumerate() {
        let errors = |e| vec![format!("{}: {e}", path.display())];
        grapnel_config::save(path, raw).map_err(|e| SaveError { saved: i > 0, errors: errors(e) })?;
    }
    let reread = grapnel_config::load(&pairs[0].0).and_then(|f| grapnel_config::compile(&f));
    reread.map(drop).map_err(|errors| SaveError { saved: true, errors })
}

/// Errors are English; the front end words the status line.
#[derive(serde::Serialize)]
struct SaveError {
    /// Some or all files were written (a later write or reading them back failed).
    saved: bool,
    errors: Vec<String>,
}

/// Asks the running grapnel to reload its configuration.
#[tauri::command]
fn apply() -> Result<(), String> {
    grapnel_win::pipe::send("reload").map_err(|e| e.to_string())
}

fn main() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .manage(Loaded::default())
        .invoke_handler(tauri::generate_handler![initial_entry, pick_entry, load, validate, save, apply])
        .run(tauri::generate_context!())
        .expect("error while running grapnel-settings");
}
