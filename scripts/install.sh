
set -euo pipefail

MINIMAL=0
INSTALL_BIN=0
for arg in "$@"; do
  case "$arg" in
    --minimal) MINIMAL=1 ;;
    --install) INSTALL_BIN=1 ;;
  esac
done

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

SUDO=""
if [ "$(id -u)" -ne 0 ]; then
  command -v sudo >/dev/null && SUDO="sudo"
fi

if [ -r /etc/os-release ]; then
  . /etc/os-release
  DISTRO="${ID:-unknown} ${ID_LIKE:-}"
  PRETTY_NAME="${PRETTY_NAME:-$DISTRO}"
else
  DISTRO="unknown"
  PRETTY_NAME="unknown"
fi
if [ "$(uname -s 2>/dev/null || echo unknown)" = "Darwin" ]; then
  DISTRO="darwin"
  PRETTY_NAME="macOS"
fi
echo "→ Distribution détectée : $PRETTY_NAME"

install_debian() {
  $SUDO apt-get update -y
  $SUDO apt-get install -y build-essential pkg-config git curl ca-certificates
  [ "$MINIMAL" -eq 0 ] && $SUDO apt-get install -y \
    pipewire-bin pulseaudio-utils alsa-utils wl-clipboard xclip mpv ffmpeg sox \
    python3 python3-pip python3-venv
}
install_fedora() {
  $SUDO dnf install -y gcc gcc-c++ make pkgconf-pkg-config git curl
  [ "$MINIMAL" -eq 0 ] && $SUDO dnf install -y \
    pipewire-utils pulseaudio-utils alsa-utils wl-clipboard xclip mpv ffmpeg sox \
    python3 python3-pip
}
install_arch() {
  $SUDO pacman -Sy --needed --noconfirm base-devel pkgconf git curl
  [ "$MINIMAL" -eq 0 ] && $SUDO pacman -Sy --needed --noconfirm \
    pipewire pulseaudio-utils alsa-utils wl-clipboard xclip mpv ffmpeg sox \
    python python-pip
}
install_suse() {
  $SUDO zypper install -y gcc gcc-c++ make pkg-config git curl
  [ "$MINIMAL" -eq 0 ] && $SUDO zypper install -y \
    pipewire-tools pulseaudio-utils alsa-utils wl-clipboard xclip mpv ffmpeg sox \
    python3 python3-pip python3-virtualenv
}
install_darwin() {
  if ! command -v brew >/dev/null; then
    echo "⚠ Homebrew est requis pour les dépendances macOS."
    echo "  Installe-le (une seule fois) :"
    echo '  /bin/bash -c "$(curl -fsSL https://raw.githubusercontent.com/Homebrew/install/HEAD/install.sh)"'
    echo "  Puis relance ce script. (Homebrew s'ajoute au PATH via ~/.zprofile.)"
    BREW_MISSING=1
    return
  fi
  [ "$MINIMAL" -eq 0 ] && brew install sox mpv ffmpeg python@3.12

  [ "$MINIMAL" -eq 0 ] && export PATH="$(brew --prefix)/opt/python@3.12/libexec/bin:$PATH"
}

case "$DISTRO" in
  *debian*|*ubuntu*|*kali*|*raspbian*|*mint*|*pop*) install_debian ;;
  *fedora*|*rhel*|*centos*|*rocky*|*almalinux*)   install_fedora ;;
  *arch*|*manjaro*|*endeavouros*|*garuda*)        install_arch ;;
  *suse*|*opensuse*)                              install_suse ;;
  *darwin*|*macos*)                               install_darwin ;;
  *)
    echo "⚠ Distribution non reconnue ($DISTRO)."
    echo "  Installe manuellement : gcc/make, pkg-config, git, et (optionnel)"
    echo "  pipewire|pulseaudio, wl-clipboard|xclip, mpv, ffmpeg, sox, python3+pip+venv."
    ;;
esac

if ! command -v cargo >/dev/null; then
  echo "→ Installation de Rust (rustup)…"
  curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y

  . "$HOME/.cargo/env"
fi
export PATH="$HOME/.cargo/bin:$PATH"

if [ "$MINIMAL" -eq 0 ]; then
  echo
  echo "→ Installation des prérequis Zero-chan (micro + Whisper local)…"
  bash "$SCRIPT_DIR/install-zerochan.sh"
fi

echo
echo "→ Compilation de Zer0 (release + passerelle zer0-gw)…"
cargo build --release --features gateway

if [ "$INSTALL_BIN" -eq 1 ]; then
  BIN="${HOME}/.local/bin"
  mkdir -p "$BIN"
  cp target/release/zer0 "$BIN/zer0"
  [ -f target/release/zer0-gw ] && cp target/release/zer0-gw "$BIN/zer0-gw"
  echo "✅ Binaires copiés dans $BIN (ajoute-le au PATH si besoin)."
fi

echo
echo "✅ Terminé. Lance :"
echo "   ./target/release/zer0            # TUI"
echo "   ./target/release/zer0 --help"
if [ -f target/release/zer0-gw ]; then
  echo "   ./target/release/zer0-gw         # passerelle locale (optionnelle)"
fi
[ "${BREW_MISSING:-0}" -eq 1 ] && echo "⚠ Homebrew manquait : relance ce script après l'avoir installé pour sox/mpv/ffmpeg/python."
[ "$MINIMAL" -eq 0 ] || echo "   (mode --minimal : voix/média/presse-papier non installés)"
