#!/usr/bin/env bash
set -euo pipefail

echo_title()
{
  local label="$1"
  local total_width=36
  local label_len padding extra left right

  label_len=${#label}
  padding=$(((total_width - label_len - 2) / 2))
  extra=$(((total_width - label_len - 2) % 2))

  left=$(printf '=%.0s' $(seq 1 "$padding"))
  right=$(printf '=%.0s' $(seq 1 $((padding + extra))))

  echo -e "\n${left} ${label} ${right}"
}

echo_title "Svelte Check"
pnpm svelte-check

echo_title "JS Format"
pnpm format-check

echo_title "JS Lint"
pnpm lint

echo_title "Rust Format"
cargo fmt --manifest-path src-tauri/Cargo.toml --all -- --check

echo_title "Rust Lint"
cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets --all-features -- -D warnings

echo_title "Rust Test"
cargo test --manifest-path src-tauri/Cargo.toml --all
