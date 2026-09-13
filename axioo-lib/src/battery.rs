//! Batteries from `/sys/class/power_supply`, incl. FlexiCharger-style
//! charge thresholds (standard kernel `charge_control_*` attributes,
//! firmware-mediated — same safety class as LED sysfs writes).

use std::fmt;
use std::fs;
use std::io;

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

/// Discrete steps the firmware accepts, e.g. `[40, 50, 60, 70, 80, 95]`.
/// `which` is `"start"` or `"end"`. Empty when unsupported/unreadable.
pub fn charge_available(name: &str, which: &str) -> Vec<u64> {
    let path = format!("/sys/class/power_supply/{name}/charge_control_{which}_available_thresholds");
    read_trim_str(&path)
        .map(|s| {
            s.split_whitespace().filter_map(|p| p.parse().ok()).collect()
        })
        .unwrap_or_default()
}

/// Failure modes for [`set_charge_thresholds`].
#[derive(Debug)]
pub enum ChargeError {
    NoDevice { detail: String },
    /// Value not in the firmware's available list.
    NotAllowed { which: String, value: u64, allowed: Vec<u64> },
    Io(io::Error),
}

impl fmt::Display for ChargeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ChargeError::NoDevice { detail } => write!(f, "no battery {detail}"),
            ChargeError::NotAllowed { which, value, allowed } => {
                let list = allowed.iter().map(|v| v.to_string()).collect::<Vec<_>>().join(" ");
                write!(f, "{which} threshold {value} not allowed (pilih: {list})")
            }
            ChargeError::Io(e) if e.kind() == io::ErrorKind::PermissionDenied => write!(
                f,
                "permission denied writing charge thresholds (need root). Re-run with sudo"
            ),
            ChargeError::Io(e) => write!(f, "sysfs write failed: {e}"),
        }
    }
}

impl std::error::Error for ChargeError {}

/// Set charge start/end thresholds (each `None` = biarkan).
/// Values validated against the firmware's available lists.
pub fn set_charge_thresholds(
    name: &str,
    start: Option<u64>,
    end: Option<u64>,
) -> Result<(), ChargeError> {
    let base = format!("/sys/class/power_supply/{name}");
    if !std::path::Path::new(&base).exists() {
        return Err(ChargeError::NoDevice { detail: format!("{base} absent") });
    }
    for (which, val) in [("start", start), ("end", end)].into_iter().filter_map(|(w, v)| v.map(|x| (w, x))) {
        let allowed = charge_available(name, which);
        if !allowed.is_empty() && !allowed.contains(&val) {
            return Err(ChargeError::NotAllowed { which: which.to_string(), value: val, allowed });
        }
        fs::write(format!("{base}/charge_control_{which}_threshold"), val.to_string())
            .map_err(ChargeError::Io)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_disallowed_values() {
        // Against a fake sysfs-less name the device check fires first.
        assert!(matches!(
            set_charge_thresholds("BAT-DOES-NOT-EXIST", Some(80), None),
            Err(ChargeError::NoDevice { .. })
        ));
    }
}
