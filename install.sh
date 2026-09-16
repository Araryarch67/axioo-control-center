#!/usr/bin/env bash
# install.sh — auto-install Axioo Control Center (satu perintah).
#
#   curl -fsSL https://raw.githubusercontent.com/Araryarch67/axioo-control-center/main/install.sh | bash
#
# Dua mode:
#   1. Berdiri sendiri (hasil curl / file lepas): git clone repo ke
#      direktori sementara → jalankan setup.sh → hapus clone.
#   2. Di dalam checkout repo (ada setup.sh di sebelah file ini):
#      teruskan semua argumen ke ./setup.sh.
#
# Opsi diteruskan ke setup.sh: --verbose (output mentah, default progress bar).
# Habis install repo boleh dihapus — uninstaller tersimpan di
# ~/.local/share/axioo-control-center/uninstall.sh.
set -euo pipefail

REPO_URL="https://github.com/Araryarch67/axioo-control-center.git"
HERE="$(cd "$(dirname "$0")" 2>/dev/null && pwd)" || HERE=""

# Mode 2 (checkout): setup.sh ada di sebelah file ini.
if [ -n "$HERE" ] && [ -f "$HERE/setup.sh" ]; then
    exec "$HERE/setup.sh" "$@"
fi

# Mode 1 (bootstrap via curl): clone → setup → bersih.
command -v git >/dev/null 2>&1 || { echo "butuh 'git' — install dulu (sudo pacman -S git)"; exit 1; }

WORK="$(mktemp -d /tmp/axioo-setup.XXXXXX)"
cleanup() { rm -rf "$WORK"; }
trap cleanup EXIT

echo "clone $REPO_URL …"
git clone --depth 1 "$REPO_URL" "$WORK/repo"
"$WORK/repo/setup.sh" "$@"
echo "OK — clone sementara dihapus; uninstall: ~/.local/share/axioo-control-center/uninstall.sh"
