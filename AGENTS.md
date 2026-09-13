# AGENTS.md — axioo-control-center

Utilitas kontrol hardware untuk laptop Axioo (basis Clevo) di Linux, Rust.
Pengganti Clevo Control Center Windows. Workspace Cargo: `axioo-lib`,
`axioo-ctl`, `axioo-gui`. Docs: `docs/ec-fan-protocol.md`, `docs/kbd-backlight.md`.

## Perintah

```sh
cargo build -p axioo-lib -p axioo-ctl   # build yang stabil
cargo test -p axioo-lib                 # unit test (protokol EC, kbd)
./target/debug/axioo-ctl probe          # dump kapabilitas hardware
./target/debug/axioo-ctl monitor        # dashboard live
./target/debug/axioo-ctl kbd status     # status backlight keyboard
cargo check -p axioo-gui                # ⚠️ GAGAL saat ini (lihat #1)
```

> Jangan `cargo build` workspace-wide sebelum #1 beres — `axioo-gui`
> gagal compile (lihat bawah). Jangan commit tanpa diminta eksplisit.

## Aturan safety (wajib)

- `axioo-lib` saat ini READ-ONLY kecuali `kbd::set` (sysfs LED, aman).
- **Dilarang menulis ke EC** (`0x62/0x66`, `ec_sys` write, WMI method
  call pemicu EC) sampai peta EC model target tervalidasi read-only
  (`docs/ec-fan-protocol.md` bagian F). Tulis EC hanya milik daemon
  root di masa depan (`axiood`), tidak pernah dari CLI/GUI langsung.
- Quirk driver kernel dibatasi DMI board + tipe tak dikenal saja;
  jangan override tipe yang sudah dikenal driver.

## Status fitur

| # | Fitur | Status |
|---|-------|--------|
| 0 | `probe`, `monitor`, `kbd status/get/set`, docs, README+credits | ✅ selesai, terverifikasi di Pongo Studio X (2025) |
| 1 | GUI live sensor (GPUI) | ❌ BROKEN — `main.rs` pakai `AsyncApp`+`Entity` di `std::thread`, keduanya `!Send` |
| 2 | Backlight keyboard Studio X | 🔄 pending validasi user: quirk DMI `0x17`→1-zone sudah didesain, menunggu hasil rebuild DKMS |
| 3 | Kontrol kipas (`axioo-ctl fan`) | ✅ peta TERVALIDASI idle di Studio X (2026-09-13: `0x07`=61C vs pkg 64C, RPM EC persis = hwmon 2422/2015, konsisten 5+ sampel); tooling read-only DONE (`fan dump` + `fan watch` + `fan curve` + tests); konfirmasi tracking saat load DIPARKIR atas permintaan user → langsung desain `axiood` saat dibutuhkan |
| 4 | Daemon `axiood` + D-Bus + profil | ⬜ belum mulai |
| 5 | Profil CPU/GPU (RAPL, cpufreq, NVIDIA) | ⬜ belum mulai |
| 6 | Charge threshold baterai | ⬜ belum mulai |
| 7 | Fn-keys brightness di Hyprland | ⬜ diblokir oleh #2 (butuh LED dulu) |
| 8 | Packaging AUR + systemd + udev | ⬜ belum mulai |
| 9 | Upstream quirk ke clevo-drivers | ⬜ setelah #2 terbukti |
| 10 | Per-key RGB sejati via EC | ⬜ riset; butuh reverse-engineering |

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
