param(
  [switch]$UseCross
)

$ErrorActionPreference = 'Stop'

if (-not (Get-Command cargo -ErrorAction SilentlyContinue)) {
  Write-Error "cargo not found. Install Rust from https://rustup.rs/"
}
if (-not (Get-Command rustup -ErrorAction SilentlyContinue)) {
  Write-Error "rustup not found. Install Rust from https://rustup.rs/"
}

function Ensure-Target($target) {
  try {
    rustup target add $target | Out-Null
  } catch {
    Write-Warning "Failed to add target $target: $_"
  }
}

function Has-Cross {
  return [bool](Get-Command cross -ErrorAction SilentlyContinue)
}

$targets = @()
if ($IsWindows) {
  $targets += 'x86_64-pc-windows-msvc'
  $targets += 'aarch64-pc-windows-msvc'
  # Linux targets require cross or external linkers
  if ($UseCross -or (Has-Cross)) {
    $targets += 'x86_64-unknown-linux-gnu'
    $targets += 'aarch64-unknown-linux-gnu'
  }
} else {
  # Non-Windows host: focus on Linux targets
  $targets += 'x86_64-unknown-linux-gnu'
  if ($UseCross -or (Has-Cross)) {
    $targets += 'aarch64-unknown-linux-gnu'
  }
}

Write-Host "Building targets: $($targets -join ', ')" -ForegroundColor Cyan

foreach ($t in $targets) {
  Write-Host "\n=== Building $t ===" -ForegroundColor Green
  Ensure-Target $t
  try {
    if ($t -like '*-unknown-linux-*') {
      if ($UseCross -or (Has-Cross)) {
        cross build --release --target $t
      } else {
        Write-Warning "Skipping $t (cross not installed). Install with: cargo install cross"
        continue
      }
    } else {
      cargo build --release --target $t
    }
    $bin = Join-Path "target/$t/release" (if ($IsWindows) { 'falconasm.exe' } else { 'falconasm' })
    if (Test-Path $bin) { Write-Host "Built: $bin" -ForegroundColor Yellow }
  } catch {
    Write-Warning "Build failed for $t: $_"
  }
}

Write-Host "\nDone." -ForegroundColor Cyan

