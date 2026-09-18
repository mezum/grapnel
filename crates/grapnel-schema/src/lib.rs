//! Serde types that mirror one grapnel TOML file 1:1. No semantic validation here.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

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
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub modes: BTreeMap<String, RawMode>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub modifiers: BTreeMap<String, RawModifier>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub targets: BTreeMap<String, RawTarget>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub rules: Vec<RawRule>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub actions: BTreeMap<String, Vec<RawActionImpl>>,
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
}

#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct RawMode {
    #[serde(default, skip_serializing_if = "is_false")]
    pub block_unmapped: bool,
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

#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct RawRule {
    pub keys: String,
    pub action: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub targets: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub modes: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub press: Option<Press>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fallback: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub on_mismatch: Option<Mismatch>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub timeout_ms: Option<u32>,
    /// Keep the output's modifiers pressed until the rule's user modifiers are released.
    #[serde(default, skip_serializing_if = "is_false")]
    pub keep_mods: bool,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct RawActionImpl {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub when: Vec<String>,
    #[serde(rename = "do")]
    pub steps: Vec<RawStep>,
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum ControlCmd {
    Suspend,
    Reload,
    Exit,
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
        then: String,
    },
}

#[cfg(test)]
mod tests;
