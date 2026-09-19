//! Validation and name resolution: `RawConfig` files → `Config`.

use crate::keymap::{Walker, check_conflicts, compile_action};
use crate::*;
use grapnel_keys::{Mods, Msg, msg, parse_chord, parse_key, parse_seq};
use grapnel_schema::{RawAction, RawMode, RawModifier, RawStep, RawTarget};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

pub(crate) type Names = HashMap<String, usize>;

/// What a name refers to, for "unknown name" errors.
#[derive(Clone, Copy)]
pub(crate) enum Kind {
    Mode,
    Target,
    Action,
}

impl Kind {
    pub fn unknown(self, name: &str) -> Msg {
        match self {
            Kind::Mode => msg!("config.unknown_mode", name = name),
            Kind::Target => msg!("config.unknown_target", name = name),
            Kind::Action => msg!("config.unknown_action", name = name),
        }
    }
}

pub(crate) struct Ctx<'a> {
    pub errors: &'a mut Vec<Problem>,
    pub file: &'a Path,
}

impl Ctx<'_> {
    pub fn err(&mut self, at: &str, msg: Msg) {
        self.errors.push(Problem { file: Some(self.file.to_path_buf()), at: at.to_string(), on_key: false, msg });
    }
    /// A problem with the name at `at` rather than its value.
    pub fn err_key(&mut self, at: &str, msg: Msg) {
        self.errors.push(Problem { file: Some(self.file.to_path_buf()), at: at.to_string(), on_key: true, msg });
    }
    pub fn ok<T>(&mut self, at: &str, r: Result<T, Msg>) -> Option<T> {
        r.map_err(|e| self.err(at, e)).ok()
    }
    pub fn id(&mut self, at: &str, kind: Kind, names: &Names, name: &str) -> Option<usize> {
        let id = names.get(name).copied();
        if id.is_none() {
            self.err(at, kind.unknown(name));
        }
        id
    }
    /// Each unknown name is reported at `at[i]`.
    pub fn ids(&mut self, at: &str, kind: Kind, names: &Names, list: &[String]) -> Vec<usize> {
        list.iter().enumerate().filter_map(|(i, n)| self.id(&format!("{at}[{i}]"), kind, names, n)).collect()
    }
    /// Parses keys to send; user modifiers, gestures and pad buttons cannot be sent.
    pub fn output(&mut self, at: &str, s: &str) -> Option<KeySeq> {
        let seq = self.ok(at, parse_seq(s, &[]))?;
        if let Some(c) = seq.0.iter().find(|c| matches!(c.key, Key::Gesture(..) | Key::Pad(_))) {
            self.err(at, msg!("config.cannot_send", key = grapnel_keys::format_key(&c.key)));
            return None;
        }
        Some(seq)
    }
}

fn index<T>(items: &[T], name: impl Fn(&T) -> &str) -> Names {
    items.iter().enumerate().map(|(i, t)| (name(t).to_string(), i)).collect()
}

pub fn compile(files: &[(PathBuf, RawConfig)]) -> Result<Config, Vec<Problem>> {
    if files.is_empty() {
        return Err(vec![Problem { file: None, at: String::new(), on_key: false, msg: msg!("config.no_files") }]);
    }
    let mut errors = Vec::new();
    let mode = |name: &str, m: &RawMode| Mode {
        name: name.into(),
        block_unmapped: m.block_unmapped,
        count: m.count,
        unmapped_to: None,
        hold: Mods::NONE,
    };
    let mut modes = vec![mode(DEFAULT_MODE, &RawMode::default())];
    let mut raw_mods: Vec<(&Path, &str, &RawModifier)> = Vec::new();
    let mut raw_targets: Vec<(&Path, &str, &RawTarget)> = Vec::new();
    let mut raw_actions: Vec<(&Path, &str, &RawAction)> = Vec::new();
    let mut seen: HashMap<(&str, &str), &Path> = HashMap::new();
    for (i, (path, raw)) in files.iter().enumerate() {
        let mut c = Ctx { errors: &mut errors, file: path };
        if i > 0 && raw.settings.is_some() {
            c.err("settings", msg!("config.entry_only"));
        }
        let names = raw.modes.keys().map(|n| ("modes", n));
        let names = names.chain(raw.modifiers.keys().map(|n| ("modifiers", n)));
        let names = names.chain(raw.targets.keys().map(|n| ("targets", n)));
        for (kind, name) in names.chain(raw.actions.keys().map(|n| ("actions", n))) {
            if let Some(prev) = seen.insert((kind, name), path) {
                c.err_key(&format!("{kind}.{name}"), msg!("config.duplicate", file = prev.display()));
            }
        }
        for (name, m) in &raw.modes {
            match modes.iter_mut().find(|x| x.name == *name) {
                Some(x) => (x.block_unmapped, x.count) = (m.block_unmapped, m.count),
                None => modes.push(mode(name, m)),
            }
        }
        raw_mods.extend(raw.modifiers.iter().map(|(n, m)| (path.as_path(), n.as_str(), m)));
        raw_targets.extend(raw.targets.iter().map(|(n, t)| (path.as_path(), n.as_str(), t)));
        raw_actions.extend(raw.actions.iter().map(|(n, a)| (path.as_path(), n.as_str(), a)));
    }
    let mode_ix = index(&modes, |m| &m.name);
    let target_ix: Names = raw_targets.iter().enumerate().map(|(i, t)| (t.1.to_string(), i)).collect();
    let action_ix: Names = raw_actions.iter().enumerate().map(|(i, a)| (a.1.to_string(), i)).collect();
    let user: Vec<&str> = raw_mods.iter().map(|m| m.1).collect();

    if raw_mods.len() > Mods::MAX_USER {
        // Modifier bits would overflow while parsing keys; stop here.
        let msg = msg!("config.too_many_modifiers", count = raw_mods.len(), max = Mods::MAX_USER);
        errors.push(Problem { file: None, at: "modifiers".into(), on_key: false, msg });
        return Err(errors);
    }
    let modifiers = compile_modifiers(&raw_mods, &mut errors);
    let targets = compile_targets(&raw_targets, &target_ix, &mut errors);
    let keyswap = compile_keyswap(files, &modifiers, &mut errors);

    let mut actions: Vec<Action> = raw_actions
        .iter()
        .map(|&(file, name, raw)| {
            let mut c = Ctx { errors: &mut errors, file };
            let impls = compile_action(&mut c, &format!("actions.{name}"), raw, &target_ix, &action_ix, &mode_ix);
            Action { name: name.to_string(), impls }
        })
        .collect();

    for (path, raw) in files {
        let mut c = Ctx { errors: &mut errors, file: path };
        for (name, m) in &raw.modes {
            let target = &mut modes[mode_ix[name]];
            if let Some(to) = &m.unmapped_to {
                target.unmapped_to = c.id(&format!("modes.{name}.unmapped_to"), Kind::Mode, &mode_ix, to);
            }
            target.hold = c.ok(&format!("modes.{name}.hold"), parse_real_mods(m.hold.as_deref())).unwrap_or_default();
        }
    }

    // Mode keymaps come first so they win over the global keymap.
    let (mut rules, mut prefixes, mut locations) = (Vec::new(), Vec::new(), Vec::new());
    let mut walker = Walker {
        user: &user,
        modifiers: &modifiers,
        targets: &target_ix,
        named: &action_ix,
        modes: &mode_ix,
        actions: &mut actions,
        rules: &mut rules,
        prefixes: &mut prefixes,
        locations: &mut locations,
    };
    for (path, raw) in files {
        let mut c = Ctx { errors: &mut errors, file: path };
        for (name, m) in &raw.modes {
            let at = format!("modes.{name}.keymap");
            walker.walk(&mut c, &m.keymap, &[], &at, &[mode_ix[name]]);
        }
    }
    for (path, raw) in files {
        let mut c = Ctx { errors: &mut errors, file: path };
        walker.walk(&mut c, &raw.keymap, &[], "keymap", &[]);
    }
    check_conflicts(&rules, &locations, &mut errors);

    let (entry, raw) = &files[0];
    let s = raw.settings.clone().unwrap_or_default();
    let mut c = Ctx { errors: &mut errors, file: entry };
    let initial = s.initial_mode.as_deref().unwrap_or(DEFAULT_MODE);
    let settings = Settings {
        initial_mode: c.id("settings.initial_mode", Kind::Mode, &mode_ix, initial).unwrap_or(0),
        passthrough: c.ids("settings.passthrough", Kind::Target, &target_ix, &s.passthrough),
        suspend_hotkey: s.suspend_hotkey.as_ref().and_then(|h| {
            let chord = c.ok("settings.suspend_hotkey", parse_chord(h, &[]))?;
            let ok = matches!(chord.key, Key::Vk(_)) && chord.key.real_mod().is_none();
            ok.then_some(chord).or_else(|| {
                c.err("settings.suspend_hotkey", msg!("config.hotkey"));
                None
            })
        }),
        gesture_threshold: s.gesture_threshold.unwrap_or(30).max(1),
        language: s.language,
    };
    if errors.is_empty() {
        Ok(Config { settings, modes, modifiers, targets, keyswap, rules, prefixes, actions })
    } else {
        Err(errors)
    }
}

/// `"C-S"` → Ctrl|Shift. `None` is no modifiers.
fn parse_real_mods(s: Option<&str>) -> Result<Mods, Msg> {
    let Some(s) = s else { return Ok(Mods::NONE) };
    s.split('-').try_fold(Mods::NONE, |acc, part| match part {
        "C" => Ok(acc | Mods::CTRL),
        "M" => Ok(acc | Mods::ALT),
        "S" => Ok(acc | Mods::SHIFT),
        "W" => Ok(acc | Mods::WIN),
        _ => Err(msg!("config.real_mods", value = s)),
    })
}

pub(crate) fn rule_key_problem(keys: &KeySeq, mods: &[Modifier]) -> Option<Msg> {
    if keys.0.is_empty() {
        return Some(msg!("config.empty_keys"));
    }
    keys.0.iter().find_map(|c| {
        let name = grapnel_keys::format_key(&c.key);
        if c.key.real_mod().is_some() {
            Some(msg!("config.is_modifier", key = name))
        } else if mods.iter().any(|m| m.key == c.key) {
            Some(msg!("config.is_user_modifier", key = name))
        } else {
            None
        }
    })
}

fn compile_modifiers(raw: &[(&Path, &str, &RawModifier)], errors: &mut Vec<Problem>) -> Vec<Modifier> {
    let mut out = Vec::new();
    for &(file, name, m) in raw {
        let mut c = Ctx { errors, file };
        let at = format!("modifiers.{name}");
        let valid = name.starts_with(|ch: char| ch.is_ascii_alphabetic())
            && name.chars().all(|ch| ch.is_ascii_alphanumeric())
            && !["C", "M", "S", "W"].contains(&name);
        if !valid {
            c.err_key(&at, msg!("config.modifier_name"));
        }
        let key = c.ok(&format!("{at}.key"), parse_key(&m.key));
        if key.as_ref().is_some_and(|k| k.real_mod().is_some() || matches!(k, Key::Wheel(_) | Key::Gesture(..))) {
            c.err(&format!("{at}.key"), msg!("config.modifier_key"));
        }
        let tap = c.output(&format!("{at}.tap"), m.tap.as_deref().unwrap_or(&m.key));
        let emulate = c.ok(&format!("{at}.as"), parse_real_mods(m.emulate.as_deref())).unwrap_or_default();
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

/// `[keyswap]` of every file: a keyboard key alone or with Shift → one keyboard chord.
fn compile_keyswap(
    files: &[(PathBuf, RawConfig)],
    mods: &[Modifier],
    errors: &mut Vec<Problem>,
) -> HashMap<(Key, bool), Chord> {
    let keyboard = |k: &Key| matches!(k, Key::Vk(_) | Key::Sc(_)) && k.real_mod().is_none();
    let mut out = HashMap::new();
    let mut seen: HashMap<(Key, bool), (&Path, String)> = HashMap::new();
    for (path, raw) in files {
        let mut c = Ctx { errors, file: path };
        for (from, to) in &raw.keyswap {
            let at = format!("keyswap.\"{from}\"");
            let from = parse_chord(from, &[]).ok().filter(|f| {
                matches!(f.mods, Mods::NONE | Mods::SHIFT) && keyboard(&f.key) && !mods.iter().any(|m| m.key == f.key)
            });
            let Some(from) = from else {
                c.err_key(&at, msg!("config.keyswap_from"));
                continue;
            };
            let to = parse_seq(to, &[]).ok().and_then(|s| match s.0.as_slice() {
                [t] if keyboard(&t.key) => Some(t.clone()),
                _ => None,
            });
            let Some(to) = to else {
                c.err(&at, msg!("config.keyswap_to"));
                continue;
            };
            let slot = (from.key, from.mods == Mods::SHIFT);
            if let Some((file, prev)) = seen.get(&slot) {
                c.err_key(&at, msg!("config.already_bound", file = file.display(), at = prev));
                continue;
            }
            seen.insert(slot.clone(), (path, at));
            out.insert(slot, to);
        }
    }
    out
}

fn compile_targets(raw: &[(&Path, &str, &RawTarget)], ix: &Names, errors: &mut Vec<Problem>) -> Vec<Target> {
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
            let m = Matcher::parse(v.as_ref()?).map_err(|e| msg!("config.bad_pattern", error = e));
            let m = c.ok(&format!("{at}.{key}"), m)?;
            Some((f?, m))
        });
        let fields = fields.collect();
        out.push(Target {
            name: name.to_string(),
            fields,
            not: t.not.as_ref().and_then(|n| c.id(&format!("{at}.not"), Kind::Target, ix, n)),
            any: c.ids(&format!("{at}.any"), Kind::Target, ix, &t.any),
            all: c.ids(&format!("{at}.all"), Kind::Target, ix, &t.all),
        });
    }
    for (i, (t, &(file, ..))) in out.iter().zip(raw).enumerate() {
        if reaches(&out, i, i, &mut vec![false; out.len()]) {
            let at = format!("targets.{}", t.name);
            errors.push(Problem { file: Some(file.to_path_buf()), at, on_key: true, msg: msg!("config.circular") });
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

pub(crate) fn compile_step(c: &mut Ctx, at: &str, s: &RawStep, actions: &Names, modes: &Names) -> Option<Step> {
    Some(match s {
        // A bare string calls the action of that name if there is one; `{ keys = ... }` is always keys.
        RawStep::Short(k) if actions.contains_key(k) => Step::Call { action: actions[k], arg: String::new() },
        RawStep::Short(k) | RawStep::Keys { keys: k } => Step::Keys(c.output(at, k)?),
        RawStep::Text { text } => Step::Text(text.clone()),
        RawStep::MouseMove { mouse_move: [x, y] } => Step::MouseMove { x: *x, y: *y, absolute: false },
        RawStep::MouseMoveTo { mouse_move_to: [x, y] } => Step::MouseMove { x: *x, y: *y, absolute: true },
        RawStep::Sleep { sleep } => Step::Sleep(*sleep),
        RawStep::Run { run, args } => Step::Run { program: run.clone(), args: args.clone() },
        RawStep::Call { call, arg } => {
            Step::Call { action: c.id(at, Kind::Action, actions, call)?, arg: arg.clone().unwrap_or_default() }
        }
        RawStep::Mode { mode } => Step::Mode(c.id(at, Kind::Mode, modes, mode)?),
        RawStep::Control { control } => Step::Control(*control),
        RawStep::Input { input, then, position } => {
            // A name with `{arg}` is looked up only once the text is known; until then `{arg}`
            // stands for any text, and at least one action must fit.
            let then = match then {
                Some(t) if !t.contains("{arg}") => Then::Action(c.id(at, Kind::Action, actions, t)?),
                t => {
                    let template = t.clone().unwrap_or_else(|| "{arg}".into());
                    let pattern: Vec<String> = template.split("{arg}").map(regex::escape).collect();
                    let fits = regex::Regex::new(&format!("^{}$", pattern.join(".*"))).unwrap();
                    if !actions.keys().any(|name| fits.is_match(name)) {
                        c.err(at, Kind::Action.unknown(&template));
                        return None;
                    }
                    Then::Named(template)
                }
            };
            Step::Input { prompt: input.clone(), then, position: position.unwrap_or_default() }
        }
    })
}
