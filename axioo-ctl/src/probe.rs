//! `axioo-ctl probe`: read-only hardware capability dump.

use axioo_lib::{battery, cpu, dmi, ec, hwmon, leds, nvidia, platform, rapl};

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
