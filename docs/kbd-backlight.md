# Keyboard backlight — port dari pongo725-backlight + penyesuaian Studio X 2025

Sumber logika: [kkrdwn/pongo725-backlight](https://github.com/kkrdwn/pongo725-backlight)
(Python/GTK untuk Axioo Pongo 725). Cara kerjanya sederhana dan aman:
tidak mengutak-atik EC/WMI, melainkan menulis ke interface LED kernel
yang disediakan driver (`clevo-drivers-dkms-git`):

```
brightness      -> /sys/.../rgb:kbd_backlight/brightness
"R G B"         -> /sys/.../rgb:kbd_backlight/multi_intensity
```

## Lightbar belakang (eksperimen, belum terbukti)

Lampu exhaust belakang TIDAK muncul sebagai LED tersendiri
(`ls /sys/class/leds` hanya 4 zona keyboard). Dua kemungkinan:

1. **Mirror zona keyboard** (khas Clevo) — test 30 detik: set tiap zona
   warna beda di GUI (Z1 merah, Z2 hijau, Z3 biru, Z4 putih), lihat
   belakang ikut zona mana. Tanpa ubah kode.
2. **EC indeks `0x07` via ECMD** — `packaging/clevo-drivers-axioo/`
   `studiox-lightbar-ec.patch` mendaftarkan LED ke-5
   (`rgb:kbd_backlight_4`) yang menulis `05 00 CA 07 RR GG BB`,
   pola sama seperti numpad (`0x0B`). Terpasang otomatis oleh
   `install.sh` (urutan: quirk → 4th-zone → getspecs-debug →
   numpad-ec → lightbar-ec; urutan apply dari pristine terverifikasi).
   Setelah rebuild + reload: `ls /sys/class/leds | grep kbd` harus
   menunjukkan node ke-5; tulis warna lalu lihat belakang.
   Kalau indeks `0x07` tak berpengaruh, lightbar ikut firmware
   (mirror) — pakai hasil test (1) dan beri label di UI.

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
   tipe yang sudah dikenal tidak disentuh.
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
