#!/usr/bin/env bash
# install-system.sh — one-time privileged setup so Tauri buttons actually work:
#   1. udev: kbd backlight + BAT0 charge thresholds writable by group `video`
#   2. axiood daemon binary + systemd unit + D-Bus config + polkit policy
#
# Jalankan SEKALI sebagai user biasa (password sudo diminta di terminal):
#   ./install-system.sh
set -euo pipefail
SCRIPT="$(readlink -f "$0")"
APP_DIR="$(dirname "$SCRIPT")"
REPO="$(dirname "$APP_DIR")"

log() { printf '[install-system] %s\n' "$*"; }
die() { printf '[install-system] ERROR: %s\n' "$*" >&2; exit 1; }

[[ -f "$REPO/packaging/udev/99-axioo-kbd.rules" ]] || die "repo packaging/ tidak ditemukan di $REPO"

log "membangun axiood (release)…"
cargo build --release -p axiood --manifest-path "$REPO/Cargo.toml"

log "meminta hak root untuk instalasi sistem…"
sudo -v || die "sudo gagal"

log "[1/5] udev rules (kbd + baterai, grup video)…"
UDEV_TMP="$(mktemp)"
cat "$REPO/packaging/udev/99-axioo-kbd.rules" > "$UDEV_TMP"
cat >> "$UDEV_TMP" <<'EOF'
# Axioo Control Center (Tauri) — BAT0 charge thresholds writable by video group
SUBSYSTEM=="power_supply", KERNEL=="BAT0", RUN+="/bin/chgrp video /sys%p/charge_control_start_threshold /sys%p/charge_control_end_threshold"
SUBSYSTEM=="power_supply", KERNEL=="BAT0", RUN+="/bin/chmod g+w /sys%p/charge_control_start_threshold /sys%p/charge_control_end_threshold"
EOF
sudo install -m 644 "$UDEV_TMP" /etc/udev/rules.d/99-axioo-kbd.rules
rm -f "$UDEV_TMP"
sudo udevadm control --reload-rules
# NB: satu --subsystem-match per pemanggilan (gabungan dalam satu
# perintah hanya menjalankan yang terakhir).
sudo udevadm trigger --subsystem-match=leds --action=add
sudo udevadm trigger --subsystem-match=power_supply --action=add

log "[2/5] axiood binary → /usr/bin/axiood…"
sudo install -m 755 "$REPO/target/release/axiood" /usr/bin/axiood

log "[3/5] systemd unit + enable --now…"
sudo install -m 644 "$REPO/packaging/axiood.service" /etc/systemd/system/axiood.service
sudo systemctl daemon-reload
sudo systemctl enable --now axiood

log "[4/5] D-Bus system config + polkit policy…"
# -D: buat direktori induk bila belum ada (mesin ini tak punya /etc/dbus-1).
sudo install -D -m 644 "$REPO/packaging/com.axioo.Control.conf" /etc/dbus-1/system.d/com.axioo.Control.conf
sudo install -D -m 644 "$REPO/packaging/com.axioo.Control.policy" /usr/share/polkit-1/actions/com.axioo.Control.policy
sudo systemctl reload dbus 2>/dev/null || log "(lewati reload dbus — reboot bila bus belum kenal nama)"
# Daemon yang crash-loop (AccessDenied sebelum conf ada) harus di-restart
# SETELAH conf terpasang agar klaim nama diterima.
sudo systemctl restart axiood

log "[5/5] verifikasi…"
sleep 2
systemctl is-active axiood || die "axiood gagal start — lihat 'journalctl -u axiood -e'"
busctl --system get-property com.axioo.Control /com/axioo/Control com.axioo.Control Profile \
  || die "D-Bus com.axioo.Control belum terjangkau"
if [[ -w /sys/class/leds/rgb:kbd_backlight/brightness ]]; then
  log "LED kbd writable — tombol Keyboard aktif."
else
  log "LED kbd masih read-only (reboot / re-login bila grup video baru ditambahkan: 'groups' harus memuat video)."
fi
if [[ -w /sys/class/power_supply/BAT0/charge_control_end_threshold ]]; then
  log "BAT0 threshold writable — tombol Baterai aktif."
else
  log "BAT0 masih read-only — coba cabut/colok charger atau reboot (re-trigger udev)."
fi
log "SELESAI. Restart aplikasi Tauri — tombol Performance/Fan/Keyboard/Battery harus hidup."
