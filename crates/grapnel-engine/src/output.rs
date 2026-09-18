//! Turning rules and action steps into commands, with modifier neutralization.

use crate::{Active, Command, Engine};
use grapnel_config::{ActionId, Press, Step, WindowInfo, any_matches};
use grapnel_keys::{Chord, Key, Mods};

const MAX_CALL_DEPTH: usize = 8;
/// Unassigned VK sent before releasing Alt/Win so the menu or Start does not open.
const MASK: Key = Key::Vk(0xE8);
/// Real modifiers and the key pressed when one is needed but none is held.
const REAL: [(Mods, u8); 4] = [(Mods::CTRL, 0xA2), (Mods::ALT, 0xA4), (Mods::SHIFT, 0xA0), (Mods::WIN, 0x5B)];

fn key(key: &Key, down: bool) -> Command {
    Command::Key { key: key.clone(), down }
}

fn instant(k: &Key) -> bool {
    matches!(k, Key::Wheel(_) | Key::Gesture(..))
}

impl Engine {
    /// Executes rule `i`, triggered by `trigger` completing `seq`.
    pub(crate) fn fire(&mut self, i: usize, seq: &[Chord], trigger: Key, win: &WindowInfo) -> Vec<Command> {
        let cfg = self.cfg.clone();
        let rule = &cfg.rules[i];
        let imp = cfg.actions[rule.action].impls.iter().find(|m| any_matches(&cfg.targets, &m.when, win));
        let mut out = vec![];
        let Some(imp) = imp else {
            match &rule.fallback {
                Some(f) => self.tap_seq(&f.0, &mut out),
                None => {
                    // Let the original input through: earlier chords as taps, the trigger as held.
                    self.tap_seq(&seq[..seq.len() - 1], &mut out);
                    if !matches!(trigger, Key::Gesture(..)) {
                        out.push(key(&trigger, true));
                    }
                    if !instant(&trigger) {
                        let a = Active {
                            repeat: vec![key(&trigger, true)],
                            release: vec![key(&trigger, false)],
                            restore: false,
                        };
                        self.active.insert(trigger, a);
                    }
                }
            }
            return out;
        };
        let single = match imp.steps.as_slice() {
            [Step::Keys(s)] if s.0.len() == 1 && !instant(&s.0[0].key) => Some(s.0[0].clone()),
            _ => None,
        };
        match single.filter(|_| rule.press == Press::Hold && !instant(&trigger)) {
            Some(c) => {
                self.press_mods(c.mods, &mut out);
                out.push(key(&c.key, true));
                let a = Active { repeat: vec![key(&c.key, true)], release: vec![key(&c.key, false)], restore: true };
                self.active.insert(trigger, a);
            }
            None => {
                self.run_steps(&imp.steps, "", win, 0, &mut out);
                if !instant(&trigger) {
                    let input_only = imp
                        .steps
                        .iter()
                        .all(|s| matches!(s, Step::Keys(_) | Step::Text(_) | Step::MouseMove { .. } | Step::Sleep(_)));
                    let repeat = if input_only { out.clone() } else { vec![] };
                    self.active.insert(trigger, Active { repeat, release: vec![], restore: false });
                }
            }
        }
        out
    }

    pub(crate) fn run_action(
        &mut self,
        id: ActionId,
        arg: &str,
        win: &WindowInfo,
        depth: usize,
        out: &mut Vec<Command>,
    ) {
        let cfg = self.cfg.clone();
        let action = &cfg.actions[id];
        if depth > MAX_CALL_DEPTH {
            out.push(Command::Error(format!("action '{}': calls nested deeper than {MAX_CALL_DEPTH}", action.name)));
            return;
        }
        if let Some(imp) = action.impls.iter().find(|m| any_matches(&cfg.targets, &m.when, win)) {
            self.run_steps(&imp.steps, arg, win, depth, out);
        }
    }

    fn run_steps(&mut self, steps: &[Step], arg: &str, win: &WindowInfo, depth: usize, out: &mut Vec<Command>) {
        let subst = |s: &str| s.replace("{arg}", arg);
        for step in steps {
            match step {
                Step::Keys(seq) => self.tap_seq(&seq.0, out),
                Step::Text(t) => {
                    self.press_mods(Mods::NONE, out);
                    out.push(Command::Text(subst(t)));
                    self.restore(out);
                }
                Step::MouseMove { x, y, absolute } => {
                    out.push(Command::MouseMove { x: *x, y: *y, absolute: *absolute })
                }
                Step::Sleep(ms) => out.push(Command::Sleep(*ms)),
                Step::Run { program, args } => {
                    out.push(Command::Run { program: subst(program), args: args.iter().map(|a| subst(a)).collect() })
                }
                Step::Call { action, arg: a } => {
                    let a = a.as_deref().map(subst).unwrap_or_else(|| arg.to_string());
                    self.run_action(*action, &a, win, depth + 1, out);
                }
                Step::Mode(m) => {
                    self.mode = *m;
                    out.push(Command::ModeChanged(self.cfg.modes[*m].name.clone()));
                }
                Step::Control(c) => out.push(Command::Control(*c)),
                Step::Input { prompt, then } => out.push(Command::InputBox { prompt: prompt.clone(), then: *then }),
            }
        }
    }

    /// Taps each chord with exactly its modifiers, then restores the physical modifier state.
    pub(crate) fn tap_seq(&mut self, chords: &[Chord], out: &mut Vec<Command>) {
        for c in chords.iter().filter(|c| !matches!(c.key, Key::Pad(_) | Key::Gesture(..))) {
            self.press_mods(c.mods, out);
            out.push(key(&c.key, true));
            if !matches!(c.key, Key::Wheel(_)) {
                out.push(key(&c.key, false));
            }
        }
        self.restore(out);
    }

    /// Makes the OS modifier state equal `want` (real modifiers only).
    fn press_mods(&mut self, want: Mods, out: &mut Vec<Command>) {
        for (m, left) in REAL {
            let held: Vec<Key> = self.os_mods.iter().filter(|k| k.real_mod() == Some(m)).cloned().collect();
            if want.contains(m) && held.is_empty() {
                out.push(key(&Key::Vk(left), true));
                self.os_mods.push(Key::Vk(left));
            } else if !want.contains(m) && !held.is_empty() {
                if m == Mods::ALT || m == Mods::WIN {
                    out.extend([key(&MASK, true), key(&MASK, false)]);
                }
                out.extend(held.iter().map(|k| key(k, false)));
                self.os_mods.retain(|k| !held.contains(k));
            }
        }
    }

    /// Makes the OS modifier state equal the physically held modifiers.
    pub(crate) fn restore(&mut self, out: &mut Vec<Command>) {
        let physical: Vec<Key> = self.down.iter().filter(|k| k.real_mod().is_some()).cloned().collect();
        let release: Vec<Key> = self.os_mods.iter().filter(|k| !physical.contains(k)).cloned().collect();
        if release.iter().any(|k| matches!(k.real_mod(), Some(Mods::ALT | Mods::WIN))) {
            out.extend([key(&MASK, true), key(&MASK, false)]);
        }
        out.extend(release.iter().map(|k| key(k, false)));
        let press: Vec<Key> = physical.iter().filter(|k| !self.os_mods.contains(k)).cloned().collect();
        out.extend(press.iter().map(|k| key(k, true)));
        // A bare Alt/Win press followed by the user's release would open the menu/Start.
        if press.iter().any(|k| matches!(k.real_mod(), Some(Mods::ALT | Mods::WIN))) {
            out.extend([key(&MASK, true), key(&MASK, false)]);
        }
        self.os_mods = physical;
    }
}
