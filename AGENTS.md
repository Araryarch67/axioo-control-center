# AGENTS.md — axioo-control-center

Utilitas kontrol hardware untuk laptop Axioo (basis Clevo) di Linux, Rust.
Pengganti Clevo Control Center Windows. Workspace Cargo: `axioo-lib`,
`axioo-ctl`, `axiood`, `tauri-app/src-tauri` (`axioo-center`, ganti `axioo-gui`/GPUI yang dihapus 2026-09-14). Docs: `docs/ec-fan-protocol.md`, `docs/kbd-backlight.md`, `docs/packaging.md`, `docs/per-key-rgb.md`.

## Perintah

```sh
cargo check --workspace               # ✅ hijau (termasuk axioo-center + axiood)
cargo test -p axioo-lib               # unit test (protokol EC, kbd, kurva)
cargo test -p axiood                  # daemon profile/PPD tests
./target/debug/axioo-ctl probe          # dump kapabilitas hardware
./target/debug/axioo-ctl monitor        # dashboard live
./target/debug/axioo-ctl fan dump       # butuh sudo + modprobe ec_sys
./target/debug/axioo-ctl profile get    # via axiood + PPD
./target/debug/axioo-ctl battery status # FlexiCharger thresholds
./target/debug/axioo-ctl kbd status     # status backlight keyboard
./target/debug/axioo-center             # GUI Tauri (atau cd tauri-app && ./dev.sh)
sudo systemctl enable --now axiood     # daemon (EC + RAPL + PPD sync)
```

> Jangan commit tanpa diminta eksplisit.

## Aturan safety (wajib)

- `axioo-lib` READ-ONLY kecuali `kbd::set` (sysfs LED, aman) dan
  `fan_ctrl` (one-shot tulis EC: root-only, clamp 40–100%, kedua fan,
  verify `0xCE` — disetujui 2026-09-13 setelah peta tervalidasi +
  `fan set 100` terbukti via CLI).
- **Loop kurva kontinu / tulis EC selain one-shot di atas hanya milik
  daemon root (`axiood`), tidak pernah dari CLI/GUI langsung.**
  GUI Tauri jalan sebagai user (webview tidak boleh root), one-shot
  langsung via `fan_ctrl` DITOLAK bila daemon jalan; mode manual/EC-auto
  lewat D-Bus daemon. (Aturan relaunch-pkexec milik GUI GPUI lama yang
  sudah dihapus.)
- Quirk driver kernel dibatasi DMI board + tipe tak dikenal saja;
  jangan override tipe yang sudah dikenal driver.

## Status fitur

| # | Fitur | Status |
|---|-------|--------|
| 0 | `probe`, `monitor`, `kbd status/get/set`, docs, README+credits | ✅ selesai, terverifikasi di Pongo Studio X (2025) |
| 1 | GUI live sensor (Tauri, ganti GPUI 2026-09-14) | ✅ COMPILE HIJAU + runtime stabil; tab Dashboard/Performance/Fan/Keyboard/Power/Settings, gauge, chart kurva + editor drag, kartu GPU/Mem/Baterai/RAPL, pill EC; state via zustand persist (`tauri-app/src/lib/store.ts`); tray icon + autostart login (`tauri-plugin-autostart`, `--minimized`, close-to-tray) — `cargo check` hijau, `tsc` hijau |
| 2 | Backlight keyboard Studio X | ✅ quirk TERBUKTI (`packaging/clevo-drivers-axioo`, force 3-zone `0x17` + numpad `0x0B` + rear lightbar `0x07` = 5 node, putih otomatis habis install): tulis OK via CLI; panel GUI Keyboard DONE (12 efek + rear FX independen + preview live) |
| 3 | Kontrol kipas (`axioo-ctl fan`) | ✅ peta TERVALIDASI idle (2026-09-13: `0x07`=61C vs pkg 64C, RPM EC=hwmon 2422/2015, 5+ sampel); tooling read-only DONE (`fan dump/watch/curve` + tests) + one-shot `set/auto` via `fan_ctrl` (clamp 40-100%, verify `0xCE`) |
| 4 | Daemon `axiood` + D-Bus + profil | ✅ crate `axiood` + `axioo-ctl profile` + backend Tauri (`get_snapshot`/`set_profile`/fan/kbd via `com.axioo.Control`) two-way PPD sync (B→balanced/E+P→perf, `power-saver`→Balanced+quiet), EC fan loop + RAPL PL1/PL2, `com.axioo.Control` bus, `axiood.service` — `cargo test -p axiood` 2 ok |
| 5 | Profil CPU/GPU (RAPL, cpufreq, NVIDIA) | ✅ RAPL `rapl_apply::apply_pl1_pl2` via axiood (Balanced 44/120W, Ent/Perf 44/160W); EPP/governor milik PPD (sengaja tidak disentuh); GPU `nvidia::gpus` probe OK, `power.limit` N/A di mesin ini |
| 6 | Charge threshold baterai | ✅ `axioo-lib::battery` + `axioo-ctl battery status/get/set` (standard `charge_control_*`, BAT0 start 40-95 end 60-100); `axioo-ctl battery set --start/--end` validasi + butuh root |
| 7 | Fn-keys brightness di Hyprland | ✅ `kbd brighter/dimmer` DONE + LED ada; bind Hyprland ditunda per permintaan (siap: `XF86KbdBrightnessUp/Down` → `axioo-ctl kbd brighter/dimmer`) |
| 8 | Packaging AUR + systemd + udev | ✅ `packaging/aur/PKGBUILD` + `udev/99-axioo-kbd.rules` + `axiood.service` + `com.axioo.Control.*`; `setup.sh` install daemon/udev/dbus, `uninstall.sh` clean; docs `docs/packaging.md` |
| 9 | Upstream quirk ke clevo-drivers | ✅ dianggap selesai per user 2026-09-16 (patch tetap di `packaging/clevo-drivers-axioo/studiox-kbd-quirk.patch` + panduan `UPSTREAM.md` bila nanti mau dikirim ke `nick42d/clevo-drivers`) |
| 10 | Per-key RGB sejati via EC | ⛔ DITUTUP 2026-09-16: Control Center Windows di mesin ini tidak punya mode per-key — hardware/firmware tidak support; detail `docs/per-key-rgb.md` |

## Ide solusi per fitur

**#1 GUI live.** Ganti thread+`AsyncApp` dengan pola GPUI yang benar:
sampler thread hanya memiliki `mpsc::Sender<SensorData>` (semua `Send`),
UI update lewat foreground task (`cx.spawn` + `Timer::after` poll
`try_recv`, lalu `entity.update`). Alternatif: `cx.spawn_in(window, …)`.
Contoh API di `~/.cargo/registry/src/*/gpui-0.2.2/src/app/async_context.rs`.

**#2 Backlight Studio X.** Jika quirk 1-zone tidak direspons EC:
(a) coba force `0x02` (3-zone) — node LED 3 zona; (b) capture WMI
`SET_KB_RGB_LEDS` yang dikirim Control Center Windows (via ACPICA trace
di Windows atau reverse `setup.inx`); (c) terakhir: investigasi
protokol EC per-key (zona `0xF3…`). Jika quirk BERHASIL: upstream patch
ke `nick42d/clevo-drivers` (target AUR yang dipakai) dengan DMI quirk.

**#3 Fan.** Fase read-only dulu: `axioo-ctl fan dump` baca peta EC via
`ec_sys` (root), cross-check `0x07`≈coretemp dan `0xD0–0xD3`≈`acpi_fan`
RPM (keduanya sudah tampil di `probe`). Cocok → implementasi
`auto_duty_step` (sudah ada + tested) sebagai `fan auto`; tulis via
port I/O HANYA dari `axiood`. Duty clamp 40–100 (konstanta ada).

**#4 axiood.** Ikuti arsitektur `tailord` (tuxedo-rs): service systemd
root, bus `com.axioo.Control`, profil (silent/balanced/performance),
fan-curve loop, restore saat boot/resume. CLI/GUI jadi klien tipis.

**#5 CPU/GPU.** CPU: tulis `constraint_*_power_limit_uw` RAPL
(terbaca: PL1 44W/PL2 160W/psys 230W) + `scaling_governor`/EPP via
cpufreq (tidak ada `platform_profile` di mesin ini — jangan cari itu).
GPU: `power.limit` N/A via nvidia-smi di mesin ini → investigasi
NVML langsung atau `nvidia-settings` + Coolbits.

**#6 Baterai.** `/sys/class/power_supply/BAT0` tidak punya
`charge_control_*`. Petunjuk: dmesg menunjukkan hook
`TUXEDO Flexicharger Extension` — cari atribut flexicharger yang
diekspos driver (sysfs `tuxedo_`/`clevo_wmi`), kalau ada tinggal baca/
tulis dari `axioo-lib`.

**#7 Fn-keys.** Setelah LED ada: `tuxedo_keyboard` emit
`KEY_KBDILLUM{UP,DOWN,TOGGLE}` di input device `TUXEDO Keyboard`;
bind di `hyprland.conf` ke `axioo-ctl kbd set` (naik/turunkan brightness
relatif — butuh subcommand `kbd brighter/dimmer`, belum ada).

**#8 Packaging.** `PKGBUILD` AUR (`axioo-control-center-git`), unit
`axiood.service`, udev rule agar LED kbd writable grup `video`/`input`
(menghilangkan kebutuhan sudo harian).

**#10 Per-key.** Hanya jika #2 mode 1-zone terasa kurang. Referensi
protokol USB tidak berlaku (tidak ada device `048d:…` di mesin ini);
jalur satu-satunya adalah EC — butuh dumping register saat Control
Center Windows mengganti warna per-tombol.
