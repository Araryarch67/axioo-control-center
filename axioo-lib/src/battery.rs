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
    /// Remaining / full energy in mWh (converted charge×voltage when the
    /// firmware only exposes `charge_*`). Basis for [`Battery::time_hours`].
    pub energy_now_mwh: Option<f64>,
    pub energy_full_mwh: Option<f64>,
    /// Full-charge vs design capacity, in milli-units (`mWh` bila firmware
    /// expose `energy_*`, `mAh` bila hanya `charge_*`). Rasio keduanya =
    /// health. Murni read-only sysfs.
    pub full_milli: Option<f64>,
    pub design_milli: Option<f64>,
    pub capacity_unit: Option<String>,
    /// `full / design * 100` (`None` bila firmware tak expose pasangan file).
    pub health_pct: Option<f64>,
    /// Vendor charge thresholds, when the firmware exposes them.
    pub charge_start_threshold: Option<u64>,
    pub charge_end_threshold: Option<u64>,
}

/// Pure helper (testable): health % dari pasangan full/design mentah.
/// `None` bila design nol/negatif atau full negatif (data firmware sampah).
pub fn health_pct(full: f64, design: f64) -> Option<f64> {
    if design <= 0.0 || full < 0.0 {
        return None;
    }
    Some(full / design * 100.0)
}

impl Battery {
    /// Current draw in mW (`power_now`, fallback `current × voltage`).
    pub fn draw_mw(&self) -> Option<f64> {
        match (self.power_w, self.current_a, self.voltage_v) {
            (Some(w), _, _) => Some(w * 1000.0),
            (_, Some(a), Some(v)) => Some(a * v * 1000.0),
            _ => None,
        }
    }

    pub fn is_charging(&self) -> bool {
        self.status.as_deref() == Some("Charging")
    }

    /// Hours until empty (discharging) or full (charging). Pure computation
    /// from already-read sysfs — no I/O. `None` on Full/unknown status,
    /// unreadable rate, or garbage energy numbers.
    pub fn time_hours(&self) -> Option<f64> {
        let rate = self.draw_mw().filter(|r| *r > 0.0)?;
        match self.status.as_deref() {
            Some("Charging") => {
                let remain = self.energy_full_mwh? - self.energy_now_mwh?;
                if remain <= 0.0 || !remain.is_finite() {
                    None
                } else {
                    Some(remain / rate)
                }
            }
            Some("Discharging") => {
                let now = self.energy_now_mwh?;
                if now <= 0.0 || !now.is_finite() {
                    None
                } else {
                    Some(now / rate)
                }
            }
            _ => None,
        }
    }
}

/// AC mains online? Reads `AC*`/`ADP*` `online` nodes. `None` when the
/// firmware exposes none (desktop / tak didukung). Read-only.
pub fn ac_online() -> Option<bool> {
    let entries = std::fs::read_dir("/sys/class/power_supply").ok()?;
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().into_owned();
        if !(name.starts_with("AC") || name.starts_with("ADP")) {
            continue;
        }
        match read_trim_str(&format!("/sys/class/power_supply/{name}/online")).as_deref() {
            Some("1") => return Some(true),
            Some("0") => return Some(false),
            _ => continue,
        }
    }
    None
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
        let power = u(get("power_now")).or(match (volt, amp) {
            (Some(v), Some(a)) => Some(v * a),
            _ => None,
        });
        // Health: prefer energy_* (µWh) bila ada, fallback charge_* (µAh).
        // Rasio tak peduli satuan selama pembilang/penyebut sama.
        let (full_raw, design_raw, unit) = match (get("energy_full"), get("energy_full_design")) {
            (Some(f), Some(d)) => (f, d, "mWh"),
            _ => (
                get("charge_full").unwrap_or_default(),
                get("charge_full_design").unwrap_or_default(),
                "mAh",
            ),
        };
        let (full_milli, design_milli) = (
            parse_f64(&full_raw).map(|v| v / 1000.0),
            parse_f64(&design_raw).map(|v| v / 1000.0),
        );
        let health = match (full_milli, design_milli) {
            (Some(f), Some(d)) => health_pct(f, d),
            _ => None,
        };
        // Energy basis (mWh) buat estimasi waktu: energy_* langsung,
        // atau charge (µAh) × tegangan (V). Kedua sisi HARUS satu basis.
        let to_mwh = |v: Option<String>| v.and_then(|s| parse_f64(&s)).map(|x| x / 1000.0);
        let (energy_now_mwh, energy_full_mwh) = match (get("energy_now"), get("energy_full")) {
            (Some(n), Some(f)) => (to_mwh(Some(n)), to_mwh(Some(f))),
            _ => {
                let conv = |c: Option<String>| match (c.and_then(|s| parse_f64(&s)), volt) {
                    (Some(micro_ah), Some(v)) => Some(micro_ah / 1000.0 * v),
                    _ => None,
                };
                (conv(get("charge_now")), conv(get("charge_full")))
            }
        };
        out.push(Battery {
            name,
            capacity_pct: get("capacity").and_then(|s| parse_f64(&s)),
            status: get("status"),
            technology: get("technology"),
            voltage_v: volt,
            current_a: amp,
            power_w: power,
            cycle_count: get("cycle_count").and_then(|s| s.parse().ok()),
            energy_now_mwh,
            energy_full_mwh,
            full_milli,
            design_milli,
            capacity_unit: if full_milli.is_some() && design_milli.is_some() {
                Some(unit.to_string())
            } else {
                None
            },
            health_pct: health,
            charge_start_threshold: get("charge_control_start_threshold")
                .and_then(|s| s.parse().ok()),
            charge_end_threshold: get("charge_control_end_threshold").and_then(|s| s.parse().ok()),
        });
    }
    out.sort_by(|a, b| a.name.cmp(&b.name));
    out
}

/// Discrete steps the firmware accepts, e.g. `[40, 50, 60, 70, 80, 95]`.
/// `which` is `"start"` or `"end"`. Empty when unsupported/unreadable.
pub fn charge_available(name: &str, which: &str) -> Vec<u64> {
    let path =
        format!("/sys/class/power_supply/{name}/charge_control_{which}_available_thresholds");
    read_trim_str(&path)
        .map(|s| {
            s.split_whitespace()
                .filter_map(|p| p.parse().ok())
                .collect()
        })
        .unwrap_or_default()
}

/// Failure modes for [`set_charge_thresholds`].
#[derive(Debug)]
pub enum ChargeError {
    NoDevice {
        detail: String,
    },
    /// Value not in the firmware's available list.
    NotAllowed {
        which: String,
        value: u64,
        allowed: Vec<u64>,
    },
    Io(io::Error),
}

impl fmt::Display for ChargeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ChargeError::NoDevice { detail } => write!(f, "no battery {detail}"),
            ChargeError::NotAllowed {
                which,
                value,
                allowed,
            } => {
                let list = allowed
                    .iter()
                    .map(|v| v.to_string())
                    .collect::<Vec<_>>()
                    .join(" ");
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
        return Err(ChargeError::NoDevice {
            detail: format!("{base} absent"),
        });
    }
    for (which, val) in [("start", start), ("end", end)]
        .into_iter()
        .filter_map(|(w, v)| v.map(|x| (w, x)))
    {
        let allowed = charge_available(name, which);
        if !allowed.is_empty() && !allowed.contains(&val) {
            return Err(ChargeError::NotAllowed {
                which: which.to_string(),
                value: val,
                allowed,
            });
        }
        fs::write(
            format!("{base}/charge_control_{which}_threshold"),
            val.to_string(),
        )
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

    #[test]
    fn health_math() {
        assert_eq!(health_pct(3882.0, 5070.0).map(|v| v.round()), Some(77.0));
        assert_eq!(health_pct(100.0, 100.0), Some(100.0));
        assert_eq!(health_pct(0.0, 100.0), Some(0.0));
        assert_eq!(health_pct(50.0, 0.0), None);
        assert_eq!(health_pct(-1.0, 100.0), None);
    }

    fn test_bat(status: &str, now: f64, full: f64, watts: f64) -> Battery {
        Battery {
            name: "BAT0".to_string(),
            capacity_pct: Some(50.0),
            status: Some(status.to_string()),
            technology: None,
            voltage_v: Some(15.0),
            current_a: None,
            power_w: Some(watts),
            cycle_count: None,
            full_milli: None,
            design_milli: None,
            capacity_unit: None,
            health_pct: None,
            energy_now_mwh: Some(now),
            energy_full_mwh: Some(full),
            charge_start_threshold: None,
            charge_end_threshold: None,
        }
    }

    #[test]
    fn time_estimate_math() {
        // 30Wh tersisa, draw 15W → 2 jam.
        assert_eq!(
            test_bat("Discharging", 30000.0, 60000.0, 15.0).time_hours(),
            Some(2.0)
        );
        // Charging 30→60Wh @15W → 2 jam.
        assert_eq!(
            test_bat("Charging", 30000.0, 60000.0, 15.0).time_hours(),
            Some(2.0)
        );
        assert!(test_bat("Full", 60000.0, 60000.0, 0.0)
            .time_hours()
            .is_none());
        assert!(test_bat("Discharging", 30000.0, 60000.0, 0.0)
            .time_hours()
            .is_none());
        assert!(test_bat("Unknown", 30000.0, 60000.0, 15.0)
            .time_hours()
            .is_none());
    }
}
