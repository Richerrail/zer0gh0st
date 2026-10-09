#!/usr/bin/env bash
# Fast variant: run a small 1.5B model (much faster on weak CPUs).
# Same CPU budget as serve.sh: 2 cores.
set -euo pipefail

DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
MODEL="${ZER0_MODEL:-$DIR/models/ollama_models/qwen2.5-1.5b-instruct-q8_0.gguf}"
PORT="${ZER0_PORT:-8080}"
CTX="${ZER0_CTX:-8192}"
THREADS="${ZER0_THREADS:-2}"
CPUS="${ZER0_CPUS:-0-1}"

exec taskset -c "$CPUS" llama-server \
    -m "$MODEL" \
    -c "$CTX" \
    --port "$PORT" \
    -t "$THREADS" \
    -tb "$THREADS"
