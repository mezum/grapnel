//! Loads grapnel configuration files, validates them and compiles them into [`Config`].

mod compile;
mod keymap;
mod load;
mod matcher;
mod problem;

pub use compile::compile;
pub use grapnel_schema::{ControlCmd, InputPosition, Mismatch, Press, RawConfig};
pub use load::{default_entry, load, save};
pub use matcher::{Field, Matcher, Target, TargetId, WindowInfo, any_matches, target_matches};
pub use problem::Problem;

use grapnel_keys::{Chord, Key, KeySeq, Mods};
use std::collections::HashMap;

pub type ActionId = usize;
pub type ModeId = usize;

/// Name of the implicit mode that always exists.
pub const DEFAULT_MODE: &str = "default";

#[derive(Clone, Debug)]
pub struct Config {
    pub settings: Settings,
    pub modes: Vec<Mode>,
    /// Index = user modifier bit (see `Mods::user`).
    pub modifiers: Vec<Modifier>,
    pub targets: Vec<Target>,
    /// Physical key and whether Shift is held → the chord it stands for, before the keymap.
    pub keyswap: HashMap<(Key, bool), Chord>,
    /// Keymap leaves flattened to key sequences; mode-specific ones first.
    pub rules: Vec<Rule>,
    /// Keymap nodes that set options, for chord-waiting behaviour.
    pub prefixes: Vec<Prefix>,
    pub actions: Vec<Action>,
}

#[derive(Clone, Debug)]
pub struct Settings {
    pub initial_mode: ModeId,
    pub passthrough: Vec<TargetId>,
    pub suspend_hotkey: Option<Chord>,
    pub gesture_threshold: u32,
    pub language: Option<String>,
}

#[derive(Clone, Debug)]
pub struct Mode {
    pub name: String,
    pub block_unmapped: bool,
    /// Digits typed before a binding repeat it.
    pub count: bool,
    /// Switch to this mode when an input matches no rule.
    pub unmapped_to: Option<ModeId>,
    /// Real modifiers kept pressed while in this mode.
    pub hold: Mods,
}

#[derive(Clone, Debug)]
pub struct Modifier {
    pub name: String,
    pub key: Key,
    pub tap: KeySeq,
    /// 0 = no limit.
    pub tap_timeout_ms: u32,
    /// Real modifiers added to keys that no rule matches while this modifier is held.
    pub emulate: Mods,
}

#[derive(Clone, Debug)]
pub struct Rule {
    pub keys: KeySeq,
    pub action: ActionId,
    pub targets: Vec<TargetId>,
    /// Empty = every mode.
    pub modes: Vec<ModeId>,
    pub press: Press,
    /// `None` = send the original input again.
    pub fallback: Option<KeySeq>,
    /// Output modifiers stay pressed until the modifiers of the last chord in `keys` are released.
    pub keep_mods: bool,
    /// Remembered for `ControlCmd::Repeat`.
    pub repeat: bool,
}

/// How to wait after a prefix: options of a keymap node, already merged with its ancestors'.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Policy {
    pub on_mismatch: Mismatch,
    /// 0 = no timeout.
    pub timeout_ms: u32,
    /// Sent for `on_mismatch = "fallback"`; `None` replays the pending chords.
    pub fallback: Option<KeySeq>,
}

/// Options set on one keymap node; unset fields are inherited from shallower nodes.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct NodeOptions {
    pub on_mismatch: Option<Mismatch>,
    pub timeout_ms: Option<u32>,
    pub fallback: Option<KeySeq>,
}

#[derive(Clone, Debug)]
pub struct Prefix {
    pub keys: Vec<Chord>,
    /// Empty = every mode.
    pub modes: Vec<ModeId>,
    pub options: NodeOptions,
}

#[derive(Clone, Debug)]
pub struct Action {
    pub name: String,
    pub impls: Vec<ActionImpl>,
}

#[derive(Clone, Debug)]
pub struct ActionImpl {
    pub when: Vec<TargetId>,
    pub steps: Vec<Step>,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Step {
    Keys(KeySeq),
    Text(String),
    MouseMove { x: i32, y: i32, absolute: bool },
    Sleep(u32),
    Run { program: String, args: Vec<String> },
    Call { action: ActionId, arg: Option<String> },
    Mode(ModeId),
    Control(ControlCmd),
    Input { prompt: String, then: Then, position: InputPosition },
}

/// What an input box does with the confirmed text.
#[derive(Clone, Debug, PartialEq)]
pub enum Then {
    /// Runs this action with the text as its argument.
    Action(ActionId),
    /// Runs the action named by this template with `{arg}` replaced by the text's first word,
    /// passing the rest of the text as the argument (`"{arg}"`: the text names the action).
    Named(String),
}

impl Then {
    /// The action name and argument for the confirmed `text`, for `Named`.
    pub fn resolve(template: &str, text: &str) -> (String, String) {
        let text = text.trim();
        let (word, rest) = text.split_once(char::is_whitespace).unwrap_or((text, ""));
        (template.replace("{arg}", word), rest.trim_start().to_string())
    }
}

impl Config {
    pub fn user_mod_names(&self) -> Vec<&str> {
        self.modifiers.iter().map(|m| m.name.as_str()).collect()
    }

    /// Policy for waiting after `pending` in `mode`: options of every node on its path, deeper
    /// nodes (and, at equal depth, mode nodes) overriding shallower ones field by field.
    pub fn policy(&self, pending: &[Chord], mode: ModeId) -> Policy {
        let mut path: Vec<&Prefix> = self
            .prefixes
            .iter()
            .filter(|p| (p.modes.is_empty() || p.modes.contains(&mode)) && pending.starts_with(&p.keys))
            .collect();
        path.sort_by_key(|p| (p.keys.len(), !p.modes.is_empty()));
        path.iter().fold(Policy::default(), |mut policy, p| {
            policy.on_mismatch = p.options.on_mismatch.unwrap_or(policy.on_mismatch);
            policy.timeout_ms = p.options.timeout_ms.unwrap_or(policy.timeout_ms);
            if p.options.fallback.is_some() {
                policy.fallback = p.options.fallback.clone();
            }
            policy
        })
    }

    pub fn action_id(&self, name: &str) -> Option<ActionId> {
        self.actions.iter().position(|a| a.name == name)
    }

    /// Scan codes that rules, modifiers or keyswap listen for; the hook reports these as `Key::Sc`.
    pub fn scancodes(&self) -> Vec<u16> {
        let rule_keys = self.rules.iter().flat_map(|r| r.keys.0.iter().map(|c| &c.key));
        let mut v: Vec<u16> = rule_keys
            .chain(self.modifiers.iter().map(|m| &m.key))
            .chain(self.keyswap.keys().map(|(k, _)| k))
            .filter_map(|k| match k {
                Key::Sc(s) => Some(*s),
                _ => None,
            })
            .collect();
        v.sort_unstable();
        v.dedup();
        v
    }
}
