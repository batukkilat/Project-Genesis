# run-app.ps1 — build and run genesis-app natively on Windows (real GPU).
#
# Why: under WSLg the only Vulkan adapter is llvmpipe (software rendering on
# the CPU). Running natively gives wgpu the real GPU via DX12 with zero WSL
# overhead. The repo already lives on C:, so this is the same checkout.
#
# One-time setup (PowerShell):
#   winget install Rustlang.Rustup
#   winget install Microsoft.VisualStudio.2022.BuildTools --override `
#     "--quiet --add Microsoft.VisualStudio.Workload.VCTools --includeRecommended"
#   rustup toolchain install 1.96.1        # repo toolchain (rust-toolchain.toml applies it anyway)
#
# Then, from anywhere:
#   powershell -ExecutionPolicy Bypass -File "tools\run-app.ps1"
#   tools\run-app.ps1 --config configs\env-gradient.ron
#   tools\run-app.ps1 --zoom 40 --fps 60
#
# All arguments pass straight through to genesis-app (--config, --rules,
# --actions, --mapping, --palette, --zoom, --fps, --smoke).
#
# The build lands in %LOCALAPPDATA%\genesis-target, NOT in the repo:
# Documents is commonly OneDrive-synced, and syncing a multi-gigabyte cargo
# target directory would hurt (the WSL builds avoid the repo for the same
# reason, via CARGO_TARGET_DIR in the WSL home).

$ErrorActionPreference = "Stop"

$repo = Split-Path -Parent $PSScriptRoot

if (-not (Get-Command cargo -ErrorAction SilentlyContinue)) {
    Write-Host "cargo not found. Install Rust first:" -ForegroundColor Yellow
    Write-Host "  winget install Rustlang.Rustup"
    Write-Host "then reopen this terminal and rerun."
    exit 1
}

$env:CARGO_TARGET_DIR = Join-Path $env:LOCALAPPDATA "genesis-target"

Write-Host "building genesis-app (release; first build takes several minutes)..."
Push-Location $repo
try {
    cargo run --release -p genesis-render --features app --bin genesis-app -- @args
    if ($LASTEXITCODE -ne 0) {
        Write-Host ""
        Write-Host "Build or run failed." -ForegroundColor Yellow
        Write-Host "If the error mentions 'link.exe' or 'MSVC', install the C++ build tools:"
        Write-Host "  winget install Microsoft.VisualStudio.2022.BuildTools --override `"--quiet --add Microsoft.VisualStudio.Workload.VCTools --includeRecommended`""
        Write-Host "then reopen this terminal and rerun."
        exit $LASTEXITCODE
    }
}
finally {
    Pop-Location
}
