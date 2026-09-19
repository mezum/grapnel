//! Every locale has the same keys, and every key the apps translate exists.

use std::collections::BTreeSet;
use std::path::Path;

const ROOT: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../..");

fn keys(prefix: &str, table: &toml::Table, out: &mut BTreeSet<String>) {
    for (k, v) in table {
        let key = if prefix.is_empty() { k.clone() } else { format!("{prefix}.{k}") };
        match v {
            toml::Value::Table(t) => keys(&key, t, out),
            _ => drop(out.insert(key)),
        }
    }
}

fn locale(path: &Path) -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    keys("", &std::fs::read_to_string(path).unwrap().parse().unwrap(), &mut out);
    out
}

fn sources(dir: &Path, out: &mut Vec<String>) {
    for e in std::fs::read_dir(dir).unwrap().flatten() {
        let p = e.path();
        if p.is_dir() && !p.ends_with("target") {
            sources(&p, out);
        } else if p.extension().is_some_and(|x| x == "rs") {
            out.push(std::fs::read_to_string(&p).unwrap());
        }
    }
}

#[test]
fn locales_share_keys() {
    let en = locale(&Path::new(ROOT).join("locales/en.toml"));
    for e in std::fs::read_dir(Path::new(ROOT).join("locales")).unwrap().flatten() {
        assert_eq!(locale(&e.path()), en, "{}", e.path().display());
    }
}

#[test]
fn used_keys_exist() {
    let en = locale(&Path::new(ROOT).join("locales/en.toml"));
    let mut files = Vec::new();
    sources(&Path::new(ROOT).join("apps"), &mut files);
    sources(&Path::new(ROOT).join("crates"), &mut files);
    let re = regex::Regex::new(r#"\b(?:t|report|msg)!\(\s*"([^"]+)""#).unwrap();
    let used: BTreeSet<_> = files.iter().flat_map(|f| re.captures_iter(f).map(|c| c[1].to_string())).collect();
    assert!(used.len() > 10, "{used:?}");
    let missing: Vec<_> = used.difference(&en).collect();
    assert!(missing.is_empty(), "{missing:?}");
}
