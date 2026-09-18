//! Main-thread state: engine, hooks, foreground window, suspend/passthrough and reload.

use crate::exec::Executor;
use grapnel_config::{Config, ControlCmd, Field, WindowInfo, any_matches};
use grapnel_engine::{Command, Engine, Event};
use grapnel_keys::Key;
use grapnel_win::{hook, inputbox, uia::Uia, window};
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

pub fn load(path: &Path) -> Result<Config, Vec<String>> {
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
            exec: Executor::new(hwnd),
            start: Instant::now(),
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
                log::error!("ホットキーを登録できません: {e}");
            }
        }
    }

    pub fn tooltip(&self) -> String {
        let state = if self.suspended {
            " (一時停止中)"
        } else if self.passthrough {
            " (パススルー中)"
        } else {
            ""
        };
        format!("grapnel - {}{state}", self.engine.mode_name())
    }

    pub fn is_suspended(&self) -> bool {
        self.suspended
    }

    /// Hook callback. Returns true to swallow the event.
    pub fn on_input(&mut self, ev: Event) -> bool {
        if inputbox::is_foreground() {
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
        if kind == window::CHANGED_TITLE {
            (self.win.uia_id, self.win.uia_name, self.win.uia_type) = uia;
        } else if let Some(u) = &self.uia {
            u.refresh();
        }
        self.update_hooks();
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
                Err(e) => log::error!("フックを設定できません: {e}"),
            }
            let held: Vec<Key> = MODIFIER_VKS
                .iter()
                .filter(|&&vk| unsafe { GetAsyncKeyState(vk as i32) } as u16 & 0x8000 != 0)
                .map(|&vk| Key::Vk(vk))
                .collect();
            self.engine.sync_modifiers(&held);
            log::debug!("hooks installed");
        } else if !want && self.hooks.is_some() {
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

    pub fn reload(&mut self) {
        match load(&self.config_path) {
            Ok(cfg) => {
                let cmds = self.engine.reset();
                self.exec.run(cmds);
                self.cfg = Arc::new(cfg);
                self.engine = Engine::new(self.cfg.clone());
                self.configure();
                self.update_hooks();
                crate::balloon("設定を読み込みました", &self.config_path.display().to_string(), false);
            }
            Err(errors) => {
                errors.iter().for_each(|e| log::warn!("{e}"));
                let more = if errors.len() > 1 { format!("\n(他 {} 件)", errors.len() - 1) } else { String::new() };
                crate::balloon("設定の読み込みに失敗しました", &format!("{}{more}", errors[0]), true);
            }
        }
    }

    /// Input box confirmed: run its follow-up action with the text.
    pub fn input_done(&mut self, then: usize, text: &str) {
        let cmds = self.engine.invoke(then, text, &self.win);
        self.exec.run(cmds);
    }

    /// Non-input commands, on the main thread outside the hook.
    pub fn deferred(&mut self, c: Command) {
        match c {
            Command::Run { program, args } => {
                if let Err(e) = std::process::Command::new(&program).args(&args).spawn() {
                    log::error!("{program} を起動できません: {e}");
                }
            }
            Command::InputBox { prompt, then } => {
                if let Err(e) = inputbox::open(&prompt, then) {
                    log::error!("入力欄を表示できません: {e}");
                }
            }
            Command::ModeChanged(_) => crate::set_tray_state(&self.tooltip(), self.suspended),
            Command::Control(ControlCmd::Suspend) => self.toggle_suspend(),
            Command::Control(ControlCmd::Reload) => self.reload(),
            Command::Control(ControlCmd::Exit) => unsafe { PostQuitMessage(0) },
            Command::Error(e) => log::error!("{e}"),
            other => self.exec.run(vec![other]),
        }
    }

    pub fn shutdown(&mut self) {
        let cmds = self.engine.reset();
        grapnel_win::send::send(&cmds);
        self.hooks = None;
    }
}
