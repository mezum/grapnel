//! Executing engine commands, in order. Input before the first `Sleep` is sent synchronously (inside
//! the hook, so it reaches the OS before later physical input); the rest runs on a worker thread and
//! is dropped when [`Executor::cancel`] is called. Programs start on their own thread so the hook
//! thread never blocks; UI commands go to the main thread's queue.

use crate::{Msg, post};
use grapnel_engine::Command;
use grapnel_win::send;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{Sender, channel};

fn is_input(c: &Command) -> bool {
    matches!(c, Command::Key { .. } | Command::Text(_) | Command::MouseMove { .. })
}

/// Runs `cmds` up to the first `Sleep`; returns the rest.
fn run_until_sleep(cmds: Vec<Command>) -> Vec<Command> {
    let mut batch = Vec::new();
    let mut it = cmds.into_iter();
    while let Some(c) = it.next() {
        match c {
            Command::Sleep(_) => {
                send::send(&batch);
                return std::iter::once(c).chain(it).collect();
            }
            c if is_input(&c) => batch.push(c),
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
    worker: Sender<(u64, Vec<Command>)>,
}

impl Executor {
    pub fn new() -> Executor {
        let generation = Arc::new(AtomicU64::new(0));
        let current = generation.clone();
        let (worker, rx) = channel::<(u64, Vec<Command>)>();
        std::thread::spawn(move || {
            for (gen_, mut cmds) in rx {
                // ponytail: modifier state after a Sleep was planned before it; re-plan per step if it matters.
                while let Some(Command::Sleep(ms)) = cmds.first().cloned() {
                    std::thread::sleep(std::time::Duration::from_millis(ms as u64));
                    if current.load(Ordering::SeqCst) != gen_ {
                        break;
                    }
                    cmds = run_until_sleep(cmds.split_off(1));
                }
            }
        });
        Executor { generation, worker }
    }

    pub fn run(&self, cmds: Vec<Command>) {
        let rest = run_until_sleep(cmds);
        if !rest.is_empty() {
            let _ = self.worker.send((self.generation.load(Ordering::SeqCst), rest));
        }
    }

    /// Drops delayed output that has not run yet (suspend, passthrough, reload, exit).
    pub fn cancel(&self) {
        self.generation.fetch_add(1, Ordering::SeqCst);
    }
}
