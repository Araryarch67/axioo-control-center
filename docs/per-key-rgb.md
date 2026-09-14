# Per-key RGB via EC — riset

> Status: riset, belum diimplementasi. Jalur USB `048d:…` tidak ada di Pongo Studio X (lsusb kosong),
> satu-satunya jalur adalah EC.

## Temuan
- Keyboard 4 zona sudah 100%: quirk `0x17 → 3-zone` + zona-4 numpad EC `0x0B` ala System76 (`packaging/clevo-drivers-axioo/studiox-numpad-ec.patch`).
- EC fan map `0x07/0xCD/0xD0-0xD3/0xCE` tervalidasi idle vs hwmon (5+ sampel). Tulis one-shot via `axioo-lib::fan_ctrl` aman (clamp 40-100%).
- Per-key membutuhkan dump register EC saat Control Center Windows ganti warna per-tombol (zona `0xF3…` di beberapa model Clevo). Belum ada capture.

## Rencana capture
1. Boot Windows di Studio X, install Clevo Control Center ori.
2. ACPICA trace WMI `SET_KB_RGB_LEDS` atau `ec_sys` dump (RwEverything / io port 0x62/0x66 log) saat switch dari mode 3-zone ke per-key.
3. Bandingkan dump EC sebelum/sesudah; cari range yang berubah (kandidat `0xF3-0xFF`).
4. Replikasi tulis via `ec_io_do(0x99,…)` atau WMI `0x99` dengan payload per-key, heavily test di jednej.

## Referensi
- `docs/kbd-backlight.md` — quirk 1-zone vs 3-zone
- `docs/ec-fan-protocol.md` — protokol EC fan (port 0x62/0x66 cmd 0x99/0x80)
- `axioo-lib/src/ec.rs` — `read_map()` via `ec_sys` debugfs
