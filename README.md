# axioo-control-center

Utilitas kontrol hardware untuk laptop **Axioo** (basis Clevo) di Linux —
pengganti Clevo Control Center versi Windows, ditulis ulang dari nol dalam Rust.

> Status: MVP read-only. Semua perintah saat ini **hanya membaca** hardware,
> tidak ada yang menulis ke EC / sysfs. Kontrol tulis (kipas, keyboard,
> power limit) menyusul setelah validasi protokol per model.

## Struktur workspace

```
axioo-control-center/
├── axioo-lib/     # library introspeksi read-only (DMI, hwmon, RAPL,
│                  #   NVIDIA, baterai, LED, ACPI/WMI, EC) + konstanta
│                  #   protokol EC fan (tanpa I/O) + kontrol backlight
│                  #   keyboard via LED class (`kbd` module)
├── axioo-ctl/     # CLI: `axioo-ctl probe`, `axioo-ctl monitor`,
│                  #   `axioo-ctl kbd status|get|set`
├── axioo-gui/     # GUI desktop (GPUI, framework-nya Zed) — window shell
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
./target/debug/axioo-ctl kbd status       # cek driver/LED backlight keyboard
./target/debug/axioo-ctl kbd set --preset blue --dry-run
sudo ./target/debug/axioo-ctl kbd set --preset blue --brightness 200
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
