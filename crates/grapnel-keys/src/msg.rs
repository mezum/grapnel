//! Messages shown to users: a key in `locales/*.toml` plus arguments, worded when shown, so the
//! same error can be logged in English and displayed in the user's language.

use rust_i18n::t;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Msg {
    pub key: &'static str,
    pub args: Vec<(&'static str, String)>,
}

/// `msg!("config.unknown_mode", name = n)` builds a [`Msg`].
#[macro_export]
macro_rules! msg {
    ($key:literal $(, $name:ident = $value:expr)* $(,)?) => {
        $crate::Msg { key: $key, args: vec![$((stringify!($name), $value.to_string())),*] }
    };
}

impl Msg {
    /// The message in `locale` (English when the locale lacks it).
    pub fn text(&self, locale: &str) -> String {
        let text = t!(self.key, locale = locale);
        let (names, values): (Vec<&str>, Vec<String>) = self.args.iter().map(|(n, v)| (*n, v.clone())).unzip();
        rust_i18n::replace_patterns(&text, &names, &values)
    }
}

/// English, for logs.
impl std::fmt::Display for Msg {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        f.write_str(&self.text("en"))
    }
}
