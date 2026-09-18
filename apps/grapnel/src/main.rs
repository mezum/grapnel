//! grapnel: tray-resident input remapper.
//! Usage: `grapnel [--config <path>]` or `grapnel reload|suspend|exit` to control a running instance.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod app;
mod exec;
mod logger;

use app::{App, PAD_ENABLED};
use grapnel_config::{Config, InputPosition};
use grapnel_engine::{Command, Event};
use grapnel_keys::Key;
use grapnel_win::{hook, inputbox, pipe, toast, tray, tray::Tray, window};
use std::cell::{Cell, RefCell};
use std::collections::VecDeque;
use std::path::PathBuf;
use std::sync::Mutex;
use std::sync::atomic::{AtomicIsize, Ordering};
use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::WindowsAndMessaging::*;
use windows::core::w;

pub const WM_TRAY: u32 = WM_APP + 1;
pub const WM_WINDOW: u32 = WM_APP + 2;
pub const WM_UIA: u32 = WM_APP + 3;
/// Wake-up for [`QUEUE`]. Payloads never travel in message parameters.
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
    LogError(String),
    Reloaded(Result<Config, Vec<String>>),
}

static QUEUE: Mutex<VecDeque<Msg>> = Mutex::new(VecDeque::new());

/// Queues `msg` for the main thread (from any thread).
pub fn post(msg: Msg) {
    QUEUE.lock().unwrap().push_back(msg);
    let hwnd = HWND(MAIN.load(Ordering::Relaxed) as _);
    let _ = unsafe { PostMessageW(Some(hwnd), WM_QUEUE, WPARAM(0), LPARAM(0)) };
}

thread_local! {
    static APP: RefCell<Option<App>> = const { RefCell::new(None) };
    static TRAY: RefCell<Option<Tray>> = const { RefCell::new(None) };
    static TASKBAR_CREATED: Cell<u32> = const { Cell::new(0) };
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
    let tray = Tray::add(hwnd, WM_TRAY, "grapnel").map_err(|e| log::error!("トレイアイコンを追加できません: {e}"));
    TRAY.with(|t| *t.borrow_mut() = tray.ok());
}

fn open_settings() {
    let Some(path) = with_app(|a| a.config_path.clone()) else { return };
    let exe = std::env::current_exe().unwrap_or_default().with_file_name("grapnel-settings.exe");
    if let Err(e) = std::process::Command::new(&exe).arg("--config").arg(path).spawn() {
        log::error!("{} を起動できません: {e}", exe.display());
    }
}

fn tray_menu() {
    let suspended = with_app(|a| a.is_suspended()).unwrap_or(false);
    let items = [
        (MENU_SUSPEND, "一時停止", suspended),
        (MENU_RELOAD, "設定を再読み込み", false),
        (MENU_SETTINGS, "設定ツールを開く", false),
        (MENU_EXIT, "終了", false),
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
                log::error!("入力欄を表示できません: {e}");
            }
        }
        Msg::Deferred(Command::Notice(text)) => {
            log::info!("{text}");
            if let Err(e) = toast::show(&text, 2500) {
                log::warn!("cannot show notice: {e}");
            }
        }
        Msg::Deferred(c) => drop(with_app(|a| a.deferred(c))),
        Msg::LogError(text) => balloon("grapnel のエラー", &text, true),
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
            while let Some(m) = QUEUE.lock().unwrap().pop_front() {
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
        Some(i) => PathBuf::from(args.get(i + 1).unwrap_or_else(|| fatal("--config にパスがありません"))),
        None => grapnel_config::default_entry(),
    };
    if !path.exists() && std::fs::create_dir_all(path.parent().unwrap_or(&path)).is_ok() {
        let _ = std::fs::write(&path, "# grapnel の設定ファイル。書き方は docs/spec.md を参照。\n");
    }
    path
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if let Some(cmd) = args.first().filter(|a| ["reload", "suspend", "exit"].contains(&a.as_str())) {
        pipe::send(cmd).unwrap_or_else(|e| fatal(&format!("grapnel が起動していません: {e}")));
        return;
    }
    logger::init(|text| post(Msg::LogError(text)));
    let path = config_path(&args);
    let cfg = app::load(&path).unwrap_or_else(|errors| fatal(&errors.join("\n")));
    let hwnd = create_window().unwrap_or_else(|e| fatal(&format!("ウインドウを作成できません: {e}")));
    MAIN.store(hwnd.0 as isize, Ordering::Relaxed);
    if let Err(e) = pipe::serve(|line| post(Msg::Pipe(line))) {
        fatal(&format!("grapnel は既に起動しています ({e})"));
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
