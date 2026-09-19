//! Serde types that mirror one grapnel TOML file 1:1. No semantic validation here.

pub use indexmap::IndexMap;
use serde::{Deserialize, Serialize};

fn is_false(b: &bool) -> bool {
    !*b
}

#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct RawConfig {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub include: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub settings: Option<RawSettings>,
    #[serde(default, skip_serializing_if = "IndexMap::is_empty")]
    pub modes: IndexMap<String, RawMode>,
    #[serde(default, skip_serializing_if = "IndexMap::is_empty")]
    pub modifiers: IndexMap<String, RawModifier>,
    #[serde(default, skip_serializing_if = "IndexMap::is_empty")]
    pub targets: IndexMap<String, RawTarget>,
    /// Physical key (alone or with `S-`) → the JIS key chord it types, seen by the keymap too.
    #[serde(default, skip_serializing_if = "IndexMap::is_empty")]
    pub keyswap: IndexMap<String, String>,
    #[serde(default, skip_serializing_if = "RawNode::is_empty")]
    pub keymap: RawNode,
    #[serde(default, skip_serializing_if = "IndexMap::is_empty")]
    pub actions: IndexMap<String, RawAction>,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct RawSettings {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub initial_mode: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub passthrough: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub suspend_hotkey: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub gesture_threshold: Option<u32>,
    /// Display language (`"ja"`, `"en"`, ...); unset follows Windows.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub language: Option<String>,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct RawMode {
    #[serde(default, skip_serializing_if = "is_false")]
    pub block_unmapped: bool,
    /// Digits typed before a binding repeat it (Vim's `3j`).
    #[serde(default, skip_serializing_if = "is_false")]
    pub count: bool,
    /// Mode to switch to when an input matches no rule in this mode.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub unmapped_to: Option<String>,
    /// Real modifiers (`"S"`, `"C-S"`, ...) kept pressed while in this mode.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hold: Option<String>,
    /// Bindings that only apply in this mode.
    #[serde(default, skip_serializing_if = "RawNode::is_empty")]
    pub keymap: RawNode,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct RawModifier {
    pub key: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tap: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tap_timeout_ms: Option<u32>,
    /// Real modifiers (`"C"`, `"C-S"`, ...) sent with keys that no rule matches.
    #[serde(default, rename = "as", skip_serializing_if = "Option::is_none")]
    pub emulate: Option<String>,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct RawTarget {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub app: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub class: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub control: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub uia_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub uia_name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub uia_type: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub not: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub any: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub all: Vec<String>,
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, Default, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Press {
    #[default]
    Hold,
    Tap,
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, Default, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Mismatch {
    #[default]
    Replay,
    Discard,
    Fallback,
}

/// A keymap node: chord → binding, plus options inherited by the subtree.
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
pub struct RawNode {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub options: Option<RawNodeOptions>,
    #[serde(flatten)]
    pub children: IndexMap<String, RawBinding>,
}

impl RawNode {
    pub fn is_empty(&self) -> bool {
        self.options.is_none() && self.children.is_empty()
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct RawNodeOptions {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub on_mismatch: Option<Mismatch>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub timeout_ms: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fallback: Option<String>,
}

/// What a chord in a keymap does. Tried in this order when reading.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(untagged)]
pub enum RawBinding {
    /// An action name, or keys to send.
    Short(String),
    Steps(Vec<RawStep>),
    /// A table with `do`.
    Leaf(RawLeaf),
    /// Any other table: the chord is a prefix.
    Node(RawNode),
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct RawLeaf {
    #[serde(rename = "do")]
    pub action: RawAction,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub press: Option<Press>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fallback: Option<String>,
    #[serde(default, skip_serializing_if = "is_false")]
    pub keep_mods: bool,
    /// Remembered for `{ control = "repeat" }` (Vim's `.`).
    #[serde(default, skip_serializing_if = "is_false")]
    pub repeat: bool,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub targets: Vec<String>,
    /// Unknown options, kept so they round-trip and the compiler can report them. Being flattened
    /// also makes a leaf readable only from a table, never from an array.
    #[serde(flatten)]
    pub unknown: IndexMap<String, serde_json::Value>,
}

/// An action: keys, steps, or target name (`"*"` = always) → steps, tried in order.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(untagged)]
pub enum RawAction {
    Short(String),
    Steps(Vec<RawStep>),
    ByTarget(IndexMap<String, RawSteps>),
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(untagged)]
pub enum RawSteps {
    Short(String),
    Steps(Vec<RawStep>),
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum ControlCmd {
    Suspend,
    Reload,
    Exit,
    /// Runs the last binding marked `repeat` again, as many times as then.
    Repeat,
}

/// One action step. A bare string is shorthand for `{ keys = "..." }`.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(untagged, deny_unknown_fields)]
pub enum RawStep {
    Short(String),
    Keys {
        keys: String,
    },
    Text {
        text: String,
    },
    MouseMove {
        mouse_move: [i32; 2],
    },
    MouseMoveTo {
        mouse_move_to: [i32; 2],
    },
    Sleep {
        sleep: u32,
    },
    Run {
        run: String,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        args: Vec<String>,
    },
    Call {
        call: String,
        /// Argument for the called action; `{arg}` stands for the current one. Unset is empty.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        arg: Option<String>,
    },
    Mode {
        mode: String,
    },
    Control {
        control: ControlCmd,
    },
    Input {
        input: String,
        /// Action to run with the text; without it the text names the action to run.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        then: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        position: Option<InputPosition>,
    },
}

/// Where the input box appears.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, Default, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum InputPosition {
    /// Centered, a third of the way down the primary screen.
    #[default]
    Center,
    /// Full width along the bottom of the foreground window's monitor.
    Bottom,
}

#[cfg(test)]
mod tests;
