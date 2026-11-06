
set -euo pipefail

DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$DIR"

export PATH="$HOME/.cargo/bin:$PATH"

export ZER0_CMD_NAME="${ZER0_CMD_NAME:-zer0}"
cargo run --release -- "$@"
