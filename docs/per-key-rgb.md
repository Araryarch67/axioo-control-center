# Per-key RGB via EC — DITUTUP (hardware tidak support)

> Status: DITUTUP 2026-09-16 — Control Center Windows asli di mesin ini
> TIDAK punya mode per-key. Satu-satunya jalur adalah EC dan peta zona
> firmware hanya berisi zona (keyboard 3/4/5, lightbar `0x07`, kandidat
> `0x06/0x09/0x0A` — rear terbukti 1 zona, lihat `docs/kbd-backlight.md`).
> Jalur USB `048d:…` tidak ada di Pongo Studio X (lsusb kosong).
> Kesimpulan: tidak ada protokol per-key yang bisa di-capture karena
> modenya tidak ada di firmware ini. Efek Wave/Rainbow/dkk di GUI sudah
> meng-cover animasi multi-warna di atas hardware zona.

## Temuan
- Keyboard 5 zona sudah 100%: quirk `0x17 → 3-zone` + zona-4 numpad EC `0x0B` ala System76 (`packaging/clevo-drivers-axioo/studiox-numpad-ec.patch`) + rear lightbar EC `0x07`, putih otomatis habis install driver.
- EC fan map `0x07/0xCD/0xD0-0xD3/0xCE` tervalidasi idle vs hwmon (5+ sampel). Tulis one-shot via `axioo-lib::fan_ctrl` aman (clamp 40-100%).
- Per-key membutuhkan dump register EC saat Control Center Windows ganti warna per-tombol (zona `0xF3…` di beberapa model Clevo). DITUTUP 2026-09-16: Control Center Windows di mesin ini tidak punya mode per-key — tidak ada yang bisa di-capture.

## Rencana capture (BATAL — modenya tidak ada di Windows mesin ini)
1. ~~Boot Windows di Studio X, install Clevo Control Center ori.~~
2. ~~ACPICA trace WMI `SET_KB_RGB_LEDS` atau `ec_sys` dump (RwEverything / io port 0x62/0x66 log) saat switch dari mode 3-zone ke per-key.~~
3. ~~Bandingkan dump EC sebelum/sesudah; cari range yang berubah (kandidat `0xF3-0xFF`).~~
4. ~~Replikasi tulis via `ec_io_do(0x99,…)` atau WMI `0x99` dengan payload per-key, heavily test di jednej.~~

Helper read-only (tidak menulis EC): `packaging/per-key-capture/capture-ec.sh`
(`before`/`after`/`diff` — memakai `axioo-ctl probe` + `fan dump` + `kbd status/get`
+ hexdump `ec_sys`). Jalankan `before` sebelum ke Windows dan `after` setelah
kembali ke Linux, lalu `diff` untuk melihat kandidat register yang berubah.

## Referensi
- `docs/kbd-backlight.md` — quirk 1-zone vs 3-zone
- `docs/ec-fan-protocol.md` — protokol EC fan (port 0x62/0x66 cmd 0x99/0x80)
- `axioo-lib/src/ec.rs` — `read_map()` via `ec_sys` debugfs
