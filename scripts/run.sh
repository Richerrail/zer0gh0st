#!/usr/bin/env bash
# Build and run Zer0 (TUI). Assumes llama-server is already running.
set -euo pipefail

DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$DIR"

export PATH="$HOME/.cargo/bin:$PATH"
# Nom de commande rapporté par l'agent dans ses messages (ex. « zer0 »).
export ZER0_CMD_NAME="${ZER0_CMD_NAME:-zer0}"
cargo run --release -- "$@"
