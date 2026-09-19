//! Input event handling: modifiers, repeats, gestures and rule matching.

use crate::*;

impl Engine {
    fn user_mod_index(&self, key: &Key) -> Option<usize> {
        self.cfg.modifiers.iter().position(|m| m.key == *key)
    }

    /// Real modifiers user modifier `i` keeps pressed while held: kept output modifiers, else `as`.
    fn user_held(&self, i: usize) -> Mods {
        if self.user_mods[i] == UserMod::Idle {
            Mods::NONE
        } else if self.kept[i] != Mods::NONE {
            self.kept[i]
        } else {
            self.cfg.modifiers[i].emulate
        }
    }

    /// Real modifiers kept pressed by all held user modifiers and by the current mode (`hold`).
    pub(crate) fn emulated_mods(&self) -> Mods {
        (0..self.user_mods.len()).fold(self.cfg.modes[self.mode].hold, |a, i| a | self.user_held(i))
    }

    /// Physical real modifiers plus active user modifiers.
    fn current_mods(&self) -> Mods {
        let real = self.down.iter().filter_map(Key::real_mod).fold(Mods::NONE, |a, m| a | m);
        let user = self.user_mods.iter().enumerate().filter(|(_, s)| **s == UserMod::Active);
        user.fold(real, |a, (i, _)| a | Mods::user(i))
    }

    pub(crate) fn on_down(&mut self, key: Key, win: &WindowInfo, now: u64) -> Reaction {
        let instant = matches!(key, Key::Wheel(_) | Key::Gesture(..));
        let repeat = !instant && !self.down.insert(key.clone());
        let this_mod = self.user_mod_index(&key);
        if !repeat {
            for (i, s) in self.user_mods.iter_mut().enumerate() {
                if Some(i) != this_mod && matches!(s, UserMod::Pending(_)) {
                    *s = UserMod::Active;
                }
            }
        }
        if key.real_mod().is_some() {
            if !self.os_mods.contains(&key) {
                self.os_mods.push(key);
            }
            return Reaction::default();
        }
        if let Some(i) = this_mod {
            let mut out = vec![];
            if !repeat {
                self.user_mods[i] = UserMod::Pending(now);
            }
            let emulate = self.user_held(i);
            if !repeat {
                if emulate != Mods::NONE {
                    self.restore(&mut out); // presses its `as` modifiers
                }
            } else {
                // Its `as` modifiers were the last keys pressed, so they are what repeats.
                let held = self.os_mods.iter().filter(|k| k.real_mod().is_some_and(|m| emulate.contains(m)));
                out.extend(held.map(|k| Command::Key { key: k.clone(), down: true }));
            }
            return consumed(out);
        }
        if repeat {
            if self.passing.contains(&key) {
                return Reaction::default();
            }
            if let Some(a) = self.active.remove(&key) {
                let out = self.repeat(&a, win);
                self.active.insert(key, a);
                return consumed(out);
            }
            if self.swallowed.contains(&key) {
                return consumed(vec![]);
            }
        }
        if let Key::Mouse(b) = key
            && self.gesture_buttons.contains(&b)
            && !repeat
        {
            self.gesture = Some(Gesture { button: b, acc: (0, 0), dirs: vec![] });
            self.swallowed.insert(key);
            return consumed(vec![]);
        }
        self.match_key(key, win, now)
    }

    pub(crate) fn on_up(&mut self, key: Key, win: &WindowInfo, now: u64) -> Reaction {
        self.down.remove(&key);
        if let Some(m) = key.real_mod() {
            // Last physical key of a modifier lifted by `keep_mods`: end the replacement. The OS
            // already saw this modifier released, so the physical release is swallowed.
            if let Some((lifted, _)) = self.real_kept
                && lifted.contains(m)
                && !self.down.iter().any(|k| k.real_mod() == Some(m))
            {
                self.real_kept = None;
                self.os_mods.retain(|k| *k != key);
                let mut out = vec![];
                self.restore(&mut out);
                return consumed(out);
            }
            self.os_mods.retain(|k| *k != key);
            // A held `as` modifier still wants it: press it again after this release passes.
            let mut out = vec![];
            if self.emulated_mods().contains(m) {
                self.restore(&mut out);
            }
            return Reaction { consume: false, commands: out };
        }
        if let Some(i) = self.user_mod_index(&key) {
            let m = &self.cfg.modifiers[i];
            let tap = match self.user_mods[i] {
                UserMod::Pending(t) if m.tap_timeout_ms == 0 || now.saturating_sub(t) <= m.tap_timeout_ms as u64 => {
                    m.tap.clone()
                }
                _ => KeySeq::default(),
            };
            let held = self.user_held(i);
            self.user_mods[i] = UserMod::Idle;
            self.kept[i] = Mods::NONE;
            let mut out = vec![];
            if !tap.0.is_empty() || held != Mods::NONE {
                self.tap_seq(&tap.0, &mut out); // also releases its `as` modifiers
            }
            return consumed(out);
        }
        if let Some(g) = self.gesture.take_if(|g| key == Key::Mouse(g.button)) {
            self.swallowed.remove(&key);
            return consumed(self.finish_gesture(g, win, now));
        }
        if let Some(a) = self.active.remove(&key) {
            self.swallowed.remove(&key);
            return consumed(self.release(a));
        }
        self.passing.remove(&key);
        Reaction { consume: self.swallowed.remove(&key), commands: vec![] }
    }

    pub(crate) fn track_gesture(&mut self, dx: i32, dy: i32) {
        let threshold = self.cfg.settings.gesture_threshold as i32;
        let Some(g) = &mut self.gesture else { return };
        g.acc.0 += dx;
        g.acc.1 += dy;
        let (x, y) = g.acc;
        if x.abs().max(y.abs()) < threshold {
            return;
        }
        let dir = match (x.abs() >= y.abs(), x > 0, y > 0) {
            (true, true, _) => Dir::R,
            (true, false, _) => Dir::L,
            (false, _, true) => Dir::D,
            (false, _, false) => Dir::U,
        };
        if g.dirs.last() != Some(&dir) {
            g.dirs.push(dir);
        }
        g.acc = (0, 0);
    }

    fn finish_gesture(&mut self, g: Gesture, win: &WindowInfo, now: u64) -> Vec<Command> {
        if !g.dirs.is_empty() {
            return self.match_key(Key::Gesture(g.button, g.dirs), win, now).commands;
        }
        // No movement: behave as a plain click on the button.
        let key = Key::Mouse(g.button);
        let down = Command::Key { key: key.clone(), down: true };
        let r = self.match_key(key.clone(), win, now);
        let mut out = r.commands;
        if !r.consume {
            out.push(down.clone());
        }
        if let Some(a) = self.active.remove(&key) {
            out.extend(self.release(a));
        } else if out.last() == Some(&down) {
            out.push(Command::Key { key: key.clone(), down: false });
        }
        self.swallowed.remove(&key);
        out
    }

    /// Rules usable right now, in declaration order.
    fn candidates<'a>(&'a self, win: &'a WindowInfo) -> impl Iterator<Item = usize> + 'a {
        let cfg = &self.cfg;
        (0..cfg.rules.len()).filter(move |&i| {
            let r = &cfg.rules[i];
            (r.modes.is_empty() || r.modes.contains(&self.mode)) && any_matches(&cfg.targets, &r.targets, win)
        })
    }

    fn match_key(&mut self, key: Key, win: &WindowInfo, now: u64) -> Reaction {
        let expired = self.pending.as_ref().is_some_and(|p| p.deadline.is_some_and(|d| d <= now));
        if !expired {
            return self.match_fresh(key, win, now);
        }
        let mut out = self.mismatch(None);
        let mut r = self.match_fresh(key.clone(), win, now);
        let sent_input =
            out.iter().any(|c| matches!(c, Command::Key { .. } | Command::Text(_) | Command::MouseMove { .. }));
        if sent_input && !r.consume {
            // Injected output must not be overtaken by the original event, so inject it too.
            self.inject_pass(key, &mut r.commands);
            r.consume = true;
        }
        out.append(&mut r.commands);
        Reaction { consume: r.consume, commands: out }
    }

    /// Injects `key` down and passes it through until released.
    pub(crate) fn inject_pass(&mut self, key: Key, out: &mut Vec<Command>) {
        if !matches!(key, Key::Gesture(..)) {
            out.push(Command::Key { key: key.clone(), down: true });
        }
        if !matches!(key, Key::Wheel(_) | Key::Gesture(..)) {
            self.active.insert(key.clone(), Active::Pass(key));
        }
    }

    /// In a `count` mode, a plain digit before any chord adds to the count (`0` only continues one).
    fn take_digit(&mut self, key: &Key) -> Option<Reaction> {
        let Key::Vk(vk @ 0x30..=0x39) = *key else { return None };
        let digit = (vk - 0x30) as u32;
        let wanted = self.cfg.modes[self.mode].count
            && self.pending.is_none()
            && self.current_mods() == Mods::NONE
            && (digit > 0 || self.count.is_some());
        if !wanted {
            return None;
        }
        // ponytail: capped so a stray long number cannot flood the target app.
        let n = (self.count.unwrap_or(0) * 10 + digit).min(MAX_COUNT);
        self.count = Some(n);
        self.swallowed.insert(key.clone());
        Some(consumed(vec![Command::Notice(Notice::Count(n))]))
    }

    fn match_fresh(&mut self, key: Key, win: &WindowInfo, now: u64) -> Reaction {
        if let Some(r) = self.take_digit(&key) {
            return r;
        }
        let chord = Chord { mods: self.current_mods(), key: key.clone() };
        let mut seq = self.pending.as_ref().map(|p| p.chords.clone()).unwrap_or_default();
        seq.push(chord);
        let rules = &self.cfg.rules;
        let exact = self.candidates(win).find(|&i| rules[i].keys.0 == seq);
        let prefix =
            self.candidates(win).find(|&i| rules[i].keys.0.len() > seq.len() && rules[i].keys.0.starts_with(&seq));
        let instant = matches!(key, Key::Wheel(_) | Key::Gesture(..));
        if let Some(i) = exact {
            // A one-chord binding whose action has nothing for this window: leave the input alone.
            let rule = &self.cfg.rules[i];
            let has_impl =
                self.cfg.actions[rule.action].impls.iter().any(|m| any_matches(&self.cfg.targets, &m.when, win));
            if seq.len() == 1 && rule.fallback.is_none() && !has_impl {
                if !instant {
                    self.passing.insert(key);
                }
                return Reaction::default();
            }
            self.pending = None;
            let out = self.fire(i, &seq, key.clone(), win);
            if !instant {
                self.swallowed.insert(key);
            }
            return consumed(out);
        }
        if prefix.is_some() {
            let policy = self.cfg.policy(&seq, self.mode);
            let deadline = (policy.timeout_ms > 0).then(|| now + policy.timeout_ms as u64);
            self.pending = Some(Pending { chords: seq, policy, deadline });
            if !instant {
                self.swallowed.insert(key);
            }
            return consumed(vec![]);
        }
        self.count = None; // a count ends with the binding it is for
        if self.pending.is_some() {
            return consumed(self.mismatch(Some(key)));
        }
        let mode = &self.cfg.modes[self.mode];
        let block = mode.block_unmapped && matches!(key, Key::Vk(_) | Key::Sc(_));
        let mut commands = vec![];
        if let Some(to) = mode.unmapped_to {
            let held = mode.hold;
            self.mode = to;
            commands.push(Command::ModeChanged(self.cfg.modes[to].name.clone()));
            if held != self.cfg.modes[to].hold && !block {
                // Release the mode's held modifiers first, then send the key itself (passing it
                // through would let it reach the OS before the injected release).
                self.restore(&mut commands);
                self.inject_pass(key, &mut commands);
                return consumed(commands);
            }
        }
        if block {
            self.swallowed.insert(key);
        }
        Reaction { consume: block, commands }
    }

    /// `C-x q` is undefined / `C-x` timed out.
    fn undefined(&self, pending: &[Chord], current: Option<&Key>) -> Command {
        let names = self.cfg.user_mod_names();
        let mut keys = grapnel_keys::format_seq(&KeySeq(pending.to_vec()), &names);
        match current {
            Some(k) => {
                let chord = Chord { mods: self.current_mods(), key: k.clone() };
                keys = format!("{keys} {}", grapnel_keys::format_chord(&chord, &names));
                Command::Notice(Notice::Undefined(keys))
            }
            None => Command::Notice(Notice::TimedOut(keys)),
        }
    }

    /// Resolves pending chords that cannot complete. `current` is the key that broke the sequence.
    pub(crate) fn mismatch(&mut self, current: Option<Key>) -> Vec<Command> {
        let p = self.pending.take().expect("mismatch without pending chords");
        self.count = None;
        let policy = p.policy;
        let mut out = vec![];
        if policy.on_mismatch != Mismatch::Replay {
            out.push(self.undefined(&p.chords, current.as_ref()));
        }
        match (policy.on_mismatch, policy.fallback.clone()) {
            (Mismatch::Discard, _) => {}
            (Mismatch::Fallback, Some(f)) => self.tap_seq(&f.0, &mut out),
            (Mismatch::Replay, _) | (Mismatch::Fallback, None) => {
                self.tap_seq(&p.chords, &mut out);
                if let Some(k) = current.clone().filter(|_| policy.on_mismatch == Mismatch::Replay) {
                    self.inject_pass(k, &mut out);
                    return out;
                }
            }
        }
        if let Some(k) = current
            && !matches!(k, Key::Wheel(_) | Key::Gesture(..))
        {
            self.swallowed.insert(k);
        }
        out
    }
}
