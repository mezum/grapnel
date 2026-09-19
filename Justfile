set windows-shell := ["pwsh", "-NoLogo", "-NoProfile", "-Command"]

# Stop grapnel, rebuild the settings tool, then run grapnel again
restart: stop settings run

# Stop grapnel and close the settings tool (unsaved edits there are lost), so both can be rebuilt
stop:
    Get-Process | Where-Object Name -eq grapnel-settings | Stop-Process -PassThru | Wait-Process -Timeout 10
    if (Get-Process | Where-Object Name -eq grapnel) { ./target/release/grapnel.exe exit; Start-Sleep -Milliseconds 500 }
    Get-Process | Where-Object Name -eq grapnel | Stop-Process -PassThru | Wait-Process -Timeout 10

# Build the settings tool (target/release/grapnel-settings.exe)
settings:
    cd apps/settings; cargo tauri build

# Run grapnel (release); it stays resident until exited
run:
    cargo run --release -p grapnel
