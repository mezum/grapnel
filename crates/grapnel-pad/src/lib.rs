//! Gamepad input via gilrs. Sticks become direction buttons with hysteresis; all pads are merged.

use gilrs::{Axis, Button, EventType, Gilrs};
use grapnel_keys::PadButton;

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

/// Reads all gamepads on a background thread and calls `on(button, down)` for each transition.
pub fn spawn(on: impl Fn(PadButton, bool) + Send + 'static) {
    std::thread::spawn(move || {
        let mut gilrs = match Gilrs::new() {
            Ok(g) => g,
            Err(e) => return log::error!("gamepad input unavailable: {e}"),
        };
        let mut sticks = Sticks::default();
        loop {
            let Some(ev) = gilrs.next_event_blocking(None) else { continue };
            match ev.event {
                EventType::ButtonPressed(b, _) => button(b).into_iter().for_each(|b| on(b, true)),
                EventType::ButtonReleased(b, _) => button(b).into_iter().for_each(|b| on(b, false)),
                EventType::AxisChanged(a, v, _) => sticks.axis(a, v).into_iter().for_each(|(b, d)| on(b, d)),
                _ => {}
            }
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
}
