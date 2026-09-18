//! grapnel-settings: loads, validates and saves grapnel config files; tells grapnel to reload.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use grapnel_schema::RawConfig;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// One config file as edited in the UI.
#[derive(Serialize, Deserialize)]
struct FileDoc {
    path: String,
    raw: RawConfig,
}

fn to_pairs(files: Vec<FileDoc>) -> Vec<(PathBuf, RawConfig)> {
    files.into_iter().map(|f| (PathBuf::from(f.path), f.raw)).collect()
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

/// Reads the entry file and everything it includes. A missing entry yields one empty file.
#[tauri::command]
fn load(entry: String) -> Result<Vec<FileDoc>, Vec<String>> {
    let path = PathBuf::from(&entry);
    if !path.exists() {
        return Ok(vec![FileDoc { path: entry, raw: RawConfig::default() }]);
    }
    let files = grapnel_config::load(&path)?;
    Ok(files.into_iter().map(|(p, raw)| FileDoc { path: p.display().to_string(), raw }).collect())
}

/// Returns every validation error (empty when valid).
#[tauri::command]
fn validate(files: Vec<FileDoc>) -> Vec<String> {
    grapnel_config::compile(&to_pairs(files)).err().unwrap_or_default()
}

/// Validates, then writes every file. Nothing is written when invalid.
#[tauri::command]
fn save(files: Vec<FileDoc>) -> Result<(), Vec<String>> {
    let pairs = to_pairs(files);
    grapnel_config::compile(&pairs)?;
    for (path, raw) in &pairs {
        grapnel_config::save(path, raw).map_err(|e| vec![format!("{}: {e}", path.display())])?;
    }
    Ok(())
}

/// Asks the running grapnel to reload its configuration.
#[tauri::command]
fn apply() -> Result<(), String> {
    grapnel_win::pipe::send("reload").map_err(|e| format!("grapnel に接続できません (起動していますか？): {e}"))
}

fn main() {
    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![initial_entry, load, validate, save, apply])
        .run(tauri::generate_context!())
        .expect("error while running grapnel-settings");
}
