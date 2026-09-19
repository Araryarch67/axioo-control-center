# Dukungan Perangkat

Punya Pongo? Aplikasi ini kenalan dulu sama laptopmu, terus nyalain
fitur yang cocok — yang belum cocok dikunci dulu biar aman. Cari
modelmu di tabel bawah.

## Ringkasan

Axioo tidak membuat chassis sendiri — semua Pongo adalah rebrand. Dua
keluarga besar terlihat dari dimensinya:

| Model | Rebrand dari | Status | Kipas | Keyboard |
|---|---|---|---|---|
| Pongo Studio X 2025 | Clevo X560WNR (terkonfirmasi DMI) | Penuh | Atur manual dan otomatis | RGB 5 zona |
| Pongo 535 / 725 / 725 v2 / 735 / 750 / 760 / 960 | Clevo NH5x, satu chassis 15,6 inci DDR4 (725 = NP50RNA terkonfirmasi) | Hampir penuh | Terkunci (pantau suhu saja) | 1 zona, langsung jalan |
| Pongo 755 / 755 v2 / 765 / 765 v2 / 775 | Chassis 16 inci DDR5 baru, barebone belum dikenal | Menunggu data | Terkunci (pantau suhu saja) | Mengikuti driver generik |
| Pongo 760 V2 | Keluarga 15,6 inci di atas | Khusus | Terkunci (firmware menolak diatur) | Mengikuti driver generik |
| Laptop non-Axioo | — | Info dasar saja | Terkunci | — |

Terkunci artinya aman dipakai — tidak akan merusak, hanya belum bisa
diatur sampai petanya tervalidasi. Baris "menunggu data" terisi setelah
satu pemilik mesin mengirim hasil `axioo-ctl probe --json` (tanpa root).

Catatan: batas daya RAPL mengikuti profil (bukan per model), jadi di
mesin beda kelas cek `probe` dulu. RGB per-tombol tidak ada di model
mana pun (keterbatasan firmware, lihat `docs/per-key-rgb.md`).

## Repo berkaitan (per keluarga)

Keluarga 15,6 inci DDR4 (NH5x / NP50RNA):

- [hajilok/clevo-axioo-dual-fan-linux](https://github.com/hajilok/clevo-axioo-dual-fan-linux) —
  kontrol dua kipas, ditest di Pongo 725 (CPU 6200 / GPU 5600 RPM).
  Protokol EC-nya sama dengan yang dipakai di sini.
- [kkrdwn/pongo725-backlight](https://github.com/kkrdwn/pongo725-backlight) —
  keyboard 725 satu zona via sysfs, driver generik tanpa patch.

Pola umum semua Clevo rebrand:

- [JAmanOG/colorful-p15-keyboard-backlight](https://github.com/JAmanOG/colorful-p15-keyboard-backlight) —
  pola force-type per model untuk firmware yang lapor tipe asing.
- [arbitrary-string/clevo-control-panel](https://github.com/arbitrary-string/clevo-control-panel) —
  app + CLI untuk Clevo/Tongfang generik (keyboard zona, threshold
  baterai, mode performa), backend auto-deteksi.

## Yang kami pelajari dari komunitas

**Kipas satu famili.** Pongo 725 (Clevo NH5xR/NP50RNA) memakai protokol EC
yang sama dengan Studio X: perintah `0x99`, kipas CPU `0x01`, kipas GPU
`0x02`, RPM di `0xD0–0xD3`, duty 40–100. Pelajaran pentingnya: selalu
tulis kedua kipas — tool lama yang cuma tulis `0x01` bikin kipas GPU mati.
Sumber: [hajilok/clevo-axioo-dual-fan-linux](https://github.com/hajilok/clevo-axioo-dual-fan-linux)
(ditest di 725). Peta ini kemungkinan port ke 725/735/750 sekeluarga,
tetap wajib validasi per model.

**Keyboard satu pola.** Driver menanyakan tipe backlight ke firmware;
tipe yang tidak dikenal (Studio X: `0x17`) berarti tidak ada LED sampai
di-force. Proyek [JAmanOG/colorful-p15-keyboard-backlight](https://github.com/JAmanOG/colorful-p15-keyboard-backlight)
menggeneralisasi ini jadi pilihan force-type per model — arah yang sama
dengan resep quirk di sini. Keyboard 725 sendiri jalan dengan driver
generik tanpa quirk
([kkrdwn/pongo725-backlight](https://github.com/kkrdwn/pongo725-backlight)).

**Peringatan 760 V2.** Pongo 760 V2 dilaporkan kipasnya tidak pernah
menyala di Linux dan tulis EC manual mental seketika (EC pegang penuh).
Model seperti ini hasil validasinya gagal dengan benar dan tetap
terkunci — itu pengaman yang bekerja.
Sumber: [r/linuxhardware](https://www.reddit.com/r/linuxhardware/comments/1pm8tgh/laptop_fans_never_spin_on_linux_ec_appears_to/)

## Cara menambah modelmu

Satu perintah, jalan lokal (EC read-only, tanpa tulis kipas):

1. `sudo modprobe ec_sys` (sekali, agar EC terbaca).
2. `axioo-ctl validate` — sampling idle, beban CPU otomatis, jalan-jalan
   zona keyboard (jawab y/n/skip), lalu laporan JSON tanpa `product_uuid`.
3. Lolos semua? `sudo axioo-ctl validate --apply` membuka kunci mesinmu
   langsung — tanpa update app. Buka URL issue yang dicetak agar modelmu
   dapat entri bernama di rilis berikut.

## Dua file lokal (tanpa network, tanpa rebuild)

- `devices.toml` (tabel model: nama, grade, catatan) dibaca dari
  `$AXIOO_DEVICES`, `/etc/axioo-control-center/devices.toml`,
  `/usr/share/axioo-control-center/devices.toml`, lalu bawaan app.
  Tambah model = edit file teks. `probe` tampilkan sumber tabel yang aktif.
- `/var/lib/axiood/validated` (fakta mesin ini: DMI persis, verdict,
  jumlah zona, RAPL stock, sampel EC) ditulis sekali oleh
  `validate --apply`, dibaca daemon/CLI/GUI. Root-only, tanpa UUID,
  tidak pernah dikirim ke mana pun.
