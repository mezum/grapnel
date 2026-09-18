//! Reading config files and expanding `include`.

use crate::{Matcher, RawConfig};
use std::path::{Path, PathBuf};

/// `%APPDATA%\grapnel\config.toml`.
pub fn default_entry() -> PathBuf {
    let base = std::env::var_os("APPDATA").map(PathBuf::from).unwrap_or_default();
    base.join("grapnel").join("config.toml")
}

/// Reads `entry` and every included file, depth-first in include order. Each file is read once.
pub fn load(entry: &Path) -> Result<Vec<(PathBuf, RawConfig)>, Vec<String>> {
    let mut out = Vec::new();
    let mut errors = Vec::new();
    visit(entry, &mut out, &mut errors);
    if errors.is_empty() { Ok(out) } else { Err(errors) }
}

fn visit(path: &Path, out: &mut Vec<(PathBuf, RawConfig)>, errors: &mut Vec<String>) {
    let canon = std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
    if out.iter().any(|(p, _)| std::fs::canonicalize(p).is_ok_and(|p| p == canon)) {
        return;
    }
    let raw: RawConfig = match std::fs::read_to_string(path).map_err(|e| e.to_string()).and_then(|s| {
        toml::from_str(&s).map_err(|e| e.to_string())
    }) {
        Ok(r) => r,
        Err(e) => return errors.push(format!("{}: {e}", path.display())),
    };
    let includes = raw.include.clone();
    out.push((path.to_path_buf(), raw));
    let dir = path.parent().unwrap_or(Path::new("."));
    for pattern in includes {
        match expand(&dir.join(&pattern)) {
            Ok(paths) => paths.iter().for_each(|p| visit(p, out, errors)),
            Err(e) => errors.push(format!("{}: include '{pattern}': {e}", path.display())),
        }
    }
}

/// Expands `*`/`?` in the last path component only.
// ponytail: wildcards only in the file name; add a glob crate if directory wildcards are needed.
fn expand(pattern: &Path) -> Result<Vec<PathBuf>, String> {
    let name = pattern.file_name().and_then(|n| n.to_str()).unwrap_or_default();
    if !name.contains(['*', '?']) {
        return if pattern.is_file() { Ok(vec![pattern.to_path_buf()]) } else { Err("file not found".into()) };
    }
    let m = Matcher::parse(&format!("glob:{name}"))?;
    let dir = pattern.parent().unwrap_or(Path::new("."));
    let entries = std::fs::read_dir(dir).map_err(|e| e.to_string())?;
    let mut paths: Vec<PathBuf> = entries
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.is_file() && p.file_name().and_then(|n| n.to_str()).is_some_and(|n| m.is_match(n)))
        .collect();
    paths.sort();
    Ok(paths)
}

/// Writes one file. Comments in the original file are not preserved.
pub fn save(path: &Path, raw: &RawConfig) -> std::io::Result<()> {
    let text = toml::to_string_pretty(raw).map_err(std::io::Error::other)?;
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    std::fs::write(path, text)
}
