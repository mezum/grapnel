//! Problems found while loading or compiling, with where they are.

use grapnel_keys::Msg;
use std::path::PathBuf;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Problem {
    pub file: Option<PathBuf>,
    /// Location inside the file, such as `settings.passthrough[1]` or `keymap."C-x"`; empty for
    /// the whole file.
    pub at: String,
    pub msg: Msg,
}

impl Problem {
    /// `file: at: message`, the message in `locale`.
    pub fn text(&self, locale: &str) -> String {
        let file = self.file.as_ref().map(|f| f.display().to_string());
        let at = (!self.at.is_empty()).then(|| self.at.clone());
        file.into_iter().chain(at).chain([self.msg.text(locale)]).collect::<Vec<_>>().join(": ")
    }
}

/// English, for logs.
impl std::fmt::Display for Problem {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        f.write_str(&self.text("en"))
    }
}
