//! String matchers and target evaluation against the foreground window.

use regex::Regex;

/// What the platform knows about the foreground window and focused control.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct WindowInfo {
    pub exe_name: String,
    pub exe_path: String,
    pub title: String,
    pub class: String,
    pub control: String,
    pub uia_id: String,
    pub uia_name: String,
    pub uia_type: String,
}

#[derive(Clone, Debug)]
pub enum Matcher {
    /// Lower-cased; compared case-insensitively.
    Exact(String),
    Regex(Regex),
}

impl Matcher {
    /// Plain text is exact, `glob:` is a case-insensitive whole-string glob, `re:` a regex search.
    pub fn parse(s: &str) -> Result<Matcher, String> {
        if let Some(g) = s.strip_prefix("glob:") {
            let body: String = g
                .chars()
                .map(|c| match c {
                    '*' => ".*".to_string(),
                    '?' => ".".to_string(),
                    c => regex::escape(&c.to_string()),
                })
                .collect();
            return Ok(Matcher::Regex(Regex::new(&format!("(?is)^{body}$")).unwrap()));
        }
        if let Some(r) = s.strip_prefix("re:") {
            return Regex::new(r).map(Matcher::Regex).map_err(|e| e.to_string());
        }
        Ok(Matcher::Exact(s.to_lowercase()))
    }

    pub fn is_match(&self, value: &str) -> bool {
        match self {
            Matcher::Exact(s) => value.to_lowercase() == *s,
            Matcher::Regex(r) => r.is_match(value),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Field {
    ExeName,
    ExePath,
    Title,
    Class,
    Control,
    UiaId,
    UiaName,
    UiaType,
}

impl Field {
    fn get(self, w: &WindowInfo) -> &str {
        match self {
            Field::ExeName => &w.exe_name,
            Field::ExePath => &w.exe_path,
            Field::Title => &w.title,
            Field::Class => &w.class,
            Field::Control => &w.control,
            Field::UiaId => &w.uia_id,
            Field::UiaName => &w.uia_name,
            Field::UiaType => &w.uia_type,
        }
    }
}

pub type TargetId = usize;

/// All present parts must hold; within one, any of its items.
#[derive(Clone, Debug, Default)]
pub struct Target {
    pub name: String,
    /// One entry per key, holding its items.
    pub fields: Vec<Vec<(Field, Matcher)>>,
    /// Holds when none of these match.
    pub not: Vec<TargetId>,
    pub any: Vec<TargetId>,
    pub all: Vec<TargetId>,
}

/// Evaluates target `id`. The target graph must be acyclic (checked at compile time).
pub fn target_matches(targets: &[Target], id: TargetId, w: &WindowInfo) -> bool {
    let t = &targets[id];
    t.fields.iter().all(|items| items.iter().any(|(f, m)| m.is_match(f.get(w))))
        && !t.not.iter().any(|&n| target_matches(targets, n, w))
        && (t.any.is_empty() || t.any.iter().any(|&i| target_matches(targets, i, w)))
        && t.all.iter().all(|&i| target_matches(targets, i, w))
}

/// Empty list means unconditional; otherwise any listed target must match.
pub fn any_matches(targets: &[Target], ids: &[TargetId], w: &WindowInfo) -> bool {
    ids.is_empty() || ids.iter().any(|&i| target_matches(targets, i, w))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exact_is_case_insensitive() {
        let m = Matcher::parse("Code.EXE").unwrap();
        assert!(m.is_match("code.exe"));
        assert!(!m.is_match("code.exe2"));
    }

    #[test]
    fn glob_matches_whole_string() {
        let m = Matcher::parse("glob:* - Visual Studio Code").unwrap();
        assert!(m.is_match("main.rs - visual studio code"));
        assert!(!m.is_match("main.rs - Visual Studio Code!"));
        assert!(Matcher::parse("glob:a?c").unwrap().is_match("a.c"));
        assert!(!Matcher::parse("glob:a.c").unwrap().is_match("abc"));
    }

    #[test]
    fn regex_searches() {
        assert!(Matcher::parse("re:^foo").unwrap().is_match("foobar"));
        assert!(!Matcher::parse("re:^foo").unwrap().is_match("Foobar"));
        assert!(Matcher::parse("re:(").is_err());
    }

    #[test]
    fn target_logic() {
        let exact = |s: &str| Matcher::parse(s).unwrap();
        let targets = vec![
            Target { fields: vec![vec![(Field::ExeName, exact("a.exe"))]], ..Default::default() },
            Target { not: vec![0], ..Default::default() },
            Target { any: vec![0, 1], ..Default::default() },
            Target { all: vec![0, 1], ..Default::default() },
        ];
        let a = WindowInfo { exe_name: "A.exe".into(), ..Default::default() };
        let b = WindowInfo { exe_name: "b.exe".into(), ..Default::default() };
        assert!(target_matches(&targets, 0, &a) && !target_matches(&targets, 0, &b));
        assert!(!target_matches(&targets, 1, &a) && target_matches(&targets, 1, &b));
        assert!(target_matches(&targets, 2, &a) && target_matches(&targets, 2, &b));
        assert!(!target_matches(&targets, 3, &a));
        assert!(any_matches(&targets, &[], &b));
        assert!(!any_matches(&targets, &[0], &b));
    }
}
