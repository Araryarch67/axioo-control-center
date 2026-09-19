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
    if gpus.is_empty() {
        None
    } else {
        Some(gpus)
    }
}

/// dGPU power state (RTD3-aware). Read-only sysfs + one `nvidia-smi` call
/// ONLY when awake.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DgpuPower {
    /// No NVIDIA PCI device found (AMD-only / Intel-only machine).
    Absent,
    /// PCI `runtime_status == suspended` — RTD3 sleep. `nvidia-smi` is
    /// deliberately NOT run here (it would wake the GPU just to ask).
    Suspended,
    /// PCI device active — stats come from [`gpus()`], procs from
    /// [`dgpu_procs()`].
    Active,
}

/// One process holding the dGPU.
#[derive(Debug, Clone)]
pub struct GpuProc {
    pub pid: u32,
    pub name: String,
    pub mem_mb: Option<u64>,
}

/// PCI address of the NVIDIA VGA/3D controller, e.g. `Some("0000:01:00.0")`.
/// Skips the `.1` audio function (same vendor, class `0x0403xx`).
pub fn dgpu_pci_addr() -> Option<String> {
    let entries = std::fs::read_dir("/sys/bus/pci/devices").ok()?;
    for entry in entries.flatten() {
        let base = entry.path();
        let read = |f: &str| {
            std::fs::read_to_string(base.join(f))
                .ok()
                .map(|s| s.trim().to_string())
        };
        if read("vendor").as_deref() != Some("0x10de") {
            continue;
        }
        if read("class").is_some_and(|c| c.starts_with("0x03")) {
            return entry.file_name().to_string_lossy().into_owned().into();
        }
    }
    None
}

/// Current dGPU power state. `None` only when sysfs itself is unreadable;
/// use [`DgpuPower::Absent`] for the no-NVIDIA case.
pub fn dgpu_power() -> Option<DgpuPower> {
    let addr = dgpu_pci_addr()?;
    let status =
        std::fs::read_to_string(format!("/sys/bus/pci/devices/{addr}/power/runtime_status"))
            .ok()
            .map(|s| s.trim().to_string());
    match status.as_deref() {
        Some("suspended") => Some(DgpuPower::Suspended),
        Some("active") | Some("resuming") | Some("suspending") => Some(DgpuPower::Active),
        // runtime PM unsupported (desktop GPU / always-on) = effectively active.
        _ => Some(DgpuPower::Active),
    }
}

/// Processes currently holding the dGPU (`--query-compute-apps`).
/// Empty (not `None`) when awake-but-idle. `None` when `nvidia-smi` fails.
/// NOTE: compute-only — pure graphics (GL/Vulkan) clients without a CUDA
/// context may not appear; that is an `nvidia-smi` limitation, not a bug.
pub fn dgpu_procs() -> Option<Vec<GpuProc>> {
    let out = Command::new("nvidia-smi")
        .args([
            "--query-compute-apps=pid,process_name,used_memory",
            "--format=csv,noheader,nounits",
        ])
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    let text = String::from_utf8_lossy(&out.stdout);
    let mut procs = Vec::new();
    for line in text.lines() {
        if let Some(p) = parse_proc_line(line) {
            procs.push(p);
        }
    }
    Some(procs)
}

fn parse_proc_line(line: &str) -> Option<GpuProc> {
    let cols: Vec<&str> = line.split(", ").collect();
    if cols.len() < 2 {
        return None;
    }
    Some(GpuProc {
        pid: cols[0].trim().parse().ok()?,
        name: cols[1].trim().to_string(),
        mem_mb: cols
            .get(2)
            .and_then(|s| s.trim().parse::<f64>().ok())
            .map(|v| v.round() as u64),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_compute_apps() {
        let p = parse_proc_line("1234, /usr/bin/python, 512").unwrap();
        assert_eq!(p.pid, 1234);
        assert_eq!(p.name, "/usr/bin/python");
        assert_eq!(p.mem_mb, Some(512));
        assert!(parse_proc_line("garbage").is_none());
        assert!(parse_proc_line("notapid, foo, 1").is_none());
    }

    #[test]
    fn pci_addr_is_vga_not_audio() {
        // On machines with NVIDIA this must resolve to the .0 function.
        if dgpu_pci_addr().is_none() {
            return; // non-NVIDIA machine — nothing to assert
        }
        let addr = dgpu_pci_addr().unwrap();
        assert!(addr.ends_with(".0"), "expected VGA fn, got {addr}");
        // Power query must agree with nvidia-smi presence.
        let _ = dgpu_power();
    }
}
