//! hwmon discovery: temperature + fan sensors from `/sys/class/hwmon`.

use std::fs;

use crate::{parse_f64, read_trim};

#[derive(Debug, Clone)]
pub struct TempSensor {
    pub chip: String,
    pub label: String,
    pub input_c: f64,
    pub max_c: Option<f64>,
    pub crit_c: Option<f64>,
}

#[derive(Debug, Clone)]
pub struct FanSensor {
    pub chip: String,
    pub label: String,
    pub rpm: u64,
}

/// (hwmon dir name, chip name), e.g. ("hwmon8", "coretemp").
pub fn chips() -> Vec<(String, String)> {
    let mut out = Vec::new();
    let Ok(entries) = fs::read_dir("/sys/class/hwmon") else {
        return out;
    };
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().into_owned();
        if !name.starts_with("hwmon") {
            continue;
        }
        let chip = read_trim(&entry.path().join("name")).unwrap_or_else(|| "?".to_string());
        out.push((name, chip));
    }
    out.sort();
    out
}

fn chip_name(hwmon: &str) -> String {
    read_trim(
        &std::path::Path::new("/sys/class/hwmon")
            .join(hwmon)
            .join("name"),
    )
    .unwrap_or_else(|| "?".to_string())
}

/// All `tempN_input` sensors (millidegree C -> degree C).
pub fn temps() -> Vec<TempSensor> {
    let mut out = Vec::new();
    for (hwmon, _) in chips() {
        let chip = chip_name(&hwmon);
        let dir = format!("/sys/class/hwmon/{hwmon}");
        let Ok(entries) = fs::read_dir(&dir) else {
            continue;
        };
        let mut inputs: Vec<String> = entries
            .flatten()
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .filter(|n| n.starts_with("temp") && n.ends_with("_input"))
            .collect();
        inputs.sort();
        for input in inputs {
            let prefix = input.trim_end_matches("_input");
            let base = format!("{dir}/{prefix}");
            let raw = match crate::read_trim_str(&format!("{base}_input")) {
                Some(v) => v,
                None => continue,
            };
            let Some(milli) = parse_f64(&raw) else {
                continue;
            };
            let label = crate::read_trim_str(&format!("{base}_label"))
                .unwrap_or_else(|| prefix.to_string());
            out.push(TempSensor {
                chip: chip.clone(),
                label,
                input_c: milli / 1000.0,
                max_c: crate::read_trim_str(&format!("{base}_max"))
                    .and_then(|v| parse_f64(&v))
                    .map(|v| v / 1000.0),
                crit_c: crate::read_trim_str(&format!("{base}_crit"))
                    .and_then(|v| parse_f64(&v))
                    .map(|v| v / 1000.0),
            });
        }
    }
    out
}

/// All `fanN_input` sensors (RPM).
pub fn fans() -> Vec<FanSensor> {
    let mut out = Vec::new();
    for (hwmon, _) in chips() {
        let chip = chip_name(&hwmon);
        let dir = format!("/sys/class/hwmon/{hwmon}");
        let Ok(entries) = fs::read_dir(&dir) else {
            continue;
        };
        let mut inputs: Vec<String> = entries
            .flatten()
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .filter(|n| n.starts_with("fan") && n.ends_with("_input"))
            .collect();
        inputs.sort();
        for input in inputs {
            let prefix = input.trim_end_matches("_input");
            let base = format!("{dir}/{prefix}");
            let raw = match crate::read_trim_str(&format!("{base}_input")) {
                Some(v) => v,
                None => continue,
            };
            let Ok(rpm) = raw.parse::<u64>() else {
                continue;
            };
            let label = crate::read_trim_str(&format!("{base}_label"))
                .unwrap_or_else(|| prefix.to_string());
            out.push(FanSensor {
                chip: chip.clone(),
                label,
                rpm,
            });
        }
    }
    out
}
