# Matugen (theme + keyboard ikut wallpaper)

Matugen itu generator palet warna dari wallpaper (gaya Material You).
Di app ini paletnya dipakai dua tempat: theme GUI "matugen" dan
"follow wallpaper" di tab Keyboard. Tanpa matugen, keduanya diam dan
app pakai theme statis — bukan error.

## Kontrak file (satu-satunya yang app butuhkan)

`~/.cache/ryoku/colors.json` — JSON datar `{kunci: "#rrggbb"}`.
Kunci minimal: `primary`. Selebihnya opsional (ada fallback):

```json
{
  "primary": "#a8c7fa",
  "onPrimary": "#0a305f",
  "background": "#111318",
  "onSurface": "#e2e2e9",
  "surface": "#1a1d24",
  "tertiary": "#7fc4c4",
  "error": "#f2b8b5"
}
```

App baca file ini (read-only), potong tiap nilai ke 7 char, dan
mengawasi foldernya — ganti wallpaper = palet ikut ganti live, tanpa
restart. Tidak ada network, tidak ada daemon matugen yang wajib jalan.

## Setup

**Ryoku:** sudah jalan — compositor menulis `colors.json` tiap ganti
wallpaper. Tidak perlu apa-apa.

**Setup lain:** install matugen (repo/AUR), lalu tiap ganti wallpaper
tulis ulang file di path kontrak (symlink, copy, atau template output
matugen — lihat dokumentasi matugen untuk format templatenya), misal
dari hook wallpaper/SWWW/hyprpaper. Begitu file ada dan valid, theme
"matugen" di Settings langsung tulis "following wallpaper · live".

## Troubleshooting

- Theme matugen tidak berubah: cek file ada (`cat
  ~/.cache/ryoku/colors.json`) dan `primary`-nya hex valid.
- Keyboard tidak follow: butuh `primary` valid + toggle follow di tab
  Keyboard; status gagal tampil di bar status Keys.
- `matugen_rev` 0 di snapshot = file tak ada = fallback statis. Normal.
