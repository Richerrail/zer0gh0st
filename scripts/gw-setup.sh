#!/usr/bin/env bash
# ─────────────────────────────────────────────────────────────────────────────
# gw-setup.sh — découpage de sécurité ZER0│GH0ST (Linux, à exécuter en root)
#
# But : la clé n'existe plus dans le processus agent.
#   1. utilisateurs dédiés : zer0-gw (clés) / zer0 (agent)
#   2. binaire zer0-gw installé + service durci
#   3. règles nftables : zer0 → loopback uniquement ; zer0-gw → 443 sortant
#
# Après l'installation, éditez /etc/zer0-gw/gw.toml et /etc/zer0-gw/keys.env
# puis retirez les clés de ~/.config/zer0/config.toml côté agent.
#
# Notes :
#   - Le build est fait EN TANT QU'UTILISATEUR INVOQUANT (jamais root) :
#     sous sudo, $HOME vaut /root et le rustup de root n'a souvent pas de
#     toolchain par défaut — et un build root laisserait target/ à root.
#     RUSTUP_HOME/CARGO_HOME personnalisés : exportez-les avant le script.
#   - Rebuild automatique si les sources sont plus récentes que le binaire.
#   - FORCE_BUILD=1 sudo scripts/gw-setup.sh  → reconstruit dans tous les cas.
# ─────────────────────────────────────────────────────────────────────────────
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
BIN_DIR="${BIN_DIR:-/usr/local/bin}"
ETC_DIR="${ETC_DIR:-/etc/zer0-gw}"
BIN_SRC="$ROOT_DIR/target/release/zer0-gw"
INVOKER="${SUDO_USER:-}"

[ "$(id -u)" -eq 0 ] || { echo "✗ à exécuter en root (sudo $0)"; exit 1; }

echo "── 1/5  Utilisateurs dédiés"
id zer0-gw &>/dev/null || useradd -r -s /usr/sbin/nologin -d /var/lib/zer0-gw -m zer0-gw
id zer0    &>/dev/null || useradd -m -s /bin/bash zer0

echo "── 2/5  Binaire zer0-gw"
echo "   → dépôt : $ROOT_DIR"
echo "   → binaire source attendu : $BIN_SRC"

# Construit en tant qu'utilisateur invoquant (avec SON rustup), jamais root.
build_as_invoker() {
    [ -n "$INVOKER" ] && [ "$INVOKER" != "root" ] || return 1
    local user_home
    user_home="$(getent passwd "$INVOKER" | cut -d: -f6)"
    [ -n "$user_home" ] || return 1
    [ -x "$user_home/.cargo/bin/cargo" ] || return 1

    echo "   → build en tant que $INVOKER (rustup : $user_home/.rustup)"
    sudo -u "$INVOKER" \
        env HOME="$user_home" \
            RUSTUP_HOME="${RUSTUP_HOME:-$user_home/.rustup}" \
            CARGO_HOME="${CARGO_HOME:-$user_home/.cargo}" \
            PATH="$user_home/.cargo/bin:/usr/local/bin:/usr/bin:/bin" \
        cargo build --release --features gateway --manifest-path "$ROOT_DIR/Cargo.toml"
}

# Repli : cargo système utilisable par root (rustup configuré pour root, ou binaire autonome).
build_as_root() {
    command -v cargo &>/dev/null || return 1
    cargo --version &>/dev/null || return 1   # un shim rustup sans défaut échoue ici
    echo "   → build en tant que root (cargo système)"
    ( cd "$ROOT_DIR" && cargo build --release --features gateway )
}

needs_build() {
    [ "${FORCE_BUILD:-0}" = "1" ] && return 0
    [ -x "$BIN_SRC" ] || return 0
    # Sources plus récentes que le binaire → reconstruire (évite d'installer
    # une passerelle périmée, sans endpoints admin par exemple).
    find "$ROOT_DIR/src" "$ROOT_DIR/Cargo.toml" -newer "$BIN_SRC" -print -quit 2>/dev/null | grep -q .
}

if needs_build; then
    if ! out="$(build_as_invoker 2>&1)"; then
        echo "$out" | tail -5 || true
        if ! out2="$(build_as_root 2>&1)"; then
            echo "$out2" | tail -5 || true
            echo "✗ pas de toolchain utilisable."
            echo "  Le plus simple : construire avec TON utilisateur puis relancer :"
            echo "    cargo build --release --features gateway"
            echo "    sudo scripts/gw-setup.sh"
            exit 1
        fi
    fi
else
    echo "   → binaire à jour : $BIN_SRC"
fi

[ -x "$BIN_SRC" ] || { echo "✗ binaire introuvable : $BIN_SRC"; exit 1; }
echo "   → source : $(stat -c%s "$BIN_SRC") o · $(date -r "$BIN_SRC" '+%d/%m %H:%M') · sha256 $(sha256sum "$BIN_SRC" | cut -c1-12)"
install -m 0755 "$BIN_SRC" "$BIN_DIR/zer0-gw"
# Vérification : le fichier installé doit être identique à la source.
if ! cmp -s "$BIN_SRC" "$BIN_DIR/zer0-gw"; then
    echo "✗ l'installation ne correspond pas à $BIN_SRC — abandon"
    exit 1
fi
echo "   → installé : $BIN_DIR/zer0-gw · $(stat -c%s "$BIN_DIR/zer0-gw") o · sha256 $(sha256sum "$BIN_DIR/zer0-gw" | cut -c1-12)"

echo "── 3/5  Configuration /etc/zer0-gw"
install -d -m 1770 -o root -g zer0-gw "$ETC_DIR"
# Corrige des droits hérités : le groupe zer0-gw doit pouvoir ÉCRIRE les clés
# déposées (/provider add) — sticky pour protéger gw.toml des suppressions.
chown root:zer0-gw "$ETC_DIR"
chmod 1770 "$ETC_DIR"
if [ ! -f "$ETC_DIR/gw.toml" ]; then
    install -m 0640 -o zer0-gw -g zer0-gw "$ROOT_DIR/deploy/gw.example.toml" "$ETC_DIR/gw.toml"
    echo "   → éditez $ETC_DIR/gw.toml (providers + clés)"
else
    echo "   → gw.toml déjà présent, non modifié"
fi
# keys.env : clés « style variable d'env » (OPENROUTER_API_KEY=…) injectées par
# systemd — à éditer à la main si tu n'utilises pas /provider add.
if [ ! -f "$ETC_DIR/keys.env" ]; then
    install -m 0600 -o zer0-gw -g zer0-gw /dev/null "$ETC_DIR/keys.env"
fi
chown zer0-gw:zer0-gw "$ETC_DIR/keys.env" && chmod 0600 "$ETC_DIR/keys.env"
# keys.managed : clés déposées via /provider add — écrit par la passerelle,
# jamais lisible par l'agent (utilisateur zer0).
if [ ! -f "$ETC_DIR/keys.managed" ]; then
    install -m 0600 -o zer0-gw -g zer0-gw /dev/null "$ETC_DIR/keys.managed"
fi
chown zer0-gw:zer0-gw "$ETC_DIR/keys.managed" && chmod 0600 "$ETC_DIR/keys.managed"
echo "   → $ETC_DIR/keys.managed : géré par zer0-gw (dépôt via /provider add dans le TUI)"

# Ajoute à gw.toml les [providers.*] de l'exemple encore absents (ex. zai),
# sans toucher à ceux déjà présents ni au reste du fichier.
if command -v python3 >/dev/null && [ -f "$ROOT_DIR/deploy/gw.example.toml" ]; then
    python3 - "$ROOT_DIR/deploy/gw.example.toml" "$ETC_DIR/gw.toml" <<'PYEOF'
import re, sys

example, dst = sys.argv[1], sys.argv[2]
try:
    ex = open(example, encoding="utf-8").read()
    cur = open(dst, encoding="utf-8").read()
except OSError:
    sys.exit(0)

blocks = re.findall(r"(?ms)(^\[providers\.([^\]\n]+)\].*?)(?=^\[|\Z)", ex)
missing = [(b.strip(), n) for b, n in blocks if f"[providers.{n}]" not in cur]
if missing:
    with open(dst, "a", encoding="utf-8") as f:
        f.write("\n" + "\n\n".join(b for b, _ in missing) + "\n")
    print("   → providers ajoutés à gw.toml : " + ", ".join(n for _, n in missing))
PYEOF
fi

echo "── 4/5  Services systemd"
install -m 0644 "$ROOT_DIR/deploy/zer0-gw.service"   /etc/systemd/system/
install -m 0644 "$ROOT_DIR/deploy/zer0-agent.service" /etc/systemd/system/
systemctl daemon-reload
systemctl enable zer0-gw
# Redémarrer même si le service tourne déjà : sinon un ANCIEN binaire
# (sans endpoints admin) reste en mémoire après une mise à jour.
systemctl restart zer0-gw
systemctl status zer0-gw --no-pager | head -5 || true

# Auto-contrôle : le service doit exposer les endpoints admin. Sinon un ANCIEN
# binaire tourne encore (le process garde l'ancien exécutable en mémoire).
if command -v curl >/dev/null; then
    # Le service vient d'être redémarré : attendre qu'il ouvre son port (≤ 6 s),
    # sinon on lit un vide et on croit à tort que l'ancien binaire tourne.
    health=""
    i=0
    while [ "$i" -lt 15 ]; do
        i=$((i + 1))
        health="$(curl -s -m 2 http://127.0.0.1:8787/health || true)"
        case "$health" in *'"ok":true'*) break ;; esac
        sleep 0.4
    done
    case "$health" in
        *'"ok":true'*)
            echo "   ✓ /health répond — passerelle à jour (endpoints admin présents)" ;;
        *)
            echo "   ⚠ /health sans réponse après 6 s : '$health'"
            echo "     → service en échec ? (systemctl status zer0-gw ; journalctl -u zer0-gw -n 20)" ;;
    esac
fi

echo "── 5/5  Egress nftables (non persistant — ajoutez à votre ruleset)"
nft -f "$ROOT_DIR/deploy/20-zer0-egress.nft" && echo "   → règles chargées" \
    || echo "   ⚠ nft a échoué (conteneur ?) — chargez les règles sur l'hôte"

cat <<'FIN'

✅ Terminé.

Côté agent : plus aucune clé à saisir dans ~/.config/zer0/config.toml.
Dans le TUI :

   /provider add openrouter sk-or-…   → clé déposée sur zer0-gw (jamais ici)
   /provider keys                     → état des clés détenues par la passerelle
   /provider remove openrouter        → oublie la clé (passerelle + local)

Vérifications :
   sudo -u zer0 curl -m 3 https://example.com    # → refusé (egress)
   sudo -u zer0 curl -m 3 http://127.0.0.1:8787  # → OK (loopback)
   journalctl -u zer0-gw -f                      # journal JSON (sans clés)

Limites (SAFETY.md §7) : l'agent peut encore utiliser le relais (quota) —
mettez des plafonds de dépense côté provider.
FIN
