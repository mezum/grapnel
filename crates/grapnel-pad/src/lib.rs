//! Gamepad input via gilrs. Sticks become direction buttons with hysteresis; all pads are merged.

use gilrs::{Axis, Button, EventType, Gilrs};
use grapnel_keys::PadButton;
use std::collections::{HashMap, HashSet};

const PRESS: f32 = 0.5;
const RELEASE: f32 = 0.4;

fn button(b: Button) -> Option<PadButton> {
    Some(match b {
        Button::South => PadButton::A,
        Button::East => PadButton::B,
        Button::West => PadButton::X,
        Button::North => PadButton::Y,
        Button::LeftTrigger => PadButton::LB,
        Button::RightTrigger => PadButton::RB,
        Button::LeftTrigger2 => PadButton::LT,
        Button::RightTrigger2 => PadButton::RT,
        Button::Select => PadButton::Back,
        Button::Start => PadButton::Start,
        Button::LeftThumb => PadButton::LS,
        Button::RightThumb => PadButton::RS,
        Button::DPadUp => PadButton::Up,
        Button::DPadDown => PadButton::Down,
        Button::DPadLeft => PadButton::Left,
        Button::DPadRight => PadButton::Right,
        _ => return None,
    })
}

/// Turns stick axes into virtual direction buttons.
#[derive(Default)]
pub struct Sticks {
    pressed: Vec<PadButton>,
}

impl Sticks {
    /// Returns `(button, down)` transitions for a new axis value (gilrs: +Y is up).
    pub fn axis(&mut self, axis: Axis, value: f32) -> Vec<(PadButton, bool)> {
        use PadButton::*;
        let (neg, pos) = match axis {
            Axis::LeftStickX => (LStickLeft, LStickRight),
            Axis::LeftStickY => (LStickDown, LStickUp),
            Axis::RightStickX => (RStickLeft, RStickRight),
            Axis::RightStickY => (RStickDown, RStickUp),
            _ => return vec![],
        };
        let mut out = vec![];
        for (b, v) in [(neg, -value), (pos, value)] {
            let held = self.pressed.contains(&b);
            if !held && v >= PRESS {
                self.pressed.push(b);
                out.push((b, true));
            } else if held && v < RELEASE {
                self.pressed.retain(|p| *p != b);
                out.push((b, false));
            }
        }
        out
    }
}

/// Merges several pads: a button is down while any pad holds it.
#[derive(Default)]
pub struct Pads {
    held: HashSet<(usize, PadButton)>,
    sticks: HashMap<usize, Sticks>,
}

impl Pads {
    fn holders(&self, b: PadButton) -> usize {
        self.held.iter().filter(|(_, x)| *x == b).count()
    }

    pub fn button(&mut self, pad: usize, b: PadButton, down: bool) -> Option<(PadButton, bool)> {
        let changed = if down { self.held.insert((pad, b)) } else { self.held.remove(&(pad, b)) };
        let edge = if down { self.holders(b) == 1 } else { self.holders(b) == 0 };
        (changed && edge).then_some((b, down))
    }

    pub fn axis(&mut self, pad: usize, axis: Axis, value: f32) -> Vec<(PadButton, bool)> {
        let moves = self.sticks.entry(pad).or_default().axis(axis, value);
        moves.into_iter().filter_map(|(b, d)| self.button(pad, b, d)).collect()
    }

    /// Releases everything a disconnected pad was holding.
    pub fn disconnect(&mut self, pad: usize) -> Vec<(PadButton, bool)> {
        self.sticks.remove(&pad);
        let mine: Vec<PadButton> = self.held.iter().filter(|(p, _)| *p == pad).map(|(_, b)| *b).collect();
        mine.into_iter().filter_map(|b| self.button(pad, b, false)).collect()
    }
}

/// Reads all gamepads on a background thread and calls `on(button, down)` for each transition.
pub fn spawn(on: impl Fn(PadButton, bool) + Send + 'static) {
    std::thread::spawn(move || {
        let mut gilrs = match Gilrs::new() {
            Ok(g) => g,
            Err(e) => return log::error!("gamepad input unavailable: {e}"),
        };
        let mut pads = Pads::default();
        loop {
            let Some(ev) = gilrs.next_event_blocking(None) else { continue };
            let pad: usize = ev.id.into();
            let out = match ev.event {
                EventType::ButtonPressed(b, _) => {
                    button(b).and_then(|b| pads.button(pad, b, true)).into_iter().collect()
                }
                EventType::ButtonReleased(b, _) => {
                    button(b).and_then(|b| pads.button(pad, b, false)).into_iter().collect()
                }
                EventType::AxisChanged(a, v, _) => pads.axis(pad, a, v),
                EventType::Disconnected => pads.disconnect(pad),
                _ => vec![],
            };
            out.into_iter().for_each(|(b, d)| on(b, d));
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stick_hysteresis() {
        let mut s = Sticks::default();
        assert_eq!(s.axis(Axis::LeftStickY, 0.45), vec![]);
        assert_eq!(s.axis(Axis::LeftStickY, 0.6), vec![(PadButton::LStickUp, true)]);
        assert_eq!(s.axis(Axis::LeftStickY, 0.45), vec![]);
        assert_eq!(s.axis(Axis::LeftStickY, -0.7), vec![(PadButton::LStickDown, true), (PadButton::LStickUp, false)]);
        assert_eq!(s.axis(Axis::RightStickX, 0.9), vec![(PadButton::RStickRight, true)]);
        assert_eq!(s.axis(Axis::LeftZ, 1.0), vec![]);
    }

    #[test]
    fn pads_merge_and_disconnect() {
        let mut p = Pads::default();
        assert_eq!(p.button(0, PadButton::A, true), Some((PadButton::A, true)));
        assert_eq!(p.button(1, PadButton::A, true), None);
        assert_eq!(p.button(0, PadButton::A, false), None);
        assert_eq!(p.axis(0, Axis::LeftStickX, 0.9), vec![(PadButton::LStickRight, true)]);
        assert_eq!(p.axis(1, Axis::LeftStickX, 0.0), vec![]);
        let mut released = p.disconnect(1);
        released.extend(p.disconnect(0));
        released.sort_by_key(|(b, _)| *b as u8);
        assert_eq!(released, vec![(PadButton::A, false), (PadButton::LStickRight, false)]);
    }
}
