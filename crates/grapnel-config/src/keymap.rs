//! Keymap trees → flat rules and prefix policies; action definitions → implementations.

use crate::compile::{Ctx, Kind, Names, compile_step, rule_key_problem};
use crate::*;
use grapnel_keys::{msg, parse_seq};
use grapnel_schema::{RawAction, RawBinding, RawLeaf, RawNode, RawStep, RawSteps};
use std::path::PathBuf;

/// Everything a keymap walk needs to resolve names, and where it puts the results.
pub(crate) struct Walker<'a> {
    pub user: &'a [&'a str],
    pub modifiers: &'a [Modifier],
    pub targets: &'a Names,
    pub named: &'a Names,
    pub modes: &'a Names,
    pub actions: &'a mut Vec<Action>,
    pub rules: &'a mut Vec<Rule>,
    pub prefixes: &'a mut Vec<Prefix>,
    /// Error location of each rule, for duplicate checks.
    pub locations: &'a mut Vec<(PathBuf, String)>,
}

impl Walker<'_> {
    pub fn walk(&mut self, c: &mut Ctx, node: &RawNode, path: &[Chord], at: &str, modes: &[ModeId]) {
        if let Some(o) = &node.options {
            let fallback = o.fallback.as_ref().and_then(|f| c.output(&format!("{at}.options.fallback"), f));
            let options = NodeOptions { on_mismatch: o.on_mismatch, timeout_ms: o.timeout_ms, fallback };
            self.prefixes.push(Prefix { keys: path.to_vec(), modes: modes.to_vec(), options });
        }
        for (key, binding) in &node.children {
            let at = format!("{at}.\"{key}\"");
            let chords = match parse_seq(key, self.user) {
                Ok(s) if s.0.is_empty() => {
                    c.err(&at, msg!("config.empty_keys"));
                    continue;
                }
                Ok(s) => s.0,
                Err(e) => {
                    c.err(&at, e);
                    continue;
                }
            };
            let keys: Vec<Chord> = path.iter().cloned().chain(chords).collect();
            match binding {
                RawBinding::Node(n) => self.walk(c, n, &keys, &at, modes),
                RawBinding::Short(s) => {
                    let action = self.reference(c, &at, s);
                    self.leaf(c, &at, keys, modes, action, None);
                }
                RawBinding::Steps(steps) => {
                    let action = self.anonymous(c, &at, &RawAction::Steps(steps.clone()));
                    self.leaf(c, &at, keys, modes, action, None);
                }
                RawBinding::Leaf(l) => {
                    for name in l.unknown.keys() {
                        c.err(&at, msg!("config.unknown_option", name = name));
                    }
                    let action = match &l.action {
                        RawAction::Short(s) => self.reference(c, &at, s),
                        other => self.anonymous(c, &at, other),
                    };
                    self.leaf(c, &at, keys, modes, action, Some(l));
                }
            }
        }
    }

    /// A string binding: a named action if one exists, otherwise keys to send.
    fn reference(&mut self, c: &mut Ctx, at: &str, s: &str) -> Option<ActionId> {
        if let Some(&id) = self.named.get(s) {
            return Some(id);
        }
        if parse_seq(s, &[]).is_err() {
            c.err(at, msg!("config.unknown_action_or_key", name = s));
            return None;
        }
        self.anonymous(c, at, &RawAction::Short(s.to_string()))
    }

    fn anonymous(&mut self, c: &mut Ctx, at: &str, raw: &RawAction) -> Option<ActionId> {
        let impls = compile_action(c, at, raw, self.targets, self.named, self.modes);
        self.actions.push(Action { name: at.to_string(), impls });
        Some(self.actions.len() - 1)
    }

    fn leaf(
        &mut self,
        c: &mut Ctx,
        at: &str,
        keys: Vec<Chord>,
        modes: &[ModeId],
        action: Option<ActionId>,
        l: Option<&RawLeaf>,
    ) {
        let keys = KeySeq(keys);
        if let Some(problem) = rule_key_problem(&keys, self.modifiers) {
            c.err(at, problem);
        }
        let keep_mods = l.is_some_and(|l| l.keep_mods);
        if keep_mods && keys.0.last().is_none_or(|k| k.mods == Mods::NONE) {
            c.err(&format!("{at}.keep_mods"), msg!("config.keep_mods"));
        }
        let Some(action) = action else { return };
        self.rules.push(Rule {
            keys,
            action,
            targets: l
                .map(|l| c.ids(&format!("{at}.targets"), Kind::Target, self.targets, &l.targets))
                .unwrap_or_default(),
            modes: modes.to_vec(),
            press: l.and_then(|l| l.press).unwrap_or_default(),
            fallback: l
                .and_then(|l| l.fallback.as_ref())
                .map(|f| c.output(&format!("{at}.fallback"), f).unwrap_or_default()),
            keep_mods,
        });
        self.locations.push((c.file.to_path_buf(), at.to_string()));
    }
}

/// An action definition: keys, steps, or target name (`"*"` = always) → steps.
pub(crate) fn compile_action(
    c: &mut Ctx,
    at: &str,
    raw: &RawAction,
    targets: &Names,
    actions: &Names,
    modes: &Names,
) -> Vec<ActionImpl> {
    // A single string is reported at `at` itself; list entries at `at[i]`.
    let steps = |c: &mut Ctx, at: &str, raw: &RawSteps| -> Vec<Step> {
        match raw {
            RawSteps::Short(s) => compile_step(c, at, &RawStep::Short(s.clone()), actions, modes).into_iter().collect(),
            RawSteps::Steps(v) => v
                .iter()
                .enumerate()
                .filter_map(|(i, s)| compile_step(c, &format!("{at}[{i}]"), s, actions, modes))
                .collect(),
        }
    };
    match raw {
        RawAction::Short(s) => vec![ActionImpl { when: vec![], steps: steps(c, at, &RawSteps::Short(s.clone())) }],
        RawAction::Steps(v) => vec![ActionImpl { when: vec![], steps: steps(c, at, &RawSteps::Steps(v.clone())) }],
        RawAction::ByTarget(by) => by
            .iter()
            .map(|(t, s)| {
                let at = format!("{at}.\"{t}\"");
                let when = if t == "*" { vec![] } else { c.id(&at, Kind::Target, targets, t).into_iter().collect() };
                ActionImpl { when, steps: steps(c, &at, s) }
            })
            .collect(),
    }
}

/// Same key twice in one mode, or a key hidden behind a shorter binding (in the same mode, or a
/// global one hiding a mode one). A short binding limited by `targets` hides nothing.
pub(crate) fn check_conflicts(rules: &[Rule], locations: &[(PathBuf, String)], errors: &mut Vec<Problem>) {
    for (i, a) in rules.iter().enumerate() {
        for (j, b) in rules.iter().enumerate().filter(|(j, _)| *j != i) {
            let (fa, la) = &locations[i];
            let (fb, lb) = &locations[j];
            let hides = a.targets.is_empty() && (a.modes == b.modes || (a.modes.is_empty() && !b.modes.is_empty()));
            let problem = |msg| Problem { file: Some(fb.clone()), at: lb.clone(), msg };
            if a.keys == b.keys && a.modes == b.modes && i < j {
                errors.push(problem(msg!("config.already_bound", file = fa.display(), at = la)));
            } else if hides && b.keys.0.len() > a.keys.0.len() && b.keys.0.starts_with(&a.keys.0) {
                errors.push(problem(msg!("config.unreachable", file = fa.display(), at = la)));
            }
        }
    }
}
