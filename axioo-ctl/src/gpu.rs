//! `axioo-ctl gpu`: dGPU power state + holders (all read-only).

use axioo_lib::nvidia;

pub fn status() {
    match nvidia::dgpu_pci_addr() {
        None => {
            println!("dgpu: ABSENT (no NVIDIA PCI device)");
            return;
        }
        Some(addr) => {
            let state = match nvidia::dgpu_power() {
                Some(nvidia::DgpuPower::Suspended) => "SUSPENDED (RTD3 sleep)",
                _ => "ACTIVE",
            };
            println!("dgpu: {state}  (PCI {addr})");
        }
    }
    if nvidia::dgpu_power() == Some(nvidia::DgpuPower::Suspended) {
        println!("  stats skipped — querying would wake the GPU");
        return;
    }
    match nvidia::gpus() {
        None => println!("  nvidia-smi: unavailable"),
        Some(gs) => {
            for g in &gs {
                println!(
                    "  {}: {}°C  {:.1}W  clocks {}/{}MHz  util {}%",
                    g.name,
                    g.temp_c.map_or("-".to_string(), |v| format!("{v:.0}")),
                    g.power_w.unwrap_or(0.0),
                    g.gr_clock_mhz.map_or("-".to_string(), |v| v.to_string()),
                    g.mem_clock_mhz.map_or("-".to_string(), |v| v.to_string()),
                    g.usage_pct.map_or("-".to_string(), |v| format!("{v:.0}")),
                );
            }
        }
    }
    match nvidia::dgpu_procs() {
        None => println!("  holders: unknown (nvidia-smi failed)"),
        Some(p) if p.is_empty() => println!("  holders: none (awake but idle)"),
        Some(p) => {
            println!("  holders:");
            for h in &p {
                println!(
                    "    pid {}: {} ({} MiB)",
                    h.pid,
                    h.name,
                    h.mem_mb.map_or("-".to_string(), |v| v.to_string())
                );
            }
        }
    }
}
