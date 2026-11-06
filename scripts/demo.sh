
set -euo pipefail

DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$DIR"

if ! command -v vhs >/dev/null 2>&1; then
  cat <<'EOF'
VHS n'est pas installé (github.com/charmbracelet/vhs).
  Arch   : sudo pacman -S vhs      (ou  yay -S vhs)
  macOS  : brew install vhs
  Go     : go install github.com/charmbracelet/vhs@latest
Puis relance : ./scripts/demo.sh
EOF
  exit 1
fi

if [ ! -x target/release/zer0 ]; then
  echo "→ compilation release…"
  cargo build --release
fi

vhs scripts/demo.tape
echo "✅ Vidéo générée (voir le fichier 'Output' de scripts/demo.tape → demo.gif)."
