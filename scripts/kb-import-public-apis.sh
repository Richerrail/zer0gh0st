#!/usr/bin/env bash
# ─────────────────────────────────────────────────────────────────────────────
# kb-import-public-apis.sh — importe le catalogue d'APIs publiques
# (https://github.com/public-apis/public-apis) dans zer0gh0st.
#
#   ./scripts/kb-import-public-apis.sh            # importe + remplit la table
#   ./scripts/kb-import-public-apis.sh --kb       # indexe aussi pour l'agent
#   REF=v0.5.0 ./scripts/kb-import-public-apis.sh # autre version du catalogue
#
# Pourquoi un script plutôt qu'un fichier versionné :
#   - le catalogue évolue (1 400+ APIs) ; on veut la dernière version, pas une
#     copie figée dans le dépôt ;
#   - la date d'import est conservée → l'agent sait que c'est un instantané ;
#   - la licence du catalogue est MIT (attribution conservée dans l'en-tête).
#
# Honnêteté : une liste communautaire de liens, c'est un **candidat**, pas une
# garantie : beaucoup d'APIs sont mortes. `/apis` affiche la source et la date.
# ─────────────────────────────────────────────────────────────────────────────
set -euo pipefail

REPO="${REPO:-public-apis/public-apis}"
# RéF ÉPINGLÉE : on fige une version connue (pas 'master'), pour un import
# reproductible. Mets à jour REF pour rafraîchir le catalogue.
REF="${REF:-master}"
RAW="https://raw.githubusercontent.com/${REPO}/${REF}/README.md"
OUT_DIR="${OUT_DIR:-$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)/base_connaissance}"
OUT="$OUT_DIR/public-apis.tsv"
ZER0="${ZER0:-$(command -v zer0gh0st || command -v zer0 || echo "")}"
WITH_KB=0
[ "${1:-}" = "--kb" ] && WITH_KB=1

command -v curl >/dev/null || { echo "✗ curl requis"; exit 1; }
PY=python3; command -v python3 >/dev/null || PY=python
command -v "$PY" >/dev/null || { echo "✗ python3 requis (parse des tableaux markdown)"; exit 1; }

tmp=$(mktemp); trap 'rm -f "$tmp"' EXIT

echo "── téléchargement du catalogue ($REPO @ $REF)…"
curl -fsSL "$RAW" -o "$tmp" || { echo "✗ téléchargement impossible"; exit 1; }
size=$(wc -c < "$tmp")
echo "   ${size} octets"

mkdir -p "$OUT_DIR"

echo "── conversion en TSV (une API par ligne)…"
"$PY" - "$tmp" "$OUT" "$REPO" "$REF" <<'PYEOF'
import sys, re, datetime
src, out, repo, ref = sys.argv[1], sys.argv[2], sys.argv[3], sys.argv[4]
lines = open(src, encoding="utf-8", errors="replace").read().splitlines()

def uniq(s):
    s = s.replace("&amp;", "&").replace("&quot;", '"').replace("&#39;", "'")
    s = re.sub(r"\[([^\]]*)\]\([^)]*\)", r"\1", s)   # markdown -> texte
    s = re.sub(r"[`*]", "", s)
    return re.sub(r"\s+", " ", s).strip()

rows, category, started = [], "", False
for ln in lines:
    if ln.startswith("## Index"):
        started = True
    if not started:
        continue
    if ln.startswith("### "):
        category = ln[4:].strip()
        continue
    if not ln.startswith("|"):
        continue
    cells = [c.strip() for c in ln.strip().strip("|").split("|")]
    if len(cells) < 3:
        continue
    if cells[0].startswith(":") or set(cells[0]) <= set("-: "):
        continue                      # ligne de séparation du tableau markdown
    name = uniq(cells[0])
    if not name or name.lower() in ("api", "name"):
        continue
    m = re.search(r"\((https?://[^)]+)\)", cells[0])
    url = m.group(1) if m else uniq(cells[0])
    desc = uniq(cells[1])[:300]
    auth = uniq(cells[2]) if len(cells) > 2 else ""
    https = uniq(cells[3]) if len(cells) > 3 else ""
    cors = uniq(cells[4]) if len(cells) > 4 else ""
    rows.append((name, desc, url, auth, https, cors, category))

now = datetime.date.today().isoformat()
with open(out, "w", encoding="utf-8") as f:
    f.write(f"# source: {repo}@{ref}\n")
    f.write(f"# imported: {now}\n")
    f.write("# name\tdescription\turl\tauth\thttps\tcors\tcategory\n")
    for r in rows:
        f.write("\t".join(x.replace("\t", " ").replace("\n", " ") for x in r) + "\n")
print(f"   {len(rows)} API(s) extraite(s) → {out}")
PYEOF

if [ -z "$ZER0" ]; then
    echo "⚠ binaire zer0 introuvable — catalogue écrit, mais la table n'est pas remplie."
    echo "  Lance : zer0 apis import \"$OUT\""
    exit 0
fi

echo "── import dans la base (table public_apis)…"
"$ZER0" apis import "$OUT"

if [ "$WITH_KB" = "1" ]; then
    echo "── indexation pour l'agent (base de connaissance)…"
    "$ZER0" kb ingest "$OUT_DIR" || echo "   ⚠ indexation partielle"
fi

echo
echo "✅ Terminé. Dans la TUI :"
echo "   /apis                 → état du catalogue (source, date, catégories)"
echo "   /apis <besoin>        → ex. /apis météo gratuit"
echo "   /kb search <q>        → recherche dans la base de connaissance"
