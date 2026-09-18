//! Loads grapnel configuration files, validates them and compiles them into [`Config`].

mod compile;
mod keymap;
mod load;
mod matcher;

pub use compile::compile;
pub use grapnel_schema::{ControlCmd, Mismatch, Press, RawConfig};
pub use load::{default_entry, load, save};
pub use matcher::{Field, Matcher, Target, TargetId, WindowInfo, any_matches, target_matches};

use grapnel_keys::{Chord, Key, KeySeq, Mods};

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
}

#[derive(Clone, Debug)]
pub struct Mode {
    pub name: String,
    pub block_unmapped: bool,
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

#[derive(Clone, Debug)]
pub struct Prefix {
    pub keys: Vec<Chord>,
    /// Empty = every mode.
    pub modes: Vec<ModeId>,
    pub policy: Policy,
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
    Input { prompt: String, then: ActionId },
}

impl Config {
    pub fn user_mod_names(&self) -> Vec<&str> {
        self.modifiers.iter().map(|m| m.name.as_str()).collect()
    }

    /// Policy for waiting after `pending` in `mode`: the deepest node with options on its path.
    pub fn policy(&self, pending: &[Chord], mode: ModeId) -> Policy {
        let applies = |p: &&Prefix| (p.modes.is_empty() || p.modes.contains(&mode)) && pending.starts_with(&p.keys);
        // Deepest first; on a tie the mode-specific node (listed first) wins.
        let best = self.prefixes.iter().filter(applies).fold(None::<&Prefix>, |best, p| match best {
            Some(b) if b.keys.len() >= p.keys.len() => Some(b),
            _ => Some(p),
        });
        best.map(|p| p.policy.clone()).unwrap_or_default()
    }

    pub fn action_id(&self, name: &str) -> Option<ActionId> {
        self.actions.iter().position(|a| a.name == name)
    }

    /// Scan codes that rules or modifiers listen for; the hook reports these as `Key::Sc`.
    pub fn scancodes(&self) -> Vec<u16> {
        let rule_keys = self.rules.iter().flat_map(|r| r.keys.0.iter().map(|c| &c.key));
        let mut v: Vec<u16> = rule_keys
            .chain(self.modifiers.iter().map(|m| &m.key))
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
