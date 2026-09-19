//! `axioo-ctl probe`: read-only hardware capability dump.
//! `--json` emits the same data machine-readable (for scripts + issue reports).

use axioo_lib::{battery, cpu, devices, dmi, ec, hwmon, leds, nvidia, platform, rapl};

fn section(title: &str) {
    println!("\n== {title} ==");
}

fn na() -> &'static str {
    "(not available)"
}

pub fn run() {
    println!("axioo-ctl probe (read-only, no hardware writes)");

    section("system (DMI)");
    let dmi = dmi::read_dmi();
    if dmi.is_empty() {
        println!("  {}", na());
    }
    for (k, v) in &dmi {
        println!("  {k:<16} {v}");
    }

    section("device (dynamic database)");
    let dev = devices::current();
    println!("  id: {}", dev.id);
    println!("  model: {}", dev.display_name(&dmi));
    println!("  grade: {}", dev.grade.as_str());
    println!("  table: {}", devices::table_source());
    if let Some(w) = devices::table_warning() {
        println!("  table warning: {w}");
    }
    println!(
        "  fan writes: {}",
        if dev.fan_write_allowed {
            "allowed"
        } else {
            "locked (read-only)"
        }
    );
    println!("  kbd zones: {}", dev.kbd_zones);
    println!("  note: {}", dev.notes);

    section("cpu");
    let c = cpu::sample();
    println!(
        "  package: {}  max core: {}  freq avg/max: {} / {}",
        fmt_temp(c.package_temp_c),
        fmt_temp(c.max_core_temp_c),
        fmt_mhz(c.avg_mhz),
        fmt_mhz(c.max_mhz),
    );
    println!(
        "  driver: {}  governor: {}  epp: {}",
        c.driver.as_deref().unwrap_or(na()),
        c.governor.as_deref().unwrap_or(na()),
        c.epp.as_deref().unwrap_or(na()),
    );

    section("hwmon chips");
    for (dev, chip) in hwmon::chips() {
        println!("  {dev:<10} {chip}");
    }

    section("temperatures");
    let temps = hwmon::temps();
    if temps.is_empty() {
        println!("  {}", na());
    }
    for t in temps {
        println!("  {:<14} {:<22} {:>6.1} C", t.chip, t.label, t.input_c);
    }

    section("fans (hwmon)");
    let fans = hwmon::fans();
    if fans.is_empty() {
        println!("  {} (no fan*_input nodes; EC direct access needed)", na());
    }
    for f in fans {
        println!("  {:<14} {:<22} {:>6} RPM", f.chip, f.label, f.rpm);
    }

    section("RAPL power domains");
    let domains = rapl::domains();
    if domains.is_empty() {
        println!("  {}", na());
    }
    for d in domains {
        let energy = d
            .energy_uj
            .map_or("(energy_uj needs root)".to_string(), |e| {
                format!("energy={e} uJ")
            });
        println!("  {:<16} {:<12} {energy}", d.id, d.name);
        for (cname, w) in &d.constraints {
            println!("  {:<16}   constraint {cname} = {w:.1}W", "");
        }
    }

    section("NVIDIA GPUs");
    match nvidia::gpus() {
        None => println!("  {} (no nvidia-smi / no NVIDIA GPU)", na()),
        Some(gpus) => {
            for g in gpus {
                println!(
                    "  [{}] {}  temp={}  power={} (limit {})  gr={} mem={}",
                    g.index,
                    g.name,
                    fmt_opt_c(g.temp_c),
                    fmt_opt_w(g.power_w),
                    fmt_opt_w(g.power_limit_w),
                    fmt_opt_mhz(g.gr_clock_mhz),
                    fmt_opt_mhz(g.mem_clock_mhz),
                );
            }
        }
    }

    section("batteries");
    let batteries = battery::batteries();
    if batteries.is_empty() {
        println!("  {}", na());
    }
    for b in batteries {
        println!(
            "  {}  {}%  {}  {:.2}V  power={}  cycles={}  charge_limits={}/{}",
            b.name,
            fmt_opt1(b.capacity_pct),
            b.status.as_deref().unwrap_or("?"),
            b.voltage_v.unwrap_or(0.0),
            fmt_opt_w(b.power_w),
            b.cycle_count.map_or("?".to_string(), |c| c.to_string()),
            b.charge_start_threshold
                .map_or("?".to_string(), |c| c.to_string()),
            b.charge_end_threshold
                .map_or("?".to_string(), |c| c.to_string()),
        );
    }

    section("keyboard backlight candidates (/sys/class/leds)");
    let kbd = leds::kbd_candidates();
    if kbd.is_empty() {
        println!("  {} (driver missing? see: axioo-ctl kbd status)", na());
    }
    for l in kbd {
        println!("  {}  {}/{}", l.name, l.brightness, l.max_brightness);
    }
    println!("  total LED class devices: {}", leds::leds().len());

    section("ACPI platform profile");
    match platform::platform_profile() {
        None => println!("  {} (profiles must go via RAPL/cpufreq directly)", na()),
        Some((cur, choices)) => println!("  current={cur} choices=[{choices}]"),
    }

    section("platform devices (/sys/devices/platform)");
    let devs = platform::platform_devices();
    let relevant: Vec<_> = devs
        .iter()
        .filter(|d| {
            let u = d.to_uppercase();
            u.starts_with("CLV") || u.contains("CLEVO") || u.contains("WMI") || u.contains("EC")
        })
        .collect();
    if relevant.is_empty() {
        println!(
            "  (no CLV*/EC/WMI platform device matched; {} total)",
            devs.len()
        );
    }
    for d in relevant {
        println!("  {d}");
    }

    section("WMI GUIDs");
    let guids = platform::wmi_guids();
    if guids.is_empty() {
        println!("  {}", na());
    }
    for (dev, guid) in guids {
        println!("  {dev:<28} {guid}");
    }

    section("thermal zones / cooling devices");
    for z in platform::thermal_zones() {
        println!("  {:<16} {:<18} {}", z.name, z.kind, fmt_opt_c(z.temp_c));
    }
    for c in platform::cooling_devices() {
        println!(
            "  {:<16} {:<18} state {}/{}",
            c.name,
            c.kind,
            c.cur_state.map_or("?".to_string(), |v| v.to_string()),
            c.max_state.map_or("?".to_string(), |v| v.to_string()),
        );
    }

    section("EC access");
    println!("  ec_sys module available: {}", ec::ec_sys_available());
    println!(
        "  ec_sys.write_support param: {}",
        ec::ec_write_support().as_deref().unwrap_or(na())
    );
    println!("  EC io map readable:       {}", ec::ec_io_readable());
    let mods = ec::loaded_vendor_modules();
    if mods.is_empty() {
        println!("  vendor modules loaded:    (none - clevo/tuxedo drivers not installed)");
    } else {
        println!("  vendor modules loaded:    {}", mods.join(", "));
    }
}

fn fmt_temp(v: Option<f64>) -> String {
    v.map_or("-".to_string(), |x| format!("{x:.1}C"))
}

fn fmt_mhz(v: Option<f64>) -> String {
    v.map_or("-".to_string(), |x| format!("{x:.0}MHz"))
}

fn fmt_opt_c(v: Option<f64>) -> String {
    v.map_or("-".to_string(), |x| format!("{x:.1}C"))
}

fn fmt_opt_w(v: Option<f64>) -> String {
    v.map_or("-".to_string(), |x| format!("{x:.1}W"))
}

fn fmt_opt_mhz(v: Option<u64>) -> String {
    v.map_or("-".to_string(), |x| format!("{x}MHz"))
}

fn fmt_opt1(v: Option<f64>) -> String {
    v.map_or("-".to_string(), |x| format!("{x:.0}"))
}

/// Machine-readable twin of [`run`] — same sources, no prints besides JSON.
/// `null` = unreadable on this machine (NOT an error).
pub fn run_json() {
    use serde_json::json;
    let c = cpu::sample();
    let dev = devices::current();
    let dmi_map = dmi::read_dmi();
    let display = dev.display_name(&dmi_map);
    let out = json!({
        "tool": "axioo-ctl probe",
        "dmi": dmi_map,
        "device": {
            "id": dev.id,
            "marketing": display,
            "grade": dev.grade.as_str(),
            "fan_write_allowed": dev.fan_write_allowed,
            "kbd_zones": dev.kbd_zones,
            "notes": dev.notes,
        },
        "cpu": {
            "package_temp_c": c.package_temp_c,
            "max_core_temp_c": c.max_core_temp_c,
            "avg_mhz": c.avg_mhz,
            "max_mhz": c.max_mhz,
            "driver": c.driver,
            "governor": c.governor,
            "epp": c.epp,
        },
        "hwmon_chips": hwmon::chips(),
        "temps": hwmon::temps().iter().map(|t| json!({
            "chip": t.chip, "label": t.label, "input_c": t.input_c,
        })).collect::<Vec<_>>(),
        "fans": hwmon::fans().iter().map(|f| json!({
            "chip": f.chip, "label": f.label, "rpm": f.rpm,
        })).collect::<Vec<_>>(),
        "rapl": rapl::domains().iter().map(|d| json!({
            "id": d.id, "name": d.name, "energy_uj": d.energy_uj,
            "constraints_w": d.constraints,
        })).collect::<Vec<_>>(),
        "nvidia": nvidia::gpus().unwrap_or_default().iter().map(|g| json!({
            "index": g.index, "name": g.name, "temp_c": g.temp_c,
            "power_w": g.power_w, "power_limit_w": g.power_limit_w,
            "gr_clock_mhz": g.gr_clock_mhz, "mem_clock_mhz": g.mem_clock_mhz,
            "usage_pct": g.usage_pct,
        })).collect::<Vec<_>>(),
        "dgpu": {
            "pci_addr": nvidia::dgpu_pci_addr(),
            "power": nvidia::dgpu_power().map(|p| match p {
                nvidia::DgpuPower::Active => "active",
                nvidia::DgpuPower::Suspended => "suspended",
                nvidia::DgpuPower::Absent => "absent",
            }),
            "procs": nvidia::dgpu_procs().unwrap_or_default().iter().map(|p| json!({
                "pid": p.pid, "name": p.name, "mem_mb": p.mem_mb,
            })).collect::<Vec<_>>(),
        },
        "batteries": battery::batteries().iter().map(|b| json!({
            "name": b.name, "capacity_pct": b.capacity_pct, "status": b.status,
            "technology": b.technology, "voltage_v": b.voltage_v,
            "current_a": b.current_a, "power_w": b.power_w,
            "cycle_count": b.cycle_count, "health_pct": b.health_pct,
            "time_hours": b.time_hours(),
            "charge_start_threshold": b.charge_start_threshold,
            "charge_end_threshold": b.charge_end_threshold,
        })).collect::<Vec<_>>(),
        "ac_online": battery::ac_online(),
        "leds": leds::kbd_candidates().iter().map(|l| json!({
            "name": l.name, "brightness": l.brightness,
            "max_brightness": l.max_brightness,
        })).collect::<Vec<_>>(),
        "led_count": leds::leds().len(),
        "platform_profile": platform::platform_profile(),
        "platform_devices": platform::platform_devices().iter()
            .filter(|d| {
                let u = d.to_uppercase();
                u.starts_with("CLV") || u.contains("CLEVO") || u.contains("WMI") || u.contains("EC")
            }).collect::<Vec<_>>(),
        "wmi_guids": platform::wmi_guids(),
        "thermal_zones": platform::thermal_zones().iter().map(|z| json!({
            "name": z.name, "kind": z.kind, "temp_c": z.temp_c,
        })).collect::<Vec<_>>(),
        "cooling_devices": platform::cooling_devices().iter().map(|z| json!({
            "name": z.name, "kind": z.kind,
            "cur_state": z.cur_state, "max_state": z.max_state,
        })).collect::<Vec<_>>(),
        "ec": {
            "ec_sys_available": ec::ec_sys_available(),
            "ec_write_support": ec::ec_write_support(),
            "ec_io_readable": ec::ec_io_readable(),
            "vendor_modules": ec::loaded_vendor_modules(),
        },
    });
    println!("{}", serde_json::to_string_pretty(&out).unwrap());
}
