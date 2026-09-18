//! Debug builds log to stdout. Release builds append to `%LOCALAPPDATA%\grapnel\grapnel.log`
//! and show errors as tray balloons.

use log::{Level, LevelFilter, Metadata, Record};
use std::io::Write;
use std::sync::Mutex;

struct Logger {
    file: Option<Mutex<std::fs::File>>,
    on_error: fn(String),
}

impl log::Log for Logger {
    fn enabled(&self, m: &Metadata) -> bool {
        m.target().starts_with("grapnel") && m.level() <= log::max_level()
    }

    fn log(&self, r: &Record) {
        if !self.enabled(r.metadata()) {
            return;
        }
        let line = format!("[{} {}] {}", r.level(), r.target(), r.args());
        match &self.file {
            Some(f) => {
                let _ = writeln!(f.lock().unwrap(), "{line}");
            }
            None => println!("{line}"),
        }
        if r.level() == Level::Error && self.file.is_some() {
            (self.on_error)(r.args().to_string());
        }
    }

    fn flush(&self) {}
}

pub fn init(on_error: fn(String)) {
    let file = if cfg!(debug_assertions) {
        None
    } else {
        let dir = std::env::var_os("LOCALAPPDATA").map(std::path::PathBuf::from).unwrap_or_default().join("grapnel");
        let _ = std::fs::create_dir_all(&dir);
        std::fs::OpenOptions::new().create(true).append(true).open(dir.join("grapnel.log")).ok().map(Mutex::new)
    };
    let level = if cfg!(debug_assertions) { LevelFilter::Debug } else { LevelFilter::Info };
    log::set_max_level(level);
    let _ = log::set_logger(Box::leak(Box::new(Logger { file, on_error })));
}
