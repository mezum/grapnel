//! Platform-independent remapping state machine: input events in, output commands out.
//! Time is passed in as milliseconds so tests can drive it.

mod input;
mod output;

use grapnel_config::{
    ActionId, Config, ControlCmd, InputPosition, Mismatch, ModeId, Policy, Step, Then, WindowInfo, any_matches,
};
use grapnel_keys::{Chord, Dir, Key, KeySeq, Mods, MouseButton};
use std::collections::{HashMap, HashSet};
use std::sync::Arc;

/// Largest count typed before a binding.
const MAX_COUNT: u32 = 999;

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
    /// Replaces the focused input's text, or waits for an input named like `into` (a pattern).
    /// Like `Sleep`, it ends the batch: what follows waits for it.
    SetText {
        text: String,
        into: Option<String>,
    },
    /// Steps after a `Sleep` or `SetText`: hand them back to [`Engine::resume`] once it is over, so they are
    /// planned against the modifier state of that time.
    Resume(Later),
    Run {
        program: String,
        args: Vec<String>,
    },
    InputBox {
        prompt: String,
        then: Then,
        position: InputPosition,
    },
    ModeChanged(String),
    Control(ControlCmd),
    /// Runtime error to report to the user.
    Error(Fault),
    /// A short message for the user (shown briefly, not an error).
    Notice(Notice),
}

/// Steps left to run after a `Sleep` or `SetText`, innermost call first, and the window they were started for.
#[derive(Clone, Debug, PartialEq)]
pub struct Later {
    frames: Vec<Frame>,
    win: WindowInfo,
}

#[derive(Clone, Debug, PartialEq)]
struct Frame {
    steps: Vec<Step>,
    arg: String,
    depth: usize,
}

/// The app puts these into words, so the engine stays language-neutral.
#[derive(Clone, Debug, PartialEq)]
pub enum Notice {
    /// This key sequence (formatted, e.g. `"C-x q"`) matches nothing.
    Undefined(String),
    /// This key sequence was left unfinished.
    TimedOut(String),
    /// A count is being typed (Vim's `3` before `j`).
    Count(u32),
}

#[derive(Clone, Debug, PartialEq)]
pub enum Fault {
    /// No action has this name (or `#id`).
    UnknownAction(String),
    /// This action calls others too deeply (likely a loop).
    TooDeep(String),
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
    /// Options of the keymap node the pending chords lead to.
    policy: Policy,
    deadline: Option<u64>,
}

/// Output state of a trigger key that is still held.
enum Active {
    /// The output chord is held until the trigger is released.
    Hold(Chord),
    /// The original key was injected and is passed through until released.
    Pass(Key),
    /// A swapped key, held as the chord it stands for. Its repeats put the swap's Shift back, as
    /// another key released meanwhile may have restored the physical one.
    Swap(Chord),
    /// A tap action; key repeat re-runs only its last input step (last chord for keys).
    Tap(Option<Step>),
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
    /// Per user modifier: output modifiers kept pressed by a `keep_mods` rule (replaces `as`).
    kept: Vec<Mods>,
    /// `keep_mods` on a real-modifier trigger: `(lifted, kept)` — the physical modifiers in `lifted`
    /// are kept released and `kept` pressed until the user lets go of them.
    real_kept: Option<(Mods, Mods)>,
    pending: Option<Pending>,
    active: HashMap<Key, Active>,
    swallowed: HashSet<Key>,
    /// Keys left untouched on press (binding without an implementation here); their repeats and
    /// release stay untouched too.
    passing: HashSet<Key>,
    gesture: Option<Gesture>,
    gesture_buttons: Vec<MouseButton>,
    /// Digits typed so far in a `count` mode.
    count: Option<u32>,
    /// The last binding marked `repeat`: its action and count, for `ControlCmd::Repeat`.
    last: Option<(ActionId, u32)>,
    /// Set by a `Sleep` step: the steps after it are collected here instead of run.
    later: Option<Later>,
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
            kept: vec![Mods::NONE; cfg.modifiers.len()],
            real_kept: None,
            down: HashSet::new(),
            os_mods: Vec::new(),
            pending: None,
            active: HashMap::new(),
            swallowed: HashSet::new(),
            passing: HashSet::new(),
            gesture: None,
            gesture_buttons,
            count: None,
            last: None,
            later: None,
            cfg,
        }
    }

    pub fn mode_name(&self) -> &str {
        &self.cfg.modes[self.mode].name
    }

    pub fn handle(&mut self, ev: Event, win: &WindowInfo, now: u64) -> Reaction {
        let mut r = match ev {
            Event::Down(key) => self.on_down(key, win, now),
            Event::Up(key) => self.on_up(key, win, now),
            Event::MouseMove { dx, dy } => {
                self.track_gesture(dx, dy);
                Reaction::default()
            }
        };
        self.finish(&mut r.commands);
        r
    }

    /// Fires a chord timeout if one is due.
    pub fn tick(&mut self, now: u64) -> Vec<Command> {
        let mut out = match &self.pending {
            Some(p) if p.deadline.is_some_and(|d| d <= now) => self.mismatch(None),
            _ => Vec::new(),
        };
        self.finish(&mut out);
        out
    }

    pub fn next_deadline(&self) -> Option<u64> {
        self.pending.as_ref().and_then(|p| p.deadline)
    }

    /// Runs an action directly (input box result, pipe commands).
    pub fn invoke(&mut self, action: ActionId, arg: &str, win: &WindowInfo) -> Vec<Command> {
        if action >= self.cfg.actions.len() {
            return vec![Command::Error(Fault::UnknownAction(format!("#{action}")))];
        }
        let mut out = Vec::new();
        self.run_action(action, arg, win, 0, &mut out);
        self.finish(&mut out);
        out
    }

    /// Runs the steps a `Command::Resume` carried, after its `Sleep`.
    pub fn resume(&mut self, later: Later) -> Vec<Command> {
        let mut out = Vec::new();
        for f in later.frames {
            self.run_steps(&f.steps, &f.arg, &later.win, f.depth, &mut out);
        }
        self.finish(&mut out);
        out
    }

    /// Ends output with the steps a `Sleep` put off, if any.
    fn finish(&mut self, out: &mut Vec<Command>) {
        if let Some(later) = self.later.take().filter(|l| !l.frames.is_empty()) {
            out.push(Command::Resume(later));
        }
    }

    /// Runs what an input box's `then` asks for with the confirmed text.
    pub fn invoke_input(&mut self, then: &Then, text: &str, win: &WindowInfo) -> Vec<Command> {
        match then {
            Then::Action(id) => self.invoke(*id, text, win),
            Then::Named(template) => {
                let (name, arg) = Then::resolve(template, text);
                self.invoke_named(&name, &arg, win)
            }
        }
    }

    /// Runs the action with this name (e.g. typed into an M-x prompt).
    pub fn invoke_named(&mut self, name: &str, arg: &str, win: &WindowInfo) -> Vec<Command> {
        match self.cfg.action_id(name) {
            Some(id) => self.invoke(id, arg, win),
            None => vec![Command::Error(Fault::UnknownAction(name.into()))],
        }
    }

    /// Releases everything held and forgets all state except the mode and held modifiers.
    /// Call before unhooking.
    pub fn reset(&mut self) -> Vec<Command> {
        let mut out = Vec::new();
        for (_, a) in std::mem::take(&mut self.active) {
            out.extend(self.release(a));
        }
        self.down.retain(|k| k.real_mod().is_some());
        self.user_mods.fill(UserMod::Idle);
        self.kept.fill(Mods::NONE);
        self.real_kept = None;
        // A mode that holds modifiers must not leave them pressed while unhooked.
        let mode =
            if self.cfg.modes[self.mode].hold == Mods::NONE { self.mode } else { self.cfg.settings.initial_mode };
        self.mode = mode;
        self.restore(&mut out);
        let mods = std::mem::take(&mut self.down);
        // The last change survives (the input box resets the engine before `:w`, then `.`).
        let last = self.last.take();
        *self = Engine { mode, last, os_mods: mods.iter().cloned().collect(), ..Engine::new(self.cfg.clone()) };
        self.down = mods;
        out
    }

    /// Tells the engine which real modifier keys are physically held (e.g. read from the OS after re-hooking).
    pub fn sync_modifiers(&mut self, held: &[Key]) {
        self.down.retain(|k| k.real_mod().is_none());
        self.down.extend(held.iter().cloned());
        self.os_mods = held.to_vec();
    }
}

fn consumed(commands: Vec<Command>) -> Reaction {
    Reaction { consume: true, commands }
}
