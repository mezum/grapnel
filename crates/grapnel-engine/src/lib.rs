//! Platform-independent remapping state machine: input events in, output commands out.
//! Time is passed in as milliseconds so tests can drive it.

mod input;
mod output;

use grapnel_config::{ActionId, Config, ControlCmd, Mismatch, ModeId, WindowInfo, any_matches};
use grapnel_keys::{Chord, Dir, Key, Mods, MouseButton};
use std::collections::{HashMap, HashSet};
use std::sync::Arc;

#[derive(Clone, Debug, PartialEq)]
pub enum Event {
    /// Wheel and gesture keys only ever arrive as `Down`.
    Down(Key),
    Up(Key),
    MouseMove {
        dx: i32,
        dy: i32,
    },
}

#[derive(Clone, Debug, PartialEq)]
pub enum Command {
    /// Keyboard keys, mouse buttons and wheel notches (`down` only).
    Key {
        key: Key,
        down: bool,
    },
    Text(String),
    MouseMove {
        x: i32,
        y: i32,
        absolute: bool,
    },
    Sleep(u32),
    Run {
        program: String,
        args: Vec<String>,
    },
    InputBox {
        prompt: String,
        then: ActionId,
    },
    ModeChanged(String),
    Control(ControlCmd),
    /// Runtime error to report to the user.
    Error(String),
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Reaction {
    /// Hide the original event from the OS.
    pub consume: bool,
    pub commands: Vec<Command>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum UserMod {
    Idle,
    Pending(u64),
    Active,
}

struct Pending {
    chords: Vec<Chord>,
    /// First rule the pending chords were a prefix of; its mismatch policy applies.
    rule: usize,
    deadline: Option<u64>,
}

/// Output state of a trigger key that is still held.
struct Active {
    repeat: Vec<Command>,
    release: Vec<Command>,
    /// Restore modifiers to the physical state after `release`.
    restore: bool,
}

struct Gesture {
    button: MouseButton,
    acc: (i32, i32),
    dirs: Vec<Dir>,
}

pub struct Engine {
    cfg: Arc<Config>,
    mode: ModeId,
    down: HashSet<Key>,
    os_mods: Vec<Key>,
    user_mods: Vec<UserMod>,
    pending: Option<Pending>,
    active: HashMap<Key, Active>,
    swallowed: HashSet<Key>,
    gesture: Option<Gesture>,
    gesture_buttons: Vec<MouseButton>,
}

impl Engine {
    pub fn new(cfg: Arc<Config>) -> Engine {
        let gesture_buttons = cfg
            .rules
            .iter()
            .flat_map(|r| &r.keys.0)
            .filter_map(|c| match c.key {
                Key::Gesture(b, _) => Some(b),
                _ => None,
            })
            .collect();
        Engine {
            mode: cfg.settings.initial_mode,
            user_mods: vec![UserMod::Idle; cfg.modifiers.len()],
            down: HashSet::new(),
            os_mods: Vec::new(),
            pending: None,
            active: HashMap::new(),
            swallowed: HashSet::new(),
            gesture: None,
            gesture_buttons,
            cfg,
        }
    }

    pub fn mode_name(&self) -> &str {
        &self.cfg.modes[self.mode].name
    }

    pub fn handle(&mut self, ev: Event, win: &WindowInfo, now: u64) -> Reaction {
        match ev {
            Event::Down(key) => self.on_down(key, win, now),
            Event::Up(key) => self.on_up(key, win, now),
            Event::MouseMove { dx, dy } => {
                self.track_gesture(dx, dy);
                Reaction::default()
            }
        }
    }

    /// Fires a chord timeout if one is due.
    pub fn tick(&mut self, now: u64) -> Vec<Command> {
        match &self.pending {
            Some(p) if p.deadline.is_some_and(|d| d <= now) => self.mismatch(None),
            _ => Vec::new(),
        }
    }

    pub fn next_deadline(&self) -> Option<u64> {
        self.pending.as_ref().and_then(|p| p.deadline)
    }

    /// Runs an action directly (input box result, pipe commands).
    pub fn invoke(&mut self, action: ActionId, arg: &str, win: &WindowInfo) -> Vec<Command> {
        let mut out = Vec::new();
        self.run_action(action, arg, win, 0, &mut out);
        out
    }

    /// Releases everything held and forgets all state. Call before unhooking.
    pub fn reset(&mut self) -> Vec<Command> {
        let mut out: Vec<Command> = self.active.drain().flat_map(|(_, a)| a.release).collect();
        self.down.retain(|k| k.real_mod().is_some());
        self.restore(&mut out);
        *self = Engine { mode: self.mode, ..Engine::new(self.cfg.clone()) };
        out
    }
}

fn consumed(commands: Vec<Command>) -> Reaction {
    Reaction { consume: true, commands }
}
