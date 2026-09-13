//! CPU snapshot: package temp (coretemp), frequencies, cpufreq policy.

use std::fs;

use crate::{parse_f64, read_trim_str};

#[derive(Debug, Clone)]
pub struct CpuSample {
    pub package_temp_c: Option<f64>,
    pub max_core_temp_c: Option<f64>,
    pub avg_mhz: Option<f64>,
    pub max_mhz: Option<f64>,
    pub driver: Option<String>,
    pub governor: Option<String>,
    pub epp: Option<String>,
}

pub fn sample() -> CpuSample {
    // --- temps from coretemp chip ---
    let mut package = None;
    let mut max_core: Option<f64> = None;
    for t in crate::hwmon::temps() {
        if t.chip != "coretemp" {
            continue;
        }
        if t.label.to_lowercase().contains("package") {
            package = Some(t.input_c);
        } else {
            max_core = Some(max_core.map_or(t.input_c, |m: f64| m.max(t.input_c)));
        }
    }

    // --- freqs from cpufreq ---
    let mut freqs = Vec::new();
    let Ok(entries) = fs::read_dir("/sys/devices/system/cpu") else {
        return CpuSample {
            package_temp_c: package,
            max_core_temp_c: max_core,
            avg_mhz: None,
            max_mhz: None,
            driver: None,
            governor: None,
            epp: None,
        };
    };
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().into_owned();
        if !name.starts_with("cpu") || !name[3..].chars().all(|c| c.is_ascii_digit()) {
            continue;
        }
        let path = format!("/sys/devices/system/cpu/{name}/cpufreq/scaling_cur_freq");
        if let Some(v) = read_trim_str(&path).and_then(|s| parse_f64(&s)) {
            freqs.push(v / 1000.0);
        }
    }
    let (avg, max) = if freqs.is_empty() {
        (None, None)
    } else {
        let sum: f64 = freqs.iter().sum();
        let max = freqs.iter().cloned().fold(0.0_f64, f64::max);
        (Some(sum / freqs.len() as f64), Some(max))
    };

    CpuSample {
        package_temp_c: package,
        max_core_temp_c: max_core,
        avg_mhz: avg,
        max_mhz: max,
        driver: read_trim_str(
            "/sys/devices/system/cpu/cpu0/cpufreq/scaling_driver",
        ),
        governor: read_trim_str(
            "/sys/devices/system/cpu/cpu0/cpufreq/scaling_governor",
        ),
        epp: read_trim_str(
            "/sys/devices/system/cpu/cpu0/cpufreq/energy_performance_preference",
        ),
    }
}
