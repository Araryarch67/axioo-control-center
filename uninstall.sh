#!/usr/bin/env bash
# Bersihkan artefak AppImage axioo-control-center SAJA.
# Driver (DKMS quirk Studio X) dan source code TIDAK disentuh.
#
#   ./uninstall.sh
#
# Yang dihapus:
#   - ~/.local/share/axioo-control-center/*.AppImage (lokasi install setup.sh)
#   - ~/Applications/Axioo-Control-Center-*.AppImage (legacy; dulu dimakan
#     appimagelauncherd, tinggal sisa bila ada)
#   - ~/.local/share/applications/axioo-control-center.desktop
#   - ikon axioo-control-center (termasuk ekstraksi appimagekit_*)
#   - dist/*.AppImage + dist/AppDir
#
# Cache downloader (.tools/: linuxdeploy, appimagetool) SENGAJA disimpan
# biar build berikutnya tak download ulang. Hapus manual bila perlu.
set -euo pipefail

HERE="$(cd "$(dirname "$0")" && pwd)"
APPID="axioo-control-center"
shopt -s nullglob

rm_one() { # $1 = path/pattern-desc, $@ = files
    local desc="$1"; shift
    if [ "$#" -eq 0 ]; then
        echo "  - $desc: tidak ada, lewati"
    else
        for f in "$@"; do rm -rf "$f"; echo "  - hapus: $f"; done
    fi
}

echo "=== uninstall AppImage ($APPID) ==="

echo "[1/4] AppImage terinstall"
rm_one "AppImage ($HOME/.local/share/axioo-control-center)" \
    "$HOME"/.local/share/axioo-control-center/Axioo-Control-Center-*.AppImage
rm_one "legacy ~/Applications" "$HOME"/Applications/Axioo-Control-Center-*.AppImage
rmdir "$HOME/.local/share/axioo-control-center" 2>/dev/null || true

echo "[2/4] entri desktop + ikon"
rm_one "desktop entry" "$HOME/.local/share/applications/$APPID.desktop"
rm_one "ikon 256px" "$HOME/.local/share/icons/hicolor/256x256/apps/$APPID.png"
rm_one "ikon scalable" "$HOME/.local/share/icons/hicolor/scalable/apps/$APPID.svg"
rm_one "ikon ekstraksi AppImage" \
    "$HOME"/.local/share/icons/hicolor/256x256/apps/appimagekit_*_"$APPID".png \
    "$HOME"/.local/share/icons/hicolor/scalable/apps/appimagekit_*_"$APPID".svg
update-desktop-database "$HOME/.local/share/applications" 2>/dev/null || true
gtk-update-icon-cache -f "$HOME/.local/share/icons/hicolor" >/dev/null 2>&1 || true

echo "[3/4] dist/ (hasil build lokal)"
rm_one "AppImage di dist" "$HERE"/dist/Axioo-Control-Center-*.AppImage
rm_one "AppDir di dist" "$HERE"/dist/AppDir

echo "[4/4] axiood daemon (opsional, sudo)"
if systemctl is-active --quiet axiood 2>/dev/null; then sudo systemctl disable --now axiood || true; fi
sudo rm -f /usr/bin/axiood /etc/dbus-1/system.d/com.axioo.Control.conf /usr/share/polkit-1/actions/com.axioo.Control.policy /usr/lib/systemd/system/axiood.service /usr/lib/udev/rules.d/99-axioo-kbd.rules
sudo systemctl daemon-reload 2>/dev/null || true
sudo udevadm control --reload-rules 2>/dev/null || true
echo "  - daemon & udev dibersihkan bila ada"

echo "[5/4] sisa"
echo "  - ~/.local/bin/axioo-ctl DIBIARKAN (helper CLI; hapus manual bila tak perlu)"
echo "  - .tools/ DIBIARKAN (cache linuxdeploy/appimagetool)"
echo "  - driver DKMS TIDAK disentuh"

echo
echo "OK: artefak AppImage bersih."
