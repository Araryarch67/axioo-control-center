//! Snapshot types + builder (JSON to React).

use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc, Mutex,
};
use std::time::Instant;

use axioo_lib::{battery, cpu, devices, dmi, ec, fan, fan_ctrl, hwmon, kbd, memory, nvidia, rapl};
use serde::Serialize;

use crate::dbus::{AxiooControlProxy, PpdNewProxy, PpdOldProxy};
use crate::system::matugen_rev;

// ---------- Snapshot types (JSON to React) ----------
#[derive(Serialize, Clone, Default)]
pub(crate) struct GpuRow {
    name: String,
    usage_pct: Option<f64>,
    temp_c: Option<f64>,
    power_w: Option<f64>,
    clock_mhz: Option<u64>,
}
#[derive(Serialize, Clone, Default)]
pub(crate) struct ProfileState {
    pub(crate) daemon: bool,
    pub(crate) profile: String,
    pub(crate) quiet_fan: bool,
    pub(crate) ppd: String,
    pub(crate) curve: Vec<(i32, u8)>,
    pub(crate) fan_duty: u8,
    /// "curve" | "manual" | "ec_auto" (daemon) — "" bila daemon mati.
    pub(crate) fan_mode: String,
    /// Live package watts from the daemon (root-only counter).
    /// -1.0 = unknown (daemon down/old, or no second sample yet).
    pub(crate) power_watts: f64,
}
#[derive(Serialize, Clone, Default)]
pub(crate) struct Snapshot {
    product: String,
    /// Dynamic device DB (`axioo-lib/src/devices.rs`): stable id, grade
    /// string, and whether fan EC writes are allowed on this model.
    device_id: String,
    device_grade: String,
    fan_write_allowed: bool,
    cpu_model: String,
    is_root: bool,
    cpu_temp_line: String,
    cpu_freq_line: String,
    cpu_usage_pct: Option<f64>,
    governor: String,
    epp: String,
    gpus: Vec<GpuRow>,
    /// dGPU RTD3 state: "active" | "suspended" | "absent" (+ holders).
    /// Procs queried ONLY when active (never wake a sleeping GPU).
    dgpu_state: String,
    dgpu_procs: Vec<(u32, String, Option<u64>)>,
    fan_rpms: Vec<u64>,
    bat_pct: Option<f64>,
    bat_line: String,
    bat_start: Option<u64>,
    bat_end: Option<u64>,
    /// Full/design health (read-only sysfs; `None` bila firmware tak expose).
    bat_health_pct: Option<f64>,
    bat_full_milli: Option<f64>,
    bat_design_milli: Option<f64>,
    bat_capacity_unit: Option<String>,
    /// Hours until empty/full (None when Full/rate unreadable).
    bat_time_h: Option<f64>,
    bat_charging: bool,
    /// AC mains online (None bila firmware tak expose node AC*).
    ac_online: Option<bool>,
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
pub(crate) struct SamplerState {
    pub(crate) prev_stat: Option<cpu::CpuTimes>,
    pub(crate) prev_rapl: Vec<rapl::RaplDomain>,
    pub(crate) prev_t: Instant,
    pub(crate) n: u64,
}
/// Status animasi RGB keyboard (userspace thread via `kbd_effect`).
/// Hanya satu efek jalan; start baru mematikan yang lama.
pub(crate) struct EffectState {
    /// Nama efek aktif ("static" = tidak ada animasi).
    pub(crate) current: String,
    /// Efek rear exhaust independen ("follow" = ikut efek utama).
    pub(crate) rear: String,
    /// Pengali kecepatan terakhir (1.0 = normal).
    pub(crate) speed: f32,
    /// Flag stop untuk thread yang jalan (None = tidak ada).
    pub(crate) stops: Vec<Arc<AtomicBool>>,
}
impl EffectState {
    /// Matikan semua thread efek (idempoten). Thread keluar ≤1 tick (60ms).
    pub(crate) fn stop_all(&mut self) {
        for s in self.stops.drain(..) {
            s.store(true, Ordering::Relaxed);
        }
        self.current = "static".to_string();
        self.rear = "follow".to_string();
    }
}
pub(crate) fn ppd_display(ppd: &str) -> (&'static str, bool) {
    match ppd {
        "power-saver" => ("Balanced", false),
        "balanced" => ("Entertainment", false),
        "performance" => ("Performance", false),
        _ => ("Balanced", false),
    }
}
pub(crate) async fn query_profile() -> ProfileState {
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
                    fan_mode: proxy
                        .get_fan_mode()
                        .await
                        .unwrap_or_else(|_| "curve".to_string()),
                    power_watts: proxy.get_power_watts().await.unwrap_or(-1.0),
                };
            }
        }
        // Fallback: read PPD directly (display only).
        let mut ppd: Option<String> = None;
        if let Ok(builder) =
            PpdNewProxy::builder(&conn).destination("org.freedesktop.UPower.PowerProfiles")
        {
            if let Ok(p) = builder.build().await {
                ppd = p.active_profile().await.ok();
            }
        }
        if ppd.is_none() {
            if let Ok(builder) = PpdOldProxy::builder(&conn).destination("net.hadess.PowerProfiles")
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
                power_watts: -1.0,
            };
        }
    }
    ProfileState {
        daemon: false,
        ppd: "-".to_string(),
        profile: "Balanced".to_string(),
        power_watts: -1.0,
        ..Default::default()
    }
}
/// Probe tulis-tanpa-menulis: open O_WRONLY lalu langsung drop.
/// Aman untuk sysfs (permission dicek saat open, tak ada byte terkirim).
pub(crate) fn writable(path: &std::path::Path) -> bool {
    std::fs::OpenOptions::new()
        .write(true)
        .open(path)
        .map(|_| ())
        .is_ok()
}
pub(crate) fn f1(v: Option<f64>, unit: &str) -> String {
    v.map_or_else(|| "-".to_string(), |x| format!("{x:.1}{unit}"))
}
pub(crate) fn build_snapshot(
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
    // Daemon value first (root-only counter the GUI cannot read itself);
    // direct computation is the fallback when the daemon is down or old.
    let pkg_watts = if profile.power_watts >= 0.0 {
        Some(profile.power_watts)
    } else {
        cur_rapl
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
            })
    };
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

    let (dgpu_state, dgpu_procs) = match nvidia::dgpu_pci_addr() {
        None => ("absent".to_string(), Vec::new()),
        Some(_) => match nvidia::dgpu_power() {
            Some(nvidia::DgpuPower::Suspended) => ("suspended".to_string(), Vec::new()),
            _ => (
                "active".to_string(),
                nvidia::dgpu_procs()
                    .unwrap_or_default()
                    .iter()
                    .map(|p| (p.pid, p.name.clone(), p.mem_mb))
                    .collect(),
            ),
        },
    };

    let bats = battery::batteries();
    let bat_pct = bats.first().and_then(|b| b.capacity_pct);
    let bat_line = bats
        .first()
        .map(|b| {
            format!(
                "{} {}% {} {}",
                b.name,
                b.capacity_pct
                    .map_or("-".to_string(), |x| format!("{x:.0}")),
                b.status.as_deref().unwrap_or("?"),
                f1(b.power_w, "W"),
            )
        })
        .unwrap_or_else(|| "-".to_string());
    // FlexiCharger thresholds come straight from the battery struct.
    let bat_start = bats.first().and_then(|b| b.charge_start_threshold);
    let bat_end = bats.first().and_then(|b| b.charge_end_threshold);
    // Health = full/design ratio (read-only; computed in axioo-lib).
    let bat_health_pct = bats.first().and_then(|b| b.health_pct);
    let bat_full_milli = bats.first().and_then(|b| b.full_milli);
    let bat_design_milli = bats.first().and_then(|b| b.design_milli);
    let bat_capacity_unit = bats.first().and_then(|b| b.capacity_unit.clone());
    let bat_time_h = bats.first().and_then(|b| b.time_hours());
    let bat_charging = bats.first().is_some_and(|b| b.is_charging());
    let ac_online = battery::ac_online();

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
            let short = if msg.contains("Permission denied") || msg.contains("os error 13") {
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
    let dev = devices::current();

    Snapshot {
        product,
        device_id: dev.id.to_string(),
        device_grade: dev.grade.as_str().to_string(),
        fan_write_allowed: dev.fan_write_allowed,
        cpu_model,
        is_root: fan_ctrl::is_root(),
        cpu_temp_line: format!(
            "{} / {}",
            f1(c.package_temp_c, "°C"),
            f1(c.max_core_temp_c, "°C")
        ),
        cpu_freq_line: format!("{} / {}", f1(c.avg_mhz, "MHz"), f1(c.max_mhz, "MHz")),
        cpu_usage_pct,
        governor: c.governor.clone().unwrap_or_else(|| "?".to_string()),
        epp: c.epp.clone().unwrap_or_else(|| "?".to_string()),
        gpus,
        dgpu_state,
        dgpu_procs,
        fan_rpms,
        bat_pct,
        bat_line,
        bat_start,
        bat_end,
        bat_health_pct,
        bat_full_milli,
        bat_design_milli,
        bat_capacity_unit,
        bat_time_h,
        bat_charging,
        ac_online,
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
pub(crate) async fn get_snapshot(
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
