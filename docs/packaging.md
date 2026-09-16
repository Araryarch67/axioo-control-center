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

`packaging/udev/99-axioo-kbd.rules` — `rgb:kbd_backlight*` jadi `g+w` group `video` (hilangkan sudo harian untuk kbd backlight). Reload: `sudo udevadm control --reload-rules`.

## Upstream

Quirk `0x17` Studio X ada di `packaging/clevo-drivers-axioo/studiox-kbd-quirk.patch` — setelah terbukti stabil, kirim patch ke `nick42d/clevo-drivers` (AUR yang dipakai). Target: DMI quirk board, bukan override tipe yang sudah dikenal driver.

## Autostart login + tray

App (Tauri, jalan sebagai user) punya tray icon: klik kiri tampil/sembunyi,
klik kanan menu (Tampilkan · Start saat login · Keluar). Tombol ×/Alt+F4
menyembunyikan ke tray, bukan keluar.

"Start saat login" (aktif default via `setup.sh`; toggle di tab Settings,
juga ada di menu tray) memakai file `~/.config/autostart/axioo-center.desktop`
via `tauri-plugin-autostart` dengan argumen `--minimized` (mulai sembunyi
di tray). `is_enabled` plugin = cek existensi file, jadi toggle in-app
selalu sinkron dengan file setup.sh. Catatan:

- Nyalakan toggle dari AppImage terinstall (bukan `./dev.sh`) agar login
  menjalankan binary yang benar — plugin mencatat executable saat toggle on.
- Tray butuh host StatusNotifier: GNOME (extension), KDE (bawaan),
  Hyprland (mis. waybar `tray` module).
- Hyprland tidak memproses XDG autostart sendiri — tambah
  `exec-once = dex --autostart` (paket `dex`) atau kontak manual.
- `uninstall.sh` menghapus file autostart + verifikasi bersih.
