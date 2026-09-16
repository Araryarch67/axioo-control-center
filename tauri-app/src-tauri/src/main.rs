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

use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc, Mutex,
};
use std::time::Instant;

use axioo_lib::{battery, cpu, dmi, ec, fan, fan_ctrl, hwmon, kbd, kbd_effect, memory, nvidia, rapl};
use serde::Serialize;
use tauri::{
    menu::{CheckMenuItemBuilder, MenuBuilder, MenuItemBuilder},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
    Emitter, Manager, WindowEvent,
};
use tauri_plugin_autostart::ManagerExt;

// ---------- D-Bus proxies (async, same bus as axiood) ----------

#[zbus::proxy(
    interface = "com.axioo.Control",
    default_service = "com.axioo.Control",
    default_path = "/com/axioo/Control"
)]
trait AxiooControl {
    async fn get_profile(&self) -> zbus::Result<String>;
    async fn set_profile(&self, profile: &str) -> zbus::Result<String>;
    async fn get_ppd_profile(&self) -> zbus::Result<String>;
    async fn get_quiet_fan(&self) -> zbus::Result<bool>;
    async fn set_quiet_fan(&self, quiet: bool) -> zbus::Result<String>;
    async fn get_fan_duty(&self) -> zbus::Result<u8>;
    async fn get_curve(&self) -> zbus::Result<Vec<(i32, u8)>>;
    async fn get_fan_mode(&self) -> zbus::Result<String>;
    async fn set_fan_ec_auto(&self, auto: bool) -> zbus::Result<String>;
    async fn set_fan_manual(&self, duty: u8) -> zbus::Result<String>;
    async fn clear_fan_override(&self) -> zbus::Result<String>;
}

#[zbus::proxy(
    interface = "org.freedesktop.UPower.PowerProfiles",
    default_path = "/org/freedesktop/UPower/PowerProfiles"
)]
trait PpdNew {
    #[zbus(property)]
    fn active_profile(&self) -> zbus::Result<String>;
}

#[zbus::proxy(
    interface = "net.hadess.PowerProfiles",
    default_path = "/net/hadess/PowerProfiles"
)]
trait PpdOld {
    #[zbus(property)]
    fn active_profile(&self) -> zbus::Result<String>;
}

// ---------- Snapshot types (JSON to React) ----------

#[derive(Serialize, Clone, Default)]
struct GpuRow {
    name: String,
    usage_pct: Option<f64>,
    temp_c: Option<f64>,
    power_w: Option<f64>,
    clock_mhz: Option<u64>,
}

#[derive(Serialize, Clone, Default)]
struct ProfileState {
    daemon: bool,
    profile: String,
    quiet_fan: bool,
    ppd: String,
    curve: Vec<(i32, u8)>,
    fan_duty: u8,
    /// "curve" | "manual" | "ec_auto" (daemon) — "" bila daemon mati.
    fan_mode: String,
}

#[derive(Serialize, Clone, Default)]
struct Snapshot {
    product: String,
    cpu_model: String,
    is_root: bool,
    cpu_temp_line: String,
    cpu_freq_line: String,
    cpu_usage_pct: Option<f64>,
    governor: String,
    epp: String,
    gpus: Vec<GpuRow>,
    fan_rpms: Vec<u64>,
    bat_pct: Option<f64>,
    bat_line: String,
    bat_start: Option<u64>,
    bat_end: Option<u64>,
    mem_pct: Option<f64>,
    mem_line: String,
    pkg_watts: Option<f64>,
    ec_cpu_temp: Option<i32>,
    ec_fan1_rpm: Option<u32>,
    ec_fan2_rpm: Option<u32>,
    ec_duty: Option<u8>,
    ec_err: Option<String>,
    max_temp_c: Option<i32>,
    kbd_nodes: usize,
    kbd_max: u32,
    kbd_brightness: Option<u32>,
    kbd_rgb: Option<(u8, u8, u8)>,
    kbd_zones: Vec<(u32, (u8, u8, u8))>,
    /// `true` bila sysfs LED bisa ditulis user ini (udev rule terpasang)
    /// → tombol Keyboard benar-benar bisa dipakai.
    kbd_writable: bool,
    /// Sama untuk BAT0 charge thresholds.
    bat_writable: bool,
    /// Efek RGB aktif ("static" = warna diam).
    kbd_effect: String,
    /// Efek rear exhaust independen ("follow" = ikut efek utama).
    kbd_rear_effect: String,
    /// Pengali kecepatan efek (1.0 = normal).
    kbd_effect_speed: f32,
    /// Revisi palet matugen (mtime `colors.json`, 0 bila tak ada).
    /// Frontend theme "matugen" refresh hanya bila ini berubah —
    /// tanpa timer/IPC tambahan di luar poll snapshot yang sudah ada.
    matugen_rev: u64,
    profile: ProfileState,
    stamp: u64,
}

struct SamplerState {
    prev_stat: Option<cpu::CpuTimes>,
    prev_rapl: Vec<rapl::RaplDomain>,
    prev_t: Instant,
    n: u64,
}

/// Status animasi RGB keyboard (userspace thread via `kbd_effect`).
/// Hanya satu efek jalan; start baru mematikan yang lama.
struct EffectState {
    /// Nama efek aktif ("static" = tidak ada animasi).
    current: String,
    /// Efek rear exhaust independen ("follow" = ikut efek utama).
    rear: String,
    /// Pengali kecepatan terakhir (1.0 = normal).
    speed: f32,
    /// Flag stop untuk thread yang jalan (None = tidak ada).
    stops: Vec<Arc<AtomicBool>>,
}

impl EffectState {
    /// Matikan semua thread efek (idempoten). Thread keluar ≤1 tick (60ms).
    fn stop_all(&mut self) {
        for s in self.stops.drain(..) {
            s.store(true, Ordering::Relaxed);
        }
        self.current = "static".to_string();
        self.rear = "follow".to_string();
    }
}

fn ppd_display(ppd: &str) -> (&'static str, bool) {
    match ppd {
        "power-saver" => ("Balanced", true),
        "balanced" => ("Balanced", false),
        "performance" => ("Performance", false),
        _ => ("Balanced", false),
    }
}

async fn query_profile() -> ProfileState {
        if let Ok(conn) = zbus::Connection::system().await {
        if let Ok(proxy) = AxiooControlProxy::new(&conn).await {
            if let Ok(profile) = proxy.get_profile().await {
                return ProfileState {
                    daemon: true,
                    profile,
                    quiet_fan: proxy.get_quiet_fan().await.unwrap_or(false),
                    ppd: proxy
                        .get_ppd_profile()
                        .await
                        .unwrap_or_else(|_| "-".to_string()),
                    curve: proxy.get_curve().await.unwrap_or_default(),
                    fan_duty: proxy.get_fan_duty().await.unwrap_or(0),
                    fan_mode: proxy.get_fan_mode().await.unwrap_or_else(|_| "curve".to_string()),
                };
            }
        }
        // Fallback: read PPD directly (display only).
        let mut ppd: Option<String> = None;
        if let Ok(builder) = PpdNewProxy::builder(&conn)
            .destination("org.freedesktop.UPower.PowerProfiles")
        {
            if let Ok(p) = builder.build().await {
                ppd = p.active_profile().await.ok();
            }
        }
        if ppd.is_none() {
            if let Ok(builder) = PpdOldProxy::builder(&conn)
                .destination("net.hadess.PowerProfiles")
            {
                if let Ok(p) = builder.build().await {
                    ppd = p.active_profile().await.ok();
                }
            }
        }
        if let Some(ppd) = ppd {
            let (label, quiet) = ppd_display(&ppd);
            return ProfileState {
                daemon: false,
                profile: label.to_string(),
                quiet_fan: quiet,
                ppd,
                curve: Vec::new(),
                fan_duty: 0,
                fan_mode: String::new(),
            };
        }
    }
    ProfileState {
        daemon: false,
        ppd: "-".to_string(),
        profile: "Balanced".to_string(),
        ..Default::default()
    }
}

async fn daemon_reachable() -> bool {
    if let Ok(conn) = zbus::Connection::system().await {
        if let Ok(proxy) = AxiooControlProxy::new(&conn).await {
            return proxy.get_profile().await.is_ok();
        }
    }
    false
}

/// Probe tulis-tanpa-menulis: open O_WRONLY lalu langsung drop.
/// Aman untuk sysfs (permission dicek saat open, tak ada byte terkirim).
fn writable(path: &std::path::Path) -> bool {
    std::fs::OpenOptions::new()
        .write(true)
        .open(path)
        .map(|_| ())
        .is_ok()
}

fn f1(v: Option<f64>, unit: &str) -> String {
    v.map_or_else(|| "-".to_string(), |x| format!("{x:.1}{unit}"))
}

fn build_snapshot(
    st: &mut SamplerState,
    profile: ProfileState,
    kbd_effect: String,
    kbd_rear_effect: String,
    kbd_effect_speed: f32,
) -> Snapshot {
    let now = Instant::now();
    let dt = now.duration_since(st.prev_t).as_secs_f64().max(0.01);
    st.prev_t = now;
    st.n += 1;

    let c = cpu::sample();
    let max_temp_c = c
        .package_temp_c
        .into_iter()
        .chain(c.max_core_temp_c)
        .max_by(|a, b| a.partial_cmp(b).unwrap())
        .map(|v| v as i32);

    let cur_stat = cpu::read_times();
    let cpu_usage_pct = match (st.prev_stat, cur_stat) {
        (Some(p), Some(q)) => cpu::usage_between(&p, &q),
        _ => None,
    };
    if cur_stat.is_some() {
        st.prev_stat = cur_stat;
    }

    let cur_rapl = rapl::domains();
    let pkg_watts = cur_rapl
        .iter()
        .find(|d| d.id.ends_with(":0"))
        .and_then(|after| {
            st.prev_rapl
                .iter()
                .find(|b| b.id == after.id)
                .and_then(|before| rapl::watts(before, after, dt))
        })
        .or_else(|| {
            cur_rapl.iter().find_map(|after| {
                st.prev_rapl
                    .iter()
                    .find(|b| b.id == after.id)
                    .and_then(|before| rapl::watts(before, after, dt))
            })
        });
    st.prev_rapl = cur_rapl;

    let gpus: Vec<GpuRow> = nvidia::gpus()
        .unwrap_or_default()
        .iter()
        .map(|g| GpuRow {
            name: g.name.clone(),
            usage_pct: g.usage_pct,
            temp_c: g.temp_c,
            power_w: g.power_w,
            clock_mhz: g.gr_clock_mhz,
        })
        .collect();

    let fan_rpms: Vec<u64> = hwmon::fans().iter().map(|f| f.rpm).collect();

    let bats = battery::batteries();
    let bat_pct = bats.first().and_then(|b| b.capacity_pct);
    let bat_line = bats
        .first()
        .map(|b| {
            format!(
                "{} {}% {} {}",
                b.name,
                b.capacity_pct.map_or("-".to_string(), |x| format!("{x:.0}")),
                b.status.as_deref().unwrap_or("?"),
                f1(b.power_w, "W"),
            )
        })
        .unwrap_or_else(|| "-".to_string());
    // FlexiCharger thresholds come straight from the battery struct.
    let bat_start = bats.first().and_then(|b| b.charge_start_threshold);
    let bat_end = bats.first().and_then(|b| b.charge_end_threshold);

    let (mem_pct, mem_line) = match memory::read() {
        Some(m) => (
            Some(m.used_pct()),
            format!("{:.1} / {:.1} GB", m.used_gb(), m.total_gb()),
        ),
        None => (None, "-".to_string()),
    };

    let (ec_cpu, ec_f1, ec_f2, ec_duty, ec_err) = match ec::read_map() {
        Ok(m) => {
            let s = fan::snapshot(&m);
            (
                Some(s.cpu_temp_raw as i32),
                Some(s.fan1_rpm),
                Some(s.fan2_rpm),
                Some(fan::duty_raw_to_pct(m[fan::EC_REG_FAN1_DUTY as usize])),
                None,
            )
        }
        Err(e) => {
            let msg = e.to_string();
            let short =
                if msg.contains("Permission denied") || msg.contains("os error 13") {
                    "EC: needs root — read via sudo/pkexec".to_string()
                } else {
                    msg.chars().take(160).collect()
                };
            (None, None, None, None, Some(short))
        }
    };

    let kbds = kbd::discover();
    let mut kbd_zones = Vec::new();
    for kb in &kbds {
        if let Some(s) = kbd::read_state(kb) {
            kbd_zones.push((s.brightness, s.rgb));
        }
    }
    let (kbd_nodes, kbd_max, kbd_brightness, kbd_rgb) = match kbds.first() {
        Some(kb) => (
            kbds.len(),
            kb.max_brightness,
            kbd::read_state(kb).map(|s| s.brightness),
            kbd::read_state(kb).map(|s| s.rgb),
        ),
        None => (0, 0, None, None),
    };
    let kbd_writable = kbds
        .first()
        .map(|kb| {
            // `discover()` hanya tahu `dir`; brightness/multi_intensity pasti ada bila LED valid.
            writable(&kb.dir.join("brightness"))
        })
        .unwrap_or(false);
    let bat_writable = writable(std::path::Path::new(
        "/sys/class/power_supply/BAT0/charge_control_end_threshold",
    ));

    let product = dmi::read_dmi()
        .get("product_name")
        .cloned()
        .unwrap_or_else(|| "Axioo".to_string());
    let cpu_model = cpu::model_name().unwrap_or_else(|| "CPU".to_string());

    Snapshot {
        product,
        cpu_model,
        is_root: fan_ctrl::is_root(),
        cpu_temp_line: format!("{} / {}", f1(c.package_temp_c, "°C"), f1(c.max_core_temp_c, "°C")),
        cpu_freq_line: format!("{} / {}", f1(c.avg_mhz, "MHz"), f1(c.max_mhz, "MHz")),
        cpu_usage_pct,
        governor: c.governor.clone().unwrap_or_else(|| "?".to_string()),
        epp: c.epp.clone().unwrap_or_else(|| "?".to_string()),
        gpus,
        fan_rpms,
        bat_pct,
        bat_line,
        bat_start,
        bat_end,
        mem_pct,
        mem_line,
        pkg_watts,
        ec_cpu_temp: ec_cpu,
        ec_fan1_rpm: ec_f1,
        ec_fan2_rpm: ec_f2,
        ec_duty,
        ec_err,
        max_temp_c,
        kbd_nodes,
        kbd_max,
        kbd_brightness,
        kbd_rgb,
        kbd_zones,
        kbd_writable,
        bat_writable,
        kbd_effect,
        kbd_rear_effect,
        kbd_effect_speed,
        matugen_rev: matugen_rev(),
        profile,
        stamp: st.n,
    }
}

// ---------- Tauri commands ----------

#[tauri::command]
async fn get_snapshot(
    sampler: tauri::State<'_, Mutex<SamplerState>>,
    effects: tauri::State<'_, Mutex<EffectState>>,
) -> Result<Snapshot, String> {
    let profile = query_profile().await;
    let mut st = sampler.lock().map_err(|e| format!("sampler lock: {e}"))?;
    let (fx, fx_rear, fx_speed) = effects
        .lock()
        .map(|e| (e.current.clone(), e.rear.clone(), e.speed))
        .unwrap_or_else(|_| ("static".to_string(), "follow".to_string(), 1.0));
    Ok(build_snapshot(&mut st, profile, fx, fx_rear, fx_speed))
}

#[tauri::command]
async fn set_profile(name: String) -> Result<String, String> {
    let clean: String = name.chars().take(32).collect();
    let conn = zbus::Connection::system()
        .await
        .map_err(|e| format!("D-Bus unreachable: {e}"))?;
    let proxy = AxiooControlProxy::new(&conn)
        .await
        .map_err(|_| "axiood not running — run './install-system.sh' from tauri-app/ then restart the app".to_string())?;
    proxy
        .set_profile(&clean)
        .await
        .map_err(|e| {
            if format!("{e}").contains("ServiceUnknown") {
                "axiood not running — run './install-system.sh' from tauri-app/ then restart the app".to_string()
            } else {
                format!("daemon refused: {e}")
            }
        })
}

#[tauri::command]
async fn set_quiet_fan(quiet: bool) -> Result<String, String> {
    let conn = zbus::Connection::system()
        .await
        .map_err(|e| format!("D-Bus unreachable: {e}"))?;
    let proxy = AxiooControlProxy::new(&conn)
        .await
        .map_err(|_| "axiood not running — run './install-system.sh' from tauri-app/ then restart the app".to_string())?;
    proxy
        .set_quiet_fan(quiet)
        .await
        .map_err(|e| format!("quiet-fan failed: {e}"))
}

/// One-shot manual fan duty. REJECTED while axiood runs (daemon owns EC).
/// Clamp + verify enforced inside `fan_ctrl` (slider JS is untrusted).
#[tauri::command]
async fn fan_set_manual(duty: u8) -> Result<String, String> {
    if daemon_reachable().await {
        return Err("axiood active: direct control disabled — use mode + quiet-fan.".to_string());
    }
    let want = duty.clamp(fan::MIN_FAN_DUTY_PCT, fan::MAX_FAN_DUTY_PCT);
    match fan_ctrl::set_manual_duty(want) {
        Ok(r) => Ok(match (r.verified_pct, r.verify_skipped) {
            (Some(v), _) => format!("Manual {want}%: OK (EC {v}%)"),
            (None, true) => format!("Manual {want}%: OK (without ec_sys verification)"),
            _ => format!("Manual {want}%: OK"),
        }),
        Err(fan_ctrl::FanCtrlError::NotRoot) => Err(
            "Needs root for direct EC writes. Fix: enable axiood ('./install-system.sh') then use mode + quiet-fan.".to_string(),
        ),
        Err(e) => Err(format!("fan set failed: {e}")),
    }
}

#[tauri::command]
async fn fan_set_auto() -> Result<String, String> {
    if daemon_reachable().await {
        return Err("axiood active: direct control disabled — use mode + quiet-fan.".to_string());
    }
    match fan_ctrl::set_auto() {
        Ok(()) => Ok("EC auto: OK".to_string()),
        Err(fan_ctrl::FanCtrlError::NotRoot) => Err(
            "Needs root for direct EC writes. Fix: enable axiood ('./install-system.sh') then use mode + quiet-fan.".to_string(),
        ),
        Err(e) => Err(format!("fan auto failed: {e}")),
    }
}

async fn daemon_proxy() -> Result<AxiooControlProxy<'static>, String> {
    let conn = zbus::Connection::system()
        .await
        .map_err(|e| format!("D-Bus unreachable: {e}"))?;
    AxiooControlProxy::new(&conn)
        .await
        .map_err(|_| "axiood not running — run './install-system.sh' from tauri-app/ then restart the app".to_string())
}

/// Serahkan kipas ke firmware EC auto (via daemon; RAPL/PPD tetap ikut profil).
#[tauri::command]
async fn set_fan_ec_auto(auto: bool) -> Result<String, String> {
    let proxy = daemon_proxy().await?;
    proxy
        .set_fan_ec_auto(auto)
        .await
        .map_err(|e| format!("ec_auto failed: {e}"))
}

/// Kunci duty manual via loop daemon (clamp 40–100% di daemon).
#[tauri::command]
async fn set_fan_manual(duty: u8) -> Result<String, String> {
    let want = duty.clamp(fan::MIN_FAN_DUTY_PCT, fan::MAX_FAN_DUTY_PCT);
    let proxy = daemon_proxy().await?;
    proxy
        .set_fan_manual(want)
        .await
        .map_err(|e| format!("manual failed: {e}"))
}

/// Kembali ke kurva daemon (hapus override manual).
#[tauri::command]
async fn clear_fan_override() -> Result<String, String> {
    let proxy = daemon_proxy().await?;
    proxy
        .clear_fan_override()
        .await
        .map_err(|e| format!("return to curve failed: {e}"))
}

#[derive(serde::Deserialize)]
struct KbdSetArgs {
    zone: Option<usize>,
    brightness: u32,
    r: u8,
    g: u8,
    b: u8,
}

/// Keyboard backlight via sysfs LED (safe, udev-rule writable).
/// Mematikan efek animasi yang sedang jalan (biar tak rebutan tulis sysfs).
#[tauri::command]
async fn kbd_set(
    args: KbdSetArgs,
    effects: tauri::State<'_, Mutex<EffectState>>,
) -> Result<String, String> {
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
        kbd::set(kb, br, (args.r, args.g, args.b)).map_err(|e| {
            let msg = format!("{e}");
            if msg.contains("permission denied") {
                format!("kbd zone {i}: permission denied — run './install-system.sh' (udev rule) then restart the app")
            } else {
                format!("kbd zone {i} failed: {e}")
            }
        })?;
    }
    Ok(format!(
        "Keyboard → brightness {} rgb({},{},{})",
        args.brightness, args.r, args.g, args.b
    ))
}

#[derive(serde::Deserialize)]
struct KbdEffectArgs {
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
fn spawn_split(
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
        let rear_idx = if rear.is_some() && n >= 5 { Some(n - 1) } else { None };
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
            if let (Some(ri), Some(((r, g, b), br))) =
                (rear_idx, rear_start)
            {
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
async fn kbd_effect_start(
    args: KbdEffectArgs,
    effects: tauri::State<'_, Mutex<EffectState>>,
) -> Result<String, String> {
    let effect = kbd_effect::KbdEffect::parse(&args.effect.chars().take(16).collect::<String>())
        .ok_or_else(|| format!("unknown effect: '{}'", args.effect))?;
    let rear_raw = args.rear.unwrap_or_default().chars().take(16).collect::<String>();
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
    let mut fx = effects
        .lock()
        .map_err(|e| format!("effect lock: {e}"))?;
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
    Ok(match rear_name {
        Some(r) => format!("Effect {} running ({}x) + rear {}.", effect.label(), speed, r),
        None => format!("Effect {} running ({}x).", effect.label(), speed),
    })
}

/// Revisi palet matugen = mtime `~/.cache/ryoku/colors.json` (detik).
/// Satu `stat` syscall per snapshot — jauh lebih murah dari baca+parse JSON.
fn matugen_rev() -> u64 {
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

/// Palet matugen Ryoku (`~/.cache/ryoku/colors.json`) untuk theme GUI.
/// Murni read-only; Err bila file tak ada agar frontend fallback ke theme statis.
#[tauri::command]
fn get_matugen() -> Result<std::collections::HashMap<String, String>, String> {
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

/// Hentikan animasi RGB (kembalikan warna diam terakhir).
#[tauri::command]
async fn kbd_effect_stop(
    effects: tauri::State<'_, Mutex<EffectState>>,
) -> Result<String, String> {
    let mut fx = effects
        .lock()
        .map_err(|e| format!("effect lock: {e}"))?;
    fx.stop_all();
    Ok("Effect off — static color.".to_string())
}

#[tauri::command]
async fn battery_set(start: Option<u64>, end: Option<u64>) -> Result<String, String> {
    // Re-validate ranges server-side (JS untrusted).
    if let Some(s) = start {
        if !(40..=95).contains(&s) {
            return Err("start must be 40-95.".to_string());
        }
    }
    if let Some(e) = end {
        if !(60..=100).contains(&e) {
            return Err("end must be 60-100.".to_string());
        }
    }
    if let (Some(s), Some(e)) = (start, end) {
        if s >= e {
            return Err("start must be < end.".to_string());
        }
    }
    battery::set_charge_thresholds("BAT0", start, end).map_err(|e| {
        let msg = format!("{e}");
        if msg.contains("permission denied") {
            "battery: permission denied — run './install-system.sh' (udev rule) then restart the app".to_string()
        } else {
            format!("battery set failed: {e}")
        }
    })?;
    Ok("Charge threshold saved.".to_string())
}

fn main() {
    tauri::Builder::default()
        .plugin(tauri_plugin_autostart::init(
            tauri_plugin_autostart::MacosLauncher::LaunchAgent,
            // Argumen saat dijalankan otomatis ketika login → mulai sembunyi di tray.
            Some(vec!["--minimized"]),
        ))
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
            // Ikon tray = ikon jendela bawaan bundle (logo), fallback 32x32.
            let icon = app
                .default_window_icon()
                .cloned()
                .unwrap_or_else(|| {
                    tauri::image::Image::from_bytes(include_bytes!("../icons/32x32.png"))
                        .expect("tray fallback icon")
                });
            let show = MenuItemBuilder::with_id("show", "Show").build(app)?;
            let autostart = CheckMenuItemBuilder::with_id("autostart", "Start on login")
                .checked(
                    app.autolaunch()
                        .is_enabled()
                        .unwrap_or(false),
                )
                .build(app)?;
            let quit = MenuItemBuilder::with_id("quit", "Quit").build(app)?;
            let menu = MenuBuilder::new(app)
                .items(&[&show, &autostart, &quit])
                .build()?;
            // Clone untuk update centang dari dalam handler menu.
            let autostart_item = autostart.clone();
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
                    "autostart" => {
                        let m = app.autolaunch();
                        let now_on = m.is_enabled().unwrap_or(false);
                        let ok = if now_on { m.disable() } else { m.enable() }.is_ok();
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
            get_snapshot,
            set_profile,
            set_quiet_fan,
            fan_set_manual,
            fan_set_auto,
            set_fan_ec_auto,
            set_fan_manual,
            clear_fan_override,
            kbd_set,
            kbd_effect_start,
            kbd_effect_stop,
            battery_set,
            get_matugen,
        ])
        .run(tauri::generate_context!())
        .expect("failed to run axioo-center");
}

#[cfg(test)]
mod tests {
    use super::*;

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
        assert!(pal.contains_key("primary"), "palet Ryoku wajib punya primary");
        assert!(pal.contains_key("background"));
        for (k, v) in &pal {
            assert!(
                v.len() == 7 && v.starts_with('#'),
                "nilai {k} harus #rrggbb, dapat {v}"
            );
        }
    }
}
