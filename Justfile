set windows-shell := ["pwsh", "-NoLogo", "-NoProfile", "-Command"]

# Stop grapnel, rebuild the settings tool, then run grapnel again
restart: stop settings run

# Stop the running grapnel (the settings tool must be closed so it can be rebuilt)
stop:
    if (Get-Process grapnel-settings -ErrorAction SilentlyContinue) { Write-Error "grapnel-settings is open; close it first"; exit 1 }
    if (Get-Process grapnel -ErrorAction SilentlyContinue) { ./target/release/grapnel.exe exit; Wait-Process -Name grapnel -Timeout 10 -ErrorAction SilentlyContinue }

# Build the settings tool (target/release/grapnel-settings.exe)
settings:
    cd apps/settings; cargo tauri build

# Run grapnel (release); it stays resident until exited
run:
    cargo run --release -p grapnel
