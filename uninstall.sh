#!/usr/bin/env bash
# uninstall.sh — TOTAL uninstall of axioo-control-center, no exceptions.
# Default = everything is removed: daemon, app, cache, build cache, custom
# DKMS driver. No flags needed:
#
#   ./uninstall.sh
#
# Opt-out (if you really want to keep something):
#   KEEP_CACHE=1 ./uninstall.sh   keep .tools/ (linuxdeploy/appimagetool
#                                 downloader cache for faster builds)
#   KEEP_DRIVER=1 ./uninstall.sh  keep the custom DKMS driver
#                                 (keyboard backlight keeps working)
#
# What gets cleaned:
#   A. axiood daemon: stop + disable service, kill leftover processes, remove
#      binaries (/usr/bin + /usr/local/bin), systemd units (/etc + /usr/lib
#      + /run), system D-Bus conf, polkit policy, udev rules (/etc + /usr/lib);
#      then daemon-reload + udev reload/trigger (+ dbus reload if possible).
#   B. App (user)   : stop user service + kill running GUI/tray,
#      all of ~/.local/share/axioo-control-center,
#      leftover ~/Applications (legacy), ~/.local/bin/axioo-ctl, *axioo*
#      desktop entries, *axioo* icons, user systemd units, app
#      data/cache/config (com.axioo.ControlCenter); refresh desktop-db + icon cache.
#   C. Repo         : dist/*.AppImage + *.zsync + AppDir + icon.png
#      (regenerated via script.sh), .tools/ (unless KEEP_CACHE=1),
#      packaging/per-key-capture/captures snapshots, .vite/ cache.
#   D. Build cache  : target/, tauri-app/src-tauri/target,
#      tauri-app/node_modules, tauri-app/dist (vite output).
#   E. DKMS driver  : remove tuxedo-drivers-axioo (ALL versions) + source
#      /usr/src/tuxedo-drivers-axioo-*, then reinstall the stock
#      clevo-drivers from AUR source if still present (unless KEEP_DRIVER=1).
#
# DELIBERATELY untouched: linux-headers system packages, the
# clevo-drivers-dkms-git AUR package (+ /usr/src/clevo-drivers-* source),
# and this repo's source code. Needs sudo for parts A + E (password asked once).
set -euo pipefail

HERE="$(cd "$(dirname "$0")" && pwd)"
APPID="axioo-control-center"
TAURI_ID="com.axioo.ControlCenter"
LEFTOVER=0

# Default progress bar (details → /tmp/axioo-uninstall.log); --verbose = raw.
QUIET=1
TOTAL=6
LOGFILE="/tmp/axioo-uninstall.log"
bar() { # $1 = steps done (0..TOTAL), $2 = label
    local done=$1 label=${2:-} width=28
    local fill=$(( done * width / TOTAL ))
    local empty=$(( width - fill ))
    local f e
    printf -v f '%*s' "$fill" ''; f=${f// /█}
    printf -v e '%*s' "$empty" ''; e=${e// /░}
    printf '\r\033[K[%s%s] %d/%d %s' "$f" "$e" "$done" "$TOTAL" "$label"
}
spin_wait() { # $1 step, $2 label, $3 pid → spinner until pid finishes (failure OK)
    local step=$1 label=$2 pid=$3 spin='|/-\' i=0
    while kill -0 "$pid" 2>/dev/null; do
        bar "$(( step - 1 ))" "$label ${spin:i%4:1}"
        i=$(( i + 1 )); sleep 0.15
    done
    wait "$pid" || true
    bar "$step" "$label ✓"; echo
}
# Commands with output: verbose → stdout passes through (stderr dropped),
# quiet → everything to the log.
qout() {
    if [ "$QUIET" = 1 ]; then "$@" >>"$LOGFILE" 2>&1; else "$@" 2>/dev/null; fi
}
: >"$LOGFILE"
# say(): verbose → stdout, quiet → log only.
say()  { if [ "$QUIET" = 1 ]; then printf '%s\n' "$*" >>"$LOGFILE"; else printf '%s\n' "$*"; fi; }

for a in "$@"; do
    case "$a" in
        -v|--verbose) QUIET=0 ;;
        -q|--quiet|--silent) QUIET=1 ;;
        -h|--help)
            sed -n '2,/^set /p' "$0" | grep -v '^set ' | sed 's/^# \{0,1\}//'
            exit 0 ;;
        *) echo "unknown option: $a (this script takes no action flags — it removes everything; use --help)"; exit 2 ;;
    esac
done

if [ "$(id -u)" -eq 0 ]; then SUDO=""; else SUDO="sudo"; fi
# sudo is REQUIRED (part A daemon + E driver) — ask for the password ONCE up front
# + keep the timestamp alive so there is no mid-run prompt. Placed after
# arg parsing so --help stays password-free.
if [ -n "$SUDO" ]; then
    $SUDO -v || { say "sudo required — aborting."; exit 1; }
    # & directly (no subshell) so $! is set in this shell.
    while true; do sleep 45; $SUDO -n -v 2>/dev/null || break; done &
    KEEPID=$!
    trap 'kill $KEEPID 2>/dev/null || true' EXIT
fi
shopt -s nullglob

rm_one() { # $1 = description, $@ = files/globs (may be empty)
    local desc="$1"; shift
    if [ "$#" -eq 0 ]; then say "  - $desc: none, skipping"; return; fi
    local f
    for f in "$@"; do
        if [ -e "$f" ] || [ -L "$f" ]; then rm -rf "$f" || true; say "  - removed: $f";
        else say "  - absent: $f"; fi
    done
}
rm_root() { # like rm_one but via $SUDO
    local desc="$1"; shift
    if [ "$#" -eq 0 ]; then say "  - $desc: none, skipping"; return; fi
    local f
    for f in "$@"; do
        if $SUDO test -e "$f" || $SUDO test -L "$f" 2>/dev/null; then
            $SUDO rm -rf "$f" || true; say "  - removed: $f"
        else say "  - absent: $f"; fi
    done
}
have() { command -v "$1" >/dev/null 2>&1; }
check() { # $1 = description, $2.. = paths that MUST be gone
    local desc="$1"; shift
    local bad=() f
    for f in "$@"; do [ -e "$f" ] && bad+=("$f"); done
    if [ "${#bad[@]}" -eq 0 ]; then say "  OK  $desc";
    else LEFTOVER=1; for f in "${bad[@]}"; do say "  LEFT $desc: $f"; done; fi
}

say "=== total uninstall ($APPID) ==="
[ "$QUIET" = 1 ] && echo "axioo uninstall — progress (log: $LOGFILE)"

# ---------- A. axiood daemon ----------
say "[A] axiood daemon"
if have systemctl; then
    if systemctl is-active --quiet axiood 2>/dev/null \
        || systemctl is-enabled --quiet axiood 2>/dev/null \
        || systemctl list-unit-files axiood.service 2>/dev/null | grep -q '^axiood\.service'; then
        qout $SUDO systemctl disable --now axiood || true
        say "  - axiood service stopped + disabled"
    else
        say "  - axiood service: not registered, skipping"
    fi
else
    say "  - no systemctl, skipping service stop"
fi
if have pkill && pgrep -x axiood >/dev/null 2>&1; then
    $SUDO pkill -x axiood 2>/dev/null || true
    sleep 1
    pgrep -x axiood >/dev/null 2>&1 && $SUDO pkill -9 -x axiood 2>/dev/null || true
    say "  - leftover axiood processes killed"
fi
# Binaries + units + bus + polkit + udev — wipe EVERY location any
# installer ever used (setup.sh → /usr/lib, install-system.sh → /etc).
rm_root "axiood binaries" /usr/bin/axiood /usr/local/bin/axiood
rm_root "system axioo-ctl binaries (udev restore)" /usr/bin/axioo-ctl /usr/local/bin/axioo-ctl
rm_root "systemd units" /etc/systemd/system/axiood.service \
    /usr/lib/systemd/system/axiood.service /run/systemd/system/axiood.service
rm_root "D-Bus conf" /etc/dbus-1/system.d/com.axioo.Control.conf
rm_root "polkit policy" /usr/share/polkit-1/actions/com.axioo.Control.policy \
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
say "  - systemd + udev reloaded"
[ "$QUIET" = 1 ] && { bar 1 "daemon ✓"; echo; }

# ---------- B. app (user) ----------
say "[B] app (user $HOME)"
# Kill the GUI BEFORE deleting its files (otherwise the tray icon stays
# alive from an already-deleted binary). Order matters: stop the service
# first so Restart=on-failure doesn't revive the pkill'd process.
if have systemctl; then
    systemctl --user stop axioo-center.service 2>/dev/null || true
    systemctl --user disable axioo-center.service 2>/dev/null || true
    say "  - axioo-center user service stopped + disabled"
fi
if have pkill; then
    pkill -x axioo-center 2>/dev/null || true
    pkill -f 'Axioo Control Center.*\.AppImage' 2>/dev/null || true
    sleep 1
    if pgrep -x axioo-center >/dev/null 2>&1 \
        || pgrep -f 'Axioo Control Center.*\.AppImage' >/dev/null 2>&1; then
        pkill -9 -x axioo-center 2>/dev/null || true
        pkill -9 -f 'Axioo Control Center.*\.AppImage' 2>/dev/null || true
    fi
    say "  - GUI/tray processes killed"
fi
rm_one "user systemd units" "$HOME/.config/systemd/user/axioo-center.service" \
    "$HOME/.config/systemd/user/graphical-session.target.wants/axioo-center.service"
if have systemctl; then systemctl --user daemon-reload 2>/dev/null || true; fi
rm_one "AppImage install dir" "$HOME/.local/share/$APPID"
rm_one "legacy ~/Applications" "$HOME"/Applications/Axioo-Control-Center-*.AppImage \
    "$HOME"/Applications/axioo-control-center-*.AppImage "$HOME"/Applications/*xioo*.AppImage
rm_one "CLI helpers" "$HOME/.local/bin/axioo-ctl" "$HOME/.local/bin/axioo-center-autostart"
rm_root "man page" /usr/share/man/man1/axioo-ctl.1
rm_one "desktop entries" "$HOME/.local/share/applications/$APPID.desktop"
rm_one "login autostart" "$HOME"/.config/autostart/*xioo*.desktop
if [ -d "$HOME/.local/share/applications" ]; then
    # Leftover AppImageLauncher integrations / manual axioo-flavored copies.
    while IFS= read -r -d '' f; do rm -f "$f"; say "  - removed: $f"; done \
        < <(find "$HOME/.local/share/applications" -maxdepth 1 -iname '*axioo*.desktop' -print0 2>/dev/null || true)
fi
if [ -d "$HOME/.local/share/icons" ]; then
    while IFS= read -r -d '' f; do rm -f "$f"; say "  - removed: $f"; done \
        < <(find "$HOME/.local/share/icons" \( -iname "*axioo*.png" -o -iname "*axioo*.svg" \) -print0 2>/dev/null || true)
fi
rm_one "app data" "$HOME/.local/share/$TAURI_ID"
rm_one "app cache" "$HOME/.cache/$TAURI_ID"
rm_one "app config" "$HOME/.config/$TAURI_ID"
if have update-desktop-database; then
    update-desktop-database "$HOME/.local/share/applications" 2>/dev/null || true
fi
if have gtk-update-icon-cache; then
    gtk-update-icon-cache -f "$HOME/.local/share/icons/hicolor" >/dev/null 2>&1 || true
fi
say "  - desktop-db + icon cache refreshed"
[ "$QUIET" = 1 ] && { bar 2 "app ✓"; echo; }

# ---------- C. repo artifacts + downloader cache ----------
say "[C] repo artifacts"
rm_one "dist AppImages/zsync" "$HERE"/dist/*.AppImage "$HERE"/dist/*.zsync
rm_one "dist hicolor icons (regenerated via script.sh)" "$HERE"/dist/icon.png
rm_one "dist AppDir" "$HERE"/dist/AppDir
rm_one "capture-ec snapshots" "$HERE"/packaging/per-key-capture/captures
rm_one "root vite cache" "$HERE"/.vite
if [ "${KEEP_CACHE:-0}" = "1" ]; then
    say "  - .tools/ KEPT (KEEP_CACHE=1)"
else
    rm_one "downloader cache .tools" "$HERE"/.tools
fi
[ "$QUIET" = 1 ] && { bar 3 "repo artifacts ✓"; echo; }

# ---------- D. build cache ----------
say "[D] build cache"
rm_one "cargo target" "$HERE"/target
rm_one "tauri bundle output" "$HERE"/tauri-app/src-tauri/target
rm_one "node_modules" "$HERE"/tauri-app/node_modules
rm_one "vite output" "$HERE"/tauri-app/dist
[ "$QUIET" = 1 ] && { bar 4 "build cache ✓"; echo; }

# ---------- E. custom DKMS driver ----------
_e_shown=0
if [ "${KEEP_DRIVER:-0}" = "1" ]; then
    say "[E] DKMS driver: KEPT (KEEP_DRIVER=1)"
else
    say "[E] custom DKMS driver"
    if have dkms; then
        while IFS= read -r line; do
            # "tuxedo-drivers-axioo/4.20.1, ..." → "tuxedo-drivers-axioo/4.20.1"
            mod="$(printf '%s' "$line" | awk -F',' '{print $1}' | tr -d ' ')"
            if [ -n "$mod" ]; then
                qout $SUDO dkms remove "$mod" --all || true
                say "  - dkms remove: $mod"
            fi
        done < <(dkms status 2>/dev/null | grep '^tuxedo-drivers-axioo/' || true)
    else
        say "  - no dkms, skipping remove"
    fi
    rm_root "custom DKMS source" /usr/src/tuxedo-drivers-axioo-*
    # Reinstall the stock AUR driver if its source is still around (inverse
    # of packaging/clevo-drivers-axioo/install.sh which disables it).
    RESTORED=0
    for src in /usr/src/clevo-drivers-*/; do
        [ -d "$src" ] || continue
        base="$(basename "$src")"          # clevo-drivers-4.20.1
        mod="${base%-*}" ver="${base##*-}" # clevo-drivers / 4.20.1
        if [ -f "$src/dkms.conf" ]; then
            say "  - reinstalling stock driver: $mod/$ver (active kernel)"
            # WITHOUT --all: dkms 3.x rejects `install module/ver --all`
            # ("The action install does not support the --all parameter").
            if [ "$QUIET" = 1 ]; then
                $SUDO dkms install "$mod/$ver" </dev/null >>"$LOGFILE" 2>&1 & _dkpid=$!
                spin_wait 5 "driver ($mod)" "$_dkpid"
                _e_shown=1
            else
                $SUDO dkms install "$mod/$ver" 2>&1 | tail -n 3 || true
            fi
            RESTORED=1
        fi
    done
    if have modprobe; then
        $SUDO modprobe -r tuxedo_keyboard clevo_acpi clevo_wmi tuxedo_io 2>/dev/null || true
        $SUDO modprobe tuxedo_keyboard 2>/dev/null || true
        $SUDO modprobe clevo_acpi clevo_wmi 2>/dev/null || true
        say "  - input/acpi modules reloaded (if available)"
    fi
    if [ "$RESTORED" -eq 0 ]; then
        say "  ! no clevo-drivers-* source — keyboard driver is gone until"
        say "    you reinstall (./setup.sh) or reboot + reinstall the driver. When in doubt, reboot."
    fi
fi
# Skip the duplicate bar 5 when the driver-restore spin_wait already showed it.
[ "$QUIET" = 1 ] && [ "${_e_shown:-0}" = 0 ] && { bar 5 "driver ✓"; echo; }

# ---------- verify ----------
say
say "=== verify ==="
if have systemctl; then
    if systemctl is-active --quiet axiood 2>/dev/null; then LEFTOVER=1; say "  LEFT service: axiood still ACTIVE";
    else say "  OK  axiood service not running"; fi
    if systemctl is-enabled --quiet axiood 2>/dev/null; then LEFTOVER=1; say "  LEFT service: axiood still ENABLED";
    else say "  OK  axiood service not enabled"; fi
fi
if have pgrep && pgrep -x axiood >/dev/null 2>&1; then LEFTOVER=1; say "  LEFT axiood process still alive";
else say "  OK  no axiood process"; fi
if have systemctl && systemctl --user is-active --quiet axioo-center.service 2>/dev/null; then
    LEFTOVER=1; say "  LEFT user service: axioo-center still ACTIVE";
else say "  OK  axioo-center user service not running"; fi
if have pgrep && { pgrep -x axioo-center >/dev/null 2>&1 || pgrep -f 'Axioo Control Center.*\.AppImage' >/dev/null 2>&1; }; then
    LEFTOVER=1; say "  LEFT GUI/tray process still alive";
else say "  OK  no GUI/tray process"; fi
check "user units" "$HOME/.config/systemd/user/axioo-center.service" \
    "$HOME/.config/systemd/user/graphical-session.target.wants/axioo-center.service"
check "daemon files" /usr/bin/axiood /usr/local/bin/axiood /usr/bin/axioo-ctl /usr/local/bin/axioo-ctl \
    /etc/systemd/system/axiood.service /usr/lib/systemd/system/axiood.service \
    /etc/dbus-1/system.d/com.axioo.Control.conf \
    /usr/share/polkit-1/actions/com.axioo.Control.policy \
    /etc/udev/rules.d/99-axioo-kbd.rules /usr/lib/udev/rules.d/99-axioo-kbd.rules
check "app files" "$HOME/.local/share/$APPID" "$HOME/.local/bin/axioo-ctl" \
    "$HOME/.local/bin/axioo-center-autostart" /usr/share/man/man1/axioo-ctl.1 \
    "$HOME/.local/share/applications/$APPID.desktop" \
    "$HOME/.local/share/$TAURI_ID" "$HOME/.cache/$TAURI_ID" "$HOME/.config/$TAURI_ID"
left_auto="$(find "$HOME/.config/autostart" -maxdepth 1 -iname '*xioo*.desktop' 2>/dev/null || true)"
if [ -z "$left_auto" ]; then say "  OK  login autostart clean";
else LEFTOVER=1; say "  LEFT autostart:"; say "$left_auto" | sed 's/^/    /'; fi
left_icons="$(find "$HOME/.local/share/icons" -iname '*axioo*' 2>/dev/null || true)"
if [ -z "$left_icons" ]; then say "  OK  axioo icons clean";
else LEFTOVER=1; say "  LEFT icons:"; say "$left_icons" | sed 's/^/    /'; fi
left_apps="$(find "$HOME/.local/share/applications" -maxdepth 1 -iname '*axioo*.desktop' 2>/dev/null || true)"
if [ -z "$left_apps" ]; then say "  OK  desktop entries clean";
else LEFTOVER=1; say "  LEFT desktop:"; say "$left_apps" | sed 's/^/    /'; fi
left_legacy="$(find "$HOME/Applications" -maxdepth 1 -iname '*xioo*.AppImage' 2>/dev/null || true)"
if [ -z "$left_legacy" ]; then say "  OK  legacy ~/Applications clean";
else LEFTOVER=1; say "  LEFT legacy:"; say "$left_legacy" | sed 's/^/    /'; fi
left_dist="$(find "$HERE/dist" -maxdepth 1 \( -name '*.AppImage' -o -name '*.zsync' -o -name 'AppDir' -o -name 'icon.png' \) 2>/dev/null || true)"
if [ -z "$left_dist" ]; then say "  OK  dist/ free of AppImages";
else LEFTOVER=1; say "  LEFT dist:"; say "$left_dist" | sed 's/^/    /'; fi
check "capture snapshots + vite cache" "$HERE"/packaging/per-key-capture/captures "$HERE"/.vite
if [ "${KEEP_CACHE:-0}" = "1" ]; then say "  --  .tools/ kept (KEEP_CACHE=1)";
elif [ -e "$HERE/.tools" ]; then LEFTOVER=1; say "  LEFT .tools/ still present";
else say "  OK  .tools/ clean"; fi
check "build cache" "$HERE"/target "$HERE"/tauri-app/src-tauri/target \
    "$HERE"/tauri-app/node_modules "$HERE"/tauri-app/dist
if [ "${KEEP_DRIVER:-0}" = "1" ]; then say "  --  DKMS driver kept (KEEP_DRIVER=1)";
else
    if have dkms && dkms status 2>/dev/null | grep -q '^tuxedo-drivers-axioo/'; then
        LEFTOVER=1; say "  LEFT custom DKMS still registered (dkms status)"
    else say "  OK  custom DKMS clean"; fi
    check "custom DKMS source" /usr/src/tuxedo-drivers-axioo-*
fi

if [ "$QUIET" = 1 ]; then
    bar 6 "verify ✓"; echo
    if [ "$LEFTOVER" -eq 0 ]; then
        echo "OK: fully clean — daemon, app, cache, and custom driver are gone."
        exit 0
    fi
    echo "NOT FULLY CLEAN (details: $LOGFILE):"
    grep "LEFT" "$LOGFILE" | head -20 || true
    exit 1
fi

say
if [ "$LEFTOVER" -eq 0 ]; then
    say "OK: fully clean — no daemon, AppImage, helpers, cache, build cache,"
    say "or custom driver left."
else
    say "NOT FULLY CLEAN: there are 'LEFT' items above (usually needs sudo/password)."
    say "Re-run this script and enter your sudo password when asked."
    exit 1
fi
