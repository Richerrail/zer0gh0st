#!/usr/bin/env bash
# Installe les dépendances de Zer0│GH0ST selon la distribution.
#
#   ./scripts/install.sh            # build + audio + presse-papier + média
#   ./scripts/install.sh --minimal  # seulement de quoi compiler (gcc, git, rust)
#
# Idempotent : relançable sans risque. Utilise sudo si nécessaire.
set -euo pipefail

MINIMAL=0
[ "${1:-}" = "--minimal" ] && MINIMAL=1

SUDO=""
if [ "$(id -u)" -ne 0 ]; then
  command -v sudo >/dev/null && SUDO="sudo"
fi

. /etc/os-release 2>/dev/null || true
DISTRO="${ID:-unknown} ${ID_LIKE:-}"
echo "→ Distribution détectée : ${PRETTY_NAME:-$DISTRO}"

install_debian() {
  $SUDO apt-get update -y
  $SUDO apt-get install -y build-essential pkg-config git curl ca-certificates
  [ "$MINIMAL" -eq 0 ] && $SUDO apt-get install -y \
    pipewire-bin pulseaudio-utils alsa-utils wl-clipboard xclip mpv ffmpeg \
    python3 python3-pip
}
install_fedora() {
  $SUDO dnf install -y gcc gcc-c++ make pkgconf-pkg-config git curl
  [ "$MINIMAL" -eq 0 ] && $SUDO dnf install -y \
    pipewire-utils pulseaudio-utils alsa-utils wl-clipboard xclip mpv ffmpeg \
    python3 python3-pip
}
install_arch() {
  $SUDO pacman -Sy --needed --noconfirm base-devel pkgconf git curl
  [ "$MINIMAL" -eq 0 ] && $SUDO pacman -Sy --needed --noconfirm \
    pipewire pulseaudio-utils alsa-utils wl-clipboard xclip mpv ffmpeg \
    python python-pip
}
install_suse() {
  $SUDO zypper install -y gcc gcc-c++ make pkg-config git curl
  [ "$MINIMAL" -eq 0 ] && $SUDO zypper install -y \
    pipewire-tools pulseaudio-utils alsa-utils wl-clipboard xclip mpv ffmpeg \
    python3 python3-pip
}

case "$DISTRO" in
  *debian*|*ubuntu*|*kali*|*raspbian*|*mint*|*pop*) install_debian ;;
  *fedora*|*rhel*|*centos*|*rocky*|*almalinux*)   install_fedora ;;
  *arch*|*manjaro*|*endeavouros*|*garuda*)        install_arch ;;
  *suse*|*opensuse*)                              install_suse ;;
  *)
    echo "⚠ Distribution non reconnue ($DISTRO)."
    echo "  Installe manuellement : gcc/make, pkg-config, git, et (optionnel)"
    echo "  pipewire|pulseaudio, wl-clipboard|xclip, mpv, ffmpeg, python3+pip."
    ;;
esac

# Rust (rustup) si absent.
if ! command -v cargo >/dev/null; then
  echo "→ Installation de Rust (rustup)…"
  curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y
  # shellcheck disable=SC1091
  . "$HOME/.cargo/env"
fi
export PATH="$HOME/.cargo/bin:$PATH"

echo
echo "→ Compilation de Zer0 (release)…"
cargo build --release

echo
echo "✅ Terminé. Lance :"
echo "   ./target/release/zer0            # TUI"
echo "   ./target/release/zer0 --help"
[ "$MINIMAL" -eq 0 ] || echo "   (mode --minimal : voix/média/presse-papier non installés)"
