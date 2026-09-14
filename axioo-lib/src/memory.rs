//! Physical memory snapshot from `/proc/meminfo` (read-only).

use crate::read_trim_str;

/// Total vs available memory in KiB, as reported by the kernel.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MemInfo {
    pub total_kb: u64,
    pub avail_kb: u64,
}

impl MemInfo {
    /// 0–100% used (`MemTotal - MemAvailable`).
    pub fn used_pct(&self) -> f64 {
        if self.total_kb == 0 {
            return 0.0;
        }
        (self.total_kb.saturating_sub(self.avail_kb)) as f64 / self.total_kb as f64 * 100.0
    }

    pub fn used_gb(&self) -> f64 {
        self.total_kb.saturating_sub(self.avail_kb) as f64 / 1024.0 / 1024.0
    }

    pub fn total_gb(&self) -> f64 {
        self.total_kb as f64 / 1024.0 / 1024.0
    }
}

/// Read current memory counters.
pub fn read() -> Option<MemInfo> {
    parse_meminfo(&read_trim_str("/proc/meminfo")?)
}

/// Pure parser (testable without `/proc`).
pub fn parse_meminfo(text: &str) -> Option<MemInfo> {
    let mut total = None;
    let mut avail = None;
    for line in text.lines() {
        let (k, rest) = line.split_once(':')?;
        let num: u64 = rest.split_whitespace().next()?.parse().ok()?;
        match k.trim() {
            "MemTotal" => total = Some(num),
            "MemAvailable" => avail = Some(num),
            _ => {}
        }
    }
    Some(MemInfo {
        total_kb: total?,
        avail_kb: avail?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_meminfo() {
        let text = "MemTotal:       16384000 kB\nMemFree:         4000000 kB\nMemAvailable:    8000000 kB\n";
        let m = parse_meminfo(text).unwrap();
        assert_eq!(m.total_kb, 16384000);
        assert!((m.used_pct() - 51.17).abs() < 0.01, "got {}", m.used_pct());
        assert_eq!(parse_meminfo("MemTotal: 1 kB\n"), None);
    }
}
