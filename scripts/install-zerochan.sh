#!/usr/bin/env bash
# Installe la chaîne vocale Zero-chan : micro + transcription locale.
#
#   ./scripts/install-zerochan.sh
#
# Idempotent : le venv ~/.venvs/zer0-whisper est créé une seule fois, puis
# les dépendances Python sont actualisées. Zer0 détecte ce venv
# automatiquement ; ZER0_PYTHON reste prioritaire si une autre installation
# est préférée.
set -euo pipefail

VENV="${ZER0_WHISPER_VENV:-$HOME/.venvs/zer0-whisper}"
PYTHON_BIN="${ZER0_PYTHON:-}"

info() { printf '→ %s\n' "$*"; }
ok()   { printf '✅ %s\n' "$*"; }
die()  { printf '❌ %s\n' "$*" >&2; exit 1; }

# Plage réellement supportée par faster-whisper + av<16 (testé jusqu'à 3.14).
PY_OK='import sys; raise SystemExit(0 if (3, 9) <= sys.version_info < (3, 15) else 1)'

if [ -z "$PYTHON_BIN" ]; then
  # Ne pas prendre le premier venu : un shim mise/pyenv peut masquer le Python
  # système avec une version trop récente (3.15) sans wheels pour ctranslate2.
  for candidate in python3.12 python3.13 python3.14 python3.11 python3 python; do
    command -v "$candidate" >/dev/null 2>&1 || continue
    if "$candidate" -c "$PY_OK" >/dev/null 2>&1; then
      PYTHON_BIN="$candidate"
      break
    fi
    info "Ignoré $candidate ($("$candidate" --version 2>&1)) : hors plage 3.9–3.14"
  done
fi
[ -n "$PYTHON_BIN" ] || die "Aucun Python 3.9–3.14 utilisable. Installe Python 3.12 (ou pointe ZER0_PYTHON dessus), puis relance."

info "Python utilisé : $PYTHON_BIN ($($PYTHON_BIN --version 2>&1))"
"$PYTHON_BIN" -c "$PY_OK" || \
  die "$PYTHON_BIN n'est pas dans la plage 3.9–3.14 requise par faster-whisper + av<16."

if [ ! -x "$VENV/bin/python" ]; then
  info "Création du venv STT : $VENV"
  mkdir -p "$(dirname "$VENV")"
  "$PYTHON_BIN" -m venv "$VENV" || die \
    "Création du venv impossible. Installe le module venv (Debian/Ubuntu : python3-venv), puis relance."
fi

VENV_PYTHON="$VENV/bin/python"
# Certains Python (uv, distributions embarquées) créent un venv sans pip.
if ! "$VENV_PYTHON" -m pip --version >/dev/null 2>&1; then
  info "Initialisation de pip dans le venv…"
  "$VENV_PYTHON" -m ensurepip --upgrade >/dev/null
fi

info "Installation de faster-whisper + une version compatible de PyAV…"
"$VENV_PYTHON" -m pip install --upgrade pip >/dev/null
# av 19 a retiré `metadata_errors`, encore utilisé par faster-whisper 1.2.1.
"$VENV_PYTHON" -m pip install --upgrade "faster-whisper" "av<16"

"$VENV_PYTHON" - <<'PY'
import faster_whisper
import av
print(f"   faster-whisper {faster_whisper.__version__} · PyAV {av.__version__}")
PY

if command -v pw-record >/dev/null 2>&1; then
  ok "Enregistreur micro : pw-record"
elif command -v parecord >/dev/null 2>&1; then
  ok "Enregistreur micro : parecord"
elif command -v arecord >/dev/null 2>&1; then
  ok "Enregistreur micro : arecord"
elif command -v sox_ng >/dev/null 2>&1; then
  ok "Enregistreur micro : sox_ng"
elif command -v sox >/dev/null 2>&1; then
  ok "Enregistreur micro : sox"
else
  printf '⚠ Aucun enregistreur micro détecté. Installe PipeWire/PulseAudio/ALSA ou sox.\n' >&2
fi

ok "Zero-chan prêt. Venv STT : $VENV"
info "Zer0 le détecte automatiquement ; pour le prioriser : export ZER0_PYTHON=\"$VENV_PYTHON\""
