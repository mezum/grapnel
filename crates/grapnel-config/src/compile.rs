//! Validation and name resolution: `RawConfig` files → `Config`.

use crate::*;
use grapnel_keys::{Mods, parse_chord, parse_key, parse_seq};
use grapnel_schema::{RawModifier, RawStep, RawTarget};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

type Names = HashMap<String, usize>;

struct Ctx<'a> {
    errors: &'a mut Vec<String>,
    file: &'a Path,
}

impl Ctx<'_> {
    fn err(&mut self, at: &str, msg: impl std::fmt::Display) {
        self.errors.push(format!("{}: {at}: {msg}", self.file.display()));
    }
    fn ok<T>(&mut self, at: &str, r: Result<T, String>) -> Option<T> {
        r.map_err(|e| self.err(at, e)).ok()
    }
    fn id(&mut self, at: &str, kind: &str, names: &Names, name: &str) -> Option<usize> {
        let id = names.get(name).copied();
        if id.is_none() {
            self.err(at, format!("unknown {kind} '{name}'"));
        }
        id
    }
    fn ids(&mut self, at: &str, kind: &str, names: &Names, list: &[String]) -> Vec<usize> {
        list.iter().filter_map(|n| self.id(at, kind, names, n)).collect()
    }
    /// Parses keys to send; user modifiers, gestures and pad buttons cannot be sent.
    fn output(&mut self, at: &str, s: &str) -> Option<KeySeq> {
        let seq = self.ok(at, parse_seq(s, &[]))?;
        if let Some(c) = seq.0.iter().find(|c| matches!(c.key, Key::Gesture(..) | Key::Pad(_))) {
            self.err(at, format!("'{}' cannot be sent", grapnel_keys::format_key(&c.key)));
            return None;
        }
        Some(seq)
    }
}

fn index<T>(items: &[T], name: impl Fn(&T) -> &str) -> Names {
    items.iter().enumerate().map(|(i, t)| (name(t).to_string(), i)).collect()
}

pub fn compile(files: &[(PathBuf, RawConfig)]) -> Result<Config, Vec<String>> {
    if files.is_empty() {
        return Err(vec!["no configuration files".into()]);
    }
    let mut errors = Vec::new();
    let mut modes = vec![Mode { name: DEFAULT_MODE.into(), block_unmapped: false, unmapped_to: None }];
    let mut raw_mods: Vec<(&Path, &str, &RawModifier)> = Vec::new();
    let mut raw_targets: Vec<(&Path, &str, &RawTarget)> = Vec::new();
    let mut actions: Vec<Action> = Vec::new();
    let mut seen: HashMap<(&str, &str), &Path> = HashMap::new();
    for (i, (path, raw)) in files.iter().enumerate() {
        let mut c = Ctx { errors: &mut errors, file: path };
        if i > 0 && raw.settings.is_some() {
            c.err("settings", "only allowed in the entry file");
        }
        let names = raw.modes.keys().map(|n| ("modes", n));
        let names = names.chain(raw.modifiers.keys().map(|n| ("modifiers", n)));
        for (kind, name) in names.chain(raw.targets.keys().map(|n| ("targets", n))) {
            if let Some(prev) = seen.insert((kind, name), path) {
                c.err(&format!("{kind}.{name}"), format!("already defined in {}", prev.display()));
            }
        }
        for (name, m) in &raw.modes {
            match modes.iter_mut().find(|x| x.name == *name) {
                Some(x) => x.block_unmapped = m.block_unmapped,
                None => modes.push(Mode { name: name.clone(), block_unmapped: m.block_unmapped, unmapped_to: None }),
            }
        }
        raw_mods.extend(raw.modifiers.iter().map(|(n, m)| (path.as_path(), n.as_str(), m)));
        raw_targets.extend(raw.targets.iter().map(|(n, t)| (path.as_path(), n.as_str(), t)));
        for name in raw.actions.keys() {
            if !actions.iter().any(|a| a.name == *name) {
                actions.push(Action { name: name.clone(), impls: vec![] });
            }
        }
    }
    let mode_ix = index(&modes, |m| &m.name);
    let target_ix: Names = raw_targets.iter().enumerate().map(|(i, t)| (t.1.to_string(), i)).collect();
    let action_ix = index(&actions, |a| &a.name);
    let user: Vec<&str> = raw_mods.iter().map(|m| m.1).collect();

    if raw_mods.len() > Mods::MAX_USER {
        // Modifier bits would overflow while parsing keys; stop here.
        errors.push(format!("too many modifiers ({} > {})", raw_mods.len(), Mods::MAX_USER));
        return Err(errors);
    }
    let modifiers = compile_modifiers(&raw_mods, &mut errors);
    let targets = compile_targets(&raw_targets, &target_ix, &mut errors);

    let mut rules = Vec::new();
    for (path, raw) in files {
        let mut c = Ctx { errors: &mut errors, file: path };
        for (name, m) in &raw.modes {
            if let Some(to) = &m.unmapped_to {
                modes[mode_ix[name]].unmapped_to = c.id(&format!("modes.{name}.unmapped_to"), "mode", &mode_ix, to);
            }
        }
        for (i, r) in raw.rules.iter().enumerate() {
            let at = |f: &str| format!("rules[{i}].{f}");
            let keys = c.ok(&at("keys"), parse_seq(&r.keys, &user));
            if let Some(k) = keys.as_ref().and_then(|k| rule_key_problem(k, &modifiers)) {
                c.err(&at("keys"), k);
            }
            let has_mod = keys.as_ref().and_then(|k| k.0.last()).is_some_and(|c| c.mods != Mods::NONE);
            if r.keep_mods && !has_mod {
                c.err(&at("keep_mods"), "the last chord of keys needs a modifier");
            }
            let rule = Rule {
                keys: keys.unwrap_or_default(),
                action: c.id(&at("action"), "action", &action_ix, &r.action).unwrap_or(0),
                targets: c.ids(&at("targets"), "target", &target_ix, &r.targets),
                modes: c.ids(&at("modes"), "mode", &mode_ix, &r.modes),
                press: r.press.unwrap_or_default(),
                fallback: r.fallback.as_ref().map(|f| c.output(&at("fallback"), f).unwrap_or_default()),
                on_mismatch: r.on_mismatch.unwrap_or_default(),
                timeout_ms: r.timeout_ms.unwrap_or(0),
                keep_mods: r.keep_mods,
            };
            rules.push(rule);
        }
        for (name, impls) in &raw.actions {
            let id = action_ix[name];
            for (i, imp) in impls.iter().enumerate() {
                let at = format!("actions.{name}[{i}]");
                let when = c.ids(&format!("{at}.when"), "target", &target_ix, &imp.when);
                let steps = imp
                    .steps
                    .iter()
                    .enumerate()
                    .filter_map(|(j, s)| compile_step(&mut c, &format!("{at}.do[{j}]"), s, &action_ix, &mode_ix));
                let steps = steps.collect();
                actions[id].impls.push(ActionImpl { when, steps });
            }
        }
    }

    let (entry, raw) = &files[0];
    let s = raw.settings.clone().unwrap_or_default();
    let mut c = Ctx { errors: &mut errors, file: entry };
    let initial = s.initial_mode.as_deref().unwrap_or(DEFAULT_MODE);
    let settings = Settings {
        initial_mode: c.id("settings.initial_mode", "mode", &mode_ix, initial).unwrap_or(0),
        passthrough: c.ids("settings.passthrough", "target", &target_ix, &s.passthrough),
        suspend_hotkey: s.suspend_hotkey.as_ref().and_then(|h| {
            let chord = c.ok("settings.suspend_hotkey", parse_chord(h, &[]))?;
            let ok = matches!(chord.key, Key::Vk(_)) && chord.key.real_mod().is_none();
            ok.then_some(chord).or_else(|| {
                c.err("settings.suspend_hotkey", "must be C/M/S/W plus a keyboard key");
                None
            })
        }),
        gesture_threshold: s.gesture_threshold.unwrap_or(30).max(1),
    };
    if errors.is_empty() { Ok(Config { settings, modes, modifiers, targets, rules, actions }) } else { Err(errors) }
}

fn rule_key_problem(keys: &KeySeq, mods: &[Modifier]) -> Option<String> {
    if keys.0.is_empty() {
        return Some("empty key sequence".into());
    }
    keys.0.iter().find_map(|c| {
        let name = grapnel_keys::format_key(&c.key);
        if c.key.real_mod().is_some() {
            Some(format!("'{name}' is a modifier; use it as C-/M-/S-/W-"))
        } else if mods.iter().any(|m| m.key == c.key) {
            Some(format!("'{name}' is a user modifier key; use its tap setting"))
        } else {
            None
        }
    })
}

fn compile_modifiers(raw: &[(&Path, &str, &RawModifier)], errors: &mut Vec<String>) -> Vec<Modifier> {
    let mut out = Vec::new();
    for &(file, name, m) in raw {
        let mut c = Ctx { errors, file };
        let at = format!("modifiers.{name}");
        let valid = name.starts_with(|ch: char| ch.is_ascii_alphabetic())
            && name.chars().all(|ch| ch.is_ascii_alphanumeric())
            && !["C", "M", "S", "W"].contains(&name);
        if !valid {
            c.err(&at, "name must be alphanumeric, start with a letter and not be C/M/S/W");
        }
        let key = c.ok(&format!("{at}.key"), parse_key(&m.key));
        if key.as_ref().is_some_and(|k| k.real_mod().is_some() || matches!(k, Key::Wheel(_) | Key::Gesture(..))) {
            c.err(&format!("{at}.key"), "cannot be Ctrl/Alt/Shift/Win, a wheel or a gesture");
        }
        let tap = c.output(&format!("{at}.tap"), m.tap.as_deref().unwrap_or(&m.key));
        let emulate = m.emulate.as_deref().map_or(Ok(Mods::NONE), |s| {
            s.split('-').try_fold(Mods::NONE, |acc, part| match part {
                "C" => Ok(acc | Mods::CTRL),
                "M" => Ok(acc | Mods::ALT),
                "S" => Ok(acc | Mods::SHIFT),
                "W" => Ok(acc | Mods::WIN),
                _ => Err(format!("'{s}' must be C/M/S/W joined with '-'")),
            })
        });
        let emulate = c.ok(&format!("{at}.as"), emulate).unwrap_or_default();
        out.push(Modifier {
            name: name.to_string(),
            emulate,
            key: key.unwrap_or(Key::Vk(0)),
            tap: tap.unwrap_or_default(),
            tap_timeout_ms: m.tap_timeout_ms.unwrap_or(0),
        });
    }
    out
}

fn compile_targets(raw: &[(&Path, &str, &RawTarget)], ix: &Names, errors: &mut Vec<String>) -> Vec<Target> {
    let mut out = Vec::new();
    for &(file, name, t) in raw {
        let mut c = Ctx { errors, file };
        let at = format!("targets.{name}");
        // A path separator means "match the full path". In a regex that is an escaped `\\`,
        // so `re:^emacs\.exe$` still matches the file name.
        let app_field = |s: &str| {
            let path = match s.strip_prefix("re:") {
                Some(re) => re.contains(r"\\"),
                None => s.contains('\\'),
            };
            if path { Field::ExePath } else { Field::ExeName }
        };
        let fields = [
            (t.app.as_ref().map(|s| app_field(s)), &t.app, "app"),
            (Some(Field::Title), &t.title, "title"),
            (Some(Field::Class), &t.class, "class"),
            (Some(Field::Control), &t.control, "control"),
            (Some(Field::UiaId), &t.uia_id, "uia_id"),
            (Some(Field::UiaName), &t.uia_name, "uia_name"),
            (Some(Field::UiaType), &t.uia_type, "uia_type"),
        ];
        let fields = fields.into_iter().filter_map(|(f, v, key)| {
            let m = c.ok(&format!("{at}.{key}"), Matcher::parse(v.as_ref()?))?;
            Some((f?, m))
        });
        let fields = fields.collect();
        out.push(Target {
            name: name.to_string(),
            fields,
            not: t.not.as_ref().and_then(|n| c.id(&format!("{at}.not"), "target", ix, n)),
            any: c.ids(&format!("{at}.any"), "target", ix, &t.any),
            all: c.ids(&format!("{at}.all"), "target", ix, &t.all),
        });
    }
    for (i, t) in out.iter().enumerate() {
        if reaches(&out, i, i, &mut vec![false; out.len()]) {
            errors.push(format!("targets.{}: circular reference", t.name));
        }
    }
    out
}

fn reaches(ts: &[Target], from: TargetId, goal: TargetId, seen: &mut Vec<bool>) -> bool {
    let t = &ts[from];
    t.not
        .iter()
        .chain(&t.any)
        .chain(&t.all)
        .any(|&n| n == goal || (!std::mem::replace(&mut seen[n], true) && reaches(ts, n, goal, seen)))
}

fn compile_step(c: &mut Ctx, at: &str, s: &RawStep, actions: &Names, modes: &Names) -> Option<Step> {
    Some(match s {
        RawStep::Short(k) | RawStep::Keys { keys: k } => Step::Keys(c.output(at, k)?),
        RawStep::Text { text } => Step::Text(text.clone()),
        RawStep::MouseMove { mouse_move: [x, y] } => Step::MouseMove { x: *x, y: *y, absolute: false },
        RawStep::MouseMoveTo { mouse_move_to: [x, y] } => Step::MouseMove { x: *x, y: *y, absolute: true },
        RawStep::Sleep { sleep } => Step::Sleep(*sleep),
        RawStep::Run { run, args } => Step::Run { program: run.clone(), args: args.clone() },
        RawStep::Call { call, arg } => Step::Call { action: c.id(at, "action", actions, call)?, arg: arg.clone() },
        RawStep::Mode { mode } => Step::Mode(c.id(at, "mode", modes, mode)?),
        RawStep::Control { control } => Step::Control(*control),
        RawStep::Input { input, then } => {
            Step::Input { prompt: input.clone(), then: c.id(at, "action", actions, then)? }
        }
    })
}
