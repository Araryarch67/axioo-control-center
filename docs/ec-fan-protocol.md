# Protokol EC Fan (Clevo) — referensi untuk `axioo-ctl`

Sumber: [hajilok/clevo-axioo-dual-fan-linux](https://github.com/hajilok/clevo-axioo-dual-fan-linux)
(fork dari `SkyLandTW/clevo-indicator`), cross-check lawan
`tuxedo-fan-control` (`native/ec_access.cc`).
Terverifikasi di: P7xxDM, N1xxED, NH5x_7x, ND/NE, **Axioo Pongo 725**.

> ⚠️ Status di Pongo Studio X (2025, X560WNR-SU9): **TERVALIDASI read-only
> (2026-09-13, satu sampel via `axioo-ctl fan dump`)**: `0x07`=61C vs
> coretemp package 64.0C (selisih 3C), `0xD0/0xD1`→2422 RPM dan
> `0xD2/0xD3`→2015 RPM **persis sama** dengan kedua node `acpi_fan`
> hwmon. GPU temp `0xCD`=52C (dGPU bangun, bukan 0).
> EC generasi baru ini ternyata memakai peta yang sama.
> Tinggal konfirmasi satu sampel saat load (RPM ikut naik) —
> lihat `fan watch` di bawah.
> Aturan safety tetap: tulis EC hanya dari `axiood` di masa depan,
> tidak pernah dari CLI/GUI langsung.

## A. Akses I/O

| Nama     | Port |
|----------|------|
| `EC_SC`  | `0x66` |
| `EC_DATA`| `0x62` |

Butuh hak root (`ioperm`). Alternatif baca: `/sys/kernel/debug/ec/ec0/io`
(`modprobe ec_sys`, root + debugfs).

## B. Tulis duty kipas — `ec_io_do(cmd, port, value)`

```
tunggu IBF=0 → outb(0x99, 0x66)
tunggu IBF=0 → outb(fan_index, 0x62)
tunggu IBF=0 → outb(raw_duty, 0x62)
tunggu IBF=0
```

| Parameter   | Nilai |
|-------------|-------|
| cmd         | `0x99` |
| fan_index   | `0x01` = CPU, `0x02` = GPU, (`0x03` = fan ke-3 bila ada) |
| mode AUTO   | `cmd=0x99, port=0xFF, value=fan_index` |
| raw duty    | `pct / 100 * 255` (byte) |
| batas aman  | **40–100%** (kipas stall di bawah ~40%) |

Perbedaan kunci vs `clevo-indicator` upstream: upstream hanya menulis
index `0x01` (kipas CPU saja) → kipas GPU mati. Fork ini menulis
`0x01` **dan** `0x02` setiap kali.

## C. Baca register — `ec_io_read(reg)`

```
tunggu IBF=0 → outb(0x80, 0x66)
tunggu IBF=0 → outb(reg, 0x62)
tunggu OBF=1 → inb(0x62)
```

| Register | Isi |
|----------|-----|
| `0x07` | suhu CPU (°C langsung) |
| `0xCD` | suhu GPU (°C; 0 = GPU idle/Optimus tidur) |
| `0xCE` | duty kipas 1 (cermin nilai terakhir ditulis) |
| `0xD0` / `0xD1` | RPM kipas 1 (hi/lo) |
| `0xD2` / `0xD3` | RPM kipas 2 / GPU (hi/lo) |

Konversi:

```
duty_pct = raw / 255 * 100
rpm      = raw16 == 0 ? 0 : 2156220 / raw16,  raw16 = (hi << 8) | lo
```

## D. Kurva auto referensi (dengan hysteresis)

`temp = max(cpu, gpu)`. Naik: ≥80→100, ≥70→90, ≥60→80, ≥50→70,
≥40→60, ≥30→50, ≥20→40. Turun: ≤65→90, ≤55→80, ≤45→70, ≤35→60,
≤25→50, ≤15→40. Diterapkan ke **kedua** fan.

## E. Model privilege (pelajaran arsitektur)

Program C memakai **satu binary setuid-root**: worker (root) untuk I/O
port EC + UI GTK (turun ke UID desktop) untuk tray. Untuk `axioo-ctl`
kita pakai model yang lebih bersih dan sama amannya:

```
axiood (systemd service, root)  ← satu-satunya penulis EC
   ↕ D-Bus (com.axioo.Control)
axioo-ctl (user) / axioo-control-center GUI (user, GPUI)
```

## F. Langkah validasi di Pongo Studio X (read-only, aman)

1. `sudo modprobe ec_sys`
2. `sudo ./target/debug/axioo-ctl fan dump` (decode + cross-check
   otomatis vs coretemp/`acpi_fan`), atau
   `sudo ./target/debug/axioo-ctl fan watch -i 1` untuk rekam
   idle→load dalam satu sesi.
3. Cross-check: byte `0x07` ≈ suhu package coretemp;
   `0xD0–0xD3` via rumus RPM ≈ bacaan `acpi_fan` hwmon
   (`axioo-ctl probe` sudah menampilkan keduanya).
4. Kalau cocok → protokol valid untuk model ini, lanjut ke
   `axioo-ctl fan curve` (preview) lalu desain `axiood`.
   Kalau tidak → cari register yang berubah
   saat kipas berputar (bandingkan dump idle vs load;
   kandidat alternatif `0xD4/0xD5` ikut dicetak `fan dump`).
