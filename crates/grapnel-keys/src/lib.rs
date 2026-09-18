//! Key, chord and key-sequence types and their text syntax (`C-x t 0`, `Mu-j`, `RButton:UL`).

mod names;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum MouseButton {
    Left,
    Right,
    Middle,
    X1,
    X2,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum WheelDir {
    Up,
    Down,
    Left,
    Right,
}

/// Gesture stroke direction.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Dir {
    U,
    D,
    L,
    R,
}

impl Dir {
    pub fn letter(self) -> char {
        match self {
            Dir::U => 'U',
            Dir::D => 'D',
            Dir::L => 'L',
            Dir::R => 'R',
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum PadButton {
    A,
    B,
    X,
    Y,
    LB,
    RB,
    LT,
    RT,
    Back,
    Start,
    LS,
    RS,
    Up,
    Down,
    Left,
    Right,
    LStickUp,
    LStickDown,
    LStickLeft,
    LStickRight,
    RStickUp,
    RStickDown,
    RStickLeft,
    RStickRight,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum Key {
    Vk(u8),
    /// Scan code; `0xE0xx` for extended keys.
    Sc(u16),
    Mouse(MouseButton),
    Wheel(WheelDir),
    Gesture(MouseButton, Vec<Dir>),
    Pad(PadButton),
}

impl Key {
    /// Which of C/M/S/W this key is, if it is a real modifier key.
    pub fn real_mod(&self) -> Option<Mods> {
        match self {
            Key::Vk(0xA2 | 0xA3 | 0x11) => Some(Mods::CTRL),
            Key::Vk(0xA4 | 0xA5 | 0x12) => Some(Mods::ALT),
            Key::Vk(0xA0 | 0xA1 | 0x10) => Some(Mods::SHIFT),
            Key::Vk(0x5B | 0x5C) => Some(Mods::WIN),
            _ => None,
        }
    }
}

/// Modifier bit set: C, M, S, W, then user-defined modifiers from bit 4.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct Mods(pub u32);

impl Mods {
    pub const NONE: Mods = Mods(0);
    pub const CTRL: Mods = Mods(1);
    pub const ALT: Mods = Mods(2);
    pub const SHIFT: Mods = Mods(4);
    pub const WIN: Mods = Mods(8);
    pub const REAL: Mods = Mods(15);
    pub const MAX_USER: usize = 28;

    pub fn user(index: usize) -> Mods {
        assert!(index < Self::MAX_USER);
        Mods(1 << (4 + index))
    }
    pub fn contains(self, other: Mods) -> bool {
        self.0 & other.0 == other.0
    }
    pub fn real(self) -> Mods {
        Mods(self.0 & Self::REAL.0)
    }
}

impl std::ops::BitOr for Mods {
    type Output = Mods;
    fn bitor(self, rhs: Mods) -> Mods {
        Mods(self.0 | rhs.0)
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Chord {
    pub mods: Mods,
    pub key: Key,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Hash)]
pub struct KeySeq(pub Vec<Chord>);

const REAL_NAMES: [(&str, Mods); 4] = [("C", Mods::CTRL), ("M", Mods::ALT), ("S", Mods::SHIFT), ("W", Mods::WIN)];

/// Parses one key name, including gestures (`RButton:UL`).
pub fn parse_key(s: &str) -> Result<Key, String> {
    if let Some((btn, dirs)) = s.split_once(':')
        && let Some(&(_, b)) = names::MOUSE.iter().find(|(n, _)| n.eq_ignore_ascii_case(btn))
    {
        let dirs = dirs
            .chars()
            .map(|c| match c.to_ascii_uppercase() {
                'U' => Ok(Dir::U),
                'D' => Ok(Dir::D),
                'L' => Ok(Dir::L),
                'R' => Ok(Dir::R),
                _ => Err(format!("invalid gesture direction '{c}' in '{s}'")),
            })
            .collect::<Result<Vec<_>, _>>()?;
        if dirs.is_empty() {
            return Err(format!("gesture '{s}' has no direction"));
        }
        return Ok(Key::Gesture(b, dirs));
    }
    names::lookup(s).ok_or_else(|| format!("unknown key '{s}'"))
}

/// Parses a chord such as `C-S-x` or `Mu-j`. `user` lists user modifier names by bit index.
pub fn parse_chord(s: &str, user: &[&str]) -> Result<Chord, String> {
    let mut mods = Mods::NONE;
    let mut rest = s;
    while let Some(i) = rest.find('-').filter(|&i| i > 0 && i + 1 < rest.len()) {
        let name = &rest[..i];
        let m = REAL_NAMES
            .iter()
            .find(|(n, _)| *n == name)
            .map(|&(_, m)| m)
            .or_else(|| user.iter().position(|u| *u == name).map(Mods::user));
        let Some(m) = m else { break };
        mods = mods | m;
        rest = &rest[i + 1..];
    }
    Ok(Chord { mods, key: parse_key(rest)? })
}

/// Parses a space-separated key sequence. An empty string yields an empty sequence.
pub fn parse_seq(s: &str, user: &[&str]) -> Result<KeySeq, String> {
    s.split_whitespace().map(|c| parse_chord(c, user)).collect::<Result<_, _>>().map(KeySeq)
}

pub fn format_key(key: &Key) -> String {
    names::name(key)
}

pub fn format_chord(chord: &Chord, user: &[&str]) -> String {
    let mut out = String::new();
    for (n, m) in REAL_NAMES {
        if chord.mods.contains(m) {
            out += n;
            out.push('-');
        }
    }
    for (i, n) in user.iter().enumerate() {
        if chord.mods.contains(Mods::user(i)) {
            out += n;
            out.push('-');
        }
    }
    out + &format_key(&chord.key)
}

pub fn format_seq(seq: &KeySeq, user: &[&str]) -> String {
    seq.0.iter().map(|c| format_chord(c, user)).collect::<Vec<_>>().join(" ")
}

#[cfg(test)]
mod tests;
