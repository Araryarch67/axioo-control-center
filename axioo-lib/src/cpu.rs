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

/// Marketing model name from `/proc/cpuinfo` (first `model name` line).
/// Read-only; `None` when unreadable.
pub fn model_name() -> Option<String> {
    let text = read_trim_str("/proc/cpuinfo")?;
    parse_model_name(&text)
}

/// Pure part of [`model_name`] (testable without `/proc`).
pub fn parse_model_name(text: &str) -> Option<String> {
    for line in text.lines() {
        let (k, v) = line.split_once(':')?;
        if k.trim() == "model name" {
            let name = v.trim().to_string();
            if !name.is_empty() {
                return Some(name);
            }
        }
    }
    None
}

/// Aggregate CPU time counters from `/proc/stat` (`cpu` line only).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CpuTimes {
    pub idle: u64,
    pub total: u64,
}

/// Read current aggregate counters. Call twice (≥0.2s apart) and feed
/// both snapshots to [`usage_between`].
pub fn read_times() -> Option<CpuTimes> {
    let text = read_trim_str("/proc/stat")?;
    parse_cpu_line(&text)
}

/// Pure parser for the `cpu` aggregate line of `/proc/stat`.
/// Fields: user nice system idle iowait irq softirq steal guest guest_nice.
pub fn parse_cpu_line(text: &str) -> Option<CpuTimes> {
    let line = text.lines().find(|l| l.starts_with("cpu "))?;
    let nums: Vec<u64> = line
        .split_whitespace()
        .skip(1)
        .filter_map(|n| n.parse().ok())
        .collect();
    if nums.len() < 4 {
        return None;
    }
    let total: u64 = nums.iter().sum();
    let idle = nums[3] + nums.get(4).copied().unwrap_or(0);
    Some(CpuTimes { idle, total })
}

/// 0–100% CPU usage between two [`CpuTimes`] snapshots.
/// `None` when the counters did not advance (or wrapped).
pub fn usage_between(prev: &CpuTimes, cur: &CpuTimes) -> Option<f64> {
    let d_total = cur.total.checked_sub(prev.total)?;
    let d_idle = cur.idle.checked_sub(prev.idle)?;
    if d_total == 0 {
        return None;
    }
    Some((d_total - d_idle) as f64 / d_total as f64 * 100.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_model_name() {
        let text = "processor\t: 0\nmodel name\t: Intel(R) Core(TM) i9-14900HX\n";
        assert_eq!(parse_model_name(text), Some("Intel(R) Core(TM) i9-14900HX".to_string()));
        assert_eq!(parse_model_name("processor: 0\n"), None);
    }

    #[test]
    fn parses_stat_and_usage() {
        let a = "cpu  100 0 50 800 50 0 0 0 0 0\n";
        let b = "cpu  200 0 100 900 50 0 0 0 0 0\n";
        let pa = parse_cpu_line(a).unwrap();
        let pb = parse_cpu_line(b).unwrap();
        assert_eq!(pa, CpuTimes { idle: 850, total: 1000 });
        // Busy delta 150 of 250 total = 60%.
        assert_eq!(usage_between(&pa, &pb), Some(60.0));
        assert_eq!(usage_between(&pa, &pa), None);
        assert_eq!(parse_cpu_line("intr 123\n"), None);
    }
}
