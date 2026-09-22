//! Executing engine commands, in order. Input before the first `Sleep` or `SetText` is sent
//! synchronously (inside the hook, so it reaches the OS before later physical input); the rest waits
//! on its own thread (UI Automation must not run inside the hook either) and is dropped when
//! [`Executor::cancel`] is called. The steps after a `Sleep` or `SetText` come back to the main
//! thread as `Command::Resume`, to be planned against the modifier state after the sleep. Programs
//! start on their own thread so the hook thread never blocks; UI commands go to the main thread's
//! queue.

use crate::{Msg, post};
use grapnel_engine::Command;
use grapnel_win::send;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

/// Starts `program` on its own thread. Every caller runs on the thread that owns the input hooks, and
/// `CreateProcess` takes long enough to be felt as stuttering input.
pub fn run(program: std::path::PathBuf, args: Vec<String>) {
    std::thread::spawn(move || {
        if let Err(e) = std::process::Command::new(&program).args(&args).spawn() {
            report!("error.run", program = program.display(), error = e);
        }
    });
}

/// Writes nothing, and reports nothing, once `live` turns false (suspend, reload, ...). Whether the
/// text was written: the steps after it rely on it (Enter in VS Code's palette would run another
/// command), so they are dropped when it was not.
fn set_text(text: &str, into: Option<&str>, live: impl Fn() -> bool + Copy) -> bool {
    let start = std::time::Instant::now();
    // The pattern was checked when the config was compiled.
    let into = into.and_then(|m| grapnel_config::Matcher::parse(m).ok());
    match grapnel_win::uia::set_text(text, into.as_ref(), live) {
        Ok(true) => {
            log::debug!("set_text took {} ms", start.elapsed().as_millis());
            return true;
        }
        Ok(false) if live() => report!("error.no_input"),
        Ok(false) => {}
        Err(e) => report!("error.set_text", error = e),
    }
    false
}

fn is_input(c: &Command) -> bool {
    matches!(c, Command::Key { .. } | Command::Text(_) | Command::MouseMove { .. })
}

/// Runs `cmds` up to the first `Sleep` or `SetText`; returns the rest. `generation` goes with `Resume`.
fn run_until_sleep(cmds: Vec<Command>, generation: u64) -> Vec<Command> {
    let mut batch = Vec::new();
    let mut it = cmds.into_iter();
    while let Some(c) = it.next() {
        match c {
            Command::Sleep(_) | Command::SetText { .. } => {
                send::send(&batch);
                return std::iter::once(c).chain(it).collect();
            }
            c if is_input(&c) => batch.push(c),
            Command::Resume(later) => {
                send::send(&std::mem::take(&mut batch));
                post(Msg::Resume(generation, later));
            }
            Command::Run { program, args } => {
                send::send(&std::mem::take(&mut batch));
                run(program.into(), args);
            }
            c => {
                send::send(&std::mem::take(&mut batch));
                post(Msg::Deferred(c));
            }
        }
    }
    send::send(&batch);
    Vec::new()
}

pub struct Executor {
    generation: Arc<AtomicU64>,
}

impl Executor {
    pub fn new() -> Executor {
        Executor { generation: Arc::new(AtomicU64::new(0)) }
    }

    /// Output after a `Sleep` or `SetText` waits on its own thread, so overlapping actions each
    /// resume on time. The engine puts at most one of them in a batch: what follows it comes back
    /// as `Resume`.
    pub fn run(&self, cmds: Vec<Command>) {
        let generation = self.generation.load(Ordering::SeqCst);
        let rest = run_until_sleep(cmds, generation);
        let Some((first, rest)) = rest.split_first() else { return };
        let (first, rest, current) = (first.clone(), rest.to_vec(), self.generation.clone());
        std::thread::spawn(move || {
            let go_on = match first {
                Command::Sleep(ms) => {
                    std::thread::sleep(std::time::Duration::from_millis(ms as u64));
                    true
                }
                Command::SetText { text, into } => {
                    set_text(&text, into.as_deref(), || current.load(Ordering::SeqCst) == generation)
                }
                _ => unreachable!("run_until_sleep stops only at Sleep or SetText"),
            };
            if go_on && current.load(Ordering::SeqCst) == generation {
                run_until_sleep(rest, generation);
            }
        });
    }

    /// Whether nothing was cancelled since output of `generation` was planned.
    pub fn is_current(&self, generation: u64) -> bool {
        self.generation.load(Ordering::SeqCst) == generation
    }

    /// Drops delayed output that has not run yet (suspend, passthrough, reload, exit).
    pub fn cancel(&self) {
        self.generation.fetch_add(1, Ordering::SeqCst);
    }
}
