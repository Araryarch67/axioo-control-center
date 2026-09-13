//! NVIDIA GPUs via read-only `nvidia-smi` queries.

use std::process::Command;

#[derive(Debug, Clone)]
pub struct Gpu {
    pub index: u32,
    pub name: String,
    pub temp_c: Option<f64>,
    pub power_w: Option<f64>,
    pub power_limit_w: Option<f64>,
    pub gr_clock_mhz: Option<u64>,
    pub mem_clock_mhz: Option<u64>,
    /// `utilization.gpu` in percent (`None` when N/A, e.g. Optimus asleep).
    pub usage_pct: Option<f64>,
}

fn parse_opt_f64(s: &str) -> Option<f64> {
    let s = s.trim();
    if s.is_empty() || s == "[N/A]" || s == "N/A" {
        return None;
    }
    s.parse().ok()
}

fn parse_opt_u64(s: &str) -> Option<u64> {
    let s = s.trim();
    if s.is_empty() || s == "[N/A]" || s == "N/A" {
        return None;
    }
    s.parse().ok()
}

/// One `nvidia-smi` invocation; `None` when the tool/driver is absent.
pub fn gpus() -> Option<Vec<Gpu>> {
    let out = Command::new("nvidia-smi")
        .args([
            "--query-gpu=index,name,temperature.gpu,power.draw,power.limit,clocks.gr,clocks.mem,utilization.gpu",
            "--format=csv,noheader,nounits",
        ])
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    let text = String::from_utf8_lossy(&out.stdout);
    let mut gpus = Vec::new();
    for line in text.lines() {
        let cols: Vec<&str> = line.split(", ").collect();
        if cols.len() < 7 {
            continue;
        }
        gpus.push(Gpu {
            index: cols[0].trim().parse().unwrap_or(0),
            name: cols[1].trim().to_string(),
            temp_c: parse_opt_f64(cols[2]),
            power_w: parse_opt_f64(cols[3]),
            power_limit_w: parse_opt_f64(cols[4]),
            gr_clock_mhz: parse_opt_u64(cols[5]),
            mem_clock_mhz: parse_opt_u64(cols[6]),
            // Appended last so older drivers emitting fewer columns still parse.
            usage_pct: cols.get(7).and_then(|s| parse_opt_f64(s)),
        });
    }
    if gpus.is_empty() { None } else { Some(gpus) }
}
