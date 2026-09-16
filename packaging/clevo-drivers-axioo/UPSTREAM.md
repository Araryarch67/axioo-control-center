# Upstream quirk Studio X ke `nick42d/clevo-drivers`

Status: patch siap, TERVERIFIKASI di hardware, belum dikirim (butuh akun GitHub + fork).

## Patch

`studiox-kbd-quirk.patch` — Pongo Studio X (2025) melaporkan backlight type
`0x17` (tak dikenal driver → tidak ada LED sama sekali). Patch memaksa 3-zone
RGB **hanya** untuk board `Pongo Studio X` dengan tipe tak dikenal; tipe yang
sudah dikenal driver tidak disentuh.

Verifikasi lokal:

```sh
cd /tmp && rm -rf qcheck && mkdir qcheck \
  && cp packaging/clevo-drivers-axioo/clevo_leds.h.pristine-4.20.1 qcheck/clevo_leds.h \
  && patch --dry-run qcheck/clevo_leds.h < packaging/clevo-drivers-axioo/studiox-kbd-quirk.patch
```

Bukti hardware: quirk 1-zone tidak direspons EC → force `0x02` (3-zone)
menyalakan semua zona; tulis per-zona OK via `axioo-ctl kbd`
(`docs/kbd-backlight.md`).

## Cara kirim (pilih satu)

Opsi A — PR via `gh`:

```sh
gh repo fork nick42d/clevo-drivers --clone=false
git clone git@github.com:<USER>/clevo-drivers.git /tmp/clevo-drivers
cp packaging/clevo-drivers-axioo/clevo_leds.h.pristine-4.20.1 /tmp/ref.h
# terapkan logika patch secara manual ke tree upstream terbaru,
# kompilasi + test, lalu:
cd /tmp/clevo-drivers
git checkout -b studiox-0x17-quirk
git add -p && git commit -m "studiox: force 3-zone RGB for unknown backlight type 0x17"
gh pr create --repo nick42d/clevo-drivers --title "studiox: force 3-zone RGB for unknown backlight type 0x17" --body-file /dev/stdin <<'EOF'
Pongo Studio X (2025) reports backlight type 0x17, unknown to the driver,
so no keyboard LED is registered. Force 3-zone RGB only for this board
when the type is unknown; known types pass through untouched.

Tested on hardware: zone-0-only writes light the left section (proven
3-zone), per-zone writes OK via sysfs LEDs.
EOF
```

Opsi B — tanpa `gh` (format-patch + lampirkan ke issue upstream):

```sh
cd /tmp/clevo-drivers && git format-patch -1 --stdout > studiox-0x17-quirk.patch
# lampirkan file + isi body di atas ke issue/PR di nick42d/clevo-drivers
```

## Catatan scope

Hanya quirk `0x17 → 3-zone` yang diusulkan upstream. Patch eksperimental
lain (`studiox-4th-zone`, `studiox-numpad-ec`, `studiox-lightbar-ec`,
`studiox-lightbar-segments-ec` yang DIPENSIUNKAN) tetap lokal karena
spesifik firmware/board ini.
