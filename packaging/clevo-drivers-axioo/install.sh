#!/usr/bin/env bash
# Custom DKMS driver buat Axioo Pongo Studio X (quirk backlight 0x17).
#
# Jalankan dari folder ini:  sudo ./install.sh
# Uninstall (balik ke AUR):  sudo dkms remove tuxedo-drivers-axioo/4.20.1 --all
#                             sudo rm -rf /usr/src/tuxedo-drivers-axioo-4.20.1
#                             sudo dkms install clevo-drivers/4.20.1
set -euo pipefail

SRC=/usr/src/clevo-drivers-4.20.1          # source AUR (prasyarat: clevo-drivers-dkms-git)
DST=/usr/src/tuxedo-drivers-axioo-4.20.1   # source custom kita
HERE="$(cd "$(dirname "$0")" && pwd)"

if [ ! -d "$SRC" ]; then
    echo "butuh $SRC (yay -S clevo-drivers-dkms-git dulu)"; exit 1
fi

# 0. Kembalikan source AUR ke pristine (cabut patch percobaan manual)
if grep -q "Pongo Studio X: unknown backlight" "$SRC/clevo_leds.h" 2>/dev/null; then
    cp "$HERE/clevo_leds.h.pristine-4.20.1" "$SRC/clevo_leds.h"
    echo "reverted pristine: $SRC/clevo_leds.h"
fi

# 1. Bersihkan entri DKMS lama (nama .ko sama — tidak boleh dobel install)
dkms remove clevo-drivers/4.20.1 --all || true
dkms remove tuxedo-drivers-axioo/4.20.1 --all || true

# 2. Salin tree + terapkan quirk + dkms.conf custom
rm -rf "$DST"
cp -r "$SRC" "$DST"
patch "$DST/clevo_leds.h" < "$HERE/studiox-kbd-quirk.patch"
patch "$DST/clevo_leds.h" < "$HERE/studiox-4th-zone.patch"
patch "$DST/clevo_leds.h" < "$HERE/studiox-getspecs-debug.patch"
patch "$DST/clevo_leds.h" < "$HERE/studiox-numpad-ec.patch"
patch "$DST/clevo_leds.h" < "$HERE/studiox-lightbar-ec.patch"
# TIDAK dipakai: studiox-lightbar-segments-ec.patch (0x06/08/09/0A).
# Hasil scan 2026-09-20: tak ada yang jadi segmen rear; salah satunya
# malah menggerakkan numpad (alias dengan 0x0B). Rear = 1 zona (0x07).
cp "$HERE/dkms.conf" "$DST/dkms.conf"
echo "patched: $DST/clevo_leds.h"

# 3. Build + install + reload (probe ulang = LED muncul)
dkms install tuxedo-drivers-axioo/4.20.1
modprobe -r clevo_acpi clevo_wmi tuxedo_io tuxedo_keyboard || true
modprobe tuxedo_keyboard
modprobe clevo_acpi clevo_wmi
sleep 1
ls /sys/class/leds/ | grep -i "kbd\|backlight" && echo DRIVER_OK

# 4. Langsung nyalakan putih semua zona (termasuk rear _4) — habis reload
#    driver LED lahir dalam keadaan mati; tanpa ini keyboard gelap sampai
#    ada yang set manual. Tulis sysfs langsung (script ini sudah root):
#    brightness = max per-node, RGB 255 255 255 (putih, urutan channel bebas).
echo "menunggu node LED…"
for i in $(seq 1 20); do
    N="$(ls /sys/class/leds/ 2>/dev/null | grep -c -i 'kbd\|backlight' || true)"
    [ "${N:-0}" -ge 1 ] && break
    sleep 0.5
done
WHITE_OK=1
for led in /sys/class/leds/rgb:kbd_backlight*; do
    [ -d "$led" ] || continue
    mx="$(cat "$led/max_brightness" 2>/dev/null || echo 255)"
    echo "$mx" > "$led/brightness" 2>/dev/null || WHITE_OK=0
    echo "255 255 255" > "$led/multi_intensity" 2>/dev/null || WHITE_OK=0
    echo "  white: $(basename "$led")"
done
if [ "$WHITE_OK" -eq 1 ]; then
    echo "KEYBOARD_WHITE_OK (termasuk rear)"
else
    echo "KEYBOARD_WHITE_SEBAGIAN — cek: axioo-ctl kbd status"
fi
