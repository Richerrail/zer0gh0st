#!/usr/bin/env bash
# update.sh — installe le DERNIER binaire zer0 publié sur les Releases GitHub.
#
# À quoi ça sert, si `zer0 update` existe déjà ? À installer/rafraîchir un
# lanceur stable dans ~/.local/bin (utile quand tu construis aussi depuis les
# sources : `cargo build` écrit dans target/release, pas dans ton PATH).
#
#   ./scripts/update.sh                 # → ~/.local/bin/zer0gh0st
#   BIN_DIR=/usr/local/bin sudo ./scripts/update.sh
#   NAME=zer0 ./scripts/update.sh       # autre nom de lanceur
#
# Vérifie le SHA256 si la release publie SHA256SUMS (sinon le dit).
set -euo pipefail

REPO="${REPO:-Richerrail/zer0gh0st}"
BIN_DIR="${BIN_DIR:-$HOME/.local/bin}"
NAME="${NAME:-zer0gh0st}"

case "$(uname -s)-$(uname -m)" in
  Linux-x86_64)              TARGET=x86_64-unknown-linux-gnu ;;
  Linux-aarch64|Linux-arm64) TARGET=aarch64-unknown-linux-gnu ;;
  Darwin-arm64)              TARGET=aarch64-apple-darwin ;;
  Darwin-x86_64)             TARGET=x86_64-apple-darwin ;;
  *) echo "✗ plateforme non publiée — compile : cargo build --release"; exit 1 ;;
esac
ASSET="zer0-$TARGET"

echo "── dernière release de $REPO…"
info=$(curl -fsSL "https://api.github.com/repos/$REPO/releases/latest")
tag=$(printf '%s' "$info" | python3 -c 'import json,sys; print(json.load(sys.stdin)["tag_name"])')
url=$(printf '%s' "$info" | python3 -c "
import json,sys
d = json.load(sys.stdin)
for a in d.get('assets', []):
    if a['name'] == '$ASSET':
        print(a['browser_download_url']); break
")
[ -n "${url:-}" ] || { echo "✗ asset « $ASSET » absent de la release $tag"; exit 1; }
echo "   $tag · $ASSET"

tmp="$(mktemp)"
trap 'rm -f "$tmp"' EXIT
echo "── téléchargement…"
curl -fsSL "$url" -o "$tmp"

sums_url="https://github.com/$REPO/releases/download/$tag/SHA256SUMS"
if sums=$(curl -fsSL "$sums_url" 2>/dev/null); then
  want=$(printf '%s\n' "$sums" | awk -v f="$ASSET" '$2==f || $2=="*"f {print $1}' | head -1)
  got=$(sha256sum "$tmp" | cut -d' ' -f1)
  if [ -n "$want" ]; then
    [ "$want" = "$got" ] || { echo "✗ SHA256 invalide (attendu $want, obtenu $got) — abandon"; exit 1; }
    echo "   SHA256 vérifié ✓"
  else
    echo "   ⚠ SHA256SUMS publié mais sans entrée pour $ASSET"
  fi
else
  echo "   ⚠ pas de SHA256SUMS publié — intégrité non vérifiée"
fi

mkdir -p "$BIN_DIR"
install -m 0755 "$tmp" "$BIN_DIR/$NAME"
echo "✅ installé : $BIN_DIR/$NAME ($tag)"
"$BIN_DIR/$NAME" --version 2>/dev/null || true
