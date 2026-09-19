//! `axioo-ctl fan`: EC fan inspection + privileged one-shot control.
//!
//! Safety: `dump`/`watch`/`curve` never touch the EC. `set`/`auto` write
//! one-shot duty commands via `axioo_lib::fan_ctrl` and REFUSE unless
//! running as root — the desktop GUI invokes them through pkexec
//! (`pkexec axioo-ctl fan set 70`). Duty clamped 40–100%, both fans
//! written, read-back verified via the 0xCE mirror when `ec_sys` exists.
//! The continuous curve loop belongs to the future `axiood`, not here.

use std::thread;
use std::time::{Duration, Instant};

use axioo_lib::devices::{fan_gate, FanWrite};
use axioo_lib::{cpu, ec, fan, fan_ctrl, hwmon};

/// Refuse EC writes on unvalidated hardware. Policy lives in
/// `axioo_lib::devices`; this only renders the verdict.
fn guard_writes() -> bool {
    let gate = fan_gate();
    match gate.decision {
        FanWrite::Locked => {
            println!(
                "error: fan writes locked for this model ({}).",
                gate.display
            );
            println!("  grade: {}", gate.profile.grade.as_str());
            println!("  validate the EC map first: axioo-ctl fan dump");
            println!("  then open an issue with: axioo-ctl probe --json");
            false
        }
        FanWrite::AllowedWithWarning => {
            println!(
                "  warning: {} — same barebone family, untested.",
                gate.display
            );
            println!("  proceeding (watch RPM after write, restore with: axioo-ctl fan auto)");
            true
        }
        FanWrite::Allowed => true,
    }
}

/// `axioo-ctl fan dump`: decode fan registers + cross-check hwmon.
pub fn dump() {
    println!("axioo-ctl fan dump (read-only, no EC writes)");
    let map = match ec::read_map() {
        Ok(m) => m,
        Err(e) => {
            println!("  error: {e}");
            return;
        }
    };
    let snap = fan::snapshot(&map);

    println!("\n== EC registers ==");
    println!(
        "  0x{:02X} cpu_temp  raw {} (~{}C on validated models)",
        fan::EC_REG_CPU_TEMP,
        snap.cpu_temp_raw,
        snap.cpu_temp_raw
    );
    println!(
        "  0x{:02X} gpu_temp  raw {} ({})",
        fan::EC_REG_GPU_TEMP,
        snap.gpu_temp_raw,
        if snap.gpu_temp_raw == 0 {
            "GPU idle/asleep"
        } else {
            "°C on validated models"
        },
    );
    println!(
        "  0x{:02X} fan1_duty raw {} (~{}%)",
        fan::EC_REG_FAN1_DUTY,
        snap.fan1_duty_raw,
        snap.fan1_duty_pct
    );
    println!(
        "  0x{:02X}/0x{:02X} fan1 rpm {} (raw {:02X} {:02X})",
        fan::EC_REG_FAN1_RPM_HI,
        fan::EC_REG_FAN1_RPM_LO,
        snap.fan1_rpm,
        snap.rpm_regs[0],
        snap.rpm_regs[1]
    );
    println!(
        "  0x{:02X}/0x{:02X} fan2 rpm {} (raw {:02X} {:02X})",
        fan::EC_REG_FAN2_RPM_HI,
        fan::EC_REG_FAN2_RPM_LO,
        snap.fan2_rpm,
        snap.rpm_regs[2],
        snap.rpm_regs[3]
    );
    // Neighbor bytes help spot alternate maps (e.g. GPU RPM at 0xD4/0xD5).
    println!(
        "  0xD4..0xD7     {:02X} {:02X} {:02X} {:02X} (alternates to compare under load)",
        map[0xD4], map[0xD5], map[0xD6], map[0xD7]
    );

    println!("\n== cross-check vs hwmon (read-only validation) ==");
    let c = cpu::sample();
    match c.package_temp_c {
        Some(pkg) => {
            let diff = (pkg - snap.cpu_temp_raw as f64).abs();
            let verdict = if diff <= 5.0 {
                "MATCH"
            } else {
                "MISMATCH — map not validated"
            };
            println!(
                "  EC 0x07 = {}C  vs coretemp package = {pkg:.1}C  [{verdict}]",
                snap.cpu_temp_raw
            );
        }
        None => println!("  (no coretemp package sensor to compare against 0x07)"),
    }
    let fans = hwmon::fans();
    if fans.is_empty() {
        println!("  (no hwmon fan nodes; compare manually when the fan is audible)");
    }
    for f in &fans {
        let ec_rpms = [snap.fan1_rpm as i64, snap.fan2_rpm as i64];
        let closest = ec_rpms
            .iter()
            .map(|r| (r - f.rpm as i64).abs())
            .min()
            .unwrap_or(i64::MAX);
        let verdict = if closest <= 300 {
            "MATCH"
        } else {
            "MISMATCH — map not validated"
        };
        println!(
            "  hwmon {} = {} RPM  vs EC {} / {} RPM  [{verdict}]",
            f.label, f.rpm, snap.fan1_rpm, snap.fan2_rpm
        );
    }

    println!("\n== next steps ==");
    println!("  Repeat at idle vs load: if 0x07 tracks coretemp and");
    println!("  0xD0–0xD3 track hwmon RPM, the map is valid for this model.");
    println!("  If they don't match, compare the 0xD4/0xD5 bytes that change");
    println!("  while the fan spins (see docs/ec-fan-protocol.md section F).");
    println!("  Preview curve without writing: axioo-ctl fan curve --temp <C> --duty <pct>");
}

/// `axioo-ctl fan watch`: poll the EC map over time (read-only).
/// One log row per sample — run it while putting load on the machine,
/// then check that EC temps/RPM track coretemp/hwmon (validation step F).
pub fn watch(interval_s: f64, count: Option<u64>) {
    let interval = Duration::from_secs_f64(interval_s.max(0.2));
    println!("axioo-ctl fan watch (read-only, Ctrl-C to quit)");
    println!(
        "{:>7} {:>6} {:>6} {:>6} {:>8} {:>8} {:>7} hwmon_rpm",
        "t(s)", "ec_cpu", "ec_gpu", "duty%", "ec_rpm1", "ec_rpm2", "pkg"
    );
    let t0 = Instant::now();
    let mut shown = 0u64;
    loop {
        let map = match ec::read_map() {
            Ok(m) => m,
            Err(e) => {
                println!("  error: {e}");
                return;
            }
        };
        let snap = fan::snapshot(&map);
        let pkg = cpu::sample()
            .package_temp_c
            .map_or("-".to_string(), |v| format!("{v:.0}C"));
        let frpm = hwmon::fans()
            .iter()
            .map(|f| f.rpm.to_string())
            .collect::<Vec<_>>()
            .join("/");
        println!(
            "{:>7.1} {:>6} {:>6} {:>6} {:>8} {:>8} {:>7} {}",
            t0.elapsed().as_secs_f64(),
            format!("{}C", snap.cpu_temp_raw),
            if snap.gpu_temp_raw == 0 {
                "-".to_string()
            } else {
                format!("{}C", snap.gpu_temp_raw)
            },
            format!("{}%", snap.fan1_duty_pct),
            snap.fan1_rpm,
            snap.fan2_rpm,
            pkg,
            if frpm.is_empty() {
                "-".to_string()
            } else {
                frpm
            },
        );
        shown += 1;
        if let Some(n) = count {
            if shown >= n {
                break;
            }
        }
        thread::sleep(interval);
    }
}

/// `axioo-ctl fan curve`: preview pure `auto_duty_step`, no hardware I/O.
pub fn curve(temp_c: i32, duty: u8) {
    let next = fan::auto_duty_step(temp_c, duty);
    let raw = fan::duty_pct_to_raw(next);
    println!("axioo-ctl fan curve (preview, no EC writes)");
    println!("  input:  temp={temp_c}C duty={duty}%");
    println!("  output: duty={next}% (raw {raw:#04X})");
    if next == duty {
        println!("  (inside hysteresis band — duty held)");
    }
    println!(
        "  safe clamp: {}–{}% (fans stall below ~40%)",
        fan::MIN_FAN_DUTY_PCT,
        fan::MAX_FAN_DUTY_PCT
    );
}

/// `axioo-ctl fan set`: one-shot manual duty on both fans (needs root).
/// Fails cleanly with usage hint when not root (GUI uses pkexec).
pub fn set(pct: u8) {
    if !guard_writes() {
        std::process::exit(1);
    }
    if !fan_ctrl::is_root() {
        println!("error: fan set needs root.");
        println!("  run: pkexec axioo-ctl fan set {pct}   (or sudo)");
        std::process::exit(1);
    }
    let want = pct.clamp(fan::MIN_FAN_DUTY_PCT, fan::MAX_FAN_DUTY_PCT);
    if want != pct {
        println!("  (duty {pct}% clamped to {want}% — safe range 40–100%)");
    }
    match fan_ctrl::set_manual_duty(want) {
        Ok(rep) => {
            println!("axioo-ctl fan set: manual {want}% both fans (cmd 0x99 → 0x01+0x02)");
            match rep.verified_pct {
                Some(got) => println!("  verify OK: 0xCE mirror = {got}%"),
                None => println!("  verify skipped: ec_sys unreadable (write still applied)"),
            }
            println!("  restore EC control: axioo-ctl fan auto");
        }
        Err(e) => {
            println!("error: {e}");
            std::process::exit(1);
        }
    }
}

/// `axioo-ctl fan auto`: restore EC auto control on both fans (needs root).
pub fn auto() {
    if !guard_writes() {
        std::process::exit(1);
    }
    if !fan_ctrl::is_root() {
        println!("error: fan auto needs root.");
        println!("  run: pkexec axioo-ctl fan auto   (or sudo)");
        std::process::exit(1);
    }
    match fan_ctrl::set_auto() {
        Ok(()) => println!("axioo-ctl fan auto: control returned to EC (0x99, port 0xFF)"),
        Err(e) => {
            println!("error: {e}");
            std::process::exit(1);
        }
    }
}

/// `axioo-ctl fan ping`: no-op root probe for GUI pre-auth at startup.
/// Prints `ok` and exits 0 only when running as root (via pkexec/sudo);
/// otherwise exits 1. Never touches the EC.
pub fn ping() {
    if !fan_ctrl::is_root() {
        println!("error: fan ping needs root.");
        println!("  run: pkexec axioo-ctl fan ping   (or sudo)");
        std::process::exit(1);
    }
    println!("ok");
}
