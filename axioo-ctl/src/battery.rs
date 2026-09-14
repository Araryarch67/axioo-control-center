//! `axioo-ctl battery`: status + FlexiCharger thresholds via standard kernel API.

use axioo_lib::battery;

pub fn status() {
    let bats = battery::batteries();
    if bats.is_empty() {
        println!("battery: NOT FOUND (no BAT* under /sys/class/power_supply)");
        return;
    }
    for b in &bats {
        println!(
            "battery: {} ({} {})",
            b.name,
            b.technology.as_deref().unwrap_or("?"),
            b.status.as_deref().unwrap_or("?")
        );
        println!(
            "  capacity: {}%  voltage: {:.2}V  current: {:.2}A  power: {:.1}W  cycles: {}",
            b.capacity_pct
                .map_or("-".to_string(), |v| format!("{v:.0}")),
            b.voltage_v.unwrap_or(0.0),
            b.current_a.unwrap_or(0.0),
            b.power_w.unwrap_or(0.0),
            b.cycle_count.map_or("-".to_string(), |c| c.to_string())
        );
        let st = b
            .charge_start_threshold
            .map_or("-".to_string(), |v| v.to_string());
        let et = b
            .charge_end_threshold
            .map_or("-".to_string(), |v| v.to_string());
        let sa = battery::charge_available(&b.name, "start");
        let ea = battery::charge_available(&b.name, "end");
        println!("  charge_control: start={st} end={et}");
        if !sa.is_empty() || !ea.is_empty() {
            println!(
                "  available: start [{}]  end [{}]",
                sa.iter()
                    .map(|v| v.to_string())
                    .collect::<Vec<_>>()
                    .join(" "),
                ea.iter()
                    .map(|v| v.to_string())
                    .collect::<Vec<_>>()
                    .join(" ")
            );
        } else {
            println!("  available: (firmware tidak expose list — coba set 60-100 langsung)");
        }
    }
}

pub fn get() {
    let bats = battery::batteries();
    if bats.is_empty() {
        eprintln!("error: no battery found");
        std::process::exit(1);
    }
    for b in bats {
        let s = b
            .charge_start_threshold
            .map_or("-".to_string(), |v| v.to_string());
        let e = b
            .charge_end_threshold
            .map_or("-".to_string(), |v| v.to_string());
        println!("{} start={s} end={e} (BAT charge_control_*)", b.name);
    }
}

pub fn set(start: Option<u64>, end: Option<u64>) {
    if start.is_none() && end.is_none() {
        eprintln!("usage: axioo-ctl battery set --start 80 --end 90  (atau salah satu)");
        std::process::exit(2);
    }
    let bats = battery::batteries();
    if bats.is_empty() {
        eprintln!("error: no battery found");
        std::process::exit(1);
    }
    // Default BAT0 bila ada
    let name = bats[0].name.clone();
    match battery::set_charge_thresholds(&name, start, end) {
        Ok(()) => {
            let s = start.map_or("-".to_string(), |v| v.to_string());
            let e = end.map_or("-".to_string(), |v| v.to_string());
            println!("{name}: charge thresholds set start={s} end={e} (butuh root)");
        }
        Err(e) => {
            eprintln!("error: {e}");
            std::process::exit(3);
        }
    }
}
