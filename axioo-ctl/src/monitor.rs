//! `axioo-ctl monitor`: live read-only dashboard (temps, clocks, power, fans).

use std::thread;
use std::time::{Duration, Instant};

use axioo_lib::{battery, cpu, hwmon, nvidia, rapl};

pub fn run(interval_s: f64, count: Option<u64>) {
    let interval = Duration::from_secs_f64(interval_s.max(0.2));
    let mut prev_rapl = rapl::domains();
    let mut prev_t = Instant::now();
    let mut shown = 0u64;

    loop {
        thread::sleep(interval);
        let now = Instant::now();
        let dt = now.duration_since(prev_t).as_secs_f64();
        prev_t = now;

        let c = cpu::sample();
        let cur_rapl = rapl::domains();
        let fans = hwmon::fans();
        let gpus = nvidia::gpus().unwrap_or_default();
        let bats = battery::batteries();

        // VT100 clear screen + home cursor.
        print!("\x1B[2J\x1B[H");
        println!("axioo-ctl monitor  (read-only, Ctrl-C to quit)\n");

        println!(
            "CPU  pkg {:>5}  max-core {:>5}  freq {:>5} / {:>5}  [{} {}]",
            opt_c(c.package_temp_c),
            opt_c(c.max_core_temp_c),
            opt_mhz(c.avg_mhz),
            opt_mhz(c.max_mhz),
            c.governor.as_deref().unwrap_or("?"),
            c.epp.as_deref().unwrap_or(""),
        );
        let mut rapl_lines = 0;
        for d in &cur_rapl {
            let w = prev_rapl
                .iter()
                .find(|p| p.id == d.id)
                .and_then(|p| rapl::watts(p, d, dt))
                .map(|x| format!("{x:>5.1}W"))
                .unwrap_or_else(|| "    -".to_string());
            println!("PWR  {:<16} {w}", d.name);
            rapl_lines += 1;
        }
        if rapl_lines > 0 && cur_rapl.iter().all(|d| d.energy_uj.is_none()) {
            println!("PWR  (energy counters need root; run with sudo for live watts)");
        }
        for g in &gpus {
            println!(
                "GPU  [{}] {:>5}  {:>6}  gr {:>5} mem {:>5}",
                g.name,
                opt_c(g.temp_c),
                opt_w(g.power_w),
                opt_mhz_u(g.gr_clock_mhz),
                opt_mhz_u(g.mem_clock_mhz),
            );
        }
        if fans.is_empty() {
            println!("FAN  (no hwmon fan nodes)");
        }
        for f in &fans {
            println!("FAN  {:<14} {:>5} RPM", f.label, f.rpm);
        }
        for b in &bats {
            println!(
                "BAT  {} {:>3}% {} {}",
                b.name,
                opt_pct(b.capacity_pct),
                b.status.as_deref().unwrap_or("?"),
                opt_w(b.power_w),
            );
        }

        prev_rapl = cur_rapl;
        shown += 1;
        if let Some(n) = count {
            if shown >= n {
                break;
            }
        }
    }
}

fn opt_c(v: Option<f64>) -> String {
    v.map_or("    -".to_string(), |x| format!("{x:>5.1}C"))
}

fn opt_w(v: Option<f64>) -> String {
    v.map_or("    -".to_string(), |x| format!("{x:>5.1}W"))
}

fn opt_mhz(v: Option<f64>) -> String {
    v.map_or("    -".to_string(), |x| format!("{x:>5.0}MHz"))
}

fn opt_mhz_u(v: Option<u64>) -> String {
    v.map_or("    -".to_string(), |x| format!("{x:>5}MHz"))
}

fn opt_pct(v: Option<f64>) -> String {
    v.map_or("-".to_string(), |x| format!("{x:.0}"))
}
