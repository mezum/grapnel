#![allow(dead_code)]

use grapnel_config::{WindowInfo, compile};
use grapnel_engine::{Command, Engine, Event, Reaction};
use grapnel_keys::{Key, parse_key};
use std::path::PathBuf;
use std::sync::Arc;

pub struct T {
    pub e: Engine,
    pub win: WindowInfo,
    pub now: u64,
}

pub fn t(cfg: &str) -> T {
    let raw = toml::from_str(cfg).unwrap();
    let cfg = compile(&[(PathBuf::from("test.toml"), raw)]).unwrap();
    T { e: Engine::new(Arc::new(cfg)), win: WindowInfo::default(), now: 0 }
}

/// Loads a config file from disk, following `include`.
pub fn t_file(path: &std::path::Path) -> T {
    let files = grapnel_config::load(path).unwrap();
    let cfg = compile(&files).unwrap_or_else(|e| {
        panic!(
            "{}",
            e.join(
                "
"
            )
        )
    });
    T { e: Engine::new(Arc::new(cfg)), win: WindowInfo::default(), now: 0 }
}

pub fn k(s: &str) -> Key {
    parse_key(s).unwrap()
}

/// `"+a -a +LCtrl"` → key commands.
pub fn keys(s: &str) -> Vec<Command> {
    s.split_whitespace()
        .map(|t| {
            let (sign, name) = t.split_at(1);
            Command::Key { key: k(name), down: sign == "+" }
        })
        .collect()
}

/// Consumed with the given key commands.
pub fn eaten(s: &str) -> Reaction {
    Reaction { consume: true, commands: keys(s) }
}

pub fn pass() -> Reaction {
    Reaction::default()
}

impl T {
    pub fn down(&mut self, s: &str) -> Reaction {
        self.e.handle(Event::Down(k(s)), &self.win, self.now)
    }
    pub fn up(&mut self, s: &str) -> Reaction {
        self.e.handle(Event::Up(k(s)), &self.win, self.now)
    }
    pub fn tap(&mut self, s: &str) -> (Reaction, Reaction) {
        (self.down(s), self.up(s))
    }
    pub fn mv(&mut self, dx: i32, dy: i32) -> Reaction {
        self.e.handle(Event::MouseMove { dx, dy }, &self.win, self.now)
    }
    pub fn app(&mut self, exe: &str) {
        self.win.exe_name = exe.into();
    }
}

/// One binding `keys` → inline steps `steps` (TOML array body), with extra leaf options
/// (`press = "tap"`, ...) and extra TOML placed before it.
pub fn rule(keys: &str, steps: &str, extra_rule: &str, extra: &str) -> String {
    format!("{extra}\n[keymap.\"{keys}\"]\ndo = [{steps}]\n{extra_rule}\n")
}
