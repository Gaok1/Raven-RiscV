#!/usr/bin/env bash
set -euo pipefail

USE_CROSS=${USE_CROSS:-0}

command -v cargo >/dev/null || { echo "cargo not found. Install Rust via rustup" >&2; exit 1; }
command -v rustup >/dev/null || { echo "rustup not found. Install Rust via rustup" >&2; exit 1; }

ensure_target() {
  local t="$1"
  rustup target add "$t" >/dev/null 2>&1 || true
}

has_cross() {
  command -v cross >/dev/null 2>&1
}

targets=( "x86_64-unknown-linux-gnu" )
if [[ "$USE_CROSS" == "1" ]] || has_cross; then
  targets+=( "aarch64-unknown-linux-gnu" )
fi

echo "Building targets: ${targets[*]}"

for t in "${targets[@]}"; do
  echo -e "\n=== Building $t ==="
  ensure_target "$t"
  if [[ "$t" == *"-unknown-linux-"* ]]; then
    if [[ "$USE_CROSS" == "1" ]] || has_cross; then
      cross build --release --target "$t"
    else
      echo "Skipping $t (cross not installed). Install with: cargo install cross"
      continue
    fi
  else
    cargo build --release --target "$t"
  fi
  bin="target/$t/release/falconasm"
  [[ -f "$bin" ]] && echo "Built: $bin"
done

echo -e "\nDone."

