<div align="center">

# Axioo Control Center

[![License: AGPL v3](https://img.shields.io/badge/License-AGPL_v3-blue.svg)](LICENSE)
[![Platform](https://img.shields.io/badge/platform-Linux-lightgrey.svg)]()
[![Rust](https://img.shields.io/badge/built_with-Rust-orange.svg)]()
![Tested](https://img.shields.io/badge/tested-Pongo_Studio_X_2025-success.svg)

**Linux hardware control for Axioo laptops (Clevo-based) — a from-scratch
Rust replacement for the Windows-only Clevo Control Center.**

*Live sensors · fan control · 4-zone RGB keyboard + rear exhaust · charge limits.*

</div>

## Install

Arch Linux, satu perintah (tanpa sudo — diminta saat perlu):

```sh
./setup.sh
```

Setup mengerjakan semuanya: driver `clevo-drivers` dari AUR → quirk
keyboard Studio X → build release + AppImage → install app, daemon
`axiood`, udev rule, entri desktop. Progress detail di
`/tmp/axioo-setup.log`; mode berisik: `./setup.sh --verbose`.
Bongkar total: `./uninstall.sh`.

Syarat: kernel headers + `yay` (atau AUR helper lain — edit 1 baris di
`setup.sh`). Butuh driver manual? Lihat
[`docs/kbd-backlight.md`](docs/kbd-backlight.md).

## Cek instalasi

```sh
./target/debug/axioo-ctl probe          # kapabilitas hardware
./target/debug/axioo-ctl kbd status     # expect 5 node: 4 zona + rear
./target/debug/axioo-ctl fan dump       # butuh sudo + modprobe ec_sys
ls /sys/class/leds/ | grep kbd          # rgb:kbd_backlight{,_1,_2,_3,_4}
```

Lalu buka app-nya (atau `cd tauri-app && ./dev.sh`).

## Isinya

| Tab | What you get |
|---|---|
| Dashboard | CPU/GPU temps, clocks, fans, battery, memory, package power — one glance |
| Performance | Balanced/Entertainment/Performance modes + quiet-fan, package power caps |
| Fan | Curve / Manual / EC-auto modes, **draggable** fan curve, live RPM + EC status |
| Keyboard | 4 zones + 1 rear, presets + **custom RGB picker**, 12 animated effects + independent rear-exhaust FX, **Ikuti wallpaper** (matugen), live visualizer |
| Power | Battery, FlexiCharger start/end thresholds, CPU package power |
| Settings | Theme (incl. **MATUGEN** — follows wallpaper live), start-saat-login (tray), service status |

## Nggak jalan?

| Gejala | Obat |
|---|---|
| Keyboard gelap / `no LED found` | Quirk belum kepasang → `./setup.sh` ulang, lihat `docs/kbd-backlight.md` |
| Fan control dikunci | Map EC belum tervalidasi di mesinmu → `fan dump` + baca `docs/ec-fan-protocol.md` |
| App nggak bisa nulis LED/baterai | Udev rule belum aktif → `./setup.sh` ulang lalu reboot |

> **Tested only on the Pongo Studio X 2025 (X560WNR-SU9).**
> Pongo lain: install jalan, tapi tulis kipas dikunci sampai map EC
> tervalidasi — mulai read-only (`probe`, `fan dump`, `kbd status`).
> Punya model lain? Buka issue dengan output `probe` + DMI.

## Hardware yang didukung

Studio X 2025 = barebone **Clevo X560WNR** — profilnya berlaku juga
buat rebrand satu barebone (tentap wajib `validate` per mesin,
firmware tiap merek bisa beda dikit):

| Merek | Model |
|---|---|
| Axioo | Pongo Studio X 2025 (X560WNR-SU9) ✅ tested |
| Sager / Xotic PC | NP9561R (X560WNR1-G) — sama, belum ada tester |
| AVADirect | X560WNR-G — sama, belum ada tester |

> Habis update kernel dan keyboard mati? Rebuild DKMS:
> `sudo dkms autoinstall` lalu `sudo modprobe -r tuxedo_keyboard &&
> sudo modprobe tuxedo_keyboard` (driver tuxedo rapuh lawan kernel baru).

## Detail

* Cara kerja + safety contract: [`AGENTS.md`](AGENTS.md)
* Protokol EC kipas: [`docs/ec-fan-protocol.md`](docs/ec-fan-protocol.md)
* Backlight + quirk: [`docs/kbd-backlight.md`](docs/kbd-backlight.md)
* Kenapa nggak ada per-key RGB: [`docs/per-key-rgb.md`](docs/per-key-rgb.md)
* Packaging AUR/systemd/udev: [`docs/packaging.md`](docs/packaging.md)

## Credits

Berdiri di atas kerja komunitas Clevo/Axioo Linux —
[hajilok](https://github.com/hajilok/clevo-axioo-dual-fan-linux),
[kkrdwn](https://github.com/kkrdwn/pongo725-backlight),
[System76](https://github.com/pop-os/system76-dkms),
[tuxedo-drivers](https://github.com/tuxedocomputers/tuxedo-drivers) —
plus [novacustom](https://novacustom.com/clevo-keyboard-backlight-control-for-linux/),
[ejcosta](https://github.com/ejcosta/clevo-keyboard-backlight),
[JAmanOG](https://github.com/JAmanOG/colorful-p15-keyboard-backlight),
[arbitrary-string](https://github.com/arbitrary-string/clevo-control-panel).
Arsitektur daemon mengikuti [tuxedo-rs](https://github.com/AaronErhardt/tuxedo-rs).
Makasih semuanya 🙏

## License

[GNU Affero General Public License v3.0 or later](LICENSE).
