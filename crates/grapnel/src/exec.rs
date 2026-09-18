//! Executing engine commands. Input before the first `Sleep` is sent synchronously (inside the
//! hook, so it reaches the OS before later physical input); the rest runs on a worker thread.
//! Non-input commands are posted back to the main window.

use crate::WM_DEFERRED;
use grapnel_engine::Command;
use grapnel_win::send;
use std::sync::mpsc::{Sender, channel};
use windows::Win32::Foundation::{HWND, LPARAM, WPARAM};
use windows::Win32::UI::WindowsAndMessaging::PostMessageW;

fn is_input(c: &Command) -> bool {
    matches!(c, Command::Key { .. } | Command::Text(_) | Command::MouseMove { .. })
}

/// Hands a non-input command to the main thread.
pub fn defer(hwnd: isize, c: Command) {
    let ptr = Box::into_raw(Box::new(c)) as isize;
    if unsafe { PostMessageW(Some(HWND(hwnd as _)), WM_DEFERRED, WPARAM(0), LPARAM(ptr)) }.is_err() {
        drop(unsafe { Box::from_raw(ptr as *mut Command) });
    }
}

/// Runs `cmds` in order, never blocking the caller.
pub struct Executor {
    hwnd: isize,
    worker: Sender<Vec<Command>>,
}

impl Executor {
    pub fn new(hwnd: HWND) -> Executor {
        let (worker, rx) = channel::<Vec<Command>>();
        let hwnd = hwnd.0 as isize;
        std::thread::spawn(move || {
            for cmds in rx {
                for c in cmds {
                    match c {
                        Command::Sleep(ms) => std::thread::sleep(std::time::Duration::from_millis(ms as u64)),
                        c if is_input(&c) => send::send(&[c]),
                        c => defer(hwnd, c),
                    }
                }
            }
        });
        Executor { hwnd, worker }
    }

    // ponytail: output after a Sleep can interleave with later synchronous output; queue everything
    // through the worker if that ever matters.
    pub fn run(&self, mut cmds: Vec<Command>) {
        let later = cmds.iter().position(|c| matches!(c, Command::Sleep(_))).map(|i| cmds.split_off(i));
        let (input, other): (Vec<_>, Vec<_>) = cmds.into_iter().partition(is_input);
        send::send(&input);
        other.into_iter().for_each(|c| defer(self.hwnd, c));
        if let Some(later) = later {
            let _ = self.worker.send(later);
        }
    }
}
