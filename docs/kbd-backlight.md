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
4. **Tulis butuh root** (`sudo axioo-ctl kbd set ...`) sampai udev rule
   terpasang; dengan `packaging/udev/99-axioo-kbd.rules` (grup `video`)
   user biasa bisa tulis sysfs tanpa sudo harian. Selalu coba
   `--dry-run` dulu.
5. Skala brightness JANGAN diasumsikan 0–255 — baca `max_brightness`
   dari `axioo-ctl kbd status` dan skala dari situ.

## Efek animasi (12 efek + rear independen)

Efek jalan sebagai **thread userspace** (60ms tick) di proses GUI/CLI —
privilege sama seperti `kbd set` (sysfs, tak perlu root tambahan).
Satu thread menulis semua node; start baru mematikan yang lama.
`static` = warna diam (hentikan animasi, kembalikan warna dasar).

| Efek | Perilaku |
|---|---|
| `static` | warna diam (preset/RGB) |
| `breathing` | fade in/out warna dasar |
| `wave` | hue mengalir kiri → numpad 40°/s |
| `rainbow` | semua zona sinkron 45°/s |
| `cycle` | 6 preset bergantian dengan blend |
| `aurora` | pastel wave lambat 15°/s |
| `twinkle` | sparkle acak tiap 0.4s |
| `pulse` | heartbeat ganda |
| `gradient` | rainbow tetap per zona |
| `music` | beat pulsing (simulasi, cpal-ready) |
| `spectrum` | low/mid/high per zona (simulasi) |
| `reactive` | flash putih (placeholder) |

Rear exhaust bisa punya efek sendiri (butuh 5 node; bila <5, `rear`
diabaikan = follow):

```sh
sudo axioo-ctl kbd effect wave --rgb 0,128,255 --rear rainbow
axioo-ctl kbd effect static --rear wave   # keyboard diam, rear jalan
```

Saat `static` + rear independen: warna keyboard per-zona **tidak
disentuh**, hanya rear yang dianimasikan dan di-restore saat stop.
Kecepatan: `--speed 0.1–4.0` (CLI) / slider GUI (1.0 = normal).

## Rantai persist + restore (5 lapis)

Firmware selalu reset backlight ke **putih tiap reboot**. Warna terakhir
dipertahankan lewat 5 lapis (urutan waktu saat boot → login):

1. **udev (paling awal).** `99-axioo-kbd.rules` memanggil
   `axioo-ctl kbd restore` tiap node `rgb:kbd_backlight*` muncul
   (coldplug, jauh sebelum SDDM). Idempoten. BIOS/bootloader tetap
   putih — belum ada kode OS yang jalan.
2. **axiood pre-login.** `SetKbd` (via D-Bus, dari GUI/CLI sebagai user)
   menyimpan ke `/var/lib/axiood/kbd.json`
   (`{brightness,r,g,b,effect,rear,speed}`); daemon (root, system
   service sebelum display-manager) me-restore warna dasar saat start +
   retry 10× tiap 3 dtk bila driver telat dimuat. Hanya warna dasar
   statis — cukup untuk SDDM; animasi efek jalan lagi setelah login.
   `axioo-ctl kbd set` sebagai root ikut menyimpan ke file yang sama.
3. **GUI boot.** `maintainKbd` menulis-balik warna tersimpan **sebelum**
   `hydrateKbd` tiap tick (hardware reset tiap reboot; tanpa ini tick-1
   async menimpa warna tersimpan dengan bacaan hardware yang putih).
4. **Sesi login.** `hydrateKbd` membaca snapshot hardware → store
   (`kbdHex`/`kbdBright`/`kbdTouched`, persist zustand); `kbdTouched`
   menandai user sudah memilih warna (poll restore cepat sampai restore
   sesi selesai, anti-race).
5. **Follow wallpaper.** Mode follow-wallpaper (matugen
   `~/.cache/ryoku/colors.json`) jalan tiap tick walau tab Keyboard tak
   dibuka; revisi palet = mtime file (1 stat syscall per snapshot) +
   watcher event `matugen-changed` saat hidden.

## Fn-keys brightness (Hyprland)

`tuxedo_keyboard` emit `KEY_KBDILLUM{UP,DOWN,TOGGLE}`; subcommand
`kbd brighter`/`kbd dimmer` naik/turun satu step (buat binding):

```
bind = , XF86KbdBrightnessUp,   exec, axioo-ctl kbd brighter
bind = , XF86KbdBrightnessDown, exec, axioo-ctl kbd dimmer
```

(Binding ditunda per permintaan user — siap ditempel ke `hyprland.conf`.)

## Contoh pakai

```sh
axioo-ctl kbd status
axioo-ctl kbd get
axioo-ctl kbd set --preset blue --dry-run
sudo axioo-ctl kbd set --preset blue --brightness 200
sudo axioo-ctl kbd set --rgb 255,128,0
sudo axioo-ctl kbd set --preset off
axioo-ctl kbd brighter          # Fn-key: naik satu step
axioo-ctl kbd dimmer            # Fn-key: turun satu step
axioo-ctl kbd restore           # tulis-ulang warna tersimpan (udev/manual)
sudo axioo-ctl kbd effect wave --rgb 0,128,255 --rear rainbow  # Ctrl-C berhenti
```
