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
cp "$HERE/dkms.conf" "$DST/dkms.conf"
echo "patched: $DST/clevo_leds.h"

# 3. Build + install + reload (probe ulang = LED muncul)
dkms install tuxedo-drivers-axioo/4.20.1
modprobe -r clevo_acpi clevo_wmi tuxedo_io tuxedo_keyboard || true
modprobe tuxedo_keyboard
modprobe clevo_acpi clevo_wmi
sleep 1
ls /sys/class/leds/ | grep -i "kbd\|backlight" && echo DRIVER_OK
