//! axioo-center — Tauri 2 backend (thin client).
//!
//! SAFETY CONTRACT (same as GPUI GUI + AGENTS.md):
//! - Read-only sensors via `axioo-lib` (no root needed).
//! - Privileged writes: ONLY one-shot `fan_ctrl::{set_manual_duty,set_auto}`
//!   (root-only, clamp 40–100%, both fans, verify `0xCE`) and `kbd::set`
//!   (sysfs LED, safe) and `battery::set_charge_thresholds` (validated).
//! - The continuous curve loop belongs to `axiood`, NEVER here.
//! - Server-side guard: when `axiood` is reachable on the system bus,
//!   direct EC writes are REJECTED (daemon owns the curve).
//! - All numeric inputs are re-validated here; the JS slider is untrusted.

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod cmd_fan;
mod cmd_kbd;
mod cmd_power;
mod cmd_profile;
mod dbus;
mod snapshot;
mod system;

use std::sync::Mutex;
use std::time::Instant;

use axioo_lib::{cpu, rapl};
use tauri::{
    menu::{CheckMenuItemBuilder, MenuBuilder, MenuItemBuilder, PredefinedMenuItem},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
    Emitter, Manager, WindowEvent,
};

use crate::{
    dbus::daemon_proxy,
    snapshot::query_profile,
    snapshot::{EffectState, SamplerState},
    system::{autostart_enabled, autostart_write, matugen_watch},
};

/// Check-item profil di menu tray (ganti mode tanpa buka jendela).
/// Disimpan sebagai state agar command `set_profile` dari GUI window juga
/// bisa sinkronkan centangnya (sumber lain: klik tray sendiri).
pub(crate) struct TrayProfiles {
    balanced: tauri::menu::CheckMenuItem<tauri::Wry>,
    entertainment: tauri::menu::CheckMenuItem<tauri::Wry>,
    performance: tauri::menu::CheckMenuItem<tauri::Wry>,
}
impl Clone for TrayProfiles {
    fn clone(&self) -> Self {
        Self {
            balanced: self.balanced.clone(),
            entertainment: self.entertainment.clone(),
            performance: self.performance.clone(),
        }
    }
}
/// Sinkronkan centang tray ke label daemon ("Balanced",
/// "Balanced (quiet-fan)", "Performance + manual 80%", …).
pub(crate) fn sync_tray_checks(items: &TrayProfiles, label: &str) {
    for (item, name) in [
        (&items.balanced, "Balanced"),
        (&items.entertainment, "Entertainment"),
        (&items.performance, "Performance"),
    ] {
        let on = label == name
            || label.starts_with(&format!("{name} "))
            || label.starts_with(&format!("{name}("));
        let _ = item.set_checked(on);
    }
}
fn main() {
    tauri::Builder::default()
        // Single-instance: peluncuran kedua mati sendiri dan memunculkan
        // window instance pertama (bukan dua tray icon + dua backend yang
        // rebutan tulis sysfs LED).
        .plugin(tauri_plugin_single_instance::init(|app, _argv, _cwd| {
            if let Some(w) = app.get_webview_window("main") {
                let _ = w.unminimize();
                let _ = w.show();
                let _ = w.set_focus();
            }
        }))
        .manage(Mutex::new(SamplerState {
            prev_stat: cpu::read_times(),
            prev_rapl: rapl::domains(),
            prev_t: Instant::now(),
            n: 0,
        }))
        .manage(Mutex::new(EffectState {
            current: "static".to_string(),
            rear: "follow".to_string(),
            speed: 1.0,
            stops: Vec::new(),
        }))
        .setup(|app| {
            // Watcher palet wallpaper (event-driven, thread parkir):
            // frontend hidden tick via event, bukan poll cepat.
            std::thread::spawn({
                let h = app.handle().clone();
                move || matugen_watch(h)
            });
            // Ikon tray = ikon jendela bawaan bundle (logo), fallback 32x32.
            let icon = app
                .default_window_icon()
                .cloned()
                .unwrap_or_else(|| {
                    tauri::image::Image::from_bytes(include_bytes!("../icons/32x32.png"))
                        .expect("tray fallback icon")
                });
            let show = MenuItemBuilder::with_id("show", "Show").build(app)?;
            let p_balanced =
                CheckMenuItemBuilder::with_id("profile-balanced", "Balanced").build(app)?;
            let p_entertainment =
                CheckMenuItemBuilder::with_id("profile-entertainment", "Entertainment")
                    .build(app)?;
            let p_performance =
                CheckMenuItemBuilder::with_id("profile-performance", "Performance")
                    .build(app)?;
            let tray_profiles = TrayProfiles {
                balanced: p_balanced.clone(),
                entertainment: p_entertainment.clone(),
                performance: p_performance.clone(),
            };
            app.manage(tray_profiles.clone());
            let sep1 = PredefinedMenuItem::separator(app)?;
            let sep2 = PredefinedMenuItem::separator(app)?;
            let autostart = CheckMenuItemBuilder::with_id("autostart", "Start on login")
                .checked(autostart_enabled())
                .build(app)?;
            let quit = MenuItemBuilder::with_id("quit", "Quit").build(app)?;
            let menu = MenuBuilder::new(app)
                .items(&[
                    &show,
                    &sep1,
                    &p_balanced,
                    &p_entertainment,
                    &p_performance,
                    &sep2,
                    &autostart,
                    &quit,
                ])
                .build()?;
            // Clone untuk update centang dari dalam handler menu.
            let autostart_item = autostart.clone();
            let profile_items = tray_profiles.clone();
            // Centang awal = profil daemon saat ini (async, best-effort —
            // daemon mati = semua tak dicentang, bukan error).
            {
                let init = tray_profiles.clone();
                tauri::async_runtime::spawn(async move {
                    let p = query_profile().await;
                    if p.daemon {
                        sync_tray_checks(&init, &p.profile);
                    }
                });
            }
            TrayIconBuilder::new()
                .icon(icon)
                .tooltip("Axioo Control Center")
                .menu(&menu)
                .show_menu_on_left_click(false)
                .on_menu_event(move |app, event| match event.id.as_ref() {
                    "show" => {
                        if let Some(w) = app.get_webview_window("main") {
                            let _ = w.show();
                            let _ = w.set_focus();
                        }
                    }
                    "profile-balanced" | "profile-entertainment" | "profile-performance" => {
                        let want = match event.id.as_ref() {
                            "profile-balanced" => "Balanced",
                            "profile-entertainment" => "Entertainment",
                            _ => "Performance",
                        };
                        let h = app.clone();
                        let items = profile_items.clone();
                        tauri::async_runtime::spawn(async move {
                            let res: Result<String, String> = async {
                                let proxy = daemon_proxy().await?;
                                proxy.set_profile(want).await.map_err(|e| {
                                    if format!("{e}").contains("ServiceUnknown") {
                                        "axiood not running — enable it, then use the in-app profile buttons".to_string()
                                    } else {
                                        format!("daemon refused: {e}")
                                    }
                                })
                            }
                            .await;
                            match res {
                                Ok(label) => sync_tray_checks(&items, &label),
                                Err(msg) => {
                                    let _ = h.emit(
                                        "tray-notice",
                                        serde_json::json!({"text": msg, "error": true}),
                                    );
                                }
                            }
                        });
                    }
                    "autostart" => {
                        let now_on = autostart_enabled();
                        let ok = autostart_write(!now_on).is_ok();
                        if ok {
                            if let Some(w) = app.get_webview_window("main") {
                                let _ = w.emit("autostart-changed", !now_on);
                            }
                        }
                        let _ = autostart_item.set_checked(!now_on && ok);
                    }
                    "quit" => app.exit(0),
                    _ => {}
                })
                .on_tray_icon_event(|tray, event| {
                    // Klik kiri = tampil/sembunyi; kanan = menu (otomatis).
                    if let TrayIconEvent::Click {
                        button: MouseButton::Left,
                        button_state: MouseButtonState::Up,
                        ..
                    } = event
                    {
                        let app = tray.app_handle();
                        if let Some(w) = app.get_webview_window("main") {
                            if w.is_visible().unwrap_or(true) {
                                let _ = w.hide();
                            } else {
                                let _ = w.show();
                                let _ = w.set_focus();
                            }
                        }
                    }
                })
                .build(app)?;
            // Autostart login (--minimized) → langsung sembunyi ke tray.
            if std::env::args().any(|a| a == "--minimized") {
                if let Some(w) = app.get_webview_window("main") {
                    let _ = w.hide();
                }
            }
            Ok(())
        })
        // Tombol close (termasuk Alt+F4) = sembunyi ke tray, bukan keluar.
        // Keluar beneran hanya via menu tray → Keluar.
        .on_window_event(|win, event| {
            if let WindowEvent::CloseRequested { api, .. } = event {
                let _ = win.hide();
                api.prevent_close();
            }
        })
        .invoke_handler(tauri::generate_handler![
            snapshot::get_snapshot,
            cmd_profile::set_profile,
            cmd_profile::set_quiet_fan,
            cmd_fan::fan_set_manual,
            cmd_fan::fan_set_auto,
            cmd_fan::set_fan_ec_auto,
            cmd_fan::set_fan_manual,
            cmd_fan::clear_fan_override,
            cmd_kbd::kbd_set,
            cmd_kbd::kbd_effect_start,
            cmd_kbd::kbd_effect_stop,
            cmd_power::battery_set,
            system::get_matugen,
            system::autostart_get,
            system::autostart_set,
        ])
        .run(tauri::generate_context!())
        .expect("failed to run axioo-center");
}
