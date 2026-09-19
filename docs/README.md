# Buku Panduan Axioo Control Center

Anggap ini daftar isi buku. Kamu tidak perlu baca semuanya — pilih
jalurmu di bawah, bukunya yang mengantar.

## Jalur cepat (10 menit, pemilik laptop)

Baru install dan mau tahu laptopmu bisa apa:

1. `support-device.md` — cari modelmu di tabel, selesai.
2. Mentok sesuatu? `packaging.md` — install, autostart, uninstall.

## Jalur kontributor (mau nambah device)

1. `support-device.md` — bagian "Cara menambah modelmu".
2. `daemon.md` — kenalan sama daemon biar tahu datamu dipakai di mana.
3. `ec-fan-protocol.md` — kalau validasi kipas, ini kitabnya.

## Jalur bedah (penasaran cara kerja / mau ngoprek dalam)

1. `daemon.md` — otak sistem: profil, PPD sync, loop kipas, API D-Bus.
2. `ec-fan-protocol.md` — ngobrol langsung sama EC: register, kurva, kontrak tulis.
3. `kbd-backlight.md` — seluk-beluk lampu keyboard + quirk + restore.
4. `cc30-notes.md` — hasil intip installer Windows aslinya.
5. `per-key-rgb.md` — misteri RGB per-tombol yang (spoiler) tidak ada.

## Semua bab

| Bab | Isi singkat |
|---|---|
| `support-device.md` | Tabel dukungan + riset komunitas + cara tambah model |
| `matugen.md` | Theme + keyboard ikut wallpaper (kontrak `colors.json`) |
| `packaging.md` | AUR, systemd, udev, autostart, uninstall |
| `daemon.md` | `axiood`: profil, PPD sync, loop kipas, API D-Bus, file state |
| `ec-fan-protocol.md` | Protokol EC, kurva referensi, kontrak tulis, mode vendor |
| `kbd-backlight.md` | Quirk, zona, rantai restore keyboard |
| `per-key-rgb.md` | Kenapa per-key ditutup (arsip investigasi) |
| `cc30-notes.md` | Catatan bedah Control Center 3.0 Windows |

Aturan main repo (yang boleh dan tidak boleh disentuh, perintah
build/test) ada di `AGENTS.md` di root — itu sampul bukunya.
