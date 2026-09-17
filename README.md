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

Arch Linux, one command (no sudo — you will be asked when needed):

```sh
curl -fsSL https://raw.githubusercontent.com/Araryarch67/axioo-control-center/main/install.sh | bash
```

From a local checkout instead:

```sh
./setup.sh
```

Setup does everything: `clevo-drivers` from the AUR → Studio X
keyboard quirk → release build + AppImage → installs the app, the
`axiood` daemon, udev rules, and a desktop entry. Full progress in
`/tmp/axioo-setup.log`; noisy mode: `./setup.sh --verbose`.
Full removal: `./uninstall.sh`.

Requirements: kernel headers + `yay` (or another AUR helper — one line
in `setup.sh`). Manual driver steps: see
[`docs/kbd-backlight.md`](docs/kbd-backlight.md).

## Verify the install

```sh
./target/debug/axioo-ctl probe          # hardware capabilities
./target/debug/axioo-ctl kbd status     # expect 5 nodes: 4 zones + rear
./target/debug/axioo-ctl fan dump       # needs sudo + modprobe ec_sys
ls /sys/class/leds/ | grep kbd          # rgb:kbd_backlight{,_1,_2,_3,_4}
```

Then launch the app (or `cd tauri-app && ./dev.sh`).

## What's inside

| Tab | What you get |
|---|---|
| Dashboard | CPU/GPU temps, clocks, fans, battery, memory, package power — one glance |
| Performance | Balanced/Entertainment/Performance modes + quiet-fan, package power caps |
| Fan | Curve / Manual / EC-auto modes, **draggable** fan curve, live RPM + EC status |
| Keyboard | 4 zones + 1 rear, presets + **custom RGB picker**, 12 animated effects + independent rear-exhaust FX, **Follow wallpaper** (matugen), live visualizer |
| Power | Battery, FlexiCharger start/end thresholds, CPU package power |
| Settings | Theme (incl. **MATUGEN** — follows wallpaper live), start-at-login (tray), service status |

## Not working?

| Symptom | Fix |
|---|---|
| Keyboard dark / `no LED found` | Quirk not installed → re-run `./setup.sh`, see `docs/kbd-backlight.md` |
| Fan control locked | EC map not validated on your machine → `fan dump` + read `docs/ec-fan-protocol.md` |
| App can't write LEDs/battery | Udev rule not active → re-run `./setup.sh`, then reboot |

> **Tested only on the Pongo Studio X 2025 (X560WNR-SU9).**
> Other Pongo models: install works, but fan writes stay locked until
> the EC map is validated — start read-only (`probe`, `fan dump`,
> `kbd status`). Got another model? Open an issue with your `probe`
> output + DMI.

## Supported hardware

Studio X 2025 is a **Clevo X560WNR** barebone — its profile also covers
same-barebone rebrands (still requires per-machine `validate`, vendor
firmware may differ slightly):

| Brand | Model |
|---|---|
| Axioo | Pongo Studio X 2025 (X560WNR-SU9) ✅ tested |
| Sager / Xotic PC | NP9561R (X560WNR1-G) — same, no tester yet |
| AVADirect | X560WNR-G — same, no tester yet |

> Keyboard dead after a kernel update? Rebuild DKMS:
> `sudo dkms autoinstall` then `sudo modprobe -r tuxedo_keyboard &&
> sudo modprobe tuxedo_keyboard` (tuxedo drivers are fragile across
> new kernels).

## Details

* How it works + safety contract: [`AGENTS.md`](AGENTS.md)
* EC fan protocol: [`docs/ec-fan-protocol.md`](docs/ec-fan-protocol.md)
* Backlight + quirk: [`docs/kbd-backlight.md`](docs/kbd-backlight.md)
* Why there is no per-key RGB: [`docs/per-key-rgb.md`](docs/per-key-rgb.md)
* AUR/systemd/udev packaging: [`docs/packaging.md`](docs/packaging.md)

## Credits

Built on community Clevo/Axioo Linux work —
[hajilok](https://github.com/hajilok/clevo-axioo-dual-fan-linux),
[kkrdwn](https://github.com/kkrdwn/pongo725-backlight),
[System76](https://github.com/pop-os/system76-dkms),
[tuxedo-drivers](https://github.com/tuxedocomputers/tuxedo-drivers) —
plus [novacustom](https://novacustom.com/clevo-keyboard-backlight-control-for-linux/),
[ejcosta](https://github.com/ejcosta/clevo-keyboard-backlight),
[JAmanOG](https://github.com/JAmanOG/colorful-p15-keyboard-backlight),
[arbitrary-string](https://github.com/arbitrary-string/clevo-control-panel).
Daemon architecture follows [tuxedo-rs](https://github.com/AaronErhardt/tuxedo-rs).
Thanks everyone 🙏

## License

[GNU Affero General Public License v3.0 or later](LICENSE).
