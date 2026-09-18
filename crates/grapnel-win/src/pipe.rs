//! Named-pipe control channel (`reload`, `suspend`, `exit`). Also acts as the single-instance lock.

use std::io::{Read, Write};
use std::mem::ManuallyDrop;
use std::os::windows::io::FromRawHandle;
use windows::Win32::Foundation::{ERROR_PIPE_CONNECTED, HANDLE};
use windows::Win32::Storage::FileSystem::{FILE_FLAG_FIRST_PIPE_INSTANCE, PIPE_ACCESS_INBOUND};
use windows::Win32::System::Pipes::*;
use windows::core::PCWSTR;

pub fn name() -> String {
    format!(r"\.\pipe\grapnel-{}", std::env::var("USERNAME").unwrap_or_default())
}

/// Sends one command to the running instance.
pub fn send(cmd: &str) -> std::io::Result<()> {
    std::fs::OpenOptions::new().write(true).open(name())?.write_all(format!("{cmd}\n").as_bytes())
}

/// Creates the pipe and serves it on a thread, calling `on_line` for each received line.
/// Fails if another instance already owns the pipe.
pub fn serve(on_line: impl Fn(String) + Send + 'static) -> windows::core::Result<()> {
    let name = crate::wide(&name());
    let h = unsafe {
        CreateNamedPipeW(
            PCWSTR(name.as_ptr()),
            PIPE_ACCESS_INBOUND | FILE_FLAG_FIRST_PIPE_INSTANCE,
            PIPE_TYPE_BYTE | PIPE_WAIT | PIPE_REJECT_REMOTE_CLIENTS,
            1,
            0,
            4096,
            0,
            None,
        )
    };
    if h.is_invalid() {
        return Err(windows::core::Error::from_thread());
    }
    let raw = h.0 as isize;
    std::thread::spawn(move || {
        let h = HANDLE(raw as _);
        loop {
            if let Err(e) = unsafe { ConnectNamedPipe(h, None) }
                && e.code() != ERROR_PIPE_CONNECTED.to_hresult()
            {
                log::error!("pipe: {e}");
                return;
            }
            let mut file = ManuallyDrop::new(unsafe { std::fs::File::from_raw_handle(h.0) });
            let mut text = String::new();
            let _ = file.read_to_string(&mut text);
            let _ = unsafe { DisconnectNamedPipe(h) };
            text.lines().map(str::trim).filter(|l| !l.is_empty()).for_each(|l| on_line(l.to_string()));
        }
    });
    Ok(())
}
