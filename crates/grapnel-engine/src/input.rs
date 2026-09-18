//! Input event handling: modifiers, repeats, gestures and rule matching.

use crate::*;

impl Engine {
    fn user_mod_index(&self, key: &Key) -> Option<usize> {
        self.cfg.modifiers.iter().position(|m| m.key == *key)
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
        if key.real_mod().is_some() {
            if !self.os_mods.contains(&key) {
                self.os_mods.push(key);
            }
            return Reaction::default();
        }
        if let Some(i) = self.user_mod_index(&key) {
            if !repeat {
                self.user_mods[i] = UserMod::Pending(now);
            }
            return consumed(vec![]);
        }
        for s in &mut self.user_mods {
            if matches!(s, UserMod::Pending(_)) {
                *s = UserMod::Active;
            }
        }
        if repeat {
            if let Some(a) = self.active.get(&key) {
                return consumed(a.repeat.clone());
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
        if key.real_mod().is_some() {
            self.os_mods.retain(|k| *k != key);
            return Reaction::default();
        }
        if let Some(i) = self.user_mod_index(&key) {
            let m = &self.cfg.modifiers[i];
            let mut out = vec![];
            if let UserMod::Pending(t) = self.user_mods[i]
                && (m.tap_timeout_ms == 0 || now.saturating_sub(t) <= m.tap_timeout_ms as u64)
            {
                let tap = m.tap.clone();
                self.tap_seq(&tap.0, &mut out);
            }
            self.user_mods[i] = UserMod::Idle;
            return consumed(out);
        }
        if let Some(g) = self.gesture.take_if(|g| key == Key::Mouse(g.button)) {
            self.swallowed.remove(&key);
            return consumed(self.finish_gesture(g, win, now));
        }
        if let Some(a) = self.active.remove(&key) {
            self.swallowed.remove(&key);
            let mut out = a.release;
            if a.restore {
                self.restore(&mut out);
            }
            return consumed(out);
        }
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
            out.extend(a.release);
            if a.restore {
                self.restore(&mut out);
            }
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
        let chord = Chord { mods: self.current_mods(), key: key.clone() };
        let mut seq = self.pending.as_ref().map(|p| p.chords.clone()).unwrap_or_default();
        seq.push(chord);
        let rules = &self.cfg.rules;
        let exact = self.candidates(win).find(|&i| rules[i].keys.0 == seq);
        let prefix =
            self.candidates(win).find(|&i| rules[i].keys.0.len() > seq.len() && rules[i].keys.0.starts_with(&seq));
        let instant = matches!(key, Key::Wheel(_) | Key::Gesture(..));
        if let Some(i) = exact {
            self.pending = None;
            let out = self.fire(i, &seq, key.clone(), win);
            if !instant {
                self.swallowed.insert(key);
            }
            return consumed(out);
        }
        if let Some(i) = prefix {
            let t = self.cfg.rules[i].timeout_ms;
            let deadline = (t > 0).then(|| now + t as u64);
            self.pending = Some(Pending { chords: seq, rule: i, deadline });
            if !instant {
                self.swallowed.insert(key);
            }
            return consumed(vec![]);
        }
        if self.pending.is_some() {
            return consumed(self.mismatch(Some(key)));
        }
        let block = self.cfg.modes[self.mode].block_unmapped && matches!(key, Key::Vk(_) | Key::Sc(_));
        if block {
            self.swallowed.insert(key);
        }
        Reaction { consume: block, commands: vec![] }
    }

    /// Resolves pending chords that cannot complete. `current` is the key that broke the sequence.
    pub(crate) fn mismatch(&mut self, current: Option<Key>) -> Vec<Command> {
        let p = self.pending.take().expect("mismatch without pending chords");
        let cfg = self.cfg.clone();
        let rule = &cfg.rules[p.rule];
        let mut out = vec![];
        match (rule.on_mismatch, rule.fallback.clone()) {
            (Mismatch::Discard, _) => {}
            (Mismatch::Fallback, Some(f)) => self.tap_seq(&f.0, &mut out),
            (Mismatch::Replay, _) | (Mismatch::Fallback, None) => {
                self.tap_seq(&p.chords, &mut out);
                let replay = rule.on_mismatch == Mismatch::Replay;
                if let Some(k) = current.clone().filter(|k| replay && !matches!(k, Key::Gesture(..))) {
                    out.push(Command::Key { key: k, down: true });
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
