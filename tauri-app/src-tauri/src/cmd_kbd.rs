//! Keyboard backlight commands + effects.

use std::sync::{atomic::AtomicBool, Arc, Mutex};

use axioo_lib::{kbd, kbd_effect};

use crate::dbus::{daemon_proxy, AxiooControlProxy};
use crate::snapshot::EffectState;

/// Simpan warna terakhir ke daemon (best-effort, untuk restore sebelum
/// SDDM saat boot — GUI user tak bisa tulis `/var/lib` langsung).
/// Gagal (daemon mati) bukan error: restore sesi-login via localStorage
/// tetap jalan.
pub(crate) async fn persist_kbd_via_daemon(
    brightness: u32,
    r: u8,
    g: u8,
    b: u8,
    effect: &str,
    rear: &str,
    speed: f32,
) {
    let Ok(conn) = zbus::Connection::system().await else {
        return;
    };
    let Ok(proxy) = AxiooControlProxy::new(&conn).await else {
        return;
    };
    let _ = proxy
        .set_kbd(brightness, r, g, b, effect, rear, speed)
        .await;
}
#[derive(serde::Deserialize)]
pub(crate) struct KbdSetArgs {
    zone: Option<usize>,
    brightness: u32,
    r: u8,
    g: u8,
    b: u8,
}
/// Keyboard backlight via sysfs LED (safe, udev-rule writable).
/// Mematikan efek animasi yang sedang jalan (biar tak rebutan tulis sysfs).
/// Sukses langsung juga disimpan ke daemon (best-effort) agar warna
/// terakhir ke-restore sebelum SDDM saat boot berikutnya.
#[tauri::command]
pub(crate) async fn kbd_set(
    args: KbdSetArgs,
    effects: tauri::State<'_, Mutex<EffectState>>,
) -> Result<String, String> {
    let speed = effects.lock().map(|fx| fx.speed).unwrap_or(1.0);
    if let Ok(mut fx) = effects.lock() {
        fx.stop_all();
    }
    let kbds = kbd::discover();
    if kbds.is_empty() {
        return Err("Keyboard backlight not found.".to_string());
    }
    let targets: Vec<usize> = match args.zone {
        Some(i) => {
            if i >= kbds.len() {
                return Err(format!("Zone {i} out of range (0..{}).", kbds.len()));
            }
            vec![i]
        }
        None => (0..kbds.len()).collect(),
    };
    for i in targets {
        let kb = &kbds[i];
        let br = args.brightness.min(kb.max_brightness);
        if let Err(e) = kbd::set(kb, br, (args.r, args.g, args.b)) {
            let msg = format!("{e}");
            if msg.contains("permission denied") {
                // Fallback: daemon (root) yang tulis + simpan sekaligus.
                match daemon_proxy().await {
                    Ok(proxy) => {
                        proxy
                            .set_kbd(args.brightness, args.r, args.g, args.b, "static", "follow", speed)
                            .await
                            .map_err(|de| format!("kbd zone {i}: permission denied ({e}); daemon also failed: {de}"))?;
                        return Ok(format!(
                            "Keyboard → brightness {} rgb({},{},{}) (via axiood)",
                            args.brightness, args.r, args.g, args.b
                        ));
                    }
                    Err(_) => {
                        return Err(format!(
                            "kbd zone {i}: permission denied — run './install-system.sh' (udev rule) then restart the app"
                        ));
                    }
                }
            } else {
                return Err(format!("kbd zone {i} failed: {e}"));
            }
        }
    }
    persist_kbd_via_daemon(
        args.brightness,
        args.r,
        args.g,
        args.b,
        "static",
        "follow",
        speed,
    )
    .await;
    Ok(format!(
        "Keyboard → brightness {} rgb({},{},{})",
        args.brightness, args.r, args.g, args.b
    ))
}
#[derive(serde::Deserialize)]
pub(crate) struct KbdEffectArgs {
    effect: String,
    r: u8,
    g: u8,
    b: u8,
    brightness: u32,
    /// Pengali kecepatan (None = 1.0); di-clamp 0.1..=4.0.
    speed: Option<f32>,
    /// Efek rear exhaust independen: None/"follow"/"" = ikut efek utama,
    /// selainnya nama efek (`wave`, `rainbow`, …). Diabaikan bila <5 node.
    rear: Option<String>,
}
/// Loop animasi gabungan: zona keyboard pakai `main`, zona rear (indeks
/// terakhir bila ≥5 node) pakai `rear`. Satu thread agar tak berebut LED.
/// `tick`/`set` hanya dipanggil (murni + sysfs aman) — `axioo-lib` tak diubah.
pub(crate) fn spawn_split(
    main: kbd_effect::KbdEffect,
    rear: Option<kbd_effect::KbdEffect>,
    base: (u8, u8, u8),
    brightness: u32,
    speed: f32,
) -> Arc<AtomicBool> {
    use std::sync::atomic::Ordering;
    use std::thread;
    use std::time::Duration;
    let stop = Arc::new(AtomicBool::new(false));
    let s = stop.clone();
    thread::spawn(move || {
        let devs = kbd::discover();
        if devs.is_empty() {
            return;
        }
        let n = devs.len();
        // Rear = indeks terakhir hanya bila backend melihat ≥5 node
        // (3 keyboard + numpad + lightbar EC 0x07).
        let rear_idx = if rear.is_some() && n >= 5 {
            Some(n - 1)
        } else {
            None
        };
        let sp = speed.clamp(0.1, 4.0);
        // Main static + rear independen: keyboard DIBIARKAN (warna per-zona
        // user tidak diobrak-abrik), hanya rear yang dianimasikan.
        // Ingat warna rear semula untuk restore saat stop.
        let rear_start: Option<((u8, u8, u8), u32)> = rear_idx
            .and_then(|ri| devs.get(ri))
            .and_then(kbd::read_state)
            .map(|s| (s.rgb, s.brightness));
        let static_main = main == kbd_effect::KbdEffect::Static;
        let mut t = 0.0f32;
        let dt = 0.06;
        while !s.load(Ordering::Relaxed) {
            if static_main {
                if let (Some(ri), Some(rfx)) = (rear_idx, rear.as_ref()) {
                    if let (Some(d), Some(c)) =
                        (devs.get(ri), kbd_effect::tick(rfx, base, t, 1).first())
                    {
                        let _ = kbd::set(d, brightness, *c);
                    }
                }
            } else {
                let mut cols = kbd_effect::tick(&main, base, t, n);
                if let (Some(ri), Some(rfx)) = (rear_idx, rear.as_ref()) {
                    if let Some(c) = kbd_effect::tick(rfx, base, t, 1).first() {
                        cols[ri] = *c;
                    }
                }
                for (d, rgb) in devs.iter().zip(cols.iter()) {
                    let _ = kbd::set(d, brightness, *rgb);
                }
            }
            thread::sleep(Duration::from_millis(60));
            t += dt * sp;
        }
        if static_main {
            // Kembalikan rear ke warna semula; keyboard tak pernah disentuh.
            if let (Some(ri), Some(((r, g, b), br))) = (rear_idx, rear_start) {
                if let Some(d) = devs.get(ri) {
                    let _ = kbd::set(d, br, (r, g, b));
                }
            }
        } else {
            for d in &devs {
                let _ = kbd::set(d, brightness, base);
            }
        }
    });
    stop
}
/// Animasi RGB keyboard (thread userspace via `kbd_effect`, tulis sysfs
/// periodik 60ms — privilege sama seperti `kbd_set`, tak perlu root
/// tambahan). `"static"` = hentikan animasi (kembali ke warna diam).
/// Mematikan efek lama dulu; hanya satu yang jalan.
#[tauri::command]
pub(crate) async fn kbd_effect_start(
    args: KbdEffectArgs,
    effects: tauri::State<'_, Mutex<EffectState>>,
) -> Result<String, String> {
    let effect = kbd_effect::KbdEffect::parse(&args.effect.chars().take(16).collect::<String>())
        .ok_or_else(|| format!("unknown effect: '{}'", args.effect))?;
    let rear_raw = args
        .rear
        .unwrap_or_default()
        .chars()
        .take(16)
        .collect::<String>();
    let rear_fx = if rear_raw.is_empty() || rear_raw.eq_ignore_ascii_case("follow") {
        None
    } else {
        Some(
            kbd_effect::KbdEffect::parse(&rear_raw)
                .ok_or_else(|| format!("unknown rear effect: '{rear_raw}'"))?,
        )
    };
    if kbd::discover().is_empty() {
        return Err("Keyboard backlight not found.".to_string());
    }
    // Scope guard mutex: ubah state efek + spawn, lalu lepaskan SEBELUM
    // await D-Bus (MutexGuard std tak Send — future Tauri wajib Send).
    let (fx_name, rear_persist, bright, speed) = {
        let mut fx = effects.lock().map_err(|e| format!("effect lock: {e}"))?;
        fx.stop_all();
        if effect == kbd_effect::KbdEffect::Static && rear_fx.is_none() {
            return Ok("Effect off — static color.".to_string());
        }
        let max = kbd::discover()
            .first()
            .map(|kb| kb.max_brightness)
            .unwrap_or(255);
        let speed = args.speed.unwrap_or(1.0).clamp(0.1, 4.0);
        let rear_name = rear_fx.as_ref().map(|e| e.as_str().to_string());
        let stop = spawn_split(
            effect.clone(),
            rear_fx,
            (args.r, args.g, args.b),
            args.brightness.min(max),
            speed,
        );
        fx.stops.push(stop);
        fx.current = effect.as_str().to_string();
        fx.rear = rear_name.clone().unwrap_or_else(|| "follow".to_string());
        fx.speed = speed;
        (
            effect.as_str().to_string(),
            rear_name.clone().unwrap_or_else(|| "follow".to_string()),
            args.brightness.min(max),
            speed,
        )
    };
    // Simpan warna dasar + efek ke daemon (best-effort) untuk boot-restore.
    persist_kbd_via_daemon(
        bright,
        args.r,
        args.g,
        args.b,
        &fx_name,
        &rear_persist,
        speed,
    )
    .await;
    Ok(match rear_persist.as_str() {
        "follow" => format!("Effect {} running ({speed}x).", effect.label()),
        r => format!("Effect {} running ({speed}x) + rear {r}.", effect.label()),
    })
}
/// Hentikan animasi RGB (kembalikan warna diam terakhir).
#[tauri::command]
pub(crate) async fn kbd_effect_stop(
    effects: tauri::State<'_, Mutex<EffectState>>,
) -> Result<String, String> {
    let mut fx = effects.lock().map_err(|e| format!("effect lock: {e}"))?;
    fx.stop_all();
    Ok("Effect off — static color.".to_string())
}
