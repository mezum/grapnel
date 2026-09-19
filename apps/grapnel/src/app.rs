//! Main-thread state: engine, hooks, foreground window, suspend/passthrough and reload.

use crate::exec::Executor;
use grapnel_config::{Config, ControlCmd, Field, Problem, Then, WindowInfo, any_matches};
use grapnel_engine::{Command, Engine, Event, Fault};
use grapnel_keys::{Key, Mods};
use grapnel_win::{hook, inputbox, uia::Uia, window};
use rust_i18n::t;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Instant;
use windows::Win32::Foundation::HWND;
use windows::Win32::UI::Input::KeyboardAndMouse::*;
use windows::Win32::UI::WindowsAndMessaging::{KillTimer, PostQuitMessage, SetTimer};

pub static PAD_ENABLED: AtomicBool = AtomicBool::new(false);
const TIMER_ID: usize = 1;
const HOTKEY_ID: i32 = 1;
const MODIFIER_VKS: [u8; 8] = [0xA0, 0xA1, 0xA2, 0xA3, 0xA4, 0xA5, 0x5B, 0x5C];

fn is_down(vk: u8) -> bool {
    unsafe { GetAsyncKeyState(vk as i32) as u16 & 0x8000 != 0 }
}

/// Modifier keys the OS currently sees as held.
fn held_modifiers() -> Vec<Key> {
    MODIFIER_VKS.iter().filter(|&&vk| is_down(vk)).map(|&vk| Key::Vk(vk)).collect()
}

/// C/M/S/W currently held, via the generic VK codes.
fn held_mods() -> Mods {
    [(0x11, Mods::CTRL), (0x12, Mods::ALT), (0x10, Mods::SHIFT)]
        .into_iter()
        .chain([(0x5B, Mods::WIN), (0x5C, Mods::WIN)])
        .filter(|&(vk, _)| is_down(vk))
        .fold(Mods::NONE, |a, (_, m)| a | m)
}

pub fn load(path: &Path) -> Result<Config, Vec<Problem>> {
    grapnel_config::load(path).and_then(|files| grapnel_config::compile(&files))
}

pub struct App {
    pub hwnd: HWND,
    pub config_path: PathBuf,
    cfg: Arc<Config>,
    engine: Engine,
    win: WindowInfo,
    hooks: Option<hook::Hooks>,
    suspended: bool,
    passthrough: bool,
    uia: Option<Uia>,
    exec: Executor,
    start: Instant,
    /// Window the open input box was started from; its follow-up action runs against this.
    prompt_win: WindowInfo,
    /// Action to run with the input box text; `None` runs the action the text names.
    prompt_then: Then,
}

impl App {
    pub fn new(hwnd: HWND, config_path: PathBuf, cfg: Config) -> App {
        let mut app = App {
            hwnd,
            config_path,
            engine: Engine::new(Arc::new(cfg.clone())),
            cfg: Arc::new(cfg),
            win: WindowInfo::default(),
            hooks: None,
            suspended: false,
            passthrough: false,
            uia: None,
            exec: Executor::new(),
            start: Instant::now(),
            prompt_win: WindowInfo::default(),
            prompt_then: Then::Named("{arg}".into()),
        };
        app.configure();
        app.window_changed(window::CHANGED_FOREGROUND);
        app
    }

    fn now(&self) -> u64 {
        self.start.elapsed().as_millis() as u64
    }

    /// Applies config-dependent platform settings (scan codes, hotkey, UIA).
    fn configure(&mut self) {
        hook::set_scancodes(self.cfg.scancodes());
        let uses_uia = self
            .cfg
            .targets
            .iter()
            .flat_map(|t| &t.fields)
            .any(|(f, _)| matches!(f, Field::UiaId | Field::UiaName | Field::UiaType));
        if uses_uia && self.uia.is_none() {
            self.uia = Some(Uia::spawn(self.hwnd, crate::WM_UIA));
        }
        unsafe {
            let _ = UnregisterHotKey(Some(self.hwnd), HOTKEY_ID);
        }
        if let Some(h) = &self.cfg.settings.suspend_hotkey {
            let m = h.mods;
            let flags = [(grapnel_keys::Mods::CTRL, MOD_CONTROL), (grapnel_keys::Mods::ALT, MOD_ALT)]
                .into_iter()
                .chain([(grapnel_keys::Mods::SHIFT, MOD_SHIFT), (grapnel_keys::Mods::WIN, MOD_WIN)])
                .filter(|(f, _)| m.contains(*f))
                .fold(MOD_NOREPEAT, |a, (_, b)| a | b);
            let Key::Vk(vk) = h.key else { return };
            if let Err(e) = unsafe { RegisterHotKey(Some(self.hwnd), HOTKEY_ID, flags, vk as u32) } {
                report!("error.hotkey", error = e);
            }
        }
    }

    pub fn tooltip(&self) -> String {
        let state = if self.suspended {
            t!("tray.suspended")
        } else if self.passthrough {
            t!("tray.passthrough")
        } else {
            "".into()
        };
        format!("grapnel - {}{state}", self.engine.mode_name())
    }

    pub fn is_suspended(&self) -> bool {
        self.suspended
    }

    /// Hook callback. Returns true to swallow the event.
    pub fn on_input(&mut self, ev: Event) -> bool {
        let key = match &ev {
            Event::Down(k) | Event::Up(k) => Some(k),
            Event::MouseMove { .. } => None,
        };
        // Typing into our own prompt is not remapped, but modifier state is still tracked.
        if inputbox::is_foreground() && key.is_none_or(|k| k.real_mod().is_none()) {
            return false;
        }
        // Leave the suspend hotkey to RegisterHotKey even if a rule or mode would swallow it.
        if let (Event::Down(k), Some(h)) = (&ev, &self.cfg.settings.suspend_hotkey)
            && *k == h.key
            && held_mods() == h.mods
        {
            return false;
        }
        let now = self.now();
        let r = self.engine.handle(ev, &self.win, now);
        self.exec.run(r.commands);
        self.arm_timer();
        r.consume
    }

    pub fn on_pad(&mut self, ev: Event) {
        if self.hooks.is_some() {
            self.on_input(ev);
        }
    }

    pub fn on_timer(&mut self) {
        let now = self.now();
        let cmds = self.engine.tick(now);
        self.exec.run(cmds);
        self.arm_timer();
    }

    fn arm_timer(&self) {
        unsafe {
            match self.engine.next_deadline() {
                Some(d) => SetTimer(Some(self.hwnd), TIMER_ID, d.saturating_sub(self.now()).max(1) as u32, None),
                None => KillTimer(Some(self.hwnd), TIMER_ID).map(|_| 0).unwrap_or(0),
            };
        }
    }

    pub fn window_changed(&mut self, kind: usize) {
        let uia = (self.win.uia_id.clone(), self.win.uia_name.clone(), self.win.uia_type.clone());
        self.win = window::foreground_info();
        (self.win.uia_id, self.win.uia_name, self.win.uia_type) = uia;
        match &self.uia {
            // Passthrough is re-evaluated when the fresh UIA result arrives.
            Some(u) if kind != window::CHANGED_TITLE => u.refresh(),
            _ => self.update_hooks(),
        }
    }

    pub fn uia_changed(&mut self) {
        if let Some(u) = &self.uia {
            let f = u.latest();
            (self.win.uia_id, self.win.uia_name, self.win.uia_type) = (f.id, f.name, f.control_type);
            self.update_hooks();
        }
    }

    /// Installs or removes hooks to match the suspend/passthrough state.
    fn update_hooks(&mut self) {
        self.passthrough = any_matches(&self.cfg.targets, &self.cfg.settings.passthrough, &self.win)
            && !self.cfg.settings.passthrough.is_empty();
        let want = !self.suspended && !self.passthrough;
        if want && self.hooks.is_none() {
            match hook::install() {
                Ok(h) => self.hooks = Some(h),
                Err(e) => report!("error.hook", error = e),
            }
            self.engine.sync_modifiers(&held_modifiers());
            log::debug!("hooks installed");
        } else if !want && self.hooks.is_some() {
            self.exec.cancel();
            let cmds = self.engine.reset();
            self.exec.run(cmds);
            self.hooks = None;
            log::debug!("hooks removed");
        }
        PAD_ENABLED.store(self.hooks.is_some(), Ordering::Relaxed);
        crate::set_tray_state(&self.tooltip(), self.suspended);
    }

    pub fn toggle_suspend(&mut self) {
        self.suspended = !self.suspended;
        self.update_hooks();
    }

    /// Loads the config on a worker thread (file I/O must not stall the hook thread).
    pub fn reload(&mut self) {
        let path = self.config_path.clone();
        std::thread::spawn(move || crate::post(crate::Msg::Reloaded(load(&path))));
    }

    pub fn apply_reload(&mut self, result: Result<Config, Vec<Problem>>) {
        match result {
            Ok(cfg) => {
                self.exec.cancel();
                let cmds = self.engine.reset();
                self.exec.run(cmds);
                self.cfg = Arc::new(cfg);
                self.engine = Engine::new(self.cfg.clone());
                self.engine.sync_modifiers(&held_modifiers());
                crate::set_language(self.cfg.settings.language.as_deref());
                self.configure();
                self.update_hooks();
                log::info!("config reloaded");
                crate::balloon(&t!("reload.done"), &self.config_path.display().to_string(), false);
            }
            Err(errors) => {
                errors.iter().for_each(|e| log::warn!("{e}"));
                let more = match errors.len() {
                    1 => String::new(),
                    n => format!("\n{}", t!("reload.more", count = n - 1)),
                };
                crate::balloon(&t!("reload.failed"), &format!("{}{more}", errors[0].text(&rust_i18n::locale())), true);
            }
        }
    }

    /// Called before the input box opens: release held output and remember where we came from.
    pub fn before_prompt(&mut self, then: Then) {
        let cmds = self.engine.reset();
        self.exec.run(cmds);
        self.prompt_win = self.win.clone();
        self.prompt_then = then;
    }

    /// Input box confirmed: run its follow-up action with the text.
    pub fn input_done(&mut self, text: &str) {
        let cmds = self.engine.invoke_input(&self.prompt_then, text, &self.prompt_win);
        self.exec.run(cmds);
    }

    /// Non-input commands, on the main thread outside the hook.
    pub fn deferred(&mut self, c: Command) {
        match c {
            Command::ModeChanged(mode) => {
                crate::set_tray_state(&self.tooltip(), self.suspended);
                crate::show_toast(&t!("notice.mode", mode = mode), 1500);
            }
            Command::Control(ControlCmd::Suspend) => self.toggle_suspend(),
            Command::Control(ControlCmd::Reload) => self.reload(),
            Command::Control(ControlCmd::Exit) => unsafe { PostQuitMessage(0) },
            Command::Error(Fault::UnknownAction(name)) => report!("error.unknown_action", name = name),
            Command::Error(Fault::TooDeep(name)) => report!("error.too_deep", name = name),
            other => self.exec.run(vec![other]),
        }
    }

    /// Runs the steps after a `Sleep`, unless cancelled meanwhile (suspend, passthrough, reload).
    pub fn resume(&mut self, generation: u64, later: grapnel_engine::Later) {
        if self.exec.is_current(generation) && self.hooks.is_some() {
            let cmds = self.engine.resume(later);
            self.exec.run(cmds);
        }
    }

    pub fn shutdown(&mut self) {
        self.exec.cancel();
        let cmds = self.engine.reset();
        grapnel_win::send::send(&cmds);
        self.hooks = None;
    }
}
