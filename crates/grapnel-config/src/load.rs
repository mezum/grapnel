//! Reading config files and expanding `include`.

use crate::{Matcher, Problem, RawConfig};
use grapnel_keys::{Msg, msg};
use std::path::{Path, PathBuf};

/// `%APPDATA%\grapnel\config.toml`.
pub fn default_entry() -> PathBuf {
    let base = std::env::var_os("APPDATA").map(PathBuf::from).unwrap_or_default();
    base.join("grapnel").join("config.toml")
}

/// Reads `entry` and every included file, depth-first in include order. Each file is read once.
pub fn load(entry: &Path) -> Result<Vec<(PathBuf, RawConfig)>, Vec<Problem>> {
    let mut out = Vec::new();
    let mut errors = Vec::new();
    visit(entry, &mut out, &mut errors);
    if errors.is_empty() { Ok(out) } else { Err(errors) }
}

fn visit(path: &Path, out: &mut Vec<(PathBuf, RawConfig)>, errors: &mut Vec<Problem>) {
    let canon = std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
    if out.iter().any(|(p, _)| std::fs::canonicalize(p).is_ok_and(|p| p == canon)) {
        return;
    }
    let problem = |at: String, msg| Problem { file: Some(path.to_path_buf()), at, on_key: false, msg };
    let raw: RawConfig = match std::fs::read_to_string(path)
        .map_err(|e| msg!("config.read", error = e))
        .and_then(|s| toml::from_str(&s).map_err(|e| msg!("config.syntax", error = e)))
    {
        Ok(r) => r,
        Err(e) => return errors.push(problem(String::new(), e)),
    };
    let includes = raw.include.clone();
    out.push((path.to_path_buf(), raw));
    let dir = path.parent().unwrap_or(Path::new("."));
    for (i, pattern) in includes.iter().enumerate() {
        match expand(&dir.join(pattern)) {
            Ok(paths) => paths.iter().for_each(|p| visit(p, out, errors)),
            Err(e) => errors.push(problem(format!("include[{i}]"), e)),
        }
    }
}

/// An include pattern written both ways. `rel` is `None` where no relative path reaches it: on
/// another drive or share, which `..` cannot leave.
#[derive(Debug, PartialEq, serde::Serialize)]
pub struct IncludeForms {
    /// The pattern as given is absolute.
    pub absolute: bool,
    pub abs: String,
    pub rel: Option<String>,
}

/// How `pattern`, included from `file`, is written absolutely and relatively. It is resolved the
/// way `load` resolves it, from the folder of `file`, with `.` and `..` taken out.
pub fn include_forms(file: &Path, pattern: &str) -> IncludeForms {
    if pattern.is_empty() {
        return IncludeForms { absolute: false, abs: String::new(), rel: Some(String::new()) };
    }
    let absolute = |p: PathBuf| std::path::absolute(&p).unwrap_or(p);
    let dir = absolute(file.parent().unwrap_or(Path::new(".")).to_path_buf());
    let target = absolute(dir.join(pattern));
    IncludeForms {
        absolute: Path::new(pattern).is_absolute(),
        abs: target.display().to_string(),
        rel: relative(&dir, &target).map(|p| p.display().to_string()),
    }
}

/// `to` seen from `from`, both absolute and normalized; `None` when their roots differ.
fn relative(from: &Path, to: &Path) -> Option<PathBuf> {
    use std::path::Component;
    let (a, b): (Vec<_>, Vec<_>) = (from.components().collect(), to.components().collect());
    let same = |x: &Component, y: &Component| x.as_os_str().eq_ignore_ascii_case(y.as_os_str());
    let common = a.iter().zip(&b).take_while(|(x, y)| same(x, y)).count();
    // `C:` and `\`, or `\\server\share` and `\`.
    let root = a.iter().take_while(|c| !matches!(c, Component::Normal(_))).count();
    if common < root {
        return None;
    }
    let mut out: PathBuf = a[common..].iter().map(|_| "..").collect();
    out.extend(&b[common..]);
    Some(if out.as_os_str().is_empty() { PathBuf::from(".") } else { out })
}

/// Expands `*`/`?` in the last path component only.
// ponytail: wildcards only in the file name; add a glob crate if directory wildcards are needed.
fn expand(pattern: &Path) -> Result<Vec<PathBuf>, Msg> {
    let name = pattern.file_name().and_then(|n| n.to_str()).unwrap_or_default();
    if !name.contains(['*', '?']) {
        return if pattern.is_file() { Ok(vec![pattern.to_path_buf()]) } else { Err(msg!("config.not_found")) };
    }
    let m = Matcher::parse(&format!("glob:{name}")).map_err(|e| msg!("config.bad_pattern", error = e))?;
    let dir = pattern.parent().unwrap_or(Path::new("."));
    let entries = std::fs::read_dir(dir).map_err(|e| msg!("config.read", error = e))?;
    let mut paths: Vec<PathBuf> = entries
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.is_file() && p.file_name().and_then(|n| n.to_str()).is_some_and(|n| m.is_match(n)))
        .collect();
    paths.sort();
    Ok(paths)
}

/// Serializes a file readably: sections, named definitions and keymap nodes as tables; step
/// lists, bindings with `do` and actions inline. Inline values keep their order, which matters
/// for target-keyed implementations (as sub-tables, TOML would list them after the plain entries).
fn to_toml(raw: &RawConfig) -> Result<String, String> {
    // Everything starts inline; only what reads better as a table is expanded.
    let mut doc = toml_edit::ser::to_document(raw).map_err(|e| e.to_string())?;
    for (key, item) in doc.as_table_mut().iter_mut() {
        match key.get() {
            "settings" | "keyswap" | "actions" => {
                table(item);
            }
            "targets" | "modifiers" => {
                for (_, named) in table(item).into_iter().flat_map(|t| t.iter_mut()) {
                    table(named);
                }
            }
            "modes" => {
                for (_, mode) in table(item).into_iter().flat_map(|t| t.iter_mut()) {
                    if let Some(keymap) = table(mode).and_then(|m| m.get_mut("keymap")) {
                        node(keymap);
                    }
                }
            }
            "keymap" => node(item),
            _ => {}
        }
    }
    implicit(doc.as_table_mut());
    Ok(doc.to_string())
}

/// Turns an inline table into a `[table]`.
fn table(item: &mut toml_edit::Item) -> Option<&mut toml_edit::Table> {
    if item.is_inline_table() {
        *item = std::mem::take(item).into_table().map(toml_edit::Item::Table).unwrap_or_else(|i| i);
    }
    item.as_table_mut()
}

/// A keymap node and its sub-nodes (and `options`) as tables; bindings with `do` stay inline.
fn node(item: &mut toml_edit::Item) {
    for (_, child) in table(item).into_iter().flat_map(|t| t.iter_mut()) {
        if child.as_inline_table().is_some_and(|t| !t.contains_key("do")) {
            node(child);
        }
    }
}

/// No header for tables that only hold tables (`[modes]` above `[modes.x]`).
fn implicit(table: &mut toml_edit::Table) {
    let only_tables = !table.is_empty() && table.iter().all(|(_, i)| i.is_table());
    table.set_implicit(only_tables);
    table.iter_mut().filter_map(|(_, i)| i.as_table_mut()).for_each(implicit);
}

/// Writes one file by replacing it atomically, so a failed write never truncates the original.
/// Comments in the original file are not preserved.
pub fn save(path: &Path, raw: &RawConfig) -> std::io::Result<()> {
    let text = to_toml(raw).map_err(std::io::Error::other)?;
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let tmp = path.with_extension("toml.tmp");
    std::fs::write(&tmp, text)?;
    std::fs::rename(&tmp, path).inspect_err(|_| drop(std::fs::remove_file(&tmp)))
}
