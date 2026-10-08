#!/usr/bin/env bash
# Start llama-server with the Zer0 model (OpenAI-compatible API on :8080).
#
# By default Zer0's inference is limited to 2 of the 4 CPU cores (threads 0-1)
# so you can keep working on the other cores while the agent runs.
set -euo pipefail

DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
MODEL="${ZER0_MODEL:-$DIR/models/ollama_models/qwen3.5-4B-super-coder.Q4_0.gguf}"
PORT="${ZER0_PORT:-8080}"
CTX="${ZER0_CTX:-16384}"

# CPU budget for inference: 2 cores by default (0-1). Override to use all 4.
THREADS="${ZER0_THREADS:-2}"
CPUS="${ZER0_CPUS:-0-1}"   # lo-hi range, e.g. "2-3" or "0-3"

exec taskset -c "$CPUS" llama-server \
    -m "$MODEL" \
    -c "$CTX" \
    --port "$PORT" \
    --jinja \
    -t "$THREADS" \
    -tb "$THREADS"
