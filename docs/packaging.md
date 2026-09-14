# Packaging

## AUR (axioo-control-center-git)

PKGBUILD di `packaging/aur/PKGBUILD` — build `cargo --release --workspace` (axioo-ctl, axiood, axioo-control-center),
install ke `/usr/bin`, D-Bus `com.axioo.Control.conf` ke `/etc/dbus-1/system.d`, polkit ke `/usr/share/polkit-1/actions`,
systemd `axiood.service` ke `/usr/lib/systemd/system`, udev `99-axioo-kbd.rules` ke `/usr/lib/udev/rules.d`.

```sh
cd packaging/aur && makepkg -si
sudo systemctl enable --now axiood
```

## AppImage (setup.sh)

`./setup.sh` tetap untuk mesin fresh: yay/paru → clevo-drivers + quirk DKMS → `./script.sh` → install AppImage ke `~/.local/share/axioo-control-center` (hindari `~/Applications` agar tak diganggu appimagelauncherd) + `~/.local/bin/axioo-ctl` + desktop entry. Sejak 0.4.0, `setup.sh` juga menginstall axiood/D-Bus/udev via sudo (idempoten).

`./uninstall.sh` membersihkan AppImage + desktop + axiood/udev (daemon di-disable dulu).

## udev

`packaging/udev/99-axioo-kbd.rules` — `rgb:kbd_backlight*` jadi `g+w` group `video` (hilangkan sudo harian untuk kbd backlight). Reload: `sudo udevadm control --reload-rules`.

## Upstream

Quirk `0x17` Studio X ada di `packaging/clevo-drivers-axioo/studiox-kbd-quirk.patch` — setelah terbukti stabil, kirim patch ke `nick42d/clevo-drivers` (AUR yang dipakai). Target: DMI quirk board, bukan override tipe yang sudah dikenal driver.
