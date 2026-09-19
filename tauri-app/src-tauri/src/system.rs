//! Desktop integration: matugen theme + login autostart.

use tauri::Emitter;

/// Revisi palet matugen = mtime `~/.cache/ryoku/colors.json` (detik).
/// Satu `stat` syscall per snapshot — jauh lebih murah dari baca+parse JSON.
pub(crate) fn matugen_rev() -> u64 {
    let home = std::env::var("HOME").unwrap_or_default();
    if home.is_empty() {
        return 0;
    }
    std::fs::metadata(std::path::Path::new(&home).join(".cache/ryoku/colors.json"))
        .and_then(|m| m.modified())
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_secs())
        .unwrap_or(0)
}
/// Watch `~/.cache/ryoku/` (non-rekursif): tiap tulis `colors.json`
/// emit event `matugen-changed` + rev baru. Frontend hidden memakai ini
/// sebagai pemicu tick (ganti poll cepat) — visible tak terpengaruh.
/// Watch direktori (bukan file) agar create-pertama ikut tertangkap;
/// dir tak ada (non-Ryoku) → diam, frontend pakai fallback interval.
pub(crate) fn matugen_watch(app: tauri::AppHandle) {
    use notify::{Config, RecommendedWatcher, RecursiveMode, Watcher};
    let home = std::env::var("HOME").unwrap_or_default();
    if home.is_empty() {
        return;
    }
    let dir = std::path::Path::new(&home).join(".cache/ryoku");
    if !dir.is_dir() {
        return;
    }
    let mut last = matugen_rev();
    let handler = move |res: Result<notify::Event, notify::Error>| {
        let Ok(ev) = res else { return };
        let touches_palette = ev
            .paths
            .iter()
            .any(|p| p.file_name().is_some_and(|n| n == "colors.json"));
        if !touches_palette {
            return;
        }
        // Debounce alami: rev = mtime detik; burst tulis dalam 1 detik
        // yang sama hanya emit sekali.
        let rev = matugen_rev();
        if rev != last {
            last = rev;
            let _ = app.emit("matugen-changed", rev);
        }
    };
    let mut watcher = match RecommendedWatcher::new(handler, Config::default()) {
        Ok(w) => w,
        Err(_) => return,
    };
    if watcher.watch(&dir, RecursiveMode::NonRecursive).is_err() {
        return;
    }
    // Parkir thread selama proses hidup (watcher mati bila drop).
    loop {
        std::thread::park();
    }
}
/// Palet matugen Ryoku (`~/.cache/ryoku/colors.json`) untuk theme GUI.
/// Murni read-only; Err bila file tak ada agar frontend fallback ke theme statis.
#[tauri::command]
pub(crate) fn get_matugen() -> Result<std::collections::HashMap<String, String>, String> {
    let home = std::env::var("HOME").map_err(|e| format!("HOME unreadable: {e}"))?;
    let path = std::path::Path::new(&home).join(".cache/ryoku/colors.json");
    let raw = std::fs::read_to_string(&path).map_err(|e| format!("matugen unreadable: {e}"))?;
    let map: std::collections::HashMap<String, serde_json::Value> =
        serde_json::from_str(&raw).map_err(|e| format!("matugen json invalid: {e}"))?;
    Ok(map
        .into_iter()
        .filter_map(|(k, v)| {
            v.as_str().filter(|s| s.starts_with('#')).map(|s| {
                // kunci + hex dipotong 7 char ("#rrggbb") — against junk.
                (k, s.chars().take(7).collect::<String>())
            })
        })
        .collect())
}
// ---------- Autostart login (dikelola sendiri, tanpa plugin) ----------
//
// tauri-plugin-autostart menulis `Exec=<path AppImage mentah ber-spasi>`
// yang DITOLAK systemd-xdg-autostart-generator ("executable does not
// exist") — toggle ON = entry mati, toggle user menimpa file wrapper.
//
// Mekanisme UTAMA = systemd user unit (`axioo-center.service`,
// `WantedBy=graphical-session.target`): user manager yang start,
// TANPA setup di WM (tak perlu `exec-once` Hyprland / `add-wants`
// xdg-desktop-autostart manual — keduanya rapuh: di mesin ini
// `xdg-desktop-autostart.target` inactive + ordering-cycle melempar
// semua job autostart). Entry XDG `*.desktop` tetap ditulis sebagai
// kompat DE lain; single-instance backend melindungi dari start ganda.
// Dipakai command frontend + menu tray.
/// Wrapper stabil (tahan AppImageLauncher pindah/rename AppImage).
/// Satu sumber dengan `packaging/autostart/axioo-center-autostart`
/// agar toggle = zero-setup walau repo tak ada (AppImage terpasang).
const AUTOSTART_WRAPPER: &str = include_str!("../../../packaging/autostart/axioo-center-autostart");

/// Satu sumber dengan `packaging/systemd-user/axioo-center.service`.
const AUTOSTART_UNIT: &str = include_str!("../../../packaging/systemd-user/axioo-center.service");

/// Entry autostart milik kita (Exec stabil → wrapper).
fn autostart_file() -> std::path::PathBuf {
    let home = std::env::var("HOME").unwrap_or_default();
    std::path::Path::new(&home).join(".config/autostart/axioo-center.desktop")
}

/// Bekas file plugin (`app_name` = "Axioo Control Center", Exec mentah
/// ber-spasi, tak pernah valid) — dibersihkan tiap tulis agar tak dobel.
fn autostart_legacy_file() -> std::path::PathBuf {
    let home = std::env::var("HOME").unwrap_or_default();
    std::path::Path::new(&home).join(".config/autostart/Axioo Control Center.desktop")
}

fn autostart_desktop_content() -> String {
    let home = std::env::var("HOME").unwrap_or_default();
    ["[Desktop Entry]", "Type=Application", "Name=Axioo Control Center"]
        .join("\n")
        + "\nComment=Hardware control for Axioo (Clevo) laptops (start minimized to tray)\n"
        // --minimized = sembunyi ke tray (klik launcher pakai wrapper TANPA
        // flag agar jendela tampil; instance kedua mati via single-instance).
        // APPIMAGELAUNCHER_DISABLE agar tak ada dialog integrate saat login.
        + &format!("Exec=env APPIMAGELAUNCHER_DISABLE=1 {home}/.local/bin/axioo-center-autostart --minimized\n")
        + "Icon=axioo-center\nCategories=System;Settings;HardwareSettings;\n\
              Terminal=false\nX-GNOME-Autostart-enabled=true\nStartupWMClass=axioo-center\n"
}

pub(crate) fn autostart_enabled() -> bool {
    // Unit systemd = mekanisme utama (Hyprland); desktop = kompat DE.
    // ON bila salah satunya ada (migrasi dari instalasi lama yg cuma desktop).
    autostart_unit_link().exists() || autostart_file().exists()
}

/// Unit systemd user (`~/.config/systemd/user/axioo-center.service`).
fn autostart_unit_file() -> std::path::PathBuf {
    let home = std::env::var("HOME").unwrap_or_default();
    std::path::Path::new(&home).join(".config/systemd/user/axioo-center.service")
}

/// Symlink enable = `[Install] WantedBy=graphical-session.target`.
fn autostart_unit_link() -> std::path::PathBuf {
    let home = std::env::var("HOME").unwrap_or_default();
    std::path::Path::new(&home)
        .join(".config/systemd/user/graphical-session.target.wants/axioo-center.service")
}

fn autostart_wrapper_file() -> std::path::PathBuf {
    let home = std::env::var("HOME").unwrap_or_default();
    std::path::Path::new(&home).join(".local/bin/axioo-center-autostart")
}

/// Pastikan wrapper ada + executable (tulis dari konstanta embedded bila
/// hilang — mis. user hapus manual / install AppImage tanpa setup.sh).
fn autostart_ensure_wrapper() -> Result<(), String> {
    let p = autostart_wrapper_file();
    if p.exists() {
        return Ok(());
    }
    if let Some(dir) = p.parent() {
        std::fs::create_dir_all(dir).map_err(|e| format!("wrapper dir: {e}"))?;
    }
    std::fs::write(&p, AUTOSTART_WRAPPER).map_err(|e| format!("wrapper write: {e}"))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&p, std::fs::Permissions::from_mode(0o755))
            .map_err(|e| format!("wrapper chmod: {e}"))?;
    }
    Ok(())
}

fn autostart_daemon_reload() {
    // Best-effort: gagal (mis. tanpa systemd) bukan error toggle.
    let _ = std::process::Command::new("systemctl")
        .args(["--user", "daemon-reload"])
        .output();
}

/// Tulis/hapus entry desktop + unit systemd + bersihkan warisan plugin.
/// Ok(...) = state baru.
pub(crate) fn autostart_write(enabled: bool) -> Result<bool, String> {
    if enabled {
        autostart_ensure_wrapper()?;
        if let Some(dir) = autostart_file().parent() {
            std::fs::create_dir_all(dir).map_err(|e| format!("autostart dir: {e}"))?;
        }
        std::fs::write(autostart_file(), autostart_desktop_content())
            .map_err(|e| format!("autostart write: {e}"))?;
        if let Some(dir) = autostart_unit_file().parent() {
            std::fs::create_dir_all(dir).map_err(|e| format!("unit dir: {e}"))?;
        }
        std::fs::write(autostart_unit_file(), AUTOSTART_UNIT)
            .map_err(|e| format!("unit write: {e}"))?;
        // Enable = symlink wants (setara `systemctl --user enable`,
        // tanpa memanggil systemctl agar tetap jalan tanpa systemd aktif).
        if let Some(dir) = autostart_unit_link().parent() {
            std::fs::create_dir_all(dir).map_err(|e| format!("wants dir: {e}"))?;
        }
        let _ = std::fs::remove_file(autostart_unit_link());
        #[cfg(unix)]
        std::os::unix::fs::symlink(autostart_unit_file(), autostart_unit_link())
            .map_err(|e| format!("unit enable: {e}"))?;
        autostart_daemon_reload();
    } else {
        let _ = std::fs::remove_file(autostart_file());
        let _ = std::fs::remove_file(autostart_unit_link());
        let _ = std::fs::remove_file(autostart_unit_file());
        autostart_daemon_reload();
    }
    let _ = std::fs::remove_file(autostart_legacy_file());
    Ok(enabled)
}

#[tauri::command]
pub(crate) fn autostart_get() -> bool {
    autostart_enabled()
}

#[tauri::command]
pub(crate) fn autostart_set(enabled: bool) -> Result<bool, String> {
    autostart_write(enabled)
}
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn autostart_unit_needs_no_wm_setup() {
        // Syarat "tanpa setup di WM": WantedBy + After graphical-session
        // (user manager yg start), Exec tanpa spasi via %h.
        assert!(
            AUTOSTART_UNIT.contains("WantedBy=graphical-session.target"),
            "unit harus WantedBy=graphical-session.target"
        );
        assert!(
            AUTOSTART_UNIT.contains("After=graphical-session.target"),
            "unit harus After=graphical-session.target"
        );
        assert!(
            AUTOSTART_UNIT.contains("ExecStart=%h/.local/bin/axioo-center-autostart"),
            "Exec harus stabil tanpa spasi (wrapper %h)"
        );
        assert!(
            AUTOSTART_UNIT.contains("--minimized"),
            "unit boot harus --minimized (sembunyi ke tray)"
        );
        let desk = autostart_desktop_content();
        assert!(
            desk.contains("Exec=") && desk.contains(".local/bin/axioo-center-autostart"),
            "desktop tetap menunjuk wrapper stabil"
        );
    }

    #[test]
    fn matugen_rev_mirrors_colors_json() {
        // Di mesin Ryoku file-nya ada → rev != 0; di mesin lain → 0 (fallback).
        let rev = matugen_rev();
        let home = std::env::var("HOME").unwrap_or_default();
        let exists = std::path::Path::new(&home)
            .join(".cache/ryoku/colors.json")
            .exists();
        assert_eq!(rev != 0, exists, "rev harus != 0 iff colors.json ada");
    }

    #[test]
    fn matugen_palette_parses() {
        let home = std::env::var("HOME").unwrap_or_default();
        let path = std::path::Path::new(&home).join(".cache/ryoku/colors.json");
        if !path.exists() {
            return; // bukan mesin Ryoku — command akan Err, itu perilaku benar
        }
        let pal = get_matugen().expect("colors.json ada tapi get_matugen gagal");
        assert!(
            pal.contains_key("primary"),
            "palet Ryoku wajib punya primary"
        );
        assert!(pal.contains_key("background"));
        for (k, v) in &pal {
            assert!(
                v.len() == 7 && v.starts_with('#'),
                "nilai {k} harus #rrggbb, dapat {v}"
            );
        }
    }
}
