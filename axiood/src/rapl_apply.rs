//! RAPL power-limit apply (sysfs writes, daemon-side only).
//!
//! sengaja TIDAK di `axioo-lib` (lib read-only kecuali `kbd::set` +
//! `fan_ctrl`): tulis constraint hanya milik `axiood` root.

use std::fs;

/// Find `constraint_N_power_limit_uw` path whose `constraint_N_name` matches.
fn constraint_path(base: &str, want_name: &str) -> Option<String> {
    for n in 0..4u32 {
        let name_path = format!("{base}/constraint_{n}_name");
        let name = fs::read_to_string(&name_path).ok()?;
        if name.trim() == want_name {
            return Some(format!("{base}/constraint_{n}_power_limit_uw"));
        }
    }
    None
}

/// Apply PL1 (long_term) + PL2 (short_term) in watts to `intel-rapl:0`.
/// Returns the paths written. Errors when not root / sysfs absent.
pub fn apply_pl1_pl2(pl1_w: f64, pl2_w: f64) -> std::io::Result<Vec<String>> {
    let base = "/sys/class/powercap/intel-rapl:0";
    let mut written = Vec::new();
    for (name, w) in [("long_term", pl1_w), ("short_term", pl2_w)] {
        if let Some(path) = constraint_path(base, name) {
            let uw = (w * 1_000_000.0) as u64;
            fs::write(&path, uw.to_string())?;
            written.push(format!("{path}={uw}"));
        }
    }
    if written.is_empty() {
        return Err(std::io::Error::new(
            std::io::ErrorKind::NotFound,
            format!("no long_term/short_term constraints under {base}"),
        ));
    }
    Ok(written)
}

/// Current PL1/PL2 in watts (read-only, for logging / `axioo-ctl profile`).
#[allow(dead_code)]
pub fn current_pl1_pl2() -> (Option<f64>, Option<f64>) {
    let base = "/sys/class/powercap/intel-rapl:0";
    let read = |name: &str| {
        constraint_path(base, name).and_then(|p| {
            fs::read_to_string(p)
                .ok()?
                .trim()
                .parse::<f64>()
                .ok()
                .map(|uw| uw / 1_000_000.0)
        })
    };
    (read("long_term"), read("short_term"))
}
