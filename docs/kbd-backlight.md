# Keyboard backlight — port dari pongo725-backlight + penyesuaian Studio X 2025

Sumber logika: [kkrdwn/pongo725-backlight](https://github.com/kkrdwn/pongo725-backlight)
(Python/GTK untuk Axioo Pongo 725). Cara kerjanya sederhana dan aman:
tidak mengutak-atik EC/WMI, melainkan menulis ke interface LED kernel
yang disediakan driver (`clevo-drivers-dkms-git`):

```
brightness      -> /sys/.../rgb:kbd_backlight/brightness
"R G B"         -> /sys/.../rgb:kbd_backlight/multi_intensity
```

## Pemetaan Python → Rust

| Python (`main.py`) | Rust (`axioo-lib::kbd` + `axioo-ctl kbd`) |
|---|---|
| `KBD_PATH` hardcoded ke `/sys/devices/platform/tuxedo_keyboard/...` | `kbd::discover()` — cari node `*kbd*` di `/sys/class/leds` (nama platform device tergantung firmware) |
| slider 0–255 ditulis mentah | clamp ke `max_brightness` milik driver (bisa 0–10, bukan 0–255) |
| `multi_intensity` selalu `"R G B"` | urutan mengikuti `multi_index` device (`map_channels`), channel tak dikenal → 0 |
| preset RED/YELLOW/GREEN/CYAN/BLUE/WHITE/OFF | `kbd::preset()` — set identik |
| `pkexec` fallback saat write gagal | error message eksplisit: bedakan *driver tidak ada* vs *perlu root* |
| GUI GTK | belum di-port (GUI kita GPUI; panel keyboard menyusul setelah tulis terbukti aman) |

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
   Solusinya quirk DMI: paksa tipe tak dikenal menjadi 1-zone RGB
   (EC mesin ini menjawab perintah RGB standar). Prosedur:
   patch `/usr/src/clevo-drivers-4.20.1/clevo_leds.h` (sisipkan quirk
   setelah quirk N14xWU), lalu
   `sudo dkms remove clevo-drivers/4.20.1 --all &&
    sudo dkms install clevo-drivers/4.20.1`,
   reload modul, verifikasi node LED muncul. Quirk hanya aktif untuk
   board `Pongo Studio X` dan hanya untuk tipe yang tidak dikenal —
   tipe yang sudah dikenal tidak disentuh.
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
