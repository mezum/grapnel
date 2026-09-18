//! grapnel: tray-resident input remapper.
//! Usage: `grapnel [--config <path>]` or `grapnel reload|suspend|exit` to control a running instance.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

rust_i18n::i18n!("../../locales", fallback = "en");

/// Logs an error in English and shows it on screen in the display language.
macro_rules! report {
    ($key:literal $(, $name:ident = $value:expr)* $(,)?) => {{
        $(let $name = &$value;)* // evaluate each argument once
        log::error!("{}", rust_i18n::t!($key, locale = "en" $(, $name = $name)*));
        $crate::post($crate::Msg::Toast(rust_i18n::t!($key $(, $name = $name)*).into_owned()));
    }};
}

mod app;
mod exec;
mod logger;

use app::{App, PAD_ENABLED};
use grapnel_config::{Config, InputPosition};
use grapnel_engine::{Command, Event, Notice};
use grapnel_keys::Key;
use grapnel_win::{hook, inputbox, pipe, toast, tray, tray::Tray, window};
use rust_i18n::t;
use std::cell::{Cell, RefCell};
use std::path::PathBuf;
use std::sync::OnceLock;
use std::sync::atomic::{AtomicIsize, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::WindowsAndMessaging::*;
use windows::core::w;

pub const WM_TRAY: u32 = WM_APP + 1;
pub const WM_WINDOW: u32 = WM_APP + 2;
pub const WM_UIA: u32 = WM_APP + 3;
/// Wake-up for the [`post`] channel. Payloads never travel in message parameters.
pub const WM_QUEUE: u32 = WM_APP + 4;

const MENU_SUSPEND: u32 = 1;
const MENU_RELOAD: u32 = 2;
const MENU_SETTINGS: u32 = 3;
const MENU_EXIT: u32 = 4;

static MAIN: AtomicIsize = AtomicIsize::new(0);

/// Work handed to the main thread from hooks, workers and other threads.
pub enum Msg {
    Pipe(String),
    Pad(Event),
    Deferred(Command),
    /// An error, already in the display language.
    Toast(String),
    Reloaded(Result<Config, Vec<String>>),
}

static TX: OnceLock<Sender<Msg>> = OnceLock::new();

/// Queues `msg` for the main thread (from any thread).
pub fn post(msg: Msg) {
    let Some(tx) = TX.get() else { return };
    let _ = tx.send(msg);
    let hwnd = HWND(MAIN.load(Ordering::Relaxed) as _);
    let _ = unsafe { PostMessageW(Some(hwnd), WM_QUEUE, WPARAM(0), LPARAM(0)) };
}

thread_local! {
    static APP: RefCell<Option<App>> = const { RefCell::new(None) };
    static TRAY: RefCell<Option<Tray>> = const { RefCell::new(None) };
    static TASKBAR_CREATED: Cell<u32> = const { Cell::new(0) };
    static RX: RefCell<Option<Receiver<Msg>>> = const { RefCell::new(None) };
}

/// Next posted message; the borrow ends before it is handled, since handlers may post again.
fn next_posted() -> Option<Msg> {
    RX.with(|r| r.borrow().as_ref()?.try_recv().ok())
}

/// Runs `f` on the app unless it is already borrowed (re-entrant message).
fn with_app<R>(f: impl FnOnce(&mut App) -> R) -> Option<R> {
    APP.with(|a| a.try_borrow_mut().ok()?.as_mut().map(f))
}

pub fn set_tray_state(tip: &str, paused: bool) {
    TRAY.with(|t| t.borrow().as_ref().map(|t| t.set_state(tip, paused)));
}

pub fn balloon(title: &str, text: &str, error: bool) {
    TRAY.with(|t| t.borrow().as_ref().map(|t| t.balloon(title, text, error)));
}

fn add_tray(hwnd: HWND) {
    TRAY.with(|t| t.borrow_mut().take()); // drop (NIM_DELETE) the old icon before adding the new one
    match Tray::add(hwnd, WM_TRAY, "grapnel") {
        Ok(tray) => TRAY.with(|t| *t.borrow_mut() = Some(tray)),
        Err(e) => report!("error.tray", error = e),
    }
}

fn open_settings() {
    let Some(path) = with_app(|a| a.config_path.clone()) else { return };
    let exe = std::env::current_exe().unwrap_or_default().with_file_name("grapnel-settings.exe");
    if let Err(e) = std::process::Command::new(&exe).arg("--config").arg(path).spawn() {
        report!("error.run", program = exe.display(), error = e);
    }
}

fn tray_menu() {
    let suspended = with_app(|a| a.is_suspended()).unwrap_or(false);
    let items = [
        (MENU_SUSPEND, t!("tray.suspend"), suspended),
        (MENU_RELOAD, t!("tray.reload"), false),
        (MENU_SETTINGS, t!("tray.settings"), false),
        (MENU_EXIT, t!("tray.exit"), false),
    ];
    let hwnd = HWND(MAIN.load(Ordering::Relaxed) as _);
    match tray::menu(hwnd, &items) {
        Some(MENU_SUSPEND) => drop(with_app(App::toggle_suspend)),
        Some(MENU_RELOAD) => drop(with_app(App::reload)),
        Some(MENU_SETTINGS) => open_settings(),
        Some(MENU_EXIT) => unsafe { PostQuitMessage(0) },
        _ => {}
    }
}

fn show_toast(text: &str, ms: u32) {
    if let Err(e) = toast::show(text, ms) {
        log::warn!("cannot show a toast: {e}"); // not error: that would post another toast
    }
}

/// Handles one queued message on the main thread.
fn handle(msg: Msg) {
    match msg {
        Msg::Pipe(line) => match line.as_str() {
            "reload" => drop(with_app(App::reload)),
            "suspend" => drop(with_app(App::toggle_suspend)),
            "exit" => unsafe { PostQuitMessage(0) },
            other => log::warn!("unknown pipe command '{other}'"),
        },
        Msg::Pad(ev) => drop(with_app(|a| a.on_pad(ev))),
        // Opening a window can dispatch messages, so do it without holding the app borrow.
        Msg::Deferred(Command::InputBox { prompt, then, position }) => {
            with_app(|a| a.before_prompt(then));
            if let Err(e) = inputbox::open(&prompt, position == InputPosition::Bottom) {
                report!("error.input_box", error = e);
            }
        }
        Msg::Deferred(Command::Notice(n)) => {
            let text = |locale: &str| match &n {
                Notice::Undefined(keys) => t!("notice.undefined", locale = locale, keys = keys),
                Notice::TimedOut(keys) => t!("notice.timed_out", locale = locale, keys = keys),
            };
            log::info!("{}", text("en"));
            show_toast(&text(&rust_i18n::locale()), 2500);
        }
        Msg::Deferred(c) => drop(with_app(|a| a.deferred(c))),
        Msg::Toast(text) => show_toast(&text, 5000),
        Msg::Reloaded(result) => {
            inputbox::close(); // its action id belongs to the old config
            with_app(|a| a.apply_reload(result));
        }
    }
}

unsafe extern "system" fn wndproc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    let busy = APP.with(|a| a.try_borrow_mut().is_err());
    if busy && msg != WM_TRAY && (WM_APP..=WM_QUEUE).contains(&msg) {
        let _ = unsafe { PostMessageW(Some(hwnd), msg, wp, lp) }; // retry once the app is free
        return LRESULT(0);
    }
    match msg {
        WM_TRAY if matches!(lp.0 as u32, WM_LBUTTONUP | WM_RBUTTONUP) => tray_menu(),
        WM_WINDOW => drop(with_app(|a| a.window_changed(wp.0))),
        WM_UIA => drop(with_app(App::uia_changed)),
        WM_QUEUE => {
            while let Some(m) = next_posted() {
                handle(m);
            }
        }
        WM_HOTKEY => drop(with_app(App::toggle_suspend)),
        WM_TIMER => drop(with_app(App::on_timer)),
        m if m == TASKBAR_CREATED.with(Cell::get) => add_tray(hwnd),
        _ => return unsafe { DefWindowProcW(hwnd, msg, wp, lp) },
    }
    LRESULT(0)
}

fn create_window() -> windows::core::Result<HWND> {
    unsafe {
        let instance = GetModuleHandleW(None)?.into();
        let wc = WNDCLASSW {
            lpfnWndProc: Some(wndproc),
            hInstance: instance,
            lpszClassName: w!("grapnel_main"),
            ..Default::default()
        };
        RegisterClassW(&wc);
        TASKBAR_CREATED.with(|c| c.set(RegisterWindowMessageW(w!("TaskbarCreated"))));
        CreateWindowExW(
            WS_EX_TOOLWINDOW,
            w!("grapnel_main"),
            w!("grapnel"),
            WS_POPUP,
            0,
            0,
            0,
            0,
            None,
            None,
            Some(instance),
            None,
        )
    }
}

fn fatal(text: &str) -> ! {
    if cfg!(debug_assertions) {
        eprintln!("{text}");
    } else {
        grapnel_win::error_box(text);
    }
    std::process::exit(1);
}

fn config_path(args: &[String]) -> PathBuf {
    let path = match args.iter().position(|a| a == "--config") {
        Some(i) => PathBuf::from(args.get(i + 1).unwrap_or_else(|| fatal(&t!("fatal.no_config_path")))),
        None => grapnel_config::default_entry(),
    };
    if !path.exists() && std::fs::create_dir_all(path.parent().unwrap_or(&path)).is_ok() {
        let _ = std::fs::write(&path, format!("{}\n", t!("fatal.new_config")));
    }
    path
}

/// Uses `language`, or the Windows display language when unset.
pub fn set_language(language: Option<&str>) {
    rust_i18n::set_locale(&language.map_or_else(grapnel_win::ui_language, str::to_owned));
}

fn main() {
    set_language(None);
    let args: Vec<String> = std::env::args().skip(1).collect();
    if let Some(cmd) = args.first().filter(|a| ["reload", "suspend", "exit"].contains(&a.as_str())) {
        pipe::send(cmd).unwrap_or_else(|e| fatal(&t!("fatal.not_running", error = e)));
        return;
    }
    let (tx, rx) = mpsc::channel();
    let _ = TX.set(tx);
    RX.with(|r| *r.borrow_mut() = Some(rx));
    logger::init();
    let path = config_path(&args);
    let cfg =
        app::load(&path).unwrap_or_else(|errors| fatal(&format!("{}\n{}", t!("fatal.config"), errors.join("\n"))));
    set_language(cfg.settings.language.as_deref());
    let hwnd = create_window().unwrap_or_else(|e| fatal(&t!("fatal.window", error = e)));
    MAIN.store(hwnd.0 as isize, Ordering::Relaxed);
    if let Err(e) = pipe::serve(|line| post(Msg::Pipe(line))) {
        fatal(&t!("fatal.already_running", error = e));
    }
    add_tray(hwnd);
    let app = App::new(hwnd, path, cfg);
    set_tray_state(&app.tooltip(), false);
    APP.with(|a| *a.borrow_mut() = Some(app));
    let _watch = window::watch(hwnd, WM_WINDOW);
    hook::set_handler(|ev| with_app(|a| a.on_input(ev)).unwrap_or(false));
    grapnel_pad::spawn(|b, down| {
        if PAD_ENABLED.load(Ordering::Relaxed) {
            post(Msg::Pad(if down { Event::Down(Key::Pad(b)) } else { Event::Up(Key::Pad(b)) }));
        }
    });
    log::info!("started");
    let mut msg = MSG::default();
    while unsafe { GetMessageW(&mut msg, None, 0, 0) }.as_bool() {
        if let Some(text) = inputbox::pretranslate(&msg) {
            if let Some(text) = text {
                with_app(|a| a.input_done(&text));
            }
            continue;
        }
        unsafe {
            let _ = TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
    }
    with_app(App::shutdown);
    TRAY.with(|t| t.borrow_mut().take());
    log::info!("exited");
}

#[cfg(test)]
mod tests {
    use rust_i18n::t;

    #[test]
    fn regional_names_fall_back_to_the_language() {
        assert_eq!(t!("tray.exit", locale = "ja-JP"), "終了");
        assert_eq!(t!("tray.exit", locale = "xx"), "Exit");
    }
}
