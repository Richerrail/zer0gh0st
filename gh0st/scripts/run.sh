#!/usr/bin/env bash
# Build and run Zer0 (TUI). Assumes llama-server is already running.
set -euo pipefail

DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$DIR"

export PATH="$HOME/.cargo/bin:$PATH"
cargo run --release -- "$@"
