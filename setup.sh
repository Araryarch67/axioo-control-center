#!/usr/bin/env bash
# Setup end-to-end di mesin fresh (Arch, Pongo Studio X 2025):
#   1. base driver (clevo-drivers-dkms-git dari AUR)
#   2. patch quirk Studio X 0x17 (+ zona-4 numpad) + install DKMS
#   3. build release + AppImage
#   4. install AppImage ke ~/.local/share/axioo-control-center + entri desktop
#
# Jalankan TANPA sudo (sudo diminta di langkah yang perlu):
#   ./setup.sh        (atau ./install.sh — alias yang sama)
#
# Pasangannya: ./uninstall.sh  (bersih total: daemon + AppImage + cache)
set -euo pipefail

HERE="$(cd "$(dirname "$0")" && pwd)"
APPID="axioo-control-center"

need() { command -v "$1" >/dev/null 2>&1 || { echo "butuh '$1' — install dulu"; exit 1; }; }
need curl
need python3
need gcc
need make
command -v cargo >/dev/null 2>&1 || { echo "butuh rust (rustup.rs) — install dulu"; exit 1; }

echo "=== [0b/4] system deps (pacman, idempoten) ==="
# Prasyarat Tauri v2 di Arch (webkit/gtk/indikator) + build tools + DKMS.
# --needed = lewati yang sudah ada; rust TIDAK disentuh (pakai toolchain user).
sudo pacman -S --needed --noconfirm base-devel git curl wget file python3 \
    python-pillow gcc make pkg-config dkms webkit2gtk-4.1 gtk3 libappindicator-gtk3 \
    librsvg openssl appmenu-gtk-module 2>&1 | tail -n 3 || true

echo "=== [0/4] cek AUR helper (buat driver clevo) ==="
if ls -d /usr/src/clevo-drivers-* >/dev/null 2>&1; then
    AUR_METHOD="skip (source driver sudah ada)"
elif command -v yay >/dev/null 2>&1; then
    AUR_METHOD="yay"
elif command -v paru >/dev/null 2>&1; then
    AUR_METHOD="paru"
elif command -v git >/dev/null 2>&1; then
    AUR_METHOD="manual (git clone + makepkg)"
else
    echo "tidak ada yay/paru/git — install salah satu dulu:"
    echo "  sudo pacman -S --needed git base-devel      (lalu setup.sh pakai makepkg manual)"
    echo "  atau install yay/paru dari AUR"
    exit 1
fi
echo "AUR: $AUR_METHOD"

echo "=== [1/4] kernel headers ==="
if [ ! -d "/lib/modules/$(uname -r)/build" ]; then
    echo "install linux-headers…"
    sudo pacman -S --needed --noconfirm linux-headers
fi

echo "=== [2/4] base driver clevo-drivers-dkms-git ($AUR_METHOD) ==="
case "$AUR_METHOD" in
    skip*) echo "source driver sudah ada, lewati." ;;
    yay) yay -S --needed --noconfirm clevo-drivers-dkms-git ;;
    paru) paru -S --needed --noconfirm clevo-drivers-dkms-git ;;
    manual*)
        echo "tanpa yay/paru — clone + makepkg manual…"
        rm -rf /tmp/clevo-drivers-aur && git clone https://aur.archlinux.org/clevo-drivers-dkms-git.git /tmp/clevo-drivers-aur
        (cd /tmp/clevo-drivers-aur && makepkg -si --noconfirm)
        ;;
esac

echo "=== [3/4] quirk Studio X + DKMS install (sudo) ==="
sudo "$HERE/packaging/clevo-drivers-axioo/install.sh"

echo "=== [4/4] build + AppImage ==="
"$HERE/script.sh"
# Bundle dir = cargo target dir workspace root ($HERE/target — BUKAN
# src-tauri/target; terbukti via `cargo metadata ... target_directory`).
# Nama gaya tauri v2: axioo-control-center_<ver>_amd64.AppImage; dist/
# hanya arsip lokal. Ambil yang terbaru dari semua lokasi yang mungkin.
shopt -s nullglob
CANDS=("$HERE"/target/release/bundle/appimage/*.AppImage \
        "$HERE"/tauri-app/src-tauri/target/release/bundle/appimage/*.AppImage \
        "$HERE"/dist/*.AppImage)
[ "${#CANDS[@]}" -gt 0 ] || { echo "ERROR: AppImage tidak ketemu (build gagal?)"; exit 1; }
IMG="$(ls -t "${CANDS[@]}" | head -1)"
# Arsipkan salinan ke dist/ agar uninstall.sh selalu bisa menelusur hasil build.
cp -f "$IMG" "$HERE/dist/" 2>/dev/null || true
IMG="$HERE/dist/$(basename "$IMG")"
[ -f "$IMG" ] || { echo "ERROR: gagal mengarsipkan $IMG"; exit 1; }
echo "AppImage: $IMG"

echo "=== [4b/4] axiood daemon + D-Bus + udev (sudo) ==="
if [ -f "$HERE/target/release/axiood" ]; then
    echo "install axiood + D-Bus config + systemd unit…"
    sudo install -Dm755 "$HERE/target/release/axiood" /usr/bin/axiood
    sudo install -Dm644 "$HERE/packaging/com.axioo.Control.conf" /etc/dbus-1/system.d/com.axioo.Control.conf
    sudo install -Dm644 "$HERE/packaging/com.axioo.Control.policy" /usr/share/polkit-1/actions/com.axioo.Control.policy
    sudo install -Dm644 "$HERE/packaging/axiood.service" /usr/lib/systemd/system/axiood.service
    sudo install -Dm644 "$HERE/packaging/udev/99-axioo-kbd.rules" /usr/lib/udev/rules.d/99-axioo-kbd.rules
    sudo udevadm control --reload-rules 2>/dev/null || true
    sudo systemctl daemon-reload
    echo "enable & start axiood…"
    sudo systemctl enable --now axiood 2>&1 | head -n 20 || true
    echo "axiood: $(systemctl is-active axiood 2>/dev/null || echo unknown)"
fi

echo "=== install AppImage + axioo-ctl ==="
# JANGAN taruh di ~/Applications: appimagelauncherd menganggap folder itu
# databasenya sendiri — AppImage yang dicopy manual di-rename (suffix md5),
# di-unintegrate, dan file desktop kita ikut dibersihkan. Lokasi di bawah
# ~/.local/share tak diawasi daemon sehingga instalasi manual awet.
DESTDIR="$HOME/.local/share/axioo-control-center"
mkdir -p "$DESTDIR" "$HOME/.local/bin" "$HOME/.local/share/applications" \
    "$HOME/.local/share/icons/hicolor/256x256/apps"
# Purge versi lama dulu biar tak ada dua AppImage menumpuk (uninstall.sh
# menghapus seluruh DESTDIR ini, jadi direktori ini murni milik installer).
rm -f "$DESTDIR"/*.AppImage
cp "$IMG" "$DESTDIR/"
APPIMG="$DESTDIR/$(basename "$IMG")"
chmod +x "$APPIMG"
# Helper host agar fallback `pkexec axioo-ctl fan ...` dari GUI AppImage bisa
# dieksekusi root (binary di dalam mount FUSE /tmp/.mount_* milik user tak
# bisa diakses root → selalu "Permission denied").
if [ -f "$HERE/target/release/axioo-ctl" ]; then
    cp "$HERE/target/release/axioo-ctl" "$HOME/.local/bin/"
    chmod +x "$HOME/.local/bin/axioo-ctl"
fi
cp "$HERE/packaging/appimage/$APPID.desktop" "$HOME/.local/share/applications/"
# APPIMAGELAUNCHER_DISABLE=1 agar AppImageLauncher tak memunculkan dialog
# "integrate?" saat app dijalankan dari luar ~/Applications.
sed -i "s|^Exec=.*|Exec=env APPIMAGELAUNCHER_DISABLE=1 \"$APPIMG\"|" \
    "$HOME/.local/share/applications/$APPID.desktop"
# StartupWMClass HARUS = stem nama file AppImage: app_id Wayland / WM_CLASS
# X11 diturunkan toolkit dari nama executable (terbukti via weston +
# WAYLAND_DEBUG: axioo-center → "axioo-center", rename → ikut berubah).
# Nama AppImage tauri memuat versi, jadi tulis dinamis saat install —
# nilai statis di .desktop template pasti basi dan merusak grouping
# dock/taskbar (KDE/GNOME/Hyprland).
APPCLASS="$(basename "$APPIMG" .AppImage)"
sed -i "s|^StartupWMClass=.*|StartupWMClass=$APPCLASS|" \
    "$HOME/.local/share/applications/$APPID.desktop"
cp "$HERE/dist/icon.png" "$HOME/.local/share/icons/hicolor/256x256/apps/$APPID.png"
# dist/icon.png dibuat dari icons/android-chrome-512x512.png
# (tauri icon → tauri-app/src-tauri/icons/* untuk bundle AppImage;
#  resize 256 via PIL untuk hicolor launcher). Regenerasi bila logo ganti.
# Autostart login: .desktop mandiri (bukan via toggle in-app) agar langsung
# aktif habis setup; Exec = AppImage terinstall + --minimized (mulai di tray).
# Toggle di Settings / menu tray tetap bisa mematikan lagi (satu file yang sama).
mkdir -p "$HOME/.config/autostart"
AUTOSTART="$HOME/.config/autostart/axioo-center.desktop"
cat > "$AUTOSTART" <<EOF
[Desktop Entry]
Type=Application
Name=Axioo Control Center
Comment=Hardware control for Axioo (Clevo) laptops (start minimized to tray)
Exec="$APPIMG" --minimized
Icon=$APPID
Categories=System;Settings;HardwareSettings;
Terminal=false
StartupWMClass=$APPCLASS
EOF
update-desktop-database "$HOME/.local/share/applications" 2>/dev/null || true
gtk-update-icon-cache -f "$HOME/.local/share/icons/hicolor" >/dev/null 2>&1 || true

echo
echo "OK semua:"
echo "  driver : ls /sys/class/leds/ | grep kbd   (mesti ada rgb:kbd_backlight*)"
echo "  app    : $APPIMG"
echo "  menu   : cari 'Axioo Control Center' di launcher"
echo "  tray   : login → mulai di tray (matikan via Settings / menu tray)"
echo "  REBOOT SEKALI: driver DKMS + udev + grup video + daemon baru aktif bersih"
echo "             habis reboot (wajib biar tombol Keyboard/Baterai tidak read-only)."
echo "  bersih : ./uninstall.sh  (hapus total: daemon + AppImage + cache)"
