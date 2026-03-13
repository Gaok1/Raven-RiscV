#!/usr/bin/env bash
rustup target add riscv32im-unknown-none-elf
rustup component add rust-src
cargo build --release
