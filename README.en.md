# grapnel

[日本語](README.md)

A resident tool for Windows 11 that remaps keyboard, mouse, and gamepad input into other input.

- Emacs-style chords (`C-x t 0`), Vim-style modes, and user-defined modifier keys that act differently on tap and hold
- Mouse buttons, wheel, and gestures, as well as gamepad buttons and sticks, can be used as input
- Rules and actions switch per app, window, and control (including UI Automation)
- The hook is removed while a specified app is in the foreground (pass-through)
- Configuration is TOML, and can also be edited with the GUI settings tool (`grapnel-settings`)

Documents (Japanese): [Requirements](docs/requirements.md) / [Specification (how to write the config)](docs/spec.md) / [Design](docs/design.md)

## Build

Requirements: Rust stable (pinned by rust-toolchain.toml), the `wasm32-unknown-unknown` target, [trunk](https://trunkrs.dev/), and [tauri-cli](https://tauri.app/) 2.x.

```sh
cargo build --release                 # resident app: target/release/grapnel.exe
cargo test                            # tests for the core crates
cd apps/settings && cargo tauri build --no-bundle   # settings tool: target/release/grapnel-settings.exe
```

When developing the settings tool, use `cargo tauri dev` in `apps/settings`.

With [just](https://github.com/casey/just), `just restart` stops the running grapnel, builds the settings tool, and starts grapnel in one step (`just --list` shows all recipes). It closes any open settings tool, so unsaved edits are lost.

## Usage

Running `grapnel.exe` puts it in the system tray. The config file defaults to `%APPDATA%\grapnel\config.toml`; an empty one is created if it does not exist.

```sh
grapnel.exe --config D:\path\to\config.toml   # start with a specific config file
grapnel.exe reload                            # tell the running instance to reload
grapnel.exe suspend                           # toggle suspend
grapnel.exe exit                              # exit
```

The tray icon menu lets you suspend, reload, reinstall the input hooks, open the settings tool, and exit. If Windows has dropped the hooks and remapping stopped working, choose "Reinstall the input hooks".
Place `grapnel-settings.exe` in the same folder as `grapnel.exe` to open it from the menu.

A minimal config:

```toml
[modifiers.Mu]
key = "Muhenkan"          # Muhenkan: Muhenkan when tapped, a modifier when held

[keymap]
"Mu-h" = "Left"           # Muhenkan+h to Left
"C-x C-s" = "C-s"         # key sequence (press C-x, then C-s)
```

See the [specification](docs/spec.md) for details. Examples: Emacs-style key bindings in [examples/emacs.toml](examples/emacs.toml), using F19 like the macOS Cmd key in [examples/mac-cmd.toml](examples/mac-cmd.toml), Vim-style modes (normal / input / visual / command / search) in [examples/vim.toml](examples/vim.toml), and using a JIS keyboard as the AX layout in [examples/ax.toml](examples/ax.toml) (keyswap; when included together with other configs, key bindings also follow the positions of the AX layout's symbols). All but ax.toml load shared actions from [examples/default-actions.toml](examples/default-actions.toml).

## Limitations

- Input is not distinguished by which keyboard or mouse it came from.
- Output to gamepads is not supported. The gamepad's original input still reaches apps.
- To remap input over apps running as administrator, grapnel must also be run as administrator.
- Saving from the settings tool removes comments in the file.

## License

Licensed under either [MIT](LICENSE-MIT) or [Apache-2.0](LICENSE-APACHE), at your option. For third-party materials included, see [THIRD_PARTY_NOTICES.md](THIRD_PARTY_NOTICES.md).
