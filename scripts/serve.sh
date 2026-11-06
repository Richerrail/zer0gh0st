
set -euo pipefail

DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
MODEL="${ZER0_MODEL:-$DIR/models/ollama_models/qwen3.5-4B-super-coder.Q4_0.gguf}"
PORT="${ZER0_PORT:-8080}"
CTX="${ZER0_CTX:-16384}"

THREADS="${ZER0_THREADS:-$(nproc)}"
THREADS_BATCH="${ZER0_THREADS_BATCH:-$(( THREADS / 2 > 0 ? THREADS / 2 : 1 ))}"
CPUS="${ZER0_CPUS:-0-1}"

exec taskset -c "$CPUS" llama-server \
    -m "$MODEL" \
    -c "$CTX" \
    --port "$PORT" \
    --jinja \
    --reasoning-budget 0 \
    --no-warmup \
    -np 1 -b 512 -ub 256 \
    -t "$THREADS" \
    -tb "$THREADS_BATCH"
