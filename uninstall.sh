#!/usr/bin/env bash
# uninstall.sh — uninstall TOTAL axioo-control-center, tanpa kecuali.
# Default = semuanya dihapus: daemon, aplikasi, cache, build cache, driver
# DKMS custom. Tidak perlu flag apa pun:
#
#   ./uninstall.sh
#
# Opt-out (bila memang mau menyisakan sesuatu):
#   KEEP_CACHE=1 ./uninstall.sh   pertahankan .tools/ (cache downloader
#                                 linuxdeploy/appimagetool biar build cepat)
#   KEEP_DRIVER=1 ./uninstall.sh  pertahankan driver DKMS custom
#                                 (backlight keyboard tetap jalan)
#
# Yang dibersihkan:
#   A. Daemon axiood  : stop + disable service, kill sisa proses, hapus binary
#      (/usr/bin + /usr/local/bin), unit systemd (/etc + /usr/lib + /run),
#      conf D-Bus system, policy polkit, udev rules (/etc + /usr/lib);
#      lalu daemon-reload + reload/trigger udev (+ reload dbus bila bisa).
#   B. Aplikasi (user): seluruh ~/.local/share/axioo-control-center,
#      sisa ~/Applications (legacy), ~/.local/bin/axioo-ctl, desktop entry
#      *axioo*, ikon *axioo*, data/cache/config aplikasi
#      (com.axioo.ControlCenter); refresh desktop-db + icon cache.
#   C. Repo            : dist/*.AppImage + *.zsync + AppDir + icon.png
#      (generate ulang via script.sh), .tools/ (kecuali KEEP_CACHE=1),
#      snapshot packaging/per-key-capture/captures, cache vite .vite/.
#   D. Build cache     : target/, tauri-app/src-tauri/target,
#      tauri-app/node_modules, tauri-app/dist (output vite).
#   E. Driver DKMS     : copot tuxedo-drivers-axioo (SEMUA versi) + source
#      /usr/src/tuxedo-drivers-axioo-*, lalu pasang ulang driver bawaan
#      clevo-drivers dari source AUR bila masih ada (kecuali KEEP_DRIVER=1).
#
# SENGAJA TIDAK disentuh: paket sistem linux-headers, paket AUR
# clevo-drivers-dkms-git (+ source /usr/src/clevo-drivers-*), dan source
# code repo ini. Butuh sudo untuk bagian A + E (password diminta sekali).
set -euo pipefail

HERE="$(cd "$(dirname "$0")" && pwd)"
APPID="axioo-control-center"
TAURI_ID="com.axioo.ControlCenter"
LEFTOVER=0

for a in "$@"; do
    case "$a" in
        -h|--help)
            sed -n '2,/^set /p' "$0" | grep -v '^set ' | sed 's/^# \{0,1\}//'
            exit 0 ;;
        *) echo "opsi tak dikenal: $a (script ini tanpa opsi aksi — langsung hapus semua; pakai --help)"; exit 2 ;;
    esac
done

if [ "$(id -u)" -eq 0 ]; then SUDO=""; else SUDO="sudo"; fi
# sudo WAJIB (bagian A daemon + E driver) — minta password SEKALI di awal
# + jaga timestamp agar tidak prompt di tengah jalan. Ditaruh setelah
# parse argumen supaya --help tetap tanpa password.
if [ -n "$SUDO" ]; then
    $SUDO -v || { say "butuh sudo — batal."; exit 1; }
    # & langsung (tanpa subshell) agar $! terisi di shell ini.
    while true; do sleep 45; $SUDO -n -v 2>/dev/null || break; done &
    KEEPID=$!
    trap 'kill $KEEPID 2>/dev/null || true' EXIT
fi
shopt -s nullglob

say()  { printf '%s\n' "$*"; }
rm_one() { # $1 = deskripsi, $@ = files/glob (boleh kosong)
    local desc="$1"; shift
    if [ "$#" -eq 0 ]; then say "  - $desc: tidak ada, lewati"; return; fi
    local f
    for f in "$@"; do
        if [ -e "$f" ] || [ -L "$f" ]; then rm -rf "$f" || true; say "  - hapus: $f";
        else say "  - tidak ada: $f"; fi
    done
}
rm_root() { # seperti rm_one tapi via $SUDO
    local desc="$1"; shift
    if [ "$#" -eq 0 ]; then say "  - $desc: tidak ada, lewati"; return; fi
    local f
    for f in "$@"; do
        if $SUDO test -e "$f" || $SUDO test -L "$f" 2>/dev/null; then
            $SUDO rm -rf "$f" || true; say "  - hapus: $f"
        else say "  - tidak ada: $f"; fi
    done
}
have() { command -v "$1" >/dev/null 2>&1; }
check() { # $1 = deskripsi, $2.. = path yang HARUS sudah hilang
    local desc="$1"; shift
    local bad=() f
    for f in "$@"; do [ -e "$f" ] && bad+=("$f"); done
    if [ "${#bad[@]}" -eq 0 ]; then say "  OK  $desc";
    else LEFTOVER=1; for f in "${bad[@]}"; do say "  SISA $desc: $f"; done; fi
}

say "=== uninstall total ($APPID) ==="

# ---------- A. daemon axiood ----------
say "[A] daemon axiood"
if have systemctl; then
    if systemctl is-active --quiet axiood 2>/dev/null \
        || systemctl is-enabled --quiet axiood 2>/dev/null \
        || systemctl list-unit-files axiood.service 2>/dev/null | grep -q '^axiood\.service'; then
        $SUDO systemctl disable --now axiood 2>/dev/null || true
        say "  - service axiood di-stop + disable"
    else
        say "  - service axiood: tidak terdaftar, lewati"
    fi
else
    say "  - systemctl tak ada, lewati stop service"
fi
if have pkill && pgrep -x axiood >/dev/null 2>&1; then
    $SUDO pkill -x axiood 2>/dev/null || true
    sleep 1
    pgrep -x axiood >/dev/null 2>&1 && $SUDO pkill -9 -x axiood 2>/dev/null || true
    say "  - sisa proses axiood di-kill"
fi
# Binary + unit + bus + polkit + udev — sikat SEMUA lokasi yang pernah dipakai
# installer mana pun (setup.sh → /usr/lib, install-system.sh → /etc).
rm_root "binary axiood" /usr/bin/axiood /usr/local/bin/axiood
rm_root "unit systemd" /etc/systemd/system/axiood.service \
    /usr/lib/systemd/system/axiood.service /run/systemd/system/axiood.service
rm_root "conf D-Bus" /etc/dbus-1/system.d/com.axioo.Control.conf
rm_root "policy polkit" /usr/share/polkit-1/actions/com.axioo.Control.policy \
    /etc/polkit-1/actions/com.axioo.Control.policy
rm_root "udev rules" /etc/udev/rules.d/99-axioo-kbd.rules \
    /usr/lib/udev/rules.d/99-axioo-kbd.rules
if have systemctl; then
    $SUDO systemctl daemon-reload 2>/dev/null || true
    $SUDO systemctl reset-failed axiood 2>/dev/null || true
    $SUDO systemctl reload dbus 2>/dev/null || true
fi
if have udevadm; then
    $SUDO udevadm control --reload-rules 2>/dev/null || true
    $SUDO udevadm trigger --subsystem-match=leds --action=add 2>/dev/null || true
    $SUDO udevadm trigger --subsystem-match=power_supply --action=add 2>/dev/null || true
fi
say "  - systemd + udev di-reload"

# ---------- B. aplikasi (user) ----------
say "[B] aplikasi (user $HOME)"
rm_one "dir install AppImage" "$HOME/.local/share/$APPID"
rm_one "legacy ~/Applications" "$HOME"/Applications/Axioo-Control-Center-*.AppImage \
    "$HOME"/Applications/axioo-control-center-*.AppImage "$HOME"/Applications/*xioo*.AppImage
rm_one "helper CLI" "$HOME/.local/bin/axioo-ctl"
rm_one "desktop entry" "$HOME/.local/share/applications/$APPID.desktop"
rm_one "autostart login" "$HOME"/.config/autostart/*xioo*.desktop
if [ -d "$HOME/.local/share/applications" ]; then
    # Sisa integrasi AppImageLauncher / salinan manual bernada axioo.
    while IFS= read -r -d '' f; do rm -f "$f"; say "  - hapus: $f"; done \
        < <(find "$HOME/.local/share/applications" -maxdepth 1 -iname '*axioo*.desktop' -print0 2>/dev/null || true)
fi
if [ -d "$HOME/.local/share/icons" ]; then
    while IFS= read -r -d '' f; do rm -f "$f"; say "  - hapus: $f"; done \
        < <(find "$HOME/.local/share/icons" \( -iname "*axioo*.png" -o -iname "*axioo*.svg" \) -print0 2>/dev/null || true)
fi
rm_one "data aplikasi" "$HOME/.local/share/$TAURI_ID"
rm_one "cache aplikasi" "$HOME/.cache/$TAURI_ID"
rm_one "config aplikasi" "$HOME/.config/$TAURI_ID"
if have update-desktop-database; then
    update-desktop-database "$HOME/.local/share/applications" 2>/dev/null || true
fi
if have gtk-update-icon-cache; then
    gtk-update-icon-cache -f "$HOME/.local/share/icons/hicolor" >/dev/null 2>&1 || true
fi
say "  - desktop-db + icon cache di-refresh"

# ---------- C. artefak repo + cache downloader ----------
say "[C] artefak repo"
rm_one "AppImage/zsync di dist" "$HERE"/dist/*.AppImage "$HERE"/dist/*.zsync
rm_one "ikon hicolor di dist (generate ulang via script.sh)" "$HERE"/dist/icon.png
rm_one "AppDir di dist" "$HERE"/dist/AppDir
rm_one "snapshot capture-ec" "$HERE"/packaging/per-key-capture/captures
rm_one "cache vite di root" "$HERE"/.vite
if [ "${KEEP_CACHE:-0}" = "1" ]; then
    say "  - .tools/ DIPERTAHANKAN (KEEP_CACHE=1)"
else
    rm_one "cache downloader .tools" "$HERE"/.tools
fi

# ---------- D. build cache ----------
say "[D] build cache"
rm_one "cargo target" "$HERE"/target
rm_one "tauri bundle output" "$HERE"/tauri-app/src-tauri/target
rm_one "node_modules" "$HERE"/tauri-app/node_modules
rm_one "output vite" "$HERE"/tauri-app/dist

# ---------- E. driver DKMS custom ----------
if [ "${KEEP_DRIVER:-0}" = "1" ]; then
    say "[E] driver DKMS: DIPERTAHANKAN (KEEP_DRIVER=1)"
else
    say "[E] driver DKMS custom"
    if have dkms; then
        while IFS= read -r line; do
            # baris "tuxedo-drivers-axioo/4.20.1, ..." → "tuxedo-drivers-axioo/4.20.1"
            mod="$(printf '%s' "$line" | awk -F',' '{print $1}' | tr -d ' ')"
            if [ -n "$mod" ]; then
                $SUDO dkms remove "$mod" --all 2>/dev/null || true
                say "  - dkms remove: $mod"
            fi
        done < <(dkms status 2>/dev/null | grep '^tuxedo-drivers-axioo/' || true)
    else
        say "  - dkms tak ada, lewati remove"
    fi
    rm_root "source DKMS custom" /usr/src/tuxedo-drivers-axioo-*
    # Kembalikan driver bawaan AUR bila sourcenya masih ada (kebalikan
    # packaging/clevo-drivers-axioo/install.sh yang menonaktifkannya).
    RESTORED=0
    for src in /usr/src/clevo-drivers-*/; do
        [ -d "$src" ] || continue
        base="$(basename "$src")"          # clevo-drivers-4.20.1
        mod="${base%-*}" ver="${base##*-}" # clevo-drivers / 4.20.1
        if [ -f "$src/dkms.conf" ]; then
            say "  - pasang ulang bawaan: $mod/$ver (kernel aktif)"
            # TANPA --all: dkms-3.x menolak `install module/ver --all`
            # ("The action install does not support the --all parameter").
            $SUDO dkms install "$mod/$ver" 2>&1 | tail -n 3 || true
            RESTORED=1
        fi
    done
    if have modprobe; then
        $SUDO modprobe -r tuxedo_keyboard clevo_acpi clevo_wmi tuxedo_io 2>/dev/null || true
        $SUDO modprobe tuxedo_keyboard 2>/dev/null || true
        $SUDO modprobe clevo_acpi clevo_wmi 2>/dev/null || true
        say "  - modul input/acpi dimuat ulang (bila tersedia)"
    fi
    if [ "$RESTORED" -eq 0 ]; then
        say "  ! source clevo-drivers-* tak ada — driver keyboard hilang sampai"
        say "    install ulang (./setup.sh) atau reboot + reinstall driver. Bila ragu, reboot."
    fi
fi

# ---------- verifikasi ----------
say
say "=== verifikasi ==="
if have systemctl; then
    if systemctl is-active --quiet axiood 2>/dev/null; then LEFTOVER=1; say "  SISA service: axiood masih ACTIVE";
    else say "  OK  service axiood tidak jalan"; fi
    if systemctl is-enabled --quiet axiood 2>/dev/null; then LEFTOVER=1; say "  SISA service: axiood masih ENABLED";
    else say "  OK  service axiood tidak enabled"; fi
fi
if have pgrep && pgrep -x axiood >/dev/null 2>&1; then LEFTOVER=1; say "  SISA proses axiood masih hidup";
else say "  OK  tidak ada proses axiood"; fi
check "file daemon" /usr/bin/axiood /usr/local/bin/axiood \
    /etc/systemd/system/axiood.service /usr/lib/systemd/system/axiood.service \
    /etc/dbus-1/system.d/com.axioo.Control.conf \
    /usr/share/polkit-1/actions/com.axioo.Control.policy \
    /etc/udev/rules.d/99-axioo-kbd.rules /usr/lib/udev/rules.d/99-axioo-kbd.rules
check "file aplikasi" "$HOME/.local/share/$APPID" "$HOME/.local/bin/axioo-ctl" \
    "$HOME/.local/share/applications/$APPID.desktop" \
    "$HOME/.local/share/$TAURI_ID" "$HOME/.cache/$TAURI_ID" "$HOME/.config/$TAURI_ID"
left_auto="$(find "$HOME/.config/autostart" -maxdepth 1 -iname '*xioo*.desktop' 2>/dev/null || true)"
if [ -z "$left_auto" ]; then say "  OK  autostart login bersih";
else LEFTOVER=1; say "  SISA autostart:"; say "$left_auto" | sed 's/^/    /'; fi
left_icons="$(find "$HOME/.local/share/icons" -iname '*axioo*' 2>/dev/null || true)"
if [ -z "$left_icons" ]; then say "  OK  ikon axioo bersih";
else LEFTOVER=1; say "  SISA ikon:"; say "$left_icons" | sed 's/^/    /'; fi
left_apps="$(find "$HOME/.local/share/applications" -maxdepth 1 -iname '*axioo*.desktop' 2>/dev/null || true)"
if [ -z "$left_apps" ]; then say "  OK  desktop entry bersih";
else LEFTOVER=1; say "  SISA desktop:"; say "$left_apps" | sed 's/^/    /'; fi
left_legacy="$(find "$HOME/Applications" -maxdepth 1 -iname '*xioo*.AppImage' 2>/dev/null || true)"
if [ -z "$left_legacy" ]; then say "  OK  legacy ~/Applications bersih";
else LEFTOVER=1; say "  SISA legacy:"; say "$left_legacy" | sed 's/^/    /'; fi
left_dist="$(find "$HERE/dist" -maxdepth 1 \( -name '*.AppImage' -o -name '*.zsync' -o -name 'AppDir' -o -name 'icon.png' \) 2>/dev/null || true)"
if [ -z "$left_dist" ]; then say "  OK  dist/ bersih dari AppImage";
else LEFTOVER=1; say "  SISA dist:"; say "$left_dist" | sed 's/^/    /'; fi
check "snapshot capture + vite cache" "$HERE"/packaging/per-key-capture/captures "$HERE"/.vite
if [ "${KEEP_CACHE:-0}" = "1" ]; then say "  --  .tools/ dipertahankan (KEEP_CACHE=1)";
elif [ -e "$HERE/.tools" ]; then LEFTOVER=1; say "  SISA .tools/ masih ada";
else say "  OK  .tools/ bersih"; fi
check "build cache" "$HERE"/target "$HERE"/tauri-app/src-tauri/target \
    "$HERE"/tauri-app/node_modules "$HERE"/tauri-app/dist
if [ "${KEEP_DRIVER:-0}" = "1" ]; then say "  --  driver DKMS dipertahankan (KEEP_DRIVER=1)";
else
    if have dkms && dkms status 2>/dev/null | grep -q '^tuxedo-drivers-axioo/'; then
        LEFTOVER=1; say "  SISA DKMS custom masih terdaftar (dkms status)"
    else say "  OK  DKMS custom bersih"; fi
    check "source DKMS custom" /usr/src/tuxedo-drivers-axioo-*
fi

say
if [ "$LEFTOVER" -eq 0 ]; then
    say "OK: bersih total — tidak ada daemon, AppImage, helper, cache, build cache,"
    say "atau driver custom yang tersisa."
else
    say "BELUM BERSIH TOTAL: ada item 'SISA' di atas (biasanya butuh sudo/password)."
    say "Jalankan ulang script ini dan masukkan password sudo saat diminta."
    exit 1
fi
