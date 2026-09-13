# axioo-control-center

Utilitas kontrol hardware untuk laptop **Axioo** (basis Clevo) di Linux —
pengganti Clevo Control Center versi Windows, ditulis ulang dari nol dalam Rust.

> Status: kontrol one-shot jalan — kipas (`fan set|auto`, root, clamp
> 40–100%, kedua fan, verify `0xCE`) dan backlight keyboard
> (`kbd set`, 3 zona di Studio X). GUI menulis langsung sebagai root
> (relaunch via `pkexec` sekali di awal). Loop kurva kontinu milik
> `axiood` di masa depan (belum ada).
>
> **Teruji hanya di Pongo Studio X 2025 (X560WNR-SU9)** — belum semua
> Pongo. Model lain (terutama tipe backlight/EC berbeda) butuh validasi
> sendiri sebelum tulis; mulai dari `probe` + `fan dump` + `kbd status`
> (read-only).

## Prasyarat driver (wajib buat Pongo)

```sh
yay -S clevo-drivers-dkms-git
```

**Pongo Studio X (2025)**: firmware melaporkan tipe backlight `0x17`
yang tidak dikenal driver, sehingga LED keyboard tidak muncul sama
sekali. Pasang quirk dulu:

```sh
cd packaging/clevo-drivers-axioo && sudo ./install.sh
ls /sys/class/leds/ | grep kbd   # harus muncul rgb:kbd_backlight{,_1,_2}
```

Detail: `docs/kbd-backlight.md`. Tanpa ini, semua perintah `kbd`
menolak dengan "no keyboard-backlight LED found".

## Struktur workspace

```
axioo-control-center/
├── axioo-lib/     # library introspeksi (DMI, hwmon, RAPL, NVIDIA,
│                  #   baterai, LED, ACPI/WMI, EC) + protokol EC fan +
│                  #   kontrol backlight keyboard (`kbd`) + tulis EC
│                  #   one-shot (`fan_ctrl`, root-only)
├── axioo-ctl/     # CLI: `probe`, `monitor`, `fan dump|watch|curve|set|auto`,
│                  #   `kbd status|get|set|brighter|dimmer`
├── axioo-gui/     # GUI desktop (GPUI): Dashboard, Performa, Kipas,
│                  #   Keyboard (status/brightness/preset/visualizer), Daya
├── packaging/
│   └── clevo-drivers-axioo/  # quirk DKMS 0x17 → 3-zone buat Studio X
│                             # (patch + dkms.conf + install.sh)
└── docs/
    ├── ec-fan-protocol.md   # protokol EC fan Clevo (referensi fase kontrol)
    └── kbd-backlight.md     # port backlight keyboard + penyesuaian Studio X
```

## Build & jalan

```sh
cargo build
./target/debug/axioo-ctl probe            # dump kapabilitas hardware
./target/debug/axioo-ctl monitor          # dashboard live (Ctrl-C keluar)
./target/debug/axioo-ctl monitor -i 2 -c 5
./target/debug/axioo-ctl fan dump         # butuh sudo + ec_sys (peta EC fan)
./target/debug/axioo-ctl kbd status       # cek driver/LED backlight keyboard
./target/debug/axioo-ctl kbd set --preset blue --dry-run
sudo ./target/debug/axioo-ctl kbd set --preset blue --brightness 255
./target/debug/axioo-control-center       # GUI (relaunch root via pkexec)
cargo test -p axioo-lib                   # unit test konversi protokol EC + kbd
```

Butuh live watt RAPL? Jalankan dengan sudo (counter `energy_uj`
hanya bisa dibaca root di kernel baru):

```sh
sudo ./target/debug/axioo-ctl monitor
```

## Arsitektur docel (fase kontrol)

```
clevo-drivers-dkms-git (kernel: clevo_acpi/wmi, tuxedo_keyboard/io)
        ↓ sysfs + ioctl
axioo-lib (+ modul control, di-review eksplisit per model)
        ↓ D-Bus (com.axioo.Control)
axiood (systemd service, root — satu-satunya penulis EC)
        ↓
axio-ctl (CLI, user) + axioo-control-center (GUI GPUI, user)
```

## Credits

Proyek ini berdiri di atas kerja komunitas Clevo/Axioo Linux:

- **[hajilok/clevo-axioo-dual-fan-linux](https://github.com/hajilok/clevo-axioo-dual-fan-linux)**
  — kontrol kipas ganda (CPU+GPU) untuk laptop Clevo/rebrand Axioo Pongo.
  Protokol EC (`cmd 0x99`, register RPM/suhu, kurva auto) yang dipakai
  `axioo-lib::fan` dan `docs/ec-fan-protocol.md` bersumber dari sini.
- **[kkrdwn/pongo725-backlight](https://github.com/kkrdwn/pongo725-backlight)**
  — kontrol backlight keyboard RGB untuk Axioo Pongo 725 via interface
  LED kernel. Logika preset/brightness/RGB yang di-port ke
  `axioo-lib::kbd` + `axioo-ctl kbd` bersumber dari sini
  (lihat `docs/kbd-backlight.md` untuk pemetaan port-nya).

Terima kasih untuk kedua maintainer-nya 🙏

## Referensi lain

- Arsitektur daemon + D-Bus mencontoh
  [tuxedo-rs / tailord](https://github.com/AaronErhardt/tuxedo-rs)
