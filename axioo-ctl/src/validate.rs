//! `axioo-ctl validate`: self-check for a new model + scrubbed report.
//!
//! EC read-only (never writes fan duty). CPU load is a built-in busy
//! loop; keyboard walk needs human eyes. Report drops `product_uuid`.

use std::io::{self, IsTerminal, Write};
use std::time::{Duration, Instant};

use axioo_lib::{cpu, devices, dmi, ec, fan, hwmon, kbd, rapl};
use serde_json::json;

const ISSUES_URL: &str = "https://github.com/Araryarch67/axioo-control-center/issues/new";

pub fn run(load_secs: u64, skip_kbd: bool, file: Option<String>, apply: bool) {
    println!("axioo-ctl validate (local self-check; EC read-only, no fan writes)");

    let dev = devices::current();
    println!("\n== device ==");
    println!("  id: {}", dev.id);
    println!("  model: {}", dev.display_name(&dmi::read_dmi()));
    println!("  grade: {}", dev.grade.as_str());
    if dev.fan_write_allowed {
        println!("  note: already validated — this run is a health re-check.");
    }

    println!("\n== DMI (scrubbed: product_uuid removed) ==");
    let dmi_map = scrubbed_dmi();
    for (k, v) in &dmi_map {
        println!("  {k:<16} {v}");
    }

    // ---- EC idle sample ----
    println!("\n== EC idle sample ==");
    let idle = ec_sample();
    match &idle {
        Some(s) => print_sample("idle", s),
        None => println!("  SKIPPED ({})", ec_hint()),
    }

    // ---- load phase ----
    let hot = if load_secs > 0 && idle.is_some() {
        println!("\n== load phase ({load_secs}s all-core busy loop) ==");
        println!("  fans will get loud — that is the point. Ctrl-C aborts (no writes pending).");
        cpu_load(load_secs);
        println!("\n== EC hot sample ==");
        match ec_sample() {
            Some(s) => {
                print_sample("hot", &s);
                Some(s)
            }
            None => {
                println!("  unreadable after load ({})", ec_hint());
                None
            }
        }
    } else {
        if load_secs == 0 {
            println!("\n== load phase skipped (--load-secs 0) ==");
        }
        None
    };

    // ---- verdicts ----
    let checks = verdicts(idle.as_ref(), hot.as_ref());
    println!("\n== verdicts ==");
    if checks.is_empty() {
        println!("  none (EC unreadable — see hint above)");
    }
    for c in &checks {
        println!("  [{}] {}: EC {} vs sys {}", c.verdict, c.name, c.ec, c.sys);
    }

    if apply {
        let summary = checks
            .iter()
            .map(|c| format!("{}:{}", c.name, c.verdict))
            .collect::<Vec<_>>()
            .join(";");
        if checks.is_empty() {
            println!("\n--apply refused: EC unreadable, nothing proven.");
            std::process::exit(1);
        }
        if checks.iter().any(|c| c.verdict == "MISMATCH") {
            println!("\n--apply refused: a check MISMATCHed — model stays locked.");
            std::process::exit(1);
        }
        let zones = kbd::discover().len();
        let stock_rapl = rapl::domains()
            .iter()
            .map(|d| {
                let lim = d
                    .constraints
                    .iter()
                    .map(|(n, w)| format!("{n}={w:.0}W"))
                    .collect::<Vec<_>>()
                    .join(",");
                format!("{}:{}", d.id, lim)
            })
            .collect::<Vec<_>>()
            .join(";");
        let ec_note = idle.as_ref().map_or("none".to_string(), |s| {
            format!("cpu{}C/rpm{}/{}", s.cpu_raw, s.fan1_rpm, s.fan2_rpm)
        });
        let facts = format!("zones={zones}\nstock_rapl={stock_rapl}\nec_idle={ec_note}\n");
        match devices::write_attestation(&dmi_map, &summary, &facts) {
            Ok(()) => println!("\nthis machine unlocked locally (grade: locally validated)."),
            Err(e) => {
                println!("\n--apply needs root for /var/lib/axiood: {e}");
                println!("  run: sudo axioo-ctl validate --apply [--load-secs N]");
                std::process::exit(1);
            }
        }
    }

    // ---- keyboard walk ----
    println!("\n== keyboard zone walk ==");
    let walk = if skip_kbd {
        println!("  skipped (--skip-kbd)");
        Vec::new()
    } else {
        kbd_walk()
    };

    // ---- report ----
    let title = format!("validate: {} ({})", dev.display_name(&dmi_map), dev.id);
    let display = dev.display_name(&dmi_map);
    let report = json!({
        "tool": "axioo-ctl validate",
        "device": {
            "id": dev.id, "marketing": display,
            "grade": dev.grade.as_str(),
            "fan_write_allowed": dev.fan_write_allowed,
            "kbd_zones": dev.kbd_zones,
        },
        "dmi": dmi_map,
        "ec": {
            "readable": idle.is_some(),
            "idle": idle.as_ref().map(sample_json),
            "hot": hot.as_ref().map(sample_json),
            "checks": checks.iter().map(|c| json!({
                "name": c.name, "ec": c.ec,
                "sys": c.sys, "verdict": c.verdict,
            })).collect::<Vec<_>>(),
        },
        "kbd_walk": walk,
        "rapl_constraints_w": rapl::domains().iter().map(|d| json!({
            "id": d.id, "constraints_w": d.constraints,
        })).collect::<Vec<_>>(),
    });
    let pretty = serde_json::to_string_pretty(&report).unwrap();
    match file {
        Some(p) => match std::fs::write(&p, &pretty) {
            Ok(()) => println!("\nreport written: {p}"),
            Err(e) => {
                println!("\nerror writing {p}: {e}");
                std::process::exit(1);
            }
        },
        None => println!("\n== report (paste into issue) ==\n{pretty}"),
    }

    let body = format!(
        "grade: {}\nverdicts: {}\n(report JSON attached/pasted below)",
        dev.grade.as_str(),
        checks
            .iter()
            .map(|c| format!("{}={}", c.name, c.verdict))
            .collect::<Vec<_>>()
            .join(", "),
    );
    println!("\nopen an issue with the report:");
    println!("  {ISSUES_URL}?title={}&body={}", pct(&title), pct(&body));
}

fn scrubbed_dmi() -> std::collections::BTreeMap<String, String> {
    let mut m = dmi::read_dmi();
    m.remove("product_uuid");
    m.remove("product_serial");
    m
}

fn ec_hint() -> &'static str {
    "need: sudo modprobe ec_sys (read-only; no writes performed)"
}

#[derive(Debug, Clone)]
struct EcSample {
    cpu_raw: u8,
    gpu_raw: u8,
    fan1_rpm: u32,
    fan2_rpm: u32,
    duty_pct: u8,
    pkg_c: Option<f64>,
    hwmon_rpm: Vec<u64>,
}

fn ec_sample() -> Option<EcSample> {
    let map = ec::read_map().ok()?;
    let snap = fan::snapshot(&map);
    Some(EcSample {
        cpu_raw: snap.cpu_temp_raw,
        gpu_raw: snap.gpu_temp_raw,
        fan1_rpm: snap.fan1_rpm,
        fan2_rpm: snap.fan2_rpm,
        duty_pct: snap.fan1_duty_pct,
        pkg_c: cpu::sample().package_temp_c,
        hwmon_rpm: hwmon::fans().iter().map(|f| f.rpm).collect(),
    })
}

fn sample_json(s: &EcSample) -> serde_json::Value {
    json!({
        "cpu_raw": s.cpu_raw, "gpu_raw": s.gpu_raw,
        "fan1_rpm": s.fan1_rpm, "fan2_rpm": s.fan2_rpm,
        "duty_pct": s.duty_pct, "pkg_c": s.pkg_c,
        "hwmon_rpm": s.hwmon_rpm,
    })
}

fn print_sample(tag: &str, s: &EcSample) {
    println!(
        "  {tag}: EC cpu={}C gpu={} duty={}% rpm1={} rpm2={} | pkg={} hwmon={:?}",
        s.cpu_raw,
        if s.gpu_raw == 0 {
            "-".to_string()
        } else {
            format!("{}C", s.gpu_raw)
        },
        s.duty_pct,
        s.fan1_rpm,
        s.fan2_rpm,
        s.pkg_c.map_or("-".to_string(), |v| format!("{v:.0}C")),
        s.hwmon_rpm,
    );
}

struct Check {
    name: &'static str,
    ec: String,
    sys: String,
    verdict: &'static str,
}

/// Same thresholds as `fan dump` cross-check: temp ±5C, RPM ±300.
fn verdicts(idle: Option<&EcSample>, hot: Option<&EcSample>) -> Vec<Check> {
    let s = match hot.or(idle) {
        Some(v) => v,
        None => return Vec::new(),
    };
    let mut out = Vec::new();
    match s.pkg_c {
        Some(pkg) => {
            let diff = (pkg - s.cpu_raw as f64).abs();
            out.push(Check {
                name: "cpu-temp",
                ec: format!("0x07={}C", s.cpu_raw),
                sys: format!("pkg={pkg:.0}C"),
                verdict: if diff <= 5.0 { "MATCH" } else { "MISMATCH" },
            });
        }
        None => out.push(Check {
            name: "cpu-temp",
            ec: format!("0x07={}C", s.cpu_raw),
            sys: "no coretemp".to_string(),
            verdict: "UNKNOWN",
        }),
    }
    if s.hwmon_rpm.is_empty() {
        out.push(Check {
            name: "fan-rpm",
            ec: format!("{}/{} RPM", s.fan1_rpm, s.fan2_rpm),
            sys: "no hwmon fans".to_string(),
            verdict: "UNKNOWN",
        });
    }
    for (i, rpm) in s.hwmon_rpm.iter().enumerate() {
        let closest = [s.fan1_rpm as i64, s.fan2_rpm as i64]
            .iter()
            .map(|r| (r - *rpm as i64).abs())
            .min()
            .unwrap_or(i64::MAX);
        out.push(Check {
            name: if i == 0 { "fan-rpm" } else { "fan-rpm-extra" },
            ec: format!("{}/{} RPM", s.fan1_rpm, s.fan2_rpm),
            sys: format!("hwmon={rpm} RPM"),
            verdict: if closest <= 300 { "MATCH" } else { "MISMATCH" },
        });
    }
    // Hot sample proves the EC bytes track reality (not stuck values).
    if let (Some(a), Some(b)) = (idle, hot) {
        let moving = a.cpu_raw != b.cpu_raw || a.fan1_rpm != b.fan1_rpm;
        out.push(Check {
            name: "ec-tracks-load",
            ec: format!(
                "cpu {}C→{}C rpm {}→{}",
                a.cpu_raw, b.cpu_raw, a.fan1_rpm, b.fan1_rpm
            ),
            sys: format!(
                "pkg {}→{}",
                a.pkg_c.map_or("-".to_string(), |v| format!("{v:.0}C")),
                b.pkg_c.map_or("-".to_string(), |v| format!("{v:.0}C")),
            ),
            verdict: if moving { "MATCH" } else { "MISMATCH" },
        });
    }
    out
}

fn cpu_load(secs: u64) {
    let threads = std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(4);
    let end = Instant::now() + Duration::from_secs(secs);
    std::thread::scope(|scope| {
        for _ in 0..threads {
            scope.spawn(|| {
                let mut x: u64 = 0;
                while Instant::now() < end {
                    for _ in 0..20000 {
                        x = x.wrapping_add(1);
                        std::hint::black_box(x);
                    }
                }
            });
        }
        while Instant::now() < end {
            print!(".");
            io::stdout().flush().ok();
            std::thread::sleep(Duration::from_secs(1));
        }
        println!();
    });
    // Let temps settle a beat so the hot sample isn't mid-spike noise.
    std::thread::sleep(Duration::from_secs(2));
}

fn prompt(q: &str) -> String {
    print!("{q}");
    io::stdout().flush().ok();
    let mut s = String::new();
    io::stdin().read_line(&mut s).ok();
    let t = s.trim().to_lowercase();
    if t.is_empty() {
        "skip".to_string()
    } else {
        t
    }
}

/// Light each node red, ask eyes to confirm, restore previous state.
/// Returns per-node answers for the report. Never fails the run.
fn kbd_walk() -> Vec<serde_json::Value> {
    let nodes = kbd::discover();
    if nodes.is_empty() {
        println!("  no kbd LED nodes (driver missing? try: axioo-ctl kbd status)");
        return Vec::new();
    }
    if !io::stdin().is_terminal() {
        println!("  non-interactive stdin — skipped (run in a terminal, or --skip-kbd)");
        return nodes
            .iter()
            .map(|kb| json!({"node": kb.name, "red_seen": "skipped-no-tty"}))
            .collect();
    }
    let mut out = Vec::new();
    for kb in &nodes {
        let saved = kbd::read_state(kb);
        match kbd::set(kb, kb.max_brightness, (255, 0, 0)) {
            Ok(()) => {
                let ans = prompt(&format!("  {} lights RED? [y/n/skip] ", kb.name));
                out.push(json!({"node": kb.name, "red_seen": ans}));
                if let Some(st) = saved {
                    let _ = kbd::set(kb, st.brightness, st.rgb);
                }
            }
            Err(e) => {
                println!("  {}: cannot write ({e}) — skipped", kb.name);
                out.push(json!({"node": kb.name, "red_seen": "unwritable"}));
            }
        }
    }
    println!("  walk done (previous colors restored best-effort)");
    out
}

fn pct(s: &str) -> String {
    let mut o = String::new();
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                o.push(b as char)
            }
            b' ' => o.push('+'),
            _ => o.push_str(&format!("%{b:02X}")),
        }
    }
    o
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scrub_removes_uuid() {
        let m = scrubbed_dmi();
        assert!(!m.contains_key("product_uuid"));
        assert!(!m.contains_key("product_serial"));
    }

    #[test]
    fn verdicts_empty_without_ec() {
        assert!(verdicts(None, None).is_empty());
    }

    #[test]
    fn verdicts_match_on_sane_numbers() {
        let s = EcSample {
            cpu_raw: 60,
            gpu_raw: 0,
            fan1_rpm: 2400,
            fan2_rpm: 2000,
            duty_pct: 50,
            pkg_c: Some(61.0),
            hwmon_rpm: vec![2422, 2015],
        };
        let v = verdicts(Some(&s), None);
        assert!(v.iter().all(|c| c.verdict == "MATCH"));
    }

    #[test]
    fn pct_encodes() {
        assert_eq!(pct("a b+c"), "a+b%2Bc");
    }
}
