//! Turning rules and action steps into commands, with modifier neutralization.

use crate::{Active, Command, Engine, Fault};
use grapnel_config::{ActionId, Press, Step, WindowInfo, any_matches};
use grapnel_keys::{Chord, Key, KeySeq, Mods};

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
                    self.inject_pass(trigger, &mut out);
                }
            }
            return out;
        };
        // Keys-only output is held like a keyboard: earlier chords are tapped, the last one held.
        let held = match imp.steps.as_slice() {
            [Step::Keys(s)] => s.0.split_last().filter(|(last, _)| !instant(&last.key)),
            _ => None,
        };
        match held.filter(|_| rule.press == Press::Hold && !instant(&trigger)) {
            Some((last, before)) => {
                self.tap_chords(before, &mut out);
                self.press_mods(last.mods, &mut out);
                out.push(key(&last.key, true));
                if rule.keep_mods {
                    self.keep(seq, last.mods);
                }
                self.active.insert(trigger, Active::Hold(last.clone()));
            }
            None => {
                self.run_steps(&imp.steps, "", win, 0, &mut out);
                if !instant(&trigger) {
                    let last = match imp.steps.last() {
                        Some(Step::Keys(s)) => s.0.last().map(|c| Step::Keys(KeySeq(vec![c.clone()]))),
                        Some(s @ (Step::Text(_) | Step::MouseMove { .. })) => Some(s.clone()),
                        _ => None,
                    };
                    self.active.insert(trigger, Active::Tap(last));
                }
            }
        }
        out
    }

    /// Makes the user modifiers of the triggering chord keep the output modifiers pressed until they
    /// are released. Modifiers the user physically pressed in that chord stay under their control.
    fn keep(&mut self, seq: &[Chord], mods: Mods) {
        let Some(trigger) = seq.last() else { return };
        let user = trigger.mods.0 >> 4;
        let kept = Mods(mods.real().0 & !trigger.mods.real().0);
        if user == 0 {
            // Real-modifier trigger (e.g. Alt-Tab → Ctrl-Tab): replace the trigger's modifiers.
            let lifted = Mods(trigger.mods.real().0 & !mods.real().0);
            self.real_kept = Some((lifted, kept));
        }
        for i in (0..self.kept.len()).filter(|i| user & (1 << i) != 0) {
            self.kept[i] = kept;
        }
    }

    /// Output for a key repeat of a held trigger: like a keyboard, only the last key repeats.
    pub(crate) fn repeat(&mut self, a: &Active, win: &WindowInfo) -> Vec<Command> {
        let mut out = vec![];
        match a {
            Active::Hold(c) => out.push(key(&c.key, true)),
            Active::Pass(k) => out.push(key(k, true)),
            Active::Tap(Some(step)) => self.run_steps(std::slice::from_ref(step), "", win, 0, &mut out),
            Active::Tap(None) => {}
        }
        out
    }

    /// Output when a held trigger is released. `a` must already be removed from `active`.
    pub(crate) fn release(&mut self, a: Active) -> Vec<Command> {
        let mut out = vec![];
        match a {
            Active::Hold(c) => {
                let shared = self.active.values().any(|o| matches!(o, Active::Hold(h) if h.key == c.key));
                if !shared {
                    self.key_up(&c.key, &mut out);
                }
                self.restore(&mut out);
            }
            Active::Pass(k) => out.push(key(&k, false)),
            Active::Tap(_) => {}
        }
        out
    }

    /// Releases `k`, masking a bare Alt/Win release.
    fn key_up(&self, k: &Key, out: &mut Vec<Command>) {
        if matches!(k.real_mod(), Some(Mods::ALT | Mods::WIN)) {
            out.extend([key(&MASK, true), key(&MASK, false)]);
        }
        out.push(key(k, false));
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
            out.push(Command::Error(Fault::TooDeep(action.name.clone())));
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
                    self.restore(out); // press/release the modes' `hold` modifiers
                }
                Step::Control(c) => out.push(Command::Control(*c)),
                Step::Input { prompt, then, position } => {
                    out.push(Command::InputBox { prompt: prompt.clone(), then: *then, position: *position })
                }
            }
        }
    }

    /// Taps each chord with exactly its modifiers, then restores the held modifier state.
    pub(crate) fn tap_seq(&mut self, chords: &[Chord], out: &mut Vec<Command>) {
        self.tap_chords(chords, out);
        self.restore(out);
    }

    fn tap_chords(&mut self, chords: &[Chord], out: &mut Vec<Command>) {
        for c in chords.iter().filter(|c| !matches!(c.key, Key::Pad(_) | Key::Gesture(..))) {
            self.press_mods(c.mods, out);
            out.push(key(&c.key, true));
            if !matches!(c.key, Key::Wheel(_)) {
                self.key_up(&c.key, out);
            }
        }
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

    /// Makes the OS modifier state equal the held modifiers: physical ones plus the `as`
    /// modifiers of held user modifiers.
    pub(crate) fn restore(&mut self, out: &mut Vec<Command>) {
        let (lifted, real_kept) = self.real_kept.unwrap_or_default();
        let mut physical: Vec<Key> =
            self.down.iter().filter(|k| k.real_mod().is_some_and(|m| !lifted.contains(m))).cloned().collect();
        let emulate = self.emulated_mods() | real_kept;
        for (m, left) in REAL {
            if emulate.contains(m) && !physical.iter().any(|k| k.real_mod() == Some(m)) {
                physical.push(Key::Vk(left));
            }
        }
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
