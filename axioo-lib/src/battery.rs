//! Batteries from `/sys/class/power_supply`.

use std::fs;

use crate::{parse_f64, read_trim_str};

#[derive(Debug, Clone)]
pub struct Battery {
    pub name: String,
    pub capacity_pct: Option<f64>,
    pub status: Option<String>,
    pub technology: Option<String>,
    pub voltage_v: Option<f64>,
    pub current_a: Option<f64>,
    pub power_w: Option<f64>,
    pub cycle_count: Option<u64>,
    /// Vendor charge thresholds, when the firmware exposes them.
    pub charge_start_threshold: Option<u64>,
    pub charge_end_threshold: Option<u64>,
}

pub fn batteries() -> Vec<Battery> {
    let mut out = Vec::new();
    let Ok(entries) = fs::read_dir("/sys/class/power_supply") else {
        return out;
    };
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().into_owned();
        if !(name.starts_with("BAT") || name.starts_with("CMB")) {
            continue;
        }
        let base = format!("/sys/class/power_supply/{name}");
        let get = |f: &str| read_trim_str(&format!("{base}/{f}"));
        let u = |v: Option<String>| v.and_then(|s| parse_f64(&s)).map(|x| x / 1_000_000.0);
        let volt = u(get("voltage_now"));
        let amp = u(get("current_now"));
        let power = u(get("power_now")).or_else(|| match (volt, amp) {
            (Some(v), Some(a)) => Some(v * a),
            _ => None,
        });
        out.push(Battery {
            name,
            capacity_pct: get("capacity").and_then(|s| parse_f64(&s)),
            status: get("status"),
            technology: get("technology"),
            voltage_v: volt,
            current_a: amp,
            power_w: power,
            cycle_count: get("cycle_count").and_then(|s| s.parse().ok()),
            charge_start_threshold: get("charge_control_start_threshold")
                .and_then(|s| s.parse().ok()),
            charge_end_threshold: get("charge_control_end_threshold")
                .and_then(|s| s.parse().ok()),
        });
    }
    out.sort_by(|a, b| a.name.cmp(&b.name));
    out
}
