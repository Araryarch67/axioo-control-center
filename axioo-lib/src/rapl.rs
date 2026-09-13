//! Intel RAPL energy counters (`/sys/class/powercap`) -> package power in watts.
//!
//! Note: on recent kernels `energy_uj` is readable by root only, while
//! `constraint_*` power limits are world-readable. Domains are therefore
//! reported even when the energy counter is inaccessible; live wattage
//! additionally needs root (or a privileged daemon in the future).

use std::fs;

use crate::{parse_f64, parse_u64, read_trim_str};

#[derive(Debug, Clone)]
pub struct RaplDomain {
    /// e.g. "intel-rapl:0" or "intel-rapl:0:1" (subdomain).
    pub id: String,
    pub name: String,
    /// `None` when the kernel restricts the counter (needs root).
    pub energy_uj: Option<u64>,
    pub max_uj: Option<u64>,
    /// (constraint name, limit in watts), e.g. ("long_term", 45.0).
    pub constraints: Vec<(String, f64)>,
}

/// Top-level sockets plus their subdomains (cores/uncore/dram/...).
pub fn domains() -> Vec<RaplDomain> {
    let mut out = Vec::new();
    let Ok(entries) = fs::read_dir("/sys/class/powercap") else {
        return out;
    };
    let mut names: Vec<String> = entries
        .flatten()
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .filter(|n| n.starts_with("intel-rapl"))
        .collect();
    names.sort();
    for id in names {
        let base = format!("/sys/class/powercap/{id}");
        // Skip the control-type dirs ("intel-rapl", "intel-rapl-mmio") which
        // carry no counters themselves.
        if !id.contains(':') {
            continue;
        }
        let name = read_trim_str(&format!("{base}/name")).unwrap_or_else(|| id.clone());
        let energy = read_trim_str(&format!("{base}/energy_uj")).and_then(|v| parse_u64(&v));
        let max = read_trim_str(&format!("{base}/max_energy_range_uj"))
            .and_then(|v| parse_u64(&v));
        let mut constraints = Vec::new();
        for n in 0..4u32 {
            let cname =
                read_trim_str(&format!("{base}/constraint_{n}_name")).unwrap_or_default();
            if cname.is_empty() {
                continue;
            }
            if let Some(uw) = read_trim_str(&format!("{base}/constraint_{n}_power_limit_uw"))
                .and_then(|v| parse_f64(&v))
            {
                constraints.push((cname, uw / 1_000_000.0));
            }
        }
        out.push(RaplDomain { id, name, energy_uj: energy, max_uj: max, constraints });
    }
    out
}

/// Wrap-aware power between two snapshots of the SAME domain.
/// `None` when either snapshot lacks a readable counter.
pub fn watts(before: &RaplDomain, after: &RaplDomain, dt_s: f64) -> Option<f64> {
    if before.id != after.id || dt_s <= 0.0 {
        return None;
    }
    let (b, a, max) = match (before.energy_uj, after.energy_uj, after.max_uj) {
        (Some(b), Some(a), Some(m)) => (b, a, m),
        _ => return None,
    };
    let delta = if a >= b { a - b } else { max.saturating_sub(b).saturating_add(a) };
    Some(delta as f64 / 1_000_000.0 / dt_s)
}
