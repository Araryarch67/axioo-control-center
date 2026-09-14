//! ACPI / platform layer: platform_profile, platform devices, WMI GUIDs,
//! thermal zones and cooling devices.

use std::fs;

use crate::{parse_f64, read_trim_str};

/// (`current_profile`, `available_choices`) when the firmware exposes it.
pub fn platform_profile() -> Option<(String, String)> {
    let cur = read_trim_str("/sys/firmware/acpi/platform_profile")?;
    let choices = read_trim_str("/sys/firmware/acpi/platform_profile_choices").unwrap_or_default();
    Some((cur, choices))
}

/// Device names under `/sys/devices/platform` (where `CLVxxxx` Clevo
/// WMI devices show up).
pub fn platform_devices() -> Vec<String> {
    let mut out = Vec::new();
    let Ok(entries) = fs::read_dir("/sys/devices/platform") else {
        return out;
    };
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().into_owned();
        // Skip driver symlinks noise; keep real devices.
        if name == "power" || name == "uevent" || name.starts_with("serial") {
            continue;
        }
        out.push(name);
    }
    out.sort();
    out
}

/// (device, guid) pairs from the WMI bus.
pub fn wmi_guids() -> Vec<(String, String)> {
    let mut out = Vec::new();
    let Ok(entries) = fs::read_dir("/sys/bus/wmi/devices") else {
        return out;
    };
    for entry in entries.flatten() {
        let dev = entry.file_name().to_string_lossy().into_owned();
        if let Some(guid) = read_trim(&entry.path().join("guid")) {
            out.push((dev, guid));
        }
    }
    out.sort();
    out
}

#[derive(Debug, Clone)]
pub struct ThermalZone {
    pub name: String,
    pub kind: String,
    pub temp_c: Option<f64>,
}

pub fn thermal_zones() -> Vec<ThermalZone> {
    let mut out = Vec::new();
    for n in 0..32u32 {
        let base = format!("/sys/class/thermal/thermal_zone{n}");
        let kind = match read_trim_str(&format!("{base}/type")) {
            Some(k) => k,
            None => continue,
        };
        let temp = read_trim_str(&format!("{base}/temp"))
            .and_then(|v| parse_f64(&v))
            .map(|v| v / 1000.0);
        out.push(ThermalZone {
            name: format!("thermal_zone{n}"),
            kind,
            temp_c: temp,
        });
    }
    out
}

#[derive(Debug, Clone)]
pub struct CoolingDevice {
    pub name: String,
    pub kind: String,
    pub cur_state: Option<u64>,
    pub max_state: Option<u64>,
}

pub fn cooling_devices() -> Vec<CoolingDevice> {
    let mut out = Vec::new();
    for n in 0..32u32 {
        let base = format!("/sys/class/thermal/cooling_device{n}");
        let kind = match read_trim_str(&format!("{base}/type")) {
            Some(k) => k,
            None => continue,
        };
        out.push(CoolingDevice {
            name: format!("cooling_device{n}"),
            kind,
            cur_state: read_trim_str(&format!("{base}/cur_state")).and_then(|v| v.parse().ok()),
            max_state: read_trim_str(&format!("{base}/max_state")).and_then(|v| v.parse().ok()),
        });
    }
    out
}

use crate::read_trim;
