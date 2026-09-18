# axiood — daemon privileged (EC fan loop + RAPL + PPD sync)

`axiood` adalah service systemd root — **satu-satunya penulis EC untuk
loop kontinu**. CLI (`axioo-ctl profile/...`) dan GUI Tauri adalah klien
tipis via D-Bus `com.axioo.Control` (`/com/axioo/Control`).
EPP/governor **sengaja tidak disentuh** (milik PPD).

```sh
sudo systemctl enable --now axiood
axioo-ctl profile get
axiood --help   # --interval SECS --profile NAME --no-fan --no-rapl --no-ppd
```

Butuh root (tulis EC + RAPL); tanpa root daemon keluar (exit 1) agar
systemd me-restart dengan konteks yang benar. Unit: `docs/packaging.md`.

## Profil (3 mode + quiet-fan per mode)

| Profil Axioo | PPD | RAPL PL1/PL2 | Kipas normal |
|---|---|---|---|
| Balanced | `power-saver` | 44W / 120W | kurva referensi EC |
| Entertainment | `balanced` | 44W / 160W | kurva referensi EC |
| Performance | `performance` | 44W / 160W | kurva referensi EC |

Profil hanya beda di RAPL/PPD — **kurva kipas normal SAMA semua**
(meniru kurva auto EC firmware, `REFERENCE_CURVE`; kurva agresif custom
terbukti terlalu berisik). `quiet-fan` murni opsi kipas per mode:
**pin 40% di semua suhu** (garis datar di chart GUI; CPU akan throttle
saat load berat — proteksi termal firmware tetap jalan). Legacy `Quiet`
= Balanced + quiet-fan on.

## Two-way sync PPD

- Boot: sumber kebenaran = PPD `ActiveProfile` → dipetakan ke profil
  Axioo (`--profile` override bila diberikan).
- GUI/CLI set profil → daemon set PPD balik + apply RAPL + ganti kurva
  (via flag `pending_apply`, dieksekusi di tick loop).
- Polling tiap 3 dtk: perubahan PPD dari luar (mis. slider GNOME)
  dipetakan balik ke profil Axioo; flag `quiet` dipertahankan (PPD tak
  menyentuhnya).
- Pemetaan 1-ke-1: `power-saver`↔Balanced, `balanced`↔Entertainment,
  `performance`↔Performance. Nilai PPD tak dikenal = pertahankan profil
  saat ini.

## Loop kipas (tick 2 dtk, default)

Suhu = `max(CPU EC 0x07, GPU EC 0xCD)`, fallback coretemp. Tiga mode
(`docs/ec-fan-protocol.md` bagian G):

- `ec_auto` (**default**) — `set_auto()` satu-kali, lalu diam; firmware
  yang pegang. RAPL/PPD tetap ikut profil.
- `curve` — step `auto_duty_step` + hysteresis; tulis hanya bila duty
  berubah (via `fan_ctrl::set_manual_duty`, verify `0xCE`).
- `manual` — kunci duty user (clamp 40–100% di setter D-Bus).

Flag debug: `--no-fan` / `--no-rapl` / `--no-ppd` mematikan subsistem
masing-masing (interval 0.5–30 dtk via `--interval`).

## D-Bus API (`com.axioo.Control`)

| Method | Argumen | Perilaku |
|---|---|---|
| `GetProfile` / `SetProfile` | `profile: String` | get/set profil (`Balanced`/`Entertainment`/`Performance`, legacy `Quiet`); return label |
| `GetPpdProfile` | — | profil PPD terakhir dilihat daemon |
| `GetFanDuty` | — | duty terakhir ditulis loop (%) |
| `GetCurve` | — | kurva efektif `(temp, duty)` buat chart GUI; kosong bila `ec_auto`, datar bila manual |
| `GetQuietFan` / `SetQuietFan` | `quiet: bool` | toggle quiet-fan per mode |
| `GetFanMode` | — | `"curve"` / `"manual"` / `"ec_auto"` |
| `SetFanEcAuto` | `auto: bool` | serahkan ke firmware (loop `set_auto` satu-kali) |
| `SetFanManual` | `duty: u8` | kunci duty (clamp 40–100%; mematikan `ec_auto`) |
| `ClearFanOverride` | — | kembali ke kurva |
| `SetKbd` | `brightness, r, g, b, effect, rear, speed` | tulis sysfs + simpan `/var/lib/axiood/kbd.json` (user tak bisa tulis `/var/lib` langsung) |
| `GetKbd` | — | state tersimpan (JSON; `""` bila belum ada) |
| prop `Profile` | — | alias `GetProfile` |

Semua input numerik/string divalidasi server-side (klien JS/argv
untrusted): efek tak dikenal → `InvalidArgs`, brightness di-clamp,
speed di-clamp 0.1–4.0.

## State di disk

- `/var/lib/axiood/kbd.json` —
  `{"brightness":255,"r":255,"g":95,"b":86,"effect":"static","rear":"follow","speed":1.0}`.
  Ditulis tiap `SetKbd` + tiap `axioo-ctl kbd set` sebagai root.
  Di-restore daemon saat start (sebelum SDDM) + retry bila driver telat;
  hanya warna dasar statis (animasi efek jalan lagi setelah GUI login).
  Rantai restore lengkap: `docs/kbd-backlight.md`.

## CLI via daemon

```sh
axioo-ctl profile get                    # profil + quiet-fan + PPD
axioo-ctl profile set Performance
axioo-ctl profile quiet-fan on|off|toggle|status
```

## Test

```sh
cargo test -p axiood   # PPD roundtrip + quiet-pin/curve tests
```
