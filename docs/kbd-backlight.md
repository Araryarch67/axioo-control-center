# Keyboard backlight — port dari pongo725-backlight + penyesuaian Studio X 2025

Sumber logika: [kkrdwn/pongo725-backlight](https://github.com/kkrdwn/pongo725-backlight)
(Python/GTK untuk Axioo Pongo 725). Cara kerjanya sederhana dan aman:
tidak mengutak-atik EC/WMI, melainkan menulis ke interface LED kernel
yang disediakan driver (`clevo-drivers-dkms-git`):

```
brightness      -> /sys/.../rgb:kbd_backlight/brightness
"R G B"         -> /sys/.../rgb:kbd_backlight/multi_intensity
```

## Lightbar belakang (TERBUKTI 2026-09-20: EC indeks `0x07`)

Lampu exhaust belakang TIDAK muncul sebagai LED tersendiri secara
default (`ls /sys/class/leds` hanya 4 zona keyboard). Dites dua jalur:

1. **Mirror zona keyboard — GAGAL.** Tiap zona diset warna beda,
   belakang tidak ikut zona mana pun. Lightbar butuh jalur sendiri.
2. **EC indeks `0x07` via ECMD — BERHASIL.** `packaging/`
   `clevo-drivers-axioo/studiox-lightbar-ec.patch` mendaftarkan LED
   ke-5 (`rgb:kbd_backlight_4`) yang menulis `05 00 CA 07 RR GG BB`,
   pola sama seperti numpad (`0x0B`). Tulis merah → belakang merah.
   Terpasang otomatis oleh `install.sh` (urutan: quirk → 4th-zone →
   getspecs-debug → numpad-ec → lightbar-ec; urutan apply dari
   pristine terverifikasi).
3. **Animasi rear independen.** Rear ikut semua efek (satu thread
   menulis 5 node), plus bisa punya efek sendiri: GUI → tab Keyboard →
   dropdown "Rear exhaust" (Follow/Wave/Rainbow/…), atau
   `axioo-ctl kbd effect static --rear wave` (keyboard diam, rear
   jalan; warna keyboard per-zona tidak disentuh, rear di-restore
   saat stop). Butuh 5 node; bila <5, rear diabaikan (follow).

## Segmen lightbar (DITUTUP 2026-09-20: rear = 1 zona)

Di Windows bar belakang terlihat multi-segmen. Bedah DSDT
(`Device (DCHU)` → `SCMD` handler `0x67`) menunjukkan peta zona
firmware: zona 0,1,2 → EC `3,4,5` (keyboard); zona 3 → EC `0x07`
(lightbar); zona 4 → EC `0x06`; zona 6 → EC `0x09` (+`0x0A`).

Hasil scan hardware per-node (`_4…_8` merah bergantian, 2026-09-20):
rear tetap **satu warna penuh** (ikut `_4`/EC `0x07` saja) dan salah
satu indeks kandidat malah menggerakkan **numpad** (alias dengan
`0x0B`). Kesimpulan: di firmware ini rear = 1 zona RGB, tidak ada
segmen terpisah. `studiox-lightbar-segments-ec.patch` DIPENSIUNKAN
(tetap di repo sebagai dokumentasi, tidak dipasang `install.sh`).
"Multi-segmen" di Windows kemungkinan animasi satu zona dari waktu
ke waktu (efek Wave/Rainbow kita sudah covers ini).

## Pemetaan Python → Rust

| Python (`main.py`) | Rust (`axioo-lib::kbd` + `axioo-ctl kbd`) |
|---|---|
| `KBD_PATH` hardcoded ke `/sys/devices/platform/tuxedo_keyboard/...` | `kbd::discover()` — cari node `*kbd*` di `/sys/class/leds` (nama platform device tergantung firmware) |
| slider 0–255 ditulis mentah | clamp ke `max_brightness` milik driver (bisa 0–10, bukan 0–255) |
| `multi_intensity` selalu `"R G B"` | urutan mengikuti `multi_index` device (`map_channels`), channel tak dikenal → 0 |
| preset RED/YELLOW/GREEN/CYAN/BLUE/WHITE/OFF | `kbd::preset()` — set identik |
| `pkexec` fallback saat write gagal | error message eksplisit: bedakan *driver tidak ada* vs *perlu root* |
| GUI GTK | panel Keyboard di GUI (tab 02 CONTROL): status zona, brightness stepper, preset warna, visualizer per-zona — tulis langsung (root) |

## Penyesuaian khusus Pongo Studio X (2025, X560WNR-SU9)

1. **Driver belum terinstall di mesin ini.** `axioo-ctl probe` menunjukkan:
   tidak ada modul vendor + tidak ada node LED keyboard. Tanpa driver,
   `axioo-ctl kbd set` akan menolak dengan pesan yang jelas (bukan
   error misterius). Prasyarat:
   ```
   yay -S clevo-drivers-dkms-git
   sudo modprobe clevo_acpi clevo_wmi tuxedo_keyboard
   axioo-ctl kbd status
   ```
2. **Firmware 2025 bisa melaporkan tipe backlight tak dikenal.**
   Terbukti di Pongo Studio X (2025, X560WNR-SU9): `CLEVO_CMD_GET_SPECS`
   menjawab tipe **`0x17`** (tidak ada di tabel `0x01/0x02/0x06/0xf3`),
   sehingga `clevo_leds_init()` tidak mendaftarkan LED sama sekali.
   Solusinya quirk DMI: paksa tipe tak dikenal menjadi **3-zone RGB**
   (terbukti di hardware: tulis 1-zone hanya menyalakan seksi kiri,
   sisanya zona lain — jadi butuh semua 3 zona terekspos).
   Prosedur: jalankan `sudo ./install.sh` di
   `packaging/clevo-drivers-axioo/` (menyalin tree AUR, menerapkan
   `studiox-kbd-quirk.patch`, DKMS build sebagai
   `tuxedo-drivers-axioo/4.20.1`, reload modul). Quirk hanya aktif untuk
   board `Pongo Studio X` dan hanya untuk tipe yang tidak dikenal —
   tipe yang sudah dikenal tidak disentuh. Setelah reload, `install.sh`
   langsung menyalakan putih semua zona (termasuk rear) agar keyboard
   tidak gelap.
3. **Zona ke-4 (numpad) tidak bisa via WMI `0x67`** (hanya indeks EC
   3/4/5/7/9; indeks 7 = lightbar, tak terlihat di mesin ini).
   Terbukti dari DSDT + driver System76: numpad = indeks EC **`0x0B`**,
   dikirim langsung via `ECMD` (`05 00 CA 0B RR GG BB`).
   `studiox-numpad-ec.patch` mendaftarkan node ke-4 yang menulis lewat
   jalur itu (alat System76 dipakai sebagai referensi yang sudah
   terbukti di produksi).
3. **Tulis butuh root** (`sudo axioo-ctl kbd set ...`) sampai daemon
   `axiood` ada. Selalu coba `--dry-run` dulu.
4. Skala brightness JANGAN diasumsikan 0–255 — baca `max_brightness`
   dari `axioo-ctl kbd status` dan skala dari situ.

## Contoh pakai

```sh
axioo-ctl kbd status
axioo-ctl kbd set --preset blue --dry-run
sudo axioo-ctl kbd set --preset blue --brightness 200
sudo axioo-ctl kbd set --rgb 255,128,0
sudo axioo-ctl kbd set --preset off
```
