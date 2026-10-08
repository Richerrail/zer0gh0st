#!/usr/bin/env bash
# Régénère le dossier public `gh0st/` : sources + docs + scripts, SANS modèles,
# clés, données ni binaires. Prêt à pousser sur GitHub.
#
#   ./scripts/package.sh
#
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
OUT="$ROOT/gh0st"

# Préserve un éventuel dépôt git déjà initialisé dans gh0st/
if [ -d "$OUT/.git" ]; then
  find "$OUT" -mindepth 1 -maxdepth 1 ! -name .git -exec rm -rf {} +
else
  rm -rf "$OUT"
  mkdir -p "$OUT"
fi
cd "$ROOT"
# Code et scripts
cp -r src scripts "$OUT/"

# Sources / manifestes / licences / docs
cp Cargo.toml Cargo.lock LICENSE \
   README.md SPEC.md SAFETY.md ARCHITECTURE.md ECOSYSTEM.md CHANGELOG.md \
   ROADMAP.md ROADMAP_FUN.md ROADMAP_PORT_SLASH.md BRAINSTORM_ZEROCHAN.md \
   "$OUT/"

# Dossiers de contenu : gardés vides (placeholders)
mkdir -p "$OUT/music" "$OUT/base_connaissance" "$OUT/mods"
touch "$OUT/music/.gitkeep" "$OUT/base_connaissance/.gitkeep"

# .gitignore public
cat > "$OUT/.gitignore" <<'EOF'
# Build output
/target

# Models (never commit — download/build your own)
/models
*.gguf
*.bin

# Runtime / user data
/knowledge.db
/knowledge.db-*
/knowledge.log
/conversation.jsonl
/memory.jsonl
/.zer0

# Personal content folders (placeholders kept empty)
/base_connaissance/*
!/base_connaissance/.gitkeep
/music/*
!/music/.gitkeep
/mods/*
!/mods/README.md

# Secrets
.env
*.key
*_api_key*
EOF

# mods/README.md public
cat > "$OUT/mods/README.md" <<'EOF'
# `mods/` — modules externes (optionnels)

Zer0v1 lance certains modules **dans une fenêtre séparée** depuis ce dossier :

| Commande | Attend | Techno |
|---|---|---|
| `/kristal` | `mods/zer0-kristal/zer0-kristal` | C++/Qt6 (lecteur audio) |
| `/streaming` | `mods/streaming/zero_video.py` | Python (libtorrent→mpv→pygame) |
| `/retro` | `mods/Zer0-Retr0/index.html` | HTML/JS (jeux) |

Ces programmes ne sont **pas fournis** ici (droits/licences propres, assets volumineux).
Si un module est absent, la commande affiche `introuvable` — le reste fonctionne.

Les modules **natifs** (`/radio`, `/bintime`, `/pong`) sont en Rust dans `src/fun/`.
EOF

# Genericise les chemins personnels dans la doc
sed -i 's#/home/[A-Za-z0-9_]*/\.local/share/zerochan#~/zerochan#g;
        s#/home/[A-Za-z0-9_]*/\.local/share/zer0#~/zer0#g' "$OUT"/*.md || true

echo "✅ gh0st/ régénéré : $OUT"
du -sh "$OUT"
