set windows-shell := ["pwsh", "-NoLogo", "-NoProfile", "-Command"]

# Check formatting and lint everything (CI runs this)
lint:
    cargo fmt --check
    cargo clippy --all-targets -- -D warnings
    cargo clippy -p grapnel-settings-ui --target wasm32-unknown-unknown -- -D warnings

# Run every test (the whole run: the examples check spans crates)
test:
    cargo test

# Stop grapnel, rebuild the settings tool, then run grapnel again
restart: stop settings run

# Kill grapnel and the settings tool (unsaved edits there are lost), so both can be rebuilt
stop:
    Get-Process | Where-Object Name -in grapnel, grapnel-settings | Stop-Process -PassThru | Wait-Process -Timeout 10

# Build the settings tool (target/release/grapnel-settings.exe)
settings:
    cd apps/settings; cargo tauri build

# Run grapnel (release); it stays resident until exited
run:
    cargo run --release -p grapnel
