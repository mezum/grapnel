//! Executing engine commands, in order. Input before the first `Sleep` is sent synchronously (inside
//! the hook, so it reaches the OS before later physical input); the rest waits on its own thread and
//! is dropped when [`Executor::cancel`] is called. The steps after a `Sleep` come back to the main
//! thread as `Command::Resume`, to be planned against the modifier state after the sleep. Programs
//! start on their own thread so the hook thread never blocks; UI commands go to the main thread's
//! queue.

use crate::{Msg, post};
use grapnel_engine::Command;
use grapnel_win::send;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

fn is_input(c: &Command) -> bool {
    matches!(c, Command::Key { .. } | Command::Text(_) | Command::MouseMove { .. })
}

/// Runs `cmds` up to the first `Sleep`; returns the rest. `generation` goes with `Resume`.
fn run_until_sleep(cmds: Vec<Command>, generation: u64) -> Vec<Command> {
    let mut batch = Vec::new();
    let mut it = cmds.into_iter();
    while let Some(c) = it.next() {
        match c {
            Command::Sleep(_) => {
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
                std::thread::spawn(move || {
                    if let Err(e) = std::process::Command::new(&program).args(&args).spawn() {
                        report!("error.run", program = program, error = e);
                    }
                });
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

    /// Output after a `Sleep` waits on its own thread, so overlapping actions each resume on time.
    /// The engine puts at most one `Sleep` in a batch: what follows it comes back as `Resume`.
    pub fn run(&self, cmds: Vec<Command>) {
        let generation = self.generation.load(Ordering::SeqCst);
        let rest = run_until_sleep(cmds, generation);
        let Some((Command::Sleep(ms), rest)) = rest.split_first() else { return };
        let (ms, rest, current) = (*ms, rest.to_vec(), self.generation.clone());
        std::thread::spawn(move || {
            std::thread::sleep(std::time::Duration::from_millis(ms as u64));
            if current.load(Ordering::SeqCst) == generation {
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
