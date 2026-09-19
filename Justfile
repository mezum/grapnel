set windows-shell := ["pwsh", "-NoLogo", "-NoProfile", "-Command"]

# Stop grapnel, rebuild the settings tool, then run grapnel again
restart: stop settings run

# Stop grapnel and close the settings tool (unsaved edits there are lost), so both can be rebuilt
stop:
    Stop-Process -Name grapnel-settings -ErrorAction SilentlyContinue; Wait-Process -Name grapnel-settings -Timeout 10 -ErrorAction SilentlyContinue
    if (Get-Process grapnel -ErrorAction SilentlyContinue) { ./target/release/grapnel.exe exit; Wait-Process -Name grapnel -Timeout 10 -ErrorAction SilentlyContinue }
    Stop-Process -Name grapnel -ErrorAction SilentlyContinue; Wait-Process -Name grapnel -Timeout 10 -ErrorAction SilentlyContinue

# Build the settings tool (target/release/grapnel-settings.exe)
settings:
    cd apps/settings; cargo tauri build

# Run grapnel (release); it stays resident until exited
run:
    cargo run --release -p grapnel
