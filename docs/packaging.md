# Packaging

## AUR (axioo-control-center-git)

PKGBUILD in `packaging/aur/PKGBUILD` — full package: Tauri GUI
(`axioo-control-center`), `axioo-ctl`, `axiood`, desktop entry + icon,
D-Bus `com.axioo.Control.conf` to `/etc/dbus-1/system.d`, polkit to
`/usr/share/polkit-1/actions`, systemd `axiood.service` to
`/usr/lib/systemd/system`, udev `99-axioo-kbd.rules` to
`/usr/lib/udev/rules.d`. Build order: `npm ci && npm run build`
(frontend first — the Tauri binary embeds `tauri-app/dist`) then
`cargo build --release --workspace`. `.SRCINFO` is generated via
`makepkg --printsrcinfo` (must be included in AUR uploads);
`axioo-control-center-git.install` reloads daemon/udev and prints hints
(axiood is deliberately not auto-enabled).

```sh
cd packaging/aur && makepkg -si
sudo systemctl enable --now axiood
```

## AppImage (setup.sh)

`./setup.sh` (alias: `./install.sh`) tetap untuk mesin fresh: yay/paru → clevo-drivers + quirk DKMS → `./script.sh` → install AppImage ke `~/.local/share/axioo-control-center` (hindari `~/Applications` agar tak diganggu appimagelauncherd) + `~/.local/bin/axioo-ctl` + desktop entry. Sejak 0.4.0, `setup.sh` juga menginstall axiood/D-Bus/udev via sudo (idempoten).

`./uninstall.sh` bersih total + verifikasi, tanpa flag: stop/disable axiood lalu hapus binary/unit/D-Bus/polkit/udev (lokasi `/etc` maupun `/usr/lib`), AppImage + `~/.local/bin/axioo-ctl` + desktop entry + ikon + data/cache aplikasi, `dist/*.AppImage` + `.tools/`, build cache (`target/`, `node_modules/`, output vite/bundle), dan driver DKMS custom `tuxedo-drivers-axioo` (driver bawaan AUR dipasang ulang bila sourcenya ada). Opt-out saja yang pakai env: `KEEP_CACHE=1` (pertahankan `.tools/`), `KEEP_DRIVER=1` (pertahankan driver custom).

## Ikon + window class (semua DE/WM)

Sumber logo: `icons/android-chrome-512x512.png` → `tauri icon` (isi
`tauri-app/src-tauri/icons/*`, dibundle ke AppImage: taskbar + Alt-Tab +
jendela) + `dist/icon.png` 256px (hicolor launcher via `setup.sh`).
`Icon=axioo-control-center` (nama hicolor, bukan path) agar kebaca
GNOME/KDE/XFCE/Hyprland-launcher. `StartupWMClass` ditulis DINAMIS saat
install dari stem nama file AppImage — app_id/WM_CLASS diturunkan toolkit
dari nama executable (terbukti: `axioo-center` → `"axioo-center"`,
rename → ikut berubah), jadi nilai statis pasti basi karena nama AppImage
memuat versi.

## udev

`packaging/udev/99-axioo-kbd.rules`:

- `rgb:kbd_backlight*` jadi `g+w` grup `video` (hilangkan sudo harian
  untuk kbd backlight). Reload: `sudo udevadm control --reload-rules`.
- Restore paling awal: `ACTION==add` → `/usr/bin/axioo-ctl kbd restore`
  (baca `/var/lib/axiood/kbd.json`, terapkan semua zona, idempoten).
  `setup.sh` memasang `axioo-ctl` ke `/usr/bin` agar rule ini valid.
  Detail rantai restore: `docs/kbd-backlight.md`.

## Upstream

Quirk `0x17` Studio X ada di `packaging/clevo-drivers-axioo/studiox-kbd-quirk.patch` — setelah terbukti stabil, kirim patch ke `nick42d/clevo-drivers` (AUR yang dipakai). Target: DMI quirk board, bukan override tipe yang sudah dikenal driver. Panduan: `packaging/clevo-drivers-axioo/UPSTREAM.md`.

## axiood.service (system, root)

`packaging/axiood.service` → `/usr/lib/systemd/system/axiood.service`,
`WantedBy=multi-user.target`, `Restart=on-failure`.
**Sengaja TANPA `After=` ke `power-profiles-daemon.service`:** PPD
upstream punya `After=multi-user.target`, jadi `After=PPD` menutup
ordering-cycle (axiood→PPD→multi-user→axiood) dan systemd me-delete job
start tiap boot ("Found ordering cycle", enabled tapi inactive).
Daemon tahan tanpa PPD (fallback Balanced + polling D-Bus tiap 3 dtk),
jadi dependensi ordering keras tak diperlukan.
Butuh `CAP_SYS_RAWIO` (port EC `0x62`/`0x66`) + powercap sysfs + debugfs
`ec_sys`. Detail daemon/profil/D-Bus: `docs/daemon.md`.

## Autostart login + tray (systemd user unit = utama)

App (Tauri, jalan sebagai user) punya tray icon: klik kiri tampil/sembunyi,
klik kanan menu (Tampilkan · Start saat login · Keluar). Tombol ×/Alt+F4
menyembunyikan ke tray, bukan keluar. Single-instance ala Steam:
peluncuran kedua mati sendiri dan memunculkan jendela instance pertama
(jadi launcher + autostart boleh berbagi wrapper tanpa dobel tray).

"Start saat login" (toggle tab Settings + menu tray, aktif default via
`setup.sh`) memakai **systemd user unit** sebagai mekanisme utama —
`~/.config/systemd/user/axioo-center.service` (+ symlink wants
`graphical-session.target.wants/`), `WantedBy`+`After=graphical-session.target`,
`ExecStart=%h/.local/bin/axioo-center-autostart --minimized`.
User manager yang start — **tanpa setup di WM** (tak perlu `exec-once`
Hyprland / `add-wants xdg-desktop-autostart` manual). Entry XDG
`~/.config/autostart/axioo-center.desktop` tetap ditulis sebagai kompat
DE lain. Backend menulis keduanya + `daemon-reload`; toggle ON bila
salah satunya ada. Sumber tunggal wrapper + unit di
`packaging/autostart/` + `packaging/systemd-user/`, di-embed ke binary
via `include_str!` (toggle tetap jalan walau repo tak ada).

Kenapa bukan plugin autostart / XDG saja:

- `tauri-plugin-autostart` menulis `Exec=<path AppImage mentah ber-spasi>`
  yang DITOLAK `systemd-xdg-autostart-generator` ("executable does not
  exist") — toggle ON = entry mati. (Sudah dilepas, diganti penulis
  sendiri di backend.)
- **Launcher anti-basi:** AppImageLauncher memindah/rename AppImage tiap
  integrate/update, jadi Exec yang hardcode path AppImage diam-diam basi
  (klik launcher tak terjadi apa-apa; generator skip "executable does
  not exist"). Exec launcher = wrapper **tanpa flag** (jendela tampil),
  Exec boot = wrapper **`--minimized`** (sembunyi ke tray); wrapper
  arg-aware me-resolve AppImage terbaru tiap run
  (`~/Applications` lalu `~/.local/share/axioo-control-center`,
  fallback binary release).
- **Anti-duplikat:** wrapper export `APPIMAGELAUNCHER_DISABLE=1` (satu
  choke point — dialog "integrate?" tak lagi blokir boot-to-tray) +
  backend/toggle menghapus entry warisan plugin
  (`appimagekit_*-Axioo_Control_Center.desktop`).
- **Tray-tenang:** warning kosmetik `libayatana-appindicator is
  deprecated` (dari library sistem, bukan bug) dibungkam via filter
  stderr di wrapper — pesan lain lolos.
- XDG saja tak cukup di Hyprland: `xdg-desktop-autostart.target`
  inactive + ordering-cycle melempar semua job autostart (catatan
  2026-09-16; unit systemd kini utama, symlink manual sesi itu boleh
  dicabut setelah unit aktif).

Catatan:

- Nyalakan toggle dari AppImage terinstall (bukan `./dev.sh`) agar login
  menjalankan binary yang benar.
- Tray butuh host StatusNotifier: GNOME (extension), KDE (bawaan),
  Hyprland (mis. waybar `tray` module).
- `uninstall.sh` mematikan GUI juga: stop+disable service user +
  kill proses tray/AppImage + hapus unit user (urutan stop-dulu agar
  `Restart=on-failure` tak menghidupkan lagi) + verifikasi bersih.

## Man pages

`packaging/man/axioo-ctl.1` — referensi CLI lengkap
(`probe`, `monitor`, `fan`, `profile`, `kbd`, `battery`). Terpasang via
PKGBUILD/`setup.sh`; baca offline: `man axioo-ctl`.
