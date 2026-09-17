#!/usr/bin/env bash
# Setup end-to-end di mesin fresh (Arch, Pongo Studio X 2025):
#   1. base driver (clevo-drivers-dkms-git dari AUR)
#   2. patch quirk Studio X 0x17 (+ zona-4 numpad) + install DKMS
#   3. build release + AppImage
#   4. install AppImage ke ~/.local/share/axioo-control-center + entri desktop
#
# Jalankan TANPA sudo (sudo diminta di langkah yang perlu):
#   ./setup.sh              (progress bar default; detail di /tmp/axioo-setup.log)
#   ./setup.sh --verbose    (tampilkan semua output mentah)
#
# Pasangannya: ./uninstall.sh  (bersih total: daemon + AppImage + cache)
set -euo pipefail

QUIET=1
for a in "$@"; do
    case "$a" in
        -v|--verbose) QUIET=0 ;;
        -q|--quiet|--silent) QUIET=1 ;; # kompat: dulu opt-in, kini default
        -h|--help) echo "pakai: ./setup.sh [--verbose]"; exit 0 ;;
    esac
done
# setup.sh verbose → teruskan --verbose ke script.sh (build).
SCRIPT_ARGS=()
[ "$QUIET" = 0 ] && SCRIPT_ARGS=(--verbose)

HERE="$(cd "$(dirname "$0")" && pwd)"
APPID="axioo-control-center"

log() { [ "$QUIET" = 1 ] || echo "$@"; }
ok() { echo "$@"; }

# ---- progress-bar mode (dipakai bila --quiet) ----
TOTAL=7
LOGFILE="/tmp/axioo-setup.log"
bar() { # $1 = langkah selesai (0..TOTAL), $2 = label
    local done=$1 label=${2:-} width=28
    local fill=$(( done * width / TOTAL ))
    local empty=$(( width - fill ))
    local f e
    printf -v f '%*s' "$fill" ''; f=${f// /█}
    printf -v e '%*s' "$empty" ''; e=${e// /░}
    printf '\r\033[K[%s%s] %d/%d %s' "$f" "$e" "$done" "$TOTAL" "$label"
}
# Perintah diam (stdout/stderr → log) + spinner di baris progress.
# $1 = nomor langkah, $2 = label, sisanya = perintah.
quiet_run() {
    local step=$1 label=$2; shift 2
    local spin='|/-\' i=0 pid rc
    "$@" >>"$LOGFILE" 2>&1 & pid=$!
    while kill -0 "$pid" 2>/dev/null; do
        bar "$(( step - 1 ))" "$label ${spin:i%4:1}"
        i=$(( i + 1 )); sleep 0.15
    done
    wait "$pid"; rc=$?
    if [ "$rc" -ne 0 ]; then
        echo
        echo "GAGAL [$label] (rc=$rc) — buntut log:"
        tail -n 20 "$LOGFILE"
        exit "$rc"
    fi
    bar "$step" "$label ✓"; echo
}

need() { command -v "$1" >/dev/null 2>&1 || { echo "butuh '$1' — install dulu"; exit 1; }; }
need curl
need python3
need gcc
need make
command -v cargo >/dev/null 2>&1 || { echo "butuh rust (rustup.rs) — install dulu"; exit 1; }

if [ "$QUIET" = 1 ]; then
    : >"$LOGFILE"
    echo "axioo setup — progress (log: $LOGFILE)"
    sudo -v
fi

log "=== [0b/4] system deps (pacman, idempoten) ==="
# Prasyarat Tauri v2 di Arch (webkit/gtk/indikator) + build tools + DKMS.
# --needed = lewati yang sudah ada; rust TIDAK disentuh (pakai toolchain user).
if [ "$QUIET" = 1 ]; then
    quiet_run 1 "system deps" sudo pacman -S --needed --noconfirm base-devel git curl wget file python3 \
        python-pillow gcc make pkg-config dkms webkit2gtk-4.1 gtk3 libappindicator-gtk3 \
        librsvg openssl appmenu-gtk-module man-db
else
    sudo pacman -S --needed --noconfirm base-devel git curl wget file python3 \
        python-pillow gcc make pkg-config dkms webkit2gtk-4.1 gtk3 libappindicator-gtk3 \
        librsvg openssl appmenu-gtk-module man-db 2>&1 | tail -n 3 || true
fi

log "=== [0/4] cek AUR helper (buat driver clevo) ==="
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
[ "$QUIET" = 1 ] || echo "AUR: $AUR_METHOD"

log "=== [1/4] kernel headers ==="
if [ ! -d "/lib/modules/$(uname -r)/build" ]; then
    log "install linux-headers…"
    if [ "$QUIET" = 1 ]; then
        quiet_run 2 "kernel headers" sudo pacman -S --needed --noconfirm linux-headers
    else
        sudo pacman -S --needed --noconfirm linux-headers
    fi
elif [ "$QUIET" = 1 ]; then
    bar 2 "kernel headers (sudah ada) ✓"; echo
fi

log "=== [2/4] base driver clevo-drivers-dkms-git ($AUR_METHOD) ==="
case "$AUR_METHOD" in
    skip*) log "source driver sudah ada, lewati."; [ "$QUIET" = 1 ] && { bar 3 "base driver (sudah ada) ✓"; echo; } ;;
    yay) if [ "$QUIET" = 1 ]; then quiet_run 3 "base driver" yay -S --needed --noconfirm clevo-drivers-dkms-git; else yay -S --needed --noconfirm clevo-drivers-dkms-git; fi ;;
    paru) if [ "$QUIET" = 1 ]; then quiet_run 3 "base driver" paru -S --needed --noconfirm clevo-drivers-dkms-git; else paru -S --needed --noconfirm clevo-drivers-dkms-git; fi ;;
    manual*)
        log "tanpa yay/paru — clone + makepkg manual…"
        rm -rf /tmp/clevo-drivers-aur && git clone https://aur.archlinux.org/clevo-drivers-dkms-git.git /tmp/clevo-drivers-aur
        if [ "$QUIET" = 1 ]; then quiet_run 3 "base driver" bash -c 'cd /tmp/clevo-drivers-aur && makepkg -si --noconfirm'; else (cd /tmp/clevo-drivers-aur && makepkg -si --noconfirm); fi
        ;;
esac

log "=== [3/4] quirk Studio X + DKMS install (sudo) ==="
if [ "$QUIET" = 1 ]; then
    quiet_run 4 "quirk DKMS" sudo "$HERE/packaging/clevo-drivers-axioo/install.sh"
else
    sudo "$HERE/packaging/clevo-drivers-axioo/install.sh"
fi

log "=== [4/4] build + AppImage ==="
if [ "$QUIET" = 1 ]; then
    quiet_run 5 "build + AppImage" "$HERE/script.sh" "${SCRIPT_ARGS[@]}"
else
    "$HERE/script.sh" "${SCRIPT_ARGS[@]}"
fi
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
log "AppImage: $IMG"

log "=== [4b/4] axiood daemon + D-Bus + udev (sudo) ==="
# Verifikasi daemon benar jalan; gagal = error keras (bukan diam).
check_axiood() {
    if systemctl is-active --quiet axiood; then
        log "axiood: active"
        return 0
    fi
    echo "ERROR: axiood tidak jalan habis install:"
    systemctl status axiood --no-pager 2>&1 | head -n 15 || true
    journalctl -u axiood --no-pager -n 15 2>&1 | tail -n 15 || true
    return 1
}
_daemon_install_silent() {
    sudo install -Dm755 "$HERE/target/release/axiood" /usr/bin/axiood
    sudo install -Dm644 "$HERE/packaging/com.axioo.Control.conf" /etc/dbus-1/system.d/com.axioo.Control.conf
    sudo install -Dm644 "$HERE/packaging/com.axioo.Control.policy" /usr/share/polkit-1/actions/com.axioo.Control.policy
    # Hapus bayangan unit lama (install-system.sh dulu taruh di /etc yang
    # menimpa /usr/lib) agar unit baru pasti yang dipakai.
    sudo rm -f /etc/systemd/system/axiood.service
    sudo install -Dm644 "$HERE/packaging/axiood.service" /usr/lib/systemd/system/axiood.service
    sudo install -Dm644 "$HERE/packaging/udev/99-axioo-kbd.rules" /usr/lib/udev/rules.d/99-axioo-kbd.rules
    sudo udevadm control --reload-rules || true
    sudo systemctl daemon-reload
    sudo systemctl enable --now axiood
    # Default kipas = EC auto (instalasi lama perlu disetel eksplisit sekali).
    busctl --system call com.axioo.Control /com/axioo/Control com.axioo.Control SetFanEcAuto b true || true
    check_axiood
}
if [ -f "$HERE/target/release/axiood" ]; then
    log "install axiood + D-Bus config + systemd unit…"
    if [ "$QUIET" = 1 ]; then
        quiet_run 6 "daemon + EC auto" _daemon_install_silent
    else
        sudo install -Dm755 "$HERE/target/release/axiood" /usr/bin/axiood
        sudo install -Dm644 "$HERE/packaging/com.axioo.Control.conf" /etc/dbus-1/system.d/com.axioo.Control.conf
        sudo install -Dm644 "$HERE/packaging/com.axioo.Control.policy" /usr/share/polkit-1/actions/com.axioo.Control.policy
        # Hapus bayangan unit lama (lihat _daemon_install_silent).
        sudo rm -f /etc/systemd/system/axiood.service
        sudo install -Dm644 "$HERE/packaging/axiood.service" /usr/lib/systemd/system/axiood.service
        sudo install -Dm644 "$HERE/packaging/udev/99-axioo-kbd.rules" /usr/lib/udev/rules.d/99-axioo-kbd.rules
        sudo udevadm control --reload-rules 2>/dev/null || true
        sudo systemctl daemon-reload
        echo "enable & start axiood…"
        sudo systemctl enable --now axiood 2>&1 | head -n 20 || true
        check_axiood
    fi
    # Default kipas = EC auto: daemon baru default true + instalasi lama
    # disetel eksplisit di _daemon_install_silent (quiet) / di bawah (verbose).
    if [ "$QUIET" = 0 ]; then
        echo "fan default: EC auto…"
        busctl --system call com.axioo.Control /com/axioo/Control com.axioo.Control SetFanEcAuto b true 2>&1 | head -n 5 || true
    fi
fi

log "=== install AppImage + axioo-ctl ==="
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
# Wrapper autostart stabil (canonical: packaging/autostart/axioo-center-autostart).
# AppImageLauncher memindah/rename AppImage tiap integrate/update, jadi Exec
# .desktop TAK BOLEH hardcode path AppImage — wrapper resolve yang terbaru
# tiap login. Backend GUI (autostart_get/set) menulis Exec yang sama.
cp "$HERE/packaging/autostart/axioo-center-autostart" "$HOME/.local/bin/"
chmod +x "$HOME/.local/bin/axioo-center-autostart"
# Man page axioo-ctl (butuh man-db; sudah di system deps di atas).
sudo install -Dm644 "$HERE/packaging/man/axioo-ctl.1" /usr/share/man/man1/axioo-ctl.1
cp "$HERE/packaging/appimage/$APPID.desktop" "$HOME/.local/share/applications/"
# Exec = WRAPPER stabil TANPA --minimized (jendela tampil; instance kedua
# mati sendiri via single-instance + memunculkan jendela pertama).
# JANGAN hardcode path AppImage: AppImageLauncher memindah/rename file tiap
# integrate/update sehingga Exec basi → klik launcher tidak terjadi apa-apa.
# APPIMAGELAUNCHER_DISABLE=1 agar tak ada dialog "integrate?" dari launcher.
sed -i "s|^Exec=.*|Exec=env APPIMAGELAUNCHER_DISABLE=1 $HOME/.local/bin/axioo-center-autostart|" \
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
# aktif habis setup. Exec = WRAPPER stabil + --minimized (mulai di tray);
# klik launcher pakai wrapper TANPA flag (jendela tampil, Steam-like).
# Toggle di Settings / menu tray menulis SATU file yang sama (lihat
# autostart_get/set di tauri-app/src-tauri/src/main.rs).
mkdir -p "$HOME/.config/autostart"
AUTOSTART="$HOME/.config/autostart/axioo-center.desktop"
cat > "$AUTOSTART" <<EOF
[Desktop Entry]
Type=Application
Name=Axioo Control Center
Comment=Hardware control for Axioo (Clevo) laptops (start minimized to tray)
Exec=env APPIMAGELAUNCHER_DISABLE=1 $HOME/.local/bin/axioo-center-autostart --minimized
Icon=$APPID
Categories=System;Settings;HardwareSettings;
Terminal=false
X-GNOME-Autostart-enabled=true
StartupWMClass=$APPCLASS
EOF
update-desktop-database "$HOME/.local/share/applications" 2>/dev/null || true
gtk-update-icon-cache -f "$HOME/.local/share/icons/hicolor" >/dev/null 2>&1 || true
[ "$QUIET" = 1 ] && { bar 7 "app ✓"; echo; }

ask_reboot() {
    # Tanya reboot hanya bila ada terminal interaktif (dilewati di CI/pipe
    # tanpa tty). Bila stdin pipe tapi terminal ada (curl|bash), baca via
    # /dev/tty. Default Tidak — reboot tak pernah dipaksa.
    local ans=n
    echo "kamu perlu reboot agar driver DKMS + udev + daemon aktif bersih."
    if [ -t 0 ]; then
        printf 'reboot sekarang? (y/n) [n] '
        read -r ans || ans=n
    elif ( : </dev/tty ) 2>/dev/null; then
        printf 'reboot sekarang? (y/n) [n] '
        { read -r ans </dev/tty; } 2>/dev/null || ans=n
    else
        echo "reboot manual: sudo reboot"
        return
    fi
    case "$ans" in
        y|Y|ya|iya|yes) echo "reboot…"; sudo reboot ;;
        *) echo "OK — reboot manual nanti: sudo reboot" ;;
    esac
}

if [ "$QUIET" = 1 ]; then
    ok "OK: driver + app + axiood (fan EC auto) terinstall."
    ask_reboot
    exit 0
fi

echo
echo "OK semua:"
echo "  driver : ls /sys/class/leds/ | grep kbd   (mesti ada rgb:kbd_backlight*)"
echo "  app    : $APPIMG"
echo "  menu   : cari 'Axioo Control Center' di launcher"
echo "  tray   : login → mulai di tray (matikan via Settings / menu tray)"
echo "  REBOOT SEKALI: driver DKMS + udev + grup video + daemon baru aktif bersih"
echo "             habis reboot (wajib biar tombol Keyboard/Baterai tidak read-only)."
echo "  bersih : ./uninstall.sh  (hapus total: daemon + AppImage + cache)"
echo
ask_reboot
